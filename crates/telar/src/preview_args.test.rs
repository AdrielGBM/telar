//! Preview args as a crate depending on `telar` sees them: the derive, the probe macros, and a mounted preview built again when an arg it read changes.

use std::cell::Cell;
use std::rc::Rc;

use telar::preview::host::remounting;
use telar::preview::{ArgBinding, ArgValue, ControlKind, PreviewArg, PreviewCtx};
use telar::{Container, LayoutError, LayoutItem, LayoutStyle, reset_layout_runtime};

#[derive(Clone, Copy, Debug, PartialEq, telar::PreviewArg)]
enum Size {
    Small,
    Medium,
    Large,
}

#[derive(Clone, Copy, Debug, PartialEq, telar::preview::PreviewArg)]
enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Undeclared {
    One,
}

#[test]
fn a_derived_enum_is_a_choice_of_its_variants() {
    assert_eq!(
        Size::CONTROL,
        ControlKind::Choice {
            variants: &["Small", "Medium", "Large"]
        }
    );
    assert!(
        Size::CONTROL.is_segmented(),
        "three options fit as segments"
    );
    assert!(!Weekday::CONTROL.is_segmented(), "five want a menu");
}

#[test]
fn a_derived_enum_round_trips_through_text() {
    for size in [Size::Small, Size::Medium, Size::Large] {
        let text = size.to_arg_value().to_string();
        let back: ArgValue = text.parse().unwrap();
        assert_eq!(Size::from_arg_value(&back), Some(size), "through `{text}`");
    }
    assert_eq!(Size::to_arg_value(&Size::Large).to_string(), "Large");
    assert_eq!(Size::from_arg_value(&ArgValue::Choice("Huge".into())), None);
    assert_eq!(Size::from_arg_value(&ArgValue::Bool(true)), None);
}

#[test]
fn an_override_picks_another_variant() {
    let ctx = PreviewCtx::default();
    ctx.arg("size", Size::Small);
    let args = ctx.args();
    args.set("size", ArgValue::Choice("Large".into())).unwrap();
    assert_eq!(ctx.arg("size", Size::Small), Size::Large);
    assert!(args.set("size", ArgValue::Choice("Huge".into())).is_err());
}

/// The case implicit `.rsx` args generate: a `Path::Variant` literal whose enum may or may not derive the trait.
#[test]
fn the_probe_tells_a_derived_enum_from_one_without_the_derive() {
    let ctx = PreviewCtx::default();
    let size = telar::__preview_arg!(ctx, "size", Size::Medium);
    let undeclared = telar::__preview_arg!(ctx, "undeclared", Undeclared::One);
    assert_eq!(size, Size::Medium);
    assert_eq!(undeclared, Undeclared::One);

    let rows = ctx.args().states();
    assert_eq!(rows[0].control, Size::CONTROL);
    assert_eq!(rows[1].binding, ArgBinding::ReadOnly);
    assert_eq!(rows[1].shown.as_deref(), Some("One"));
    assert_eq!(telar::__preview_control!(Size), Size::CONTROL);
    assert_eq!(telar::__preview_control!(Undeclared), ControlKind::ReadOnly);
}

fn sized(width: f32) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(Box::new(Container::new(
        LayoutStyle::new().width(width).height(10.0),
        vec![],
    )?))
}

#[test]
fn a_mounted_preview_is_built_again_only_for_an_arg_read_at_build() {
    reset_layout_runtime();
    let builds = Rc::new(Cell::new(0));
    let ctx = PreviewCtx::default();
    let args = ctx.args().clone();
    let counted = Rc::clone(&builds);
    let _canvas = remounting(
        ctx,
        move |ctx| {
            counted.set(counted.get() + 1);
            let width = ctx.arg("width", 10.0f32);
            ctx.signal("pressed", 0u32);
            sized(width)
        },
        |failure| panic!("the preview failed: {failure}"),
    )
    .unwrap();
    assert_eq!(builds.get(), 1);

    args.set("width", ArgValue::Float(20.0)).unwrap();
    assert_eq!(
        builds.get(),
        2,
        "an arg read at build mounts the preview again"
    );

    args.set("pressed", ArgValue::Int(3)).unwrap();
    assert_eq!(builds.get(), 2, "a live arg reaches the tree as it runs");

    args.reset();
    assert_eq!(
        builds.get(),
        3,
        "a reset that moves a build arg mounts it again"
    );
}

#[test]
fn a_preview_that_failed_for_one_value_is_tried_again_for_the_next() {
    reset_layout_runtime();
    let failures = Rc::new(Cell::new(0));
    let built = Rc::new(Cell::new(0));
    let ctx = PreviewCtx::default();
    let args = ctx.args().clone();
    let (failed, succeeded) = (Rc::clone(&failures), Rc::clone(&built));
    let _canvas = remounting(
        ctx,
        move |ctx| {
            if ctx.arg("broken", true) {
                panic!("asked to break");
            }
            succeeded.set(succeeded.get() + 1);
            sized(10.0)
        },
        move |_| {
            failed.set(failed.get() + 1);
            sized(1.0)
        },
    )
    .unwrap();
    assert_eq!(failures.get(), 1);

    args.set("broken", ArgValue::Bool(false)).unwrap();
    assert_eq!(built.get(), 1, "the next mount built cleanly");
    assert_eq!(failures.get(), 1);
}
