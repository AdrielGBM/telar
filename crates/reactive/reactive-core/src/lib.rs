//! Factory functions (`create_rw_signal`, `create_effect`, `create_memo`) create
//! nodes in the reactive graph. Struct constructors (`Runtime::new`, etc.) own
//! their state. Free functions (`batch`, `set_flush_notify`) operate on the
//! thread-local runtime.

mod effect;
mod memo;
mod runtime;
mod signal;

pub use effect::{Effect, create_effect};
pub use memo::{Memo, create_memo};
pub use runtime::{
    FlushNotifyHandle, batch, begin_batch, end_batch, reset_runtime, set_flush_notify,
};
pub use signal::{ReadSignal, RwSignal, create_rw_signal};

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    #[test]
    fn signal_get_set() {
        let count = create_rw_signal(0i32);
        assert_eq!(count.get(), 0);
        count.set(42);
        assert_eq!(count.get(), 42);
    }

    #[test]
    fn signal_update() {
        let count = create_rw_signal(10i32);
        count.update(|v| *v *= 2);
        assert_eq!(count.get(), 20);
    }

    #[test]
    fn signal_with() {
        let name = create_rw_signal(String::from("rsx"));
        let len = name.with(|s| s.len());
        assert_eq!(len, 3);
    }

    #[test]
    fn rw_signal() {
        let count = create_rw_signal(0i32);
        count.set(10);
        assert_eq!(count.get(), 10);
        count.update(|v| *v += 5);
        assert_eq!(count.get(), 15);
    }

    #[test]
    fn rw_signal_read_only() {
        let sig = create_rw_signal(0i32);
        let read = sig.read_only();
        sig.set(7);
        assert_eq!(read.get(), 7);
    }

    #[test]
    fn effect_runs_immediately() {
        let ran = Rc::new(RefCell::new(false));
        let ran_clone = Rc::clone(&ran);
        let _e = create_effect(move || {
            *ran_clone.borrow_mut() = true;
        });
        assert!(*ran.borrow());
    }

    #[test]
    fn effect_reruns_on_signal_change() {
        let count = create_rw_signal(0i32);
        let log: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));
        let log_clone = Rc::clone(&log);

        let read = count.read_only();
        let _e = create_effect(move || {
            log_clone.borrow_mut().push(read.get());
        });

        count.set(1);
        count.set(2);

        assert_eq!(*log.borrow(), vec![0, 1, 2]);
    }

    #[test]
    fn memo_derives_value() {
        let count = create_rw_signal(2i32);
        let read = count.read_only();
        let doubled = create_memo(move || read.get() * 2);

        assert_eq!(doubled.get(), 4);
        count.set(5);
        assert_eq!(doubled.get(), 10);
    }

    #[test]
    fn memo_chains() {
        let n = create_rw_signal(3i32);
        let read = n.read_only();
        let doubled = create_memo(move || read.get() * 2);
        let doubled_for_quad = doubled.clone();
        let quadrupled = create_memo(move || doubled_for_quad.get() * 2);

        assert_eq!(quadrupled.get(), 12);
        n.set(5);
        assert_eq!(quadrupled.get(), 20);
    }

    #[test]
    fn batch_fires_effect_once() {
        let a = create_rw_signal(0i32);
        let b = create_rw_signal(0i32);
        let runs = Rc::new(RefCell::new(0usize));
        let runs_clone = Rc::clone(&runs);

        let a_read = a.read_only();
        let b_read = b.read_only();
        let _e = create_effect(move || {
            let _ = a_read.get() + b_read.get();
            *runs_clone.borrow_mut() += 1;
        });

        assert_eq!(*runs.borrow(), 1);

        batch(|| {
            a.set(1);
            b.set(2);
        });

        assert_eq!(*runs.borrow(), 2);
    }

    #[test]
    fn dropping_one_subscriber_keeps_others_consistent() {
        // Three effects subscribe to the same signal. Dropping the middle one and then writing repeatedly must keep the survivors firing and must not panic — exercising the in-place subscriber-list cleanup that runs alongside notify_signal's reused scratch buffer (the cleanup must stay correct now that subscribers are no longer cloned per write).
        let count = create_rw_signal(0i32);
        let a = Rc::new(RefCell::new(0i32));
        let b = Rc::new(RefCell::new(0i32));
        let c = Rc::new(RefCell::new(0i32));

        let mk = |sink: &Rc<RefCell<i32>>, sig: &RwSignal<i32>| {
            let read = sig.read_only();
            let sink = Rc::clone(sink);
            create_effect(move || {
                *sink.borrow_mut() = read.get();
            })
        };

        let _ea = mk(&a, &count);
        let eb = mk(&b, &count);
        let _ec = mk(&c, &count);

        count.set(1);
        assert_eq!((*a.borrow(), *b.borrow(), *c.borrow()), (1, 1, 1));

        drop(eb);
        count.set(2);
        count.set(3);

        // Survivors tracked every write; the dropped one is frozen at its last value, with no panic and no lost survivor.
        assert_eq!(*a.borrow(), 3);
        assert_eq!(*c.borrow(), 3);
        assert_eq!(*b.borrow(), 1);
    }
}
