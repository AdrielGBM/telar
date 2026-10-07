use std::cell::{Cell, RefCell};
use std::rc::Rc;

use reactive_core::{RwSignal, reset_runtime, signal};

use crate::{ErrorCode, HostError, Reference, Registry, Type, compile};

use super::*;

struct Readings {
    a: RwSignal<f64>,
    b: RwSignal<f64>,
    flag: RwSignal<bool>,
    reads: Rc<RefCell<Vec<String>>>,
}

impl Readings {
    fn new() -> Self {
        Self {
            a: signal(1.0),
            b: signal(2.0),
            flag: signal(true),
            reads: Rc::default(),
        }
    }

    fn compile(source: &str) -> Compiled {
        let environment = |reference: &Reference| match reference.name.as_str() {
            "flag" => Ok(Type::Bool),
            _ => Ok(Type::Number),
        };
        compile(source, &environment, &Registry::standard()).unwrap()
    }

    /// A resolver reading the signals with `.get()`, as a host's does, recording each read.
    fn resolver(&self) -> impl Resolver + 'static {
        let (a, b, flag, reads) = (self.a, self.b, self.flag, Rc::clone(&self.reads));
        move |reference: &Reference| {
            reads.borrow_mut().push(reference.name.clone());
            Ok(match reference.name.as_str() {
                "a" => Value::Number(a.get()),
                "b" => Value::Number(b.get()),
                "flag" => Value::Bool(flag.get()),
                other => return Err(HostError::new("unknown", format!("no {other}"))),
            })
        }
    }

    fn take_reads(&self) -> Vec<String> {
        std::mem::take(&mut *self.reads.borrow_mut())
    }
}

#[test]
fn a_binding_re_evaluates_only_when_what_it_read_moves() {
    reset_runtime();
    let readings = Readings::new();
    let unrelated = signal(0);
    let bound = bind(Readings::compile("$a * 10"), readings.resolver());
    assert_eq!(bound.get(), Ok(Value::Number(10.0)));
    assert_eq!(readings.take_reads(), ["a"]);

    readings.b.set(5.0);
    unrelated.set(1);
    assert_eq!(bound.get(), Ok(Value::Number(10.0)));
    assert!(readings.take_reads().is_empty(), "nothing it read moved");

    readings.a.set(3.0);
    assert_eq!(bound.get(), Ok(Value::Number(30.0)));
    assert_eq!(readings.take_reads(), ["a"]);
}

#[test]
fn an_untaken_branch_does_not_subscribe() {
    reset_runtime();
    let readings = Readings::new();
    let bound = bind(Readings::compile("if($flag, $a, $b)"), readings.resolver());
    assert_eq!(bound.get(), Ok(Value::Number(1.0)));
    assert_eq!(readings.take_reads(), ["flag", "a"]);

    readings.b.set(20.0);
    assert!(
        readings.take_reads().is_empty(),
        "$b sits in the branch not taken"
    );

    readings.flag.set(false);
    assert_eq!(bound.get(), Ok(Value::Number(20.0)));
    assert_eq!(readings.take_reads(), ["flag", "b"]);

    readings.a.set(7.0);
    assert!(
        readings.take_reads().is_empty(),
        "$a stopped being a dependency once its branch was left"
    );
    assert_eq!(bound.get(), Ok(Value::Number(20.0)));
}

#[test]
fn a_short_circuit_subscribes_lazily() {
    reset_runtime();
    let readings = Readings::new();
    readings.flag.set(false);
    let bound = bind(Readings::compile("$flag && $a > 0"), readings.resolver());
    assert_eq!(bound.get(), Ok(Value::Bool(false)));
    assert_eq!(readings.take_reads(), ["flag"]);
    readings.a.set(-1.0);
    assert!(readings.take_reads().is_empty());
}

#[test]
fn a_binding_notifies_onward_only_when_its_result_changes() {
    reset_runtime();
    let readings = Readings::new();
    let bound = bind(Readings::compile("$a > 0"), readings.resolver());
    let runs = Rc::new(Cell::new(0));
    let counted = Rc::clone(&runs);
    let _watch = reactive_core::effect(move || {
        let _ = bound.get();
        counted.set(counted.get() + 1);
    });
    assert_eq!(runs.get(), 1);
    readings.a.set(5.0);
    assert_eq!(
        runs.get(),
        1,
        "still positive, so nothing downstream reruns"
    );
    readings.a.set(-5.0);
    assert_eq!(runs.get(), 2);
}

#[test]
fn a_held_binding_keeps_its_last_good_value_through_an_error() {
    reset_runtime();
    let readings = Readings::new();
    let bound = bind_held(Readings::compile("10 / $a"), readings.resolver());
    assert_eq!(
        bound.get(),
        Held {
            value: Some(Value::Number(10.0)),
            error: None
        }
    );

    readings.a.set(0.0);
    let held = bound.get();
    assert_eq!(
        held.value,
        Some(Value::Number(10.0)),
        "the last good value stays"
    );
    assert!(
        held.error
            .is_some_and(|error| error.code == ErrorCode::DivisionByZero)
    );

    readings.a.set(4.0);
    assert_eq!(
        bound.get(),
        Held {
            value: Some(Value::Number(2.5)),
            error: None
        }
    );
}

#[test]
fn a_held_binding_that_never_succeeded_has_no_value() {
    reset_runtime();
    let readings = Readings::new();
    readings.a.set(0.0);
    let held = bind_held(Readings::compile("1 / $a"), readings.resolver()).get();
    assert_eq!(held.value, None);
    assert!(held.error.is_some());
}
