//! Previews written in Rust as a crate depending on `telar` writes them: what `preview!` records, how its body reads args, and what the builder's setters and a decorator do.

use std::cell::Cell;

use telar::preview::host::Args;
use telar::preview::{
    ArgBinding, ArgValue, Layout, Matrix, PlayError, PreviewCtx, PreviewEntry, PreviewSurface,
    preview,
};
use telar::{
    Children, Color, Container, Direction, LayoutError, LayoutItem, LayoutStyle, Props, Reactive,
    Text, TextStyle, box_item, reset_layout_runtime, testing, use_context,
};

/// A label beside a count.
#[derive(Props)]
struct TallyProps {
    #[props(into)]
    label: Reactive<String>,
    /// How many there are.
    #[props(into, default)]
    count: Reactive<u32>,
}

fn tally(props: TallyProps, _children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let TallyProps { label, count } = props;
    let text = Text::new(
        move || format!("{} · {}", label.get(), count.get()),
        LayoutStyle::new(),
        || TextStyle::new(14.0, Color::BLACK),
    )?;
    Ok(box_item(text))
}

fn primary() -> PreviewEntry {
    preview!(tally: TallyProps, "Primary — large", |p| {
        tally(
            TallyProps::props()
                .label(p.arg("label", "Apples"))
                .count(p.arg("count", 3u32))
                .build(),
            Children::default(),
        )
    })
}

fn texts(entry: &PreviewEntry, ctx: &PreviewCtx) -> Vec<String> {
    reset_layout_runtime();
    let root = entry.build_root(ctx).expect("the preview builds");
    testing::texts(&testing::mount(root, 320, 120))
}

#[test]
fn the_id_names_the_crate_the_tag_and_the_slugged_name() {
    let entry = primary();
    assert_eq!(entry.id, "preview_rust--tally--primary-large");
    assert_eq!(entry.component, "tally");
    assert_eq!(entry.name, "Primary — large");
    assert_eq!(
        entry.title, "tally",
        "listed under its component until titled"
    );
}

#[test]
fn the_location_is_where_the_preview_is_written() {
    let line = line!() + 1;
    let entry = preview!(tally: TallyProps, "Here", |_| tally(TallyProps::props().label("x").build(), Children::default()));
    assert_eq!(
        entry.file,
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/preview_rust_test.rs")
    );
    assert!(std::path::Path::new(entry.file).is_absolute());
    assert_eq!(entry.line, line);
}

#[test]
fn the_source_is_the_body_as_written() {
    assert_eq!(
        primary().source,
        "{\n    tally(\n        TallyProps::props()\n            .label(p.arg(\"label\", \"Apples\"))\n            .count(p.arg(\"count\", 3u32))\n            .build(),\n        Children::default(),\n    )\n}"
    );
}

#[test]
fn a_body_that_is_not_a_block_is_read_from_its_first_token_to_its_last() {
    let entry = preview!(tally, "Ünïcode", |_| tally(
        TallyProps::props().label("é — ü").build(),
        Children::default()
    ));
    assert_eq!(
        entry.source,
        "tally(\n    TallyProps::props().label(\"é — ü\").build(),\n    Children::default()\n)"
    );
}

#[test]
fn the_props_type_names_the_schema() {
    let schema = (primary().props.expect("the props type was named"))();
    assert_eq!(schema.name, "TallyProps");
    assert!(schema.field("count").is_some());
}

#[test]
fn a_component_without_derived_props_names_none() {
    let entry = preview!(tally, "Bare", |_| tally(
        TallyProps::props().label("x").build(),
        Children::default()
    ));
    assert!(entry.props.is_none());
    assert_eq!(entry.id, "preview_rust--tally--bare");
}

#[test]
fn the_args_the_body_reads_get_controls_and_follow_them() {
    let entry = primary();
    let ctx = PreviewCtx::default();
    assert_eq!(texts(&entry, &ctx), ["Apples · 3"]);

    let states = ctx.args().states();
    let names: Vec<&str> = states.iter().map(|state| state.name).collect();
    assert_eq!(names, ["label", "count"]);
    assert!(
        states
            .iter()
            .all(|state| state.binding == ArgBinding::Remount)
    );

    ctx.args()
        .set("label", ArgValue::Text("Pears".into()))
        .unwrap();
    assert_eq!(texts(&entry, &ctx), ["Pears · 3"]);
}

#[test]
fn a_signal_is_shared_with_its_control() {
    let entry = preview!(tally: TallyProps, "Counting", |p| {
        let count = p.signal("count", 0u32);
        tally(
            TallyProps::props().label("Presses").count(count).build(),
            Children::default(),
        )
    });
    let ctx = PreviewCtx::default();
    assert_eq!(texts(&entry, &ctx), ["Presses · 0"]);
    assert_eq!(ctx.args().states()[0].binding, ArgBinding::Live);

    ctx.signal("count", 0u32).set(4);
    assert_eq!(ctx.args().get("count"), Some(ArgValue::Int(4)));
}

