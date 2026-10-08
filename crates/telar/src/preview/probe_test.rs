use super::*;
use crate::preview::{ArgBinding, ArgValue, PreviewCtx};

#[derive(Debug, PartialEq)]
struct Shown(u8);

#[derive(PartialEq)]
struct Opaque(u8);

#[test]
fn a_type_with_preview_arg_gets_its_control() {
    assert_eq!(crate::__preview_control!(bool), ControlKind::Toggle);
    assert_eq!(
        crate::__preview_control!(Option<f32>),
        ControlKind::Optional(&ControlKind::FLOAT)
    );
}

#[test]
fn a_type_without_one_is_read_only() {
    assert_eq!(crate::__preview_control!(Shown), ControlKind::ReadOnly);
    assert_eq!(crate::__preview_control!(Opaque), ControlKind::ReadOnly);
    assert_eq!(
        crate::__preview_control!(Option<Opaque>),
        ControlKind::ReadOnly,
        "an `Option` of a type without a control has none either"
    );
}

#[test]
fn a_value_with_a_control_is_read_through_the_args() {
    let ctx = PreviewCtx::default();
    ctx.arg("label", "Save");
    ctx.args()
        .set("label", ArgValue::Text("Send".into()))
        .unwrap();
    let label: &'static str = crate::__preview_arg!(ctx, "label", "Save");
    assert_eq!(label, "Send");
    let size = crate::__preview_arg!(&ctx, "size", type f32, 16.0);
    assert_eq!(size, 16.0);
    let row = ctx
        .args()
        .states()
        .into_iter()
        .find(|s| s.name == "size")
        .unwrap();
    assert_eq!(row.control, ControlKind::FLOAT);
    assert_eq!(row.binding, ArgBinding::Remount);
}

/// A literal whose type is settled only by where it goes still takes the control tier.
#[test]
fn an_unsuffixed_literal_takes_the_control_tier() {
    let ctx = PreviewCtx::default();
    let gap: u16 = crate::__preview_arg!(ctx, "gap", 8);
    assert_eq!(gap, 8);
    let row = ctx
        .args()
        .states()
        .into_iter()
        .find(|s| s.name == "gap")
        .unwrap();
    assert_eq!(row.control, ControlKind::INTEGER);
}

#[test]
fn a_value_without_a_control_passes_through_and_is_listed_read_only() {
    let ctx = PreviewCtx::default();
    let shown = crate::__preview_arg!(ctx, "shown", Shown(3));
    let opaque = crate::__preview_arg!(ctx, "opaque", Opaque(4));
    assert_eq!(shown, Shown(3));
    assert!(opaque == Opaque(4));

    let rows = ctx.args().states();
    let shown = rows.iter().find(|s| s.name == "shown").unwrap();
    assert_eq!(shown.binding, ArgBinding::ReadOnly);
    assert_eq!(shown.shown.as_deref(), Some("Shown(3)"));
    let opaque = rows.iter().find(|s| s.name == "opaque").unwrap();
    assert_eq!(opaque.control, ControlKind::ReadOnly);
    assert_eq!(opaque.shown, None);
}

#[test]
fn a_signal_with_a_control_is_shared_with_it() {
    let ctx = PreviewCtx::default();
    let agree = crate::__preview_signal!(ctx, "agree", false);
    ctx.args().set("agree", ArgValue::Bool(true)).unwrap();
    assert!(agree.peek());
}

#[test]
fn a_signal_without_a_control_is_a_plain_signal() {
    let ctx = PreviewCtx::default();
    let items = crate::__preview_signal!(ctx, "items", type Vec<Opaque>, Vec::new());
    items.update(|items| items.push(Opaque(1)));
    assert_eq!(items.with(Vec::len), 1);
    assert!(ctx.args().set("items", ArgValue::Unset).is_err());
}
