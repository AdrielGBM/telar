//! The fixture's Rust previews as a crate depending on it sees them: built headlessly under `telar/previews`, and absent without it. Run both ways: `cargo test -p telar-rsx-fixture` and again with `--features telar/previews`.

use std::any::TypeId;

#[allow(dead_code)]
mod absent {
    pub struct Absent;

    pub fn telar_all_previews() -> Absent {
        Absent
    }
}

// Two glob imports of one name are ambiguous where it is used, so without `telar/previews` this compiles only while the fixture exports no `telar_all_previews`; with the feature, the explicit import shadows both globs.
mod catalogue {
    #[allow(unused_imports)]
    pub use super::absent::*;
    #[allow(unused_imports)]
    pub use telar_rsx_fixture::*;

    telar::__previews! {
        pub use telar_rsx_fixture::telar_all_previews;
    }
}

#[allow(unused_imports)]
use gate::*;

#[allow(dead_code)]
mod gate {
    pub const PREVIEWS_COMPILED: bool = false;
}

telar::__previews! {
    const PREVIEWS_COMPILED: bool = true;
}

fn type_id_of<T: 'static>(_: &T) -> TypeId {
    TypeId::of::<T>()
}

#[test]
fn the_catalogue_is_exported_exactly_when_previews_are_compiled() {
    let catalogue = catalogue::telar_all_previews();
    let exported = type_id_of(&catalogue) != TypeId::of::<absent::Absent>();
    assert_eq!(exported, PREVIEWS_COMPILED);
}

