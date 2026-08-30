//! What a call site gets from `#[derive(Props)]`, exercised without a table describing the struct.

use telar_macros::Props;

#[derive(Props)]
struct LabelProps {
    #[props(into)]
    text: String,
    #[props(default)]
    muted: bool,
    #[props(default = 14.0)]
    size: f32,
}

#[test]
fn an_omitted_prop_takes_its_declared_default() {
    let props = LabelProps::props().text("Save").build();
    assert_eq!(props.text, "Save");
    assert!(!props.muted);
    assert_eq!(props.size, 14.0, "the `default = 14.0` was not applied");
}

#[test]
fn a_set_prop_wins_over_its_default() {
    let props = LabelProps::props()
        .text("Save")
        .size(20.0)
        .muted(true)
        .build();
    assert_eq!(props.size, 20.0);
    assert!(props.muted);
}

/// The point of `#[props(into)]`: a call site writes the literal it has, not the type the component declares.
#[test]
fn an_into_setter_converts_what_it_is_given() {
    let borrowed = LabelProps::props().text("Save").build();
    let owned = LabelProps::props().text(String::from("Save")).build();
    assert_eq!(borrowed.text, owned.text);
}

/// The other half of that decision, and the reason it is opt-in: an unsuffixed literal reaching a generic
/// parameter infers `f64`/`i32` and then needs a `From` that does not exist. A plain setter constrains it.
#[test]
fn a_plain_setter_takes_an_unsuffixed_literal() {
    assert_eq!(LabelProps::props().text("x").size(20.0).build().size, 20.0);
}

#[derive(Props)]
struct RangeProps {
    low: f32,
    high: f32,
    #[props(default)]
    step: f32,
}

/// Two required props, and the parameters a setter does not touch stay generic — so a caller writes them in
/// whatever order reads well, not the order the struct declares.
#[test]
fn required_props_may_be_set_in_any_order() {
    let forward = RangeProps::props().low(0.0).high(10.0).step(0.5).build();
    let backward = RangeProps::props().high(10.0).low(0.0).build();
    assert_eq!((forward.low, forward.high, forward.step), (0.0, 10.0, 0.5));
    assert_eq!((backward.low, backward.high), (0.0, 10.0));
}

#[derive(Props)]
struct BareProps {
    #[props(default)]
    flag: bool,
}

/// A struct whose every prop has a default takes no type parameters at all — the empty `<>` this would
/// otherwise generate is not valid Rust.
#[test]
fn a_struct_with_no_required_props_still_builds() {
    assert!(!BareProps::props().build().flag);
    assert!(BareProps::props().flag(true).build().flag);
}
