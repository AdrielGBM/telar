//! A props struct's callbacks as a preview logs them: what `#[derive(Props)]` generates for them, and how `preview!` hands them to the canvas's action log.

use std::cell::Cell;
use std::rc::Rc;

use telar::preview::{ActionLog, PreviewActions, PreviewCtx, preview};
use telar::{
    Children, Container, Handler, LayoutError, LayoutItem, LayoutStyle, Props, box_item,
    reset_layout_runtime,
};

type OnPick = Handler<fn(u32)>;

struct Opaque;

#[derive(Props)]
struct PadProps {
    #[props(into, default = Handler::from(|| {}))]
    on_press: Handler,
    #[props(some, into, default)]
    on_toggle: Option<Handler<fn(bool)>>,
    #[props(some, into, default)]
    on_pick: Option<OnPick>,
    #[props(some, into, default)]
    on_drop: Option<Handler<fn(Opaque)>>,
    #[props(some, default)]
    style: Option<Rc<dyn Fn(u32) -> u32>>,
    #[props(into)]
    on_close: Handler,
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
    props.on_press.run();
    if let Some(on_toggle) = &props.on_toggle {
        on_toggle.run(true);
    }
    if let Some(on_pick) = &props.on_pick {
        on_pick.run(2);
    }
    if let Some(on_drop) = &props.on_drop {
        on_drop.run(Opaque);
    }
    props.on_close.run();
}

#[test]
fn the_builder_logs_every_optional_callback_it_holds_in_any_state() {
    let log = ActionLog::new();
    let props = props()
        .on_toggle(|_| {})
        .on_pick(|_| {})
        .on_drop(|_| {})
        .__preview_actions(&log)
        .on_close(|| {})
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
        .on_toggle({
            let toggled = Rc::clone(&toggled);
            move |on| toggled.set(Some(on))
        })
        .on_close(|| {})
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
        .on_close({
            let closed = Rc::clone(&closed);
            move || closed.set(true)
        })
        .build()
        .preview_actions(&log);
    run(&props);
    assert!(closed.get());
    assert_eq!(log.count_of("on_close"), 1);
    assert_eq!(
        log.count(),
        2,
        "the optional callbacks left unset stay unset and are never called"
    );
}

#[derive(Props)]
struct ClickerProps {
    #[props(into, default = Handler::from(|| {}))]
    on_press: Handler,
    #[props(default)]
    children: Children,
}

/// Presses itself as it builds, standing in for a user's tap.
fn clicker(props: ClickerProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    props.on_press.run();
    Ok(box_item(Container::new(
        LayoutStyle::new(),
        props.children.build()?,
    )?))
}

#[test]
fn a_preview_logs_the_calls_of_a_defaulted_callback_it_does_not_set() {
    let entry = preview!(clicker: ClickerProps, "Default", |_| {
        clicker(ClickerProps::props().build())
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
        let nested = Children::new(move || Ok(vec![clicker(ClickerProps::props().build())?]));
        clicker(
            ClickerProps::props()
                .on_press(move || presses.update(|count| *count += 1))
                .children(nested)
                .build(),
        )
    });
    reset_layout_runtime();
    let ctx = PreviewCtx::default();
    entry.build_root(&ctx).expect("the preview builds");
    assert_eq!(ctx.signal("presses", 0u32).get(), 1);
    assert_eq!(
        ctx.actions().count_of("on_press"),
        2,
        "the nested call is logged too, and the children prop it was handed in passes through"
    );
}

#[test]
fn a_preview_without_props_logs_nothing() {
    let entry = preview!(clicker, "Bare", |_| {
        clicker(ClickerProps::props().build())
    });
    reset_layout_runtime();
    let ctx = PreviewCtx::default();
    entry.build_root(&ctx).expect("the preview builds");
    assert_eq!(ctx.actions().count(), 0);
}

#[derive(Props)]
struct ChipProps {
    #[props(some, into, default)]
    on_remove: Option<Handler>,
}

/// What a component draws for a callback it is handed: a remove button for an `on_remove`.
fn chip(props: ChipProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let parts = match props.on_remove {
        Some(on_remove) => {
            on_remove.run();
            vec![box_item(Container::new(LayoutStyle::new(), Vec::new())?)]
        }
        None => Vec::new(),
    };
    Ok(box_item(Container::new(LayoutStyle::new(), parts)?))
}

/// Whether a callback is there can be what a component draws, so a preview hands the component only the callbacks its body set.
#[test]
fn a_preview_leaves_an_optional_callback_it_does_not_set_unset() {
    let bare = preview!(chip: ChipProps, "Bare", |_| chip(ChipProps::props().build()));
    let removable = preview!(chip: ChipProps, "Removable", |_| {
        chip(ChipProps::props().on_remove(|| {}).build())
    });
    reset_layout_runtime();
    let ctx = PreviewCtx::default();
    let root = bare.build_root(&ctx).expect("the preview builds");
    assert_eq!(
        ctx.actions().count(),
        0,
        "no `on_remove` was made up for it"
    );
    drop(root);
    let ctx = PreviewCtx::default();
    removable.build_root(&ctx).expect("the preview builds");
    assert_eq!(
        ctx.actions().count_of("on_remove"),
        1,
        "the one it set is logged"
    );
}
