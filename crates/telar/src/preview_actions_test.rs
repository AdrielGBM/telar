//! A props struct's callbacks as a preview logs them: what `#[derive(Props)]` generates for them, and how `preview!` hands them to the canvas's action log.

use std::cell::Cell;
use std::rc::Rc;

use telar::preview::{ActionLog, PreviewActions, PreviewCtx, preview};
use telar::{
    Children, Container, LayoutError, LayoutItem, LayoutStyle, Props, box_item,
    reset_layout_runtime,
};

type OnPick = Rc<dyn Fn(u32)>;

struct Opaque;

#[derive(Props)]
struct PadProps {
    #[props(default = Rc::new(|| {}))]
    on_press: Rc<dyn Fn()>,
    #[props(some, default)]
    on_toggle: Option<Rc<dyn Fn(bool)>>,
    #[props(some, default)]
    on_pick: Option<OnPick>,
    #[props(some, default)]
    on_drop: Option<Rc<dyn Fn(Opaque)>>,
    #[props(some, default)]
    style: Option<Rc<dyn Fn(u32) -> u32>>,
    on_close: Rc<dyn Fn()>,
}

fn props() -> PadPropsBuilder<PadPropsMissingOnClose> {
    PadProps::props()
}

fn calls(log: &ActionLog) -> Vec<(&'static str, Vec<String>)> {
    log.calls()
        .into_iter()
        .map(|call| (call.name, call.args))
        .collect()
}

fn run(props: &PadProps) {
    (props.on_press)();
    if let Some(on_toggle) = &props.on_toggle {
        on_toggle(true);
    }
    if let Some(on_pick) = &props.on_pick {
        on_pick(2);
    }
    if let Some(on_drop) = &props.on_drop {
        on_drop(Opaque);
    }
    (props.on_close)();
}

#[test]
fn the_builder_logs_every_optional_callback_in_any_state() {
    let log = ActionLog::new();
    let props = props()
        .__preview_actions(&log)
        .on_close(Rc::new(|| {}))
        .build();
    run(&props);
    let names: Vec<&str> = calls(&log).iter().map(|(name, _)| *name).collect();
    assert_eq!(
        names,
        ["on_press", "on_toggle", "on_pick", "on_drop"],
        "the required `on_close` is the struct's to log, not the builder's"
    );
    assert_eq!(calls(&log)[1].1, ["true"]);
    assert_eq!(
        calls(&log)[2].1,
        ["2"],
        "a callback named through an alias is found by its type"
    );
    assert!(calls(&log)[3].1[0].ends_with("Opaque>"));
    assert!(
        props.style.is_none(),
        "a callback that returns a value is no action and stays unset"
    );
}

#[test]
fn a_callback_set_before_the_builder_logs_still_runs() {
    let log = ActionLog::new();
    let toggled = Rc::new(Cell::new(None));
    let props = props()
        .on_toggle(Rc::new({
            let toggled = Rc::clone(&toggled);
            move |on| toggled.set(Some(on))
        }))
        .on_close(Rc::new(|| {}))
        .__preview_actions(&log)
        .build();
    run(&props);
    assert_eq!(toggled.get(), Some(true));
    assert_eq!(log.count_of("on_toggle"), 1);
}

#[test]
fn the_props_log_their_required_callbacks_too() {
    let log = ActionLog::new();
    let closed = Rc::new(Cell::new(false));
    let props = props()
        .on_close(Rc::new({
            let closed = Rc::clone(&closed);
            move || closed.set(true)
        }))
        .build()
        .preview_actions(&log);
    run(&props);
    assert!(closed.get());
    assert_eq!(log.count_of("on_close"), 1);
    assert_eq!(log.count(), 5);
}

#[derive(Props)]
struct ClickerProps {
    #[props(default = Rc::new(|| {}))]
    on_press: Rc<dyn Fn()>,
}

/// Presses itself as it builds, standing in for a user's tap.
fn clicker(props: ClickerProps, _children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    (props.on_press)();
    Ok(box_item(Container::new(LayoutStyle::new(), Vec::new())?))
}

#[test]
fn a_preview_logs_the_calls_of_a_callback_it_leaves_unset() {
    let entry = preview!(clicker: ClickerProps, "Default", |_| {
        clicker(ClickerProps::props().build(), Children::default())
    });
    reset_layout_runtime();
    let ctx = PreviewCtx::default();
    entry.build_root(&ctx).expect("the preview builds");
    assert_eq!(calls(&ctx.actions()), [("on_press", Vec::new())]);
}

#[test]
fn a_preview_logs_a_callback_it_sets_and_runs_it() {
    let entry = preview!(clicker: ClickerProps, "Counting", |p| {
        let presses = p.signal("presses", 0u32);
        let nested = Children::new(move || {
            let mut slots = telar::Slots::new();
            slots.push(
                None,
                clicker(ClickerProps::props().build(), Children::default())?,
            );
            Ok(slots)
        });
        clicker(
            ClickerProps::props()
                .on_press(Rc::new(move || presses.update(|count| *count += 1)))
                .build(),
            nested,
        )
    });
    reset_layout_runtime();
    let ctx = PreviewCtx::default();
    entry.build_root(&ctx).expect("the preview builds");
    assert_eq!(ctx.signal("presses", 0u32).get(), 1);
    assert_eq!(ctx.actions().count_of("on_press"), 1);
}

#[test]
fn a_preview_without_props_logs_nothing() {
    let entry = preview!(clicker, "Bare", |_| {
        clicker(ClickerProps::props().build(), Children::default())
    });
    reset_layout_runtime();
    let ctx = PreviewCtx::default();
    entry.build_root(&ctx).expect("the preview builds");
    assert_eq!(ctx.actions().count(), 0);
}
