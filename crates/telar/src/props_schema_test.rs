//! A props struct's schema as a crate depending on `telar` sees it: docs, types, defaults and the control each prop gets.

use std::rc::Rc;

use telar::preview::{ControlKind, HasPropsSchema, PreviewEntry, PropDefault, PropsSchema};
use telar::{Children, Container, LayoutError, LayoutItem, LayoutStyle, Props, Reactive};

#[derive(Clone, Debug)]
struct Track;

/// A labelled slider.
///
/// Drag the thumb to set it.
#[derive(Props)]
struct SliderProps {
    /// What the slider is for.
    #[props(into)]
    label: Reactive<String>,
    /// Where the thumb sits.
    #[props(default = 0.5, control = range(0, 1).step(0.1))]
    value: f32,
    #[props(default, some)]
    #[props(control = multiline, doc = "Shown under the slider.")]
    hint: Option<String>,
    #[props(default = Track)]
    track: Track,
    #[props(default, control = read_only)]
    locked: bool,
    #[props(default = Rc::new(|_| {}))]
    on_change: Rc<dyn Fn(f32)>,
}

fn schema() -> &'static PropsSchema {
    <SliderProps as HasPropsSchema>::schema()
}

#[test]
fn the_struct_is_named_and_documented() {
    assert_eq!(schema().name, "SliderProps");
    assert_eq!(
        schema().doc,
        "A labelled slider.\n\nDrag the thumb to set it."
    );
    assert!(
        std::ptr::eq(schema(), schema()),
        "one static, not a fresh schema per call"
    );
}

#[test]
fn the_fields_are_listed_in_declaration_order() {
    let names: Vec<&str> = schema().fields.iter().map(|field| field.name).collect();
    assert_eq!(
        names,
        ["label", "value", "hint", "track", "locked", "on_change"]
    );
    assert!(schema().field("missing").is_none());
}

#[test]
fn a_field_carries_its_type_doc_default_and_setter() {
    let label = schema().field("label").unwrap();
    assert_eq!(label.ty, "Reactive<String>");
    assert_eq!(label.doc, "What the slider is for.");
    assert!(label.is_required());
    assert_eq!(label.default, PropDefault::Required);
    assert!(label.into && !label.some);

    let value = schema().field("value").unwrap();
    assert_eq!(value.default, PropDefault::Expr("0.5"));
    assert!(!value.is_required());

    let hint = schema().field("hint").unwrap();
    assert_eq!(hint.ty, "Option<String>");
    assert_eq!(hint.doc, "Shown under the slider.");
    assert_eq!(hint.default, PropDefault::TypeDefault);
    assert!(hint.some && !hint.into);

    assert_eq!(schema().field("track").unwrap().doc, "");
    assert_eq!(
        schema().field("on_change").unwrap().default,
        PropDefault::Expr("Rc::new(|_| {})")
    );
}

#[test]
fn a_field_without_an_override_gets_its_type_s_control() {
    assert_eq!(
        schema().field("label").unwrap().control(),
        ControlKind::TEXT,
        "through `Reactive<String>` to `String`"
    );
}

#[test]
fn an_override_refines_the_type_s_control() {
    assert_eq!(
        schema().field("value").unwrap().control(),
        ControlKind::FLOAT.range(0.0, 1.0).step(0.1)
    );
    assert_eq!(
        schema().field("hint").unwrap().control(),
        ControlKind::Optional(&ControlKind::MULTILINE),
        "a refinement reaches through the `Option`"
    );
}

#[test]
fn a_type_without_a_control_is_read_only() {
    assert!(schema().field("track").unwrap().control().is_read_only());
    assert!(
        schema()
            .field("on_change")
            .unwrap()
            .control()
            .is_read_only(),
        "a callback is not a control"
    );
}

#[test]
fn read_only_wins_over_a_type_that_has_a_control() {
    assert!(schema().field("locked").unwrap().control().is_read_only());
}

/// The dot a preview draws.
#[telar::component]
fn dot(
    /// How wide it is.
    #[props(default = 4.0)]
    size: f32,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(Box::new(Container::new(
        LayoutStyle::new().width(size),
        vec![],
    )?))
}

#[test]
fn a_component_s_props_carry_its_docs() {
    let schema = <DotProps as HasPropsSchema>::schema();
    assert_eq!(schema.doc, "The dot a preview draws.");
    assert_eq!(schema.field("size").unwrap().doc, "How wide it is.");
    assert_eq!(schema.field("size").unwrap().control(), ControlKind::FLOAT);
}

fn build_dot(_: &telar::preview::PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    dot(DotProps::props().build(), Children::default())
}

const DOT_PREVIEW: PreviewEntry =
    PreviewEntry::new("telar--dot--default", "dot", "Default", build_dot)
        .props(<DotProps as HasPropsSchema>::schema);

#[test]
fn a_preview_names_its_component_s_schema() {
    let schema = DOT_PREVIEW.props.expect("the preview names a schema")();
    assert_eq!(schema.name, "DotProps");
}

#[derive(Props)]
struct OutlineProps {
    #[props(default = 2)]
    level: u8,
    #[props(default)]
    indent: f32,
    #[props(default, some)]
    limit: Option<usize>,
    #[props(into, default)]
    count: Reactive<usize>,
}

/// A numeric prop takes the number a `.rsx` literal is handed over as, whole or not, as well as a Rust value of its own type.
#[test]
fn a_numeric_prop_takes_a_markup_number_or_its_own_type() {
    let marked = OutlineProps::props()
        .level(telar::Int(3))
        .indent(telar::Int(12))
        .limit(telar::Int(5))
        .count(telar::Int(4))
        .build();
    assert_eq!(
        (
            marked.level,
            marked.indent,
            marked.limit,
            marked.count.get()
        ),
        (3, 12.0, Some(5), 4)
    );
    let written = OutlineProps::props()
        .level(4)
        .indent(telar::Float(1.5))
        .limit(6)
        .build();
    assert_eq!(
        (written.level, written.indent, written.limit),
        (4, 1.5, Some(6))
    );
}