telar::__previews! {
    mod with_previews {
        use telar::preview::host::Args;
        use telar::preview::{ArgBinding, ArgValue, Axis, ControlKind, Layout, Matrix, PreviewCtx, PreviewEntry};
        use telar::{
            AvailableSpace, ComponentList, DrawCommand, compute_layout, install_default_text_metrics,
            reset_layout_runtime, testing,
        };
        use telar_rsx_fixture::telar_all_previews;

        const SOURCE: &str = include_str!("previews.rs");

        fn entry(id: &str) -> PreviewEntry {
            telar_all_previews()
                .into_iter()
                .find(|entry| entry.id == id)
                .unwrap_or_else(|| panic!("no preview `{id}`"))
        }

        fn line_of(needle: &str) -> u32 {
            let index = SOURCE.lines().position(|line| line.contains(needle)).expect("the preview is written");
            index as u32 + 1
        }

        fn mounted(entry: &PreviewEntry, ctx: &PreviewCtx) -> ComponentList {
            reset_layout_runtime();
            install_default_text_metrics();
            let root = entry.build_root(ctx).expect("the preview builds");
            compute_layout(
                root.layout_node(),
                AvailableSpace::Definite(320.0),
                AvailableSpace::Definite(120.0),
            )
            .expect("the preview lays out");
            ComponentList::new(root)
        }

        const RSX_IDS: [&str; 3] = [
            "telar_rsx_fixture--greeting_card--ada",
            "telar_rsx_fixture--tally--rsx-literal",
            "telar_rsx_fixture--tally--rsx-bound",
        ];

        #[test]
        fn each_preview_is_listed_under_its_id() {
            let ids: Vec<&str> = telar_all_previews().iter().map(|entry| entry.id).collect();
            assert_eq!(ids[..3], RSX_IDS);
            assert_eq!(ids[3..], ["telar_rsx_fixture--tally--default", "telar_rsx_fixture--tally--counting"]);
        }

        #[test]
        fn the_crates_own_list_shadows_the_rsx_only_one() {
            let rsx: Vec<&str> = telar_rsx_fixture::__telar_previews::telar_all_previews().iter().map(|entry| entry.id).collect();
            assert_eq!(rsx, RSX_IDS);
            assert_eq!(telar_all_previews().len(), 5);
        }

        #[test]
        fn a_preview_names_its_component_and_where_it_is_written() {
            let entry = entry("telar_rsx_fixture--tally--default");
            assert_eq!(entry.component, "tally");
            assert_eq!(entry.name, "Default");
            assert_eq!(entry.title, "Fixture/Tally");
            assert_eq!(entry.file, concat!(env!("CARGO_MANIFEST_DIR"), "/src/previews.rs"));
            assert!(std::path::Path::new(entry.file).is_absolute(), "{}", entry.file);
            assert_eq!(entry.line, line_of("preview!(tally: TallyProps, \"Default\""));
        }

        #[test]
        fn a_preview_carries_its_body_as_written() {
            let source = entry("telar_rsx_fixture--tally--default").source;
            assert!(source.starts_with("{\n    tally(\n"), "{source}");
            assert!(source.contains(".label(p.arg(\"label\", \"Apples\"))"), "{source}");
            assert!(source.ends_with("\n}"), "{source}");
        }

        #[test]
        fn a_preview_describes_its_component_by_its_props() {
            let schema = (entry("telar_rsx_fixture--tally--default").props.expect("named by the macro"))();
            assert_eq!(schema.name, "TallyProps");
            assert_eq!(schema.doc, "A label beside how many of it there are.");
            let fields: Vec<&str> = schema.fields.iter().map(|field| field.name).collect();
            assert_eq!(fields, ["label", "count"]);
        }

        #[test]
        fn the_builder_settings_reach_the_entry() {
            let default = entry("telar_rsx_fixture--tally--default");
            assert_eq!(default.layout, Layout::Centered);
            assert_eq!(default.matrix, Some(Matrix::Named("themes")));
            assert!(default.decorator.is_none());

            let counting = entry("telar_rsx_fixture--tally--counting");
            assert_eq!(counting.tags, ["stateful"]);
            assert!(counting.decorator.is_some());
            assert!(counting.play.is_some());
        }

        #[test]
        fn a_preview_builds_headlessly_with_the_args_it_reads() {
            let entry = entry("telar_rsx_fixture--tally--default");
            let ctx = PreviewCtx::from(Args::for_entry(&entry));
            assert_eq!(testing::texts(&mounted(&entry, &ctx)), ["Apples · 3"]);
            let args: Vec<(&str, ArgBinding)> =
                ctx.args().states().iter().map(|state| (state.name, state.binding)).collect();
            assert_eq!(args, [("label", ArgBinding::Remount), ("count", ArgBinding::Remount)]);
        }

        #[test]
        fn a_decorated_preview_builds_inside_its_decorator() {
            let entry = entry("telar_rsx_fixture--tally--counting");
            let ctx = PreviewCtx::from(Args::for_entry(&entry));
            let tree = mounted(&entry, &ctx);
            assert_eq!(testing::texts(&tree), ["Presses · 0"]);
            let framed = tree.commands().iter().any(|command| {
                matches!(command, DrawCommand::PushMatrix { matrix } if matrix[4..] == [12.0, 12.0])
            });
            assert!(framed, "the text sits inside the frame's padding");
            assert_eq!(ctx.args().states()[0].binding, ArgBinding::Live);
        }

        /// A `.rsx` preview reads the literals of its root through its args, so each gets a control, and a change builds it again.
        #[test]
        fn an_rsx_preview_reads_its_roots_literals_through_its_args() {
            let entry = entry("telar_rsx_fixture--tally--rsx-literal");
            assert_eq!(entry.component, "tally");
            assert_eq!(entry.title, "Fixture/Tally");
            assert_eq!(entry.layout, Layout::Centered);
            assert_eq!(entry.docs, "The tally written in Rust, previewed from `.rsx`.");
            assert_eq!(entry.tags, ["rsx"]);
            assert_eq!(entry.file, concat!(env!("CARGO_MANIFEST_DIR"), "/src/tally.previews.rsx"));
            assert_eq!(entry.line, 4);
            assert!(entry.source.starts_with("[preview \"Rsx literal\""), "{}", entry.source);
            assert_eq!(
                entry.matrix,
                Some(Matrix::Axes(&[
                    Axis::Dir(&[telar::Direction::Ltr, telar::Direction::Rtl]),
                    Axis::Arg("count", &["1", "2"]),
                ]))
            );
            assert_eq!((entry.props.expect("the previewed component's props"))().name, "TallyProps");

            let ctx = PreviewCtx::from(Args::for_entry(&entry));
            assert_eq!(testing::texts(&mounted(&entry, &ctx)), ["Pears · 2"]);
            let rows: Vec<(&str, ArgBinding, ControlKind, Option<ArgValue>)> = ctx
                .args()
                .states()
                .into_iter()
                .map(|state| (state.name, state.binding, state.control, state.default))
                .collect();
            assert_eq!(
                rows,
                [
                    ("label", ArgBinding::Remount, ControlKind::TEXT, Some(ArgValue::Text("Pears".into()))),
                    ("count", ArgBinding::Remount, ControlKind::INTEGER, Some(ArgValue::Int(2))),
                ]
            );

            let before = ctx.args().remounts();
            ctx.args().set("count", ArgValue::Int(5)).unwrap();
            assert_eq!(ctx.args().remounts(), before + 1, "a literal's change builds the preview again");
            assert_eq!(testing::texts(&mounted(&entry, &ctx)), ["Pears · 5"]);
        }

        /// `args(…)` declares live args: each has its control before the preview is first built, and the tree reads the signal it shares with it.
        #[test]
        fn an_rsx_preview_with_args_gets_live_controls() {
            let entry = entry("telar_rsx_fixture--tally--rsx-bound");
            assert_eq!(entry.layout, Layout::Padded, "a variant's own option wins over the meta section's");
            let ctx = PreviewCtx::from(Args::for_entry(&entry));
            let declared = ctx.args().states();
            assert_eq!(declared[0].name, "count");
            assert_eq!(declared[0].binding, ArgBinding::Live);
            assert_eq!(declared[0].default, Some(ArgValue::Int(3)));

            assert_eq!(testing::texts(&mounted(&entry, &ctx)), ["Plums · 3"]);
            let count = ctx.args().states().into_iter().find(|state| state.name == "count").unwrap();
            assert_eq!((count.binding, count.control), (ArgBinding::Live, ControlKind::INTEGER));

            let before = ctx.args().remounts();
            ctx.args().set("count", ArgValue::Int(7)).unwrap();
            assert_eq!(ctx.args().get("count"), Some(ArgValue::Int(7)));
            assert_eq!(ctx.args().remounts(), before, "a live arg reaches the tree without building it again");
        }

        #[test]
        fn a_component_files_preview_is_titled_by_its_meta_section() {
            let entry = entry("telar_rsx_fixture--greeting_card--ada");
            assert_eq!(entry.title, "Fixture/Greeting card");
            assert_eq!(entry.docs, "A greeting in the fixture's own catalog, beside a baked mark and a baked icon.");
            assert_eq!(entry.layout, Layout::Centered);
            assert_eq!((entry.props.expect("the component's props"))().name, "GreetingCardProps");
            let names: Vec<&str> = entry.args.iter().map(|arg| arg.name).collect();
            assert_eq!(names, ["name"]);
        }
    }
}