#[test]
fn a_widget_constructor_is_a_body_too() {
    let entry = preview!(column, "Empty", |_| Container::new(
        LayoutStyle::new(),
        Vec::new()
    ));
    reset_layout_runtime();
    assert!(entry.build_root(&PreviewCtx::default()).is_ok());
}

#[test]
fn a_body_that_fails_fails_the_build() {
    let entry = preview!(tally, "Failing", |_| {
        Err::<Container, _>(LayoutError::Engine("refused".into()))
    });
    assert!(entry.build_root(&PreviewCtx::default()).is_err());
}

#[test]
fn the_setters_describe_the_canvas() {
    let entry = primary()
        .title("Fixture/Tally")
        .layout(Layout::Centered)
        .viewport(390.0, 844.0)
        .bg(Color::WHITE)
        .mode("dark")
        .locale("ar")
        .dir(Direction::Rtl)
        .tags(&["stateful"])
        .matrix(Matrix::Named("themes"))
        .play(|_| Err(PlayError::new("not yet")));
    assert_eq!(entry.title, "Fixture/Tally");
    assert_eq!(entry.layout, Layout::Centered);
    assert_eq!(entry.env.viewport, Some(telar::Size::new(390.0, 844.0)));
    assert_eq!(entry.env.background, Some(Color::WHITE));
    assert_eq!(entry.env.mode, Some("dark"));
    assert_eq!(entry.env.locale, Some("ar"));
    assert_eq!(entry.env.direction, Some(Direction::Rtl));
    assert_eq!(entry.tags, ["stateful"]);
    assert_eq!(entry.matrix, Some(Matrix::Named("themes")));
    assert!(entry.play.is_some());
}

#[test]
fn the_builder_collects_into_entries() {
    let entries: Vec<PreviewEntry> =
        vec![primary().title("A"), primary().layout(Layout::Fullscreen)];
    assert_eq!(entries.len(), 2);
}

#[derive(Clone)]
struct Frame(&'static str);

fn framed(children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let items = children.build_with(Frame("framed"))?.take_default();
    Ok(box_item(Container::new(
        LayoutStyle::new().padding_all(8.0),
        items,
    )?))
}

#[test]
fn a_decorator_wraps_the_root_and_its_context_reaches_it() {
    let entry = preview!(tally, "Framed", |_| {
        let frame = use_context::<Frame>().map_or("bare", |frame| frame.0);
        tally(
            TallyProps::props().label(frame).build(),
            Children::default(),
        )
    });
    assert_eq!(texts(&entry, &PreviewCtx::default()), ["bare · 0"]);

    let entry = entry.decorate(framed);
    assert_eq!(texts(&entry, &PreviewCtx::default()), ["framed · 0"]);
}

#[test]
fn the_decorator_builds_the_root_each_time_it_asks() {
    thread_local! {
        static BUILDS: Cell<u32> = const { Cell::new(0) };
    }
    fn twice(children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let mut items = Vec::new();
        for _ in 0..2 {
            items.extend(children.build_slot(None)?);
        }
        Ok(box_item(Container::new(LayoutStyle::new(), items)?))
    }
    let entry = preview!(tally, "Twice", |_| {
        BUILDS.with(|builds| builds.set(builds.get() + 1));
        tally(TallyProps::props().label("x").build(), Children::default())
    })
    .decorate(twice);
    assert_eq!(texts(&entry, &PreviewCtx::default()), ["x · 0", "x · 0"]);
    assert_eq!(BUILDS.with(Cell::get), 2);
}

fn r#type(props: TallyProps, children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    tally(props, children)
}

#[test]
fn a_raw_tag_names_its_component_as_its_id_does() {
    let entry = preview!(r#type, "Default", |_| r#type(
        TallyProps::props().label("x").build(),
        Children::default()
    ));
    assert_eq!(entry.component, "type");
    assert_eq!(entry.id, "preview_rust--type--default");
}

#[test]
fn an_arg_named_after_a_prop_is_described_by_it() {
    let entry = primary();
    let ctx = PreviewCtx::from(Args::for_entry(&entry));
    assert_eq!(texts(&entry, &ctx), ["Apples · 3"]);
    let count = ctx
        .args()
        .states()
        .into_iter()
        .find(|state| state.name == "count")
        .unwrap();
    assert_eq!(count.doc, "How many there are.");
    assert_eq!(count.prop.expect("linked by name").ty, "Reactive<u32>");
    assert_eq!(count.default, Some(ArgValue::Int(3)));
}

#[test]
fn a_surface_preview_builds_inside_its_surface() {
    let entry = primary().surface(PreviewSurface::new(200.0, 80.0));
    let ctx = PreviewCtx::default();
    reset_layout_runtime();
    let tree = testing::mount(entry.build_root(&ctx).unwrap(), 320, 120);
    assert_eq!(testing::texts(&tree), ["Apples · 3"]);
}
