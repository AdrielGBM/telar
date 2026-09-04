//! RSX transpiler: converts a parsed [`RsxDocument`](telar_parser::RsxDocument) AST into compilable Rust source code that depends on `telar::*`.

#![warn(rustdoc::broken_intra_doc_links)]

mod codegen;
mod discovery;
mod edges;
mod error;
mod gradient;
mod i18n;
pub mod naming;
mod paths;
mod registry;
mod rust;
mod signal_scan;
mod source_map;
mod style;
mod transition;
mod view;

pub use codegen::{TranspiledSource, transpile_source};
pub use discovery::{
    assets_root, auto_modules_enabled, collect_files_by_ext, component_name, discover_rust_modules,
    find_rsx_files, find_rsx_files_in_tree, prune_stale_generated, relative_output_path,
};
pub use error::TranspileError;
pub use i18n::{
    CatalogModel, I18N_CATALOG_PATH, I18N_MODULE, MessageModel, PartModel, catalog_files,
    parse_catalog, parse_message, to_source as bake_catalog_to_source,
};
pub use paths::{find_ancestor_dir, find_telar_root, find_workspace_root};
pub use registry::{
    builtin_tags, color_attr_keys, color_keywords, is_builtin_tag, is_control_flow_keyword,
    keyword_color_rgba, layout_attr_keys, tag_attr_keys,
};
pub use signal_scan::{SignalInfo, scan_effects, scan_locals, scan_signals};
pub use source_map::{ExprSpan, RsxSpan, SourceMap, nth_line};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    /// A class on a component call used to compile to nothing.
    ///
    /// `[style]` is the DSL's vocabulary for paint, and until now it stopped at the component boundary: a `box @squared` came out square and a `menu @squared` came out exactly as the catalogue drew it, with no diagnostic to say the class had been read and thrown away. The only remaining lever was the theme's radius, which moves every rounded thing in the application — so an app wanting one square trigger either restyled everything or edited the catalogue.
    ///
    /// The class now lands on the callee's principal surface, and only the properties it names: what it does not mention, the component still decides for itself.
    #[test]
    fn a_class_on_a_component_call_reaches_the_surface_it_paints() {
        let out = transpile_source(
            "[style]\n@squared\n    radius: 0\n\n[view]\nmenu @squared label:\"File\" items:items\n",
            "demo",
            None,
            None)
        .unwrap();
        assert!(
            out.rust_code
                .contains(".with_radius(BorderRadius::all(0.0))"),
            "the class's radius reaches the menu's trigger:\n{}",
            out.rust_code
        );
        assert!(
            !out.rust_code.contains("RectStyle {"),
            "as an amendment, not a wholesale style that would cost the trigger its border:\n{}",
            out.rust_code
        );

        let plain = transpile_source(
            "[style]\n@squared\n    radius: 0\n\n[view]\nspinner @squared\n",
            "demo",
            None,
            None,
        )
        .unwrap();
        assert!(
            plain.rust_code.contains(".style("),
            "the class reaches the callee, which is what lets rustc say it has no surface:\n{}",
            plain.rust_code
        );
    }

    /// `keep:` is what turns a viewport into one whose position survives its tree being rebuilt; without it the emission must not change at all, since every scroll that never asked to be kept is one whose position belongs to the tree it was built with.
    #[test]
    fn a_scroll_keeps_its_position_only_when_it_is_asked_to() {
        let plain =
            transpile_source("[view]\nscroll\n    text \"x\"\n", "demo", None, None).unwrap();
        assert!(
            plain.rust_code.contains("LayoutScrollArea::new(")
                && !plain.rust_code.contains("new_kept"),
            "unkeyed scrolls compile exactly as before:\n{}",
            plain.rust_code
        );

        let kept = transpile_source(
            "[view]\nscroll keep:\"panel.body\"\n    text \"x\"\n",
            "demo",
            None,
            None,
        )
        .unwrap();
        assert!(
            kept.rust_code
                .contains("LayoutScrollArea::new_kept(\"panel.body\""),
            "a keyed scroll is built through the surface's store:\n{}",
            kept.rust_code
        );
    }

    /// A bare name is the author's own binding, whatever a theme happens to call a field of its own — a theme used to make every bare lowercase name a candidate token, so a `let` three lines above was unreachable from the view and a binding named after a real token read the theme instead of itself.
    #[test]
    fn a_bare_name_is_the_binding_not_a_theme_token() {
        let logic = "let muted = telar::Color::WHITE;\nlet size = 16.0;\n";
        let out = transpile_source(
            &format!(
                "[logic]\n{logic}\n[view]\nbox fill:muted\n    spinner color:$theme.accent size:size\n"
            ),
            "demo",
            Some("crate::Theme"),
            None)
        .unwrap();
        assert!(
            out.rust_code.contains("with_fill(muted)"),
            "a bound colour is itself, not a token lookup:\n{}",
            out.rust_code
        );
        assert!(
            out.rust_code.contains(".size(size)"),
            "a bound number reaches a prop that is not a colour at all:\n{}",
            out.rust_code
        );
        assert!(
            out.rust_code.contains("theme.get().accent"),
            "a theme read is a read wherever it is written:\n{}",
            out.rust_code
        );
    }

    /// Only the zone's own bindings are in scope where the view is emitted: a `let` inside a nested `fn` body is not, and claiming it would shadow a token the author does mean.
    #[test]
    fn a_binding_nested_inside_a_fn_is_not_in_view_scope() {
        // `props` is always bound first, whether or not `[logic]` declares anything.
        let locals = scan_locals("let outer = 1.0;\nfn helper() {\n    let inner = 2.0;\n}\n");
        assert_eq!(locals, vec!["props".to_string(), "outer".to_string()]);
        assert_eq!(
            scan_locals("let (a, b) = pair();\nlet mut c: f32 = 0.0;\n"),
            vec![
                "props".to_string(),
                "a".to_string(),
                "b".to_string(),
                "c".to_string()
            ],
            "a destructuring pattern contributes every name it binds, and a type annotation none"
        );
    }

    /// An unrecognised first word in the view is a component call, so a `//` note used to compile into a call to a component named `//`, with the words after it read as its attributes. It builds nothing now, and it is carried into the generated file — which is what a diagnostic points at.
    #[test]
    fn a_note_in_the_view_builds_nothing_and_is_carried_through() {
        let src = "[view]\ncol\n    // why this box is here\n    text \"hi\"\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("// why this box is here"),
            "the note reaches the generated file:\n{code}"
        );
        assert!(
            !code.contains("Props::props()"),
            "the note is not a component call:\n{code}"
        );
        assert_eq!(
            code.matches("Text::declaring").count(),
            1,
            "and it adds no widget of its own:\n{code}"
        );
    }

    #[test]
    fn i18n_markup_text_emits_catalog_lookup() {
        let out = transpile_source("[view]\ntext t\"nav.title\"\n", "demo", None, None).unwrap();
        assert!(
            out.rust_code.contains("telar::i18n::translate"),
            "expected a catalog lookup:\n{}",
            out.rust_code
        );
        assert!(
            out.rust_code.contains("crate::__rsx_i18n::CATALOG"),
            "{}",
            out.rust_code
        );
        assert!(out.rust_code.contains("\"nav.title\""), "{}", out.rust_code);
        let plain = transpile_source("[view]\ntext \"Hi\"\n", "demo", None, None).unwrap();
        assert!(
            !plain.rust_code.contains("i18n::translate"),
            "{}",
            plain.rust_code
        );
    }

    /// A catalogue lookup in a value position is the `t!` macro, spliced like any other Rust — the macro validates its own key and reads the locale signal, so nothing here has to know it is a lookup at all.
    #[test]
    fn a_lookup_in_a_value_is_the_macro_spliced_verbatim() {
        let out = transpile_source(
            "[view]\nbutton label:t!(\"btn.save\")\n",
            "demo",
            None,
            None,
        )
        .unwrap();
        assert!(
            out.rust_code.contains(".label(t!(\"btn.save\"))"),
            "{}",
            out.rust_code
        );
        let plain =
            transpile_source("[view]\nbutton label:\"Save\"\n", "demo", None, None).unwrap();
        assert!(
            plain.rust_code.contains(".label(\"Save\")"),
            "{}",
            plain.rust_code
        );
    }

    #[test]
    fn source_map_points_generated_logic_back_to_rsx() {
        let src = "[logic]\n#[derive(Props)]\npub struct Props {\n    pub body: &'static st,\n}\nlet count = signal(0i32);\n\n[view]\ncol\n";
        let result = transpile_source(src, "demo", None, None).unwrap();
        let lines: Vec<&str> = result.rust_code.lines().collect();
        assert_eq!(lines.len(), result.source_map.len());

        let body_idx = lines
            .iter()
            .position(|l| l.contains("&'static st"))
            .expect("generated struct field");
        assert_eq!(result.source_map[body_idx], Some(3));

        let let_idx = lines
            .iter()
            .position(|l| l.contains("signal"))
            .expect("generated logic line");
        assert_eq!(result.source_map[let_idx], Some(5));

        let use_idx = lines
            .iter()
            .position(|l| l.contains("use telar::*"))
            .unwrap();
        assert_eq!(result.source_map[use_idx], None);
    }

    #[test]
    fn source_map_points_generated_view_back_to_rsx() {
        let src =
            "[view]\ncol\n    text \"hi\"\n    row\n        button on_press:(|| missing.set(1))\n";
        let result = transpile_source(src, "demo", None, None).unwrap();
        let lines: Vec<&str> = result.rust_code.lines().collect();
        assert_eq!(lines.len(), result.source_map.len());

        assert!(!result.rust_code.contains("@RSX@"));

        let btn_idx = lines
            .iter()
            .position(|l| l.contains("missing.set(1)"))
            .expect("generated button closure");
        assert_eq!(result.source_map[btn_idx], Some(4));

        let text_idx = lines
            .iter()
            .position(|l| l.contains("\"hi\""))
            .expect("generated text leaf");
        assert_eq!(result.source_map[text_idx], Some(2));

        let row_ctor = lines
            .iter()
            .position(|l| l.contains("flex_row()"))
            .expect("generated row container");
        assert_eq!(result.source_map[row_ctor], Some(3));
    }

    const COUNTER: &str = r#"[logic]
let count = signal(0i32);

[style]
@card
    width: 240
    padding: 20
    gap: 12
    axis: col
    align: center

[view]
col @card
    text "Count: {$count}" font_size:14 color:$theme.dark
    button label:"Increment" fill:$theme.primary on_press:(|| $count.update(|n| *n += 1))
"#;

    #[test]
    fn relative_output_path_mirrors_tree_and_rejects_out_of_src() {
        let src = Path::new("/proj/src");
        assert_eq!(
            relative_output_path(Path::new("/proj/src/sections/cards.rsx"), src),
            Some(PathBuf::from("sections/cards.rs"))
        );
        assert_eq!(
            relative_output_path(Path::new("/proj/src/counter.rsx"), src),
            Some(PathBuf::from("counter.rs"))
        );
        assert_eq!(
            relative_output_path(Path::new("/proj/examples/foo.rsx"), src),
            None
        );
        assert_eq!(relative_output_path(src, src), None);
    }

    /// A prop that carries both `#[props(into)]` and an inline `= expr` keeps them both. The derive reads one `#[props]` per field, so dropping the default made the prop required and every call site that left it off stopped building — with an error naming the prop, but never the declaration that lost it.
    #[test]
    fn an_inline_default_survives_the_attribute_beside_it() {
        let src = "[logic]\npub struct Props {\n    #[props(into)]\n    pub tint: Reactive<Color> = Reactive::of(|| Color::WHITE),\n}\n\n[view]\ncol\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains("#[props(default = Reactive::of(|| Color::WHITE), into)]"),
            "{code}"
        );
    }

    #[test]
    fn generates_counter() {
        let out = transpile_source(COUNTER, "counter", Some("SandboxTheme"), None).unwrap();
        let code = out.rust_code;
        assert!(code.contains("pub fn counter(props: CounterProps, children: Children)"));
        assert!(code.contains("fn style_card() -> LayoutStyle"));
        assert!(code.contains("move || format!(\"Count: {}\", { count.get() })"));
        assert!(code.contains("button(ButtonProps::props()"));
        assert!(code.contains(".label(\"Increment\")"));
        assert!(
            code.contains(
                ".fill(Reactive::of({ let theme = theme.clone(); move || theme.get().primary }))"
            ),
            "{code}"
        );
        assert!(code.contains("count.update(|n| *n += 1)"));
        assert!(code.contains("Container::new(style_card(), children!["));
        assert!(code.contains("Ok(Box::new(__col_0))"));
    }

    // Regression: a type-annotated `let count: RwSignal<i32>` was missed by the old `let count =` prefix match, so no clone was emitted and the closure captured `count` by move.
    #[test]
    fn move_clone_emitted_for_type_annotated_signal() {
        let src = "[logic]\nlet count: RwSignal<i32> = signal(0i32);\nlet doubled = memo(move || count.get() * 2);\n[view]\ntext \"hi\"\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        let code = out.rust_code;
        assert!(code.contains("let count_rsx_mv = count.clone();"));
        assert!(code.contains("let doubled = memo(move || count_rsx_mv.get() * 2);"));
    }

    // Regression: inside an unclosed call argument the clone must be block-wrapped, not emitted as a preceding `let` — that would land in the argument list and not parse.
    #[test]
    fn move_clone_in_call_arg_closure_is_block_wrapped() {
        let src = "[logic]\nlet s = signal(0i32);\nsetup(\n    move || s.set(1),\n);\n[view]\ntext \"x\"\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains("{ let s_rsx_mv = s.clone(); move || s_rsx_mv.set(1) }"),
            "a call-arg closure's clone must be block-wrapped, not a preceding let:\n{code}"
        );
        assert!(
            !code.contains("\n    let s_rsx_mv = s.clone();\n    move"),
            "the clone must not be emitted as a preceding statement inside the call args:\n{code}"
        );
    }

    // Regression: a signal name appearing only inside a string literal must not trip the clone pass, and the literal must survive byte-for-byte (it was corrupted into `"battery-charging_rsx_mv"`).
    #[test]
    fn logic_clone_pass_ignores_signal_names_inside_string_literals() {
        let src = "[logic]\nlet charging = signal(false);\nlet view = charging.read_only();\nlet glyph = memo(move || if view.get() { \"battery-charging\" } else { \"battery\" });\n[view]\ntext \"{$glyph}\" size:12\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains("\"battery-charging\""),
            "the string literal must survive intact:\n{code}"
        );
        assert!(
            !code.contains("charging_rsx_mv"),
            "a name appearing only inside a string must not be cloned/rewritten:\n{code}"
        );
    }

    #[test]
    fn self_alignment_maps_to_align_self() {
        let src = "[view]\ncol\n    box self:center width:20 height:20\n    box self:stretch\n    text \"x\"\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains(".align_self_center()"),
            "self:center should emit align_self_center:\n{code}"
        );
        assert!(
            code.contains(".align_self_stretch()"),
            "self:stretch should still emit align_self_stretch:\n{code}"
        );
    }

    /// A preview of a component that reads ambient state needs that state seeded first, and the seeding is not the same for every preview of the same component — that is the whole point of naming one per block.
    #[test]
    fn a_preview_fixture_runs_before_the_component_is_built() {
        let src = "[view]\ntext \"x\"\n\n[preview \"Charging\" fixture:mock_battery]\ndemo\n\n[preview \"Plain\"]\ndemo\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        let charging = code.find("demo_preview_0").unwrap();
        let plain = code.find("demo_preview_1").unwrap();
        let first = &code[charging..plain];
        assert!(
            first.contains("mock_battery();"),
            "the named fixture runs first:\n{first}"
        );
        assert!(
            !code[plain..].contains("mock_battery();"),
            "and only for the preview that named it:\n{}",
            &code[plain..]
        );
    }

    /// `surface:WxH` turns a preview from a tree into a window: the entry carries the size the compositor would give it and, with `animate`, the enter transition its root plays. A size that does not parse is named where it was written rather than quietly falling back to a tree, which would answer a question the author never asked.
    #[test]
    fn a_preview_can_declare_the_surface_it_is() {
        let src = "[view]\ntext \"x\"\n\n[preview \"Float\" surface:360x240 animate]\ndemo\n\n[preview \"Tree\"]\ndemo\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains(
                "surface: Some(::telar::PreviewSurface { width: 360.0, height: 240.0, animate: true })"
            ),
            "the size and the transition reach the entry:\n{code}"
        );
        assert!(
            code.contains("build: demo_preview_1, surface: None"),
            "and a preview that declares none is still a tree:\n{code}"
        );

        let bad = "[view]\ntext \"x\"\n\n[preview \"Float\" surface:wide]\ndemo\n";
        let code = transpile_source(bad, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains("compile_error!(\"[preview] surface: expects WIDTHxHEIGHT"),
            "a size that does not parse names itself:\n{code}"
        );
    }

    #[test]
    fn section_and_heading_resolve_as_widget_components() {
        let src = "[view]\nsection title:\"Cards\"\n    heading text:\"Subtitle\"\n    text \"Body\" size:14 color:dark\n";
        let code = transpile_source(src, "cards", None, None)
            .unwrap()
            .rust_code;
        assert!(
            !code.contains("Section::new") && !code.contains("Heading::new"),
            "section/heading must not reference removed components in:\n{code}"
        );
        assert!(
            code.contains("section(SectionProps::props().title(\"Cards\").build()"),
            "expected section component call in:\n{code}"
        );
        assert!(
            code.contains(
                "heading(HeadingProps::props().text(\"Subtitle\").build(), Children::default())"
            ),
            "expected heading component call in:\n{code}"
        );
    }

    #[test]
    fn extracts_props_struct_to_file_scope() {
        let src = "[logic]\n#[derive(Props)]\npub struct Props { pub title: &'static str }\n[view]\ntext \"hi\"\n";
        let out = transpile_source(src, "card", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("pub struct CardProps"),
            "struct should be renamed CardProps"
        );
        assert!(
            code.contains("pub fn card(props: CardProps, children: Children)"),
            "fn signature must use CardProps"
        );
        let struct_pos = code.find("pub struct CardProps").unwrap();
        let fn_pos = code.find("pub fn card").unwrap();
        assert!(
            struct_pos < fn_pos,
            "Props struct must precede the function"
        );
        assert!(
            code.contains("#[derive(Props)]"),
            "derive attribute must be preserved"
        );
    }

    /// A props struct is emitted as a sibling of the component function, so everything it needs has to reach file scope with it: the imports its field types name, and the comment that says what it is. Both used to stay behind in the function body — the first as a compile error at the struct, the second as a doc comment describing whichever statement happened to follow it.
    #[test]
    fn a_props_struct_takes_its_imports_and_its_comment_with_it() {
        let src = "[logic]\nuse crate::model::Item;\n\n/// What the card shows.\npub struct Props {\n    pub item: Option<Item> = None,\n}\n\nlet item = props.item;\n[view]\ntext \"hi\"\n";
        let out = transpile_source(src, "card", None, None).unwrap();
        let code = &out.rust_code;
        let struct_pos = code.find("pub struct CardProps").unwrap();
        assert!(
            code.find("use crate::model::Item;").unwrap() < struct_pos,
            "the import the field type names must precede the struct:\n{code}"
        );
        assert!(
            code.find("/// What the card shows.").unwrap() < struct_pos,
            "the struct's comment travels with it:\n{code}"
        );
        assert!(
            !code.contains("    use crate::model::Item;"),
            "the import is moved, not duplicated into the body:\n{code}"
        );
    }

    /// A field's own comment is prose, and prose has commas in it. Splitting the struct body on every comma tore such a field away from its type and dropped it silently — the first sign was a missing field at a call site, nowhere near the comment that caused it.
    #[test]
    fn a_comma_inside_a_comment_or_string_does_not_split_a_props_field() {
        let src = "[logic]\npub struct Props {\n    // One meter, never two, because a card is a glance.\n    pub meter: Option<f32> = None,\n    pub label: String = \"a, b\".to_string(),\n    pub tail: bool = false,\n}\n[view]\ntext \"hi\"\n";
        let out = transpile_source(src, "card", None, None).unwrap();
        let code = &out.rust_code;
        for field in ["meter", "label", "tail"] {
            assert!(
                code.contains(&format!("pub {field}:")),
                "field `{field}` must survive the split:\n{code}"
            );
        }
    }

    #[test]
    fn expr_spans_map_interpolation_verbatim() {
        let src = "[logic]\nlet count = signal(0i32);\n[view]\ntext \"Count: {count}\"\n";
        let out = transpile_source(src, "demo", None, None).unwrap();

        assert!(!out.rust_code.contains("@RSX@"), "markers must be stripped");

        assert_eq!(out.expr_spans.len(), 1, "expected one expression span");
        let span = &out.expr_spans[0];

        let rsx_frag = &src[span.rsx_start as usize..(span.rsx_start + span.len) as usize];
        assert_eq!(rsx_frag, "count");

        let gen_frag =
            &out.rust_code[span.gen_start as usize..(span.gen_start + span.len) as usize];
        assert_eq!(gen_frag, "count");
        assert!(out.rust_code.is_char_boundary(span.gen_start as usize));
    }

    #[test]
    fn expr_spans_are_char_boundary_safe_with_multibyte() {
        let src = "[logic]\nlet count = signal(0i32);\n[view]\ntext \"caf\u{e9} {count}\"\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        assert_eq!(out.expr_spans.len(), 1);
        let span = &out.expr_spans[0];
        let (rs, re) = (
            span.rsx_start as usize,
            (span.rsx_start + span.len) as usize,
        );
        let (gs, ge) = (
            span.gen_start as usize,
            (span.gen_start + span.len) as usize,
        );
        assert!(src.is_char_boundary(rs) && src.is_char_boundary(re));
        assert!(out.rust_code.is_char_boundary(gs) && out.rust_code.is_char_boundary(ge));
        assert_eq!(&src[rs..re], "count");
        assert_eq!(&out.rust_code[gs..ge], "count");
    }

    #[test]
    fn expr_spans_cover_if_and_let() {
        let src = "[logic]\n[view]\ncol\n    let n = 1\n    if n > 0\n        text \"hi\"\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        assert!(!out.rust_code.contains("@RSX@"));

        for span in &out.expr_spans {
            let rsx_frag = &src[span.rsx_start as usize..(span.rsx_start + span.len) as usize];
            let gen_frag =
                &out.rust_code[span.gen_start as usize..(span.gen_start + span.len) as usize];
            assert_eq!(rsx_frag, gen_frag, "span fragment must be verbatim");
        }
        let frags: Vec<&str> = out
            .expr_spans
            .iter()
            .map(|s| &src[s.rsx_start as usize..(s.rsx_start + s.len) as usize])
            .collect();
        assert!(frags.contains(&"let n = 1"), "let span missing: {frags:?}");
        assert!(
            frags.contains(&"n > 0"),
            "if-condition span missing: {frags:?}"
        );
    }

    #[test]
    fn component_with_quoted_string_attr() {
        let src = "[logic]\n[view]\nmy_widget label:\"hello\" size:16\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("my_widget(MyWidgetProps::props()"),
            "should call component fn with Props"
        );
        assert!(
            code.contains(".label(\"hello\")"),
            "an unknown component's quoted attr stays a string literal"
        );
        assert!(code.contains(".size(16.0)"), "numeric attr must become f32");
    }

    #[test]
    fn preview_section_generates_build_fn_and_entry() {
        let src = "[logic]\n[view]\ncol\n    text \"x\"\n\n[preview \"Default\"]\ncounter\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("pub fn demo_preview_0() -> Result<Box<dyn LayoutItem>, LayoutError>"),
            "missing preview build fn:\n{code}"
        );
        assert!(
            code.contains("counter(CounterProps::props().build(), Children::default())?"),
            "preview body should call the component:\n{code}"
        );
        assert!(
            code.contains("build: demo_preview_0"),
            "entry should point at the preview fn:\n{code}"
        );
        assert!(code.contains("preview_name: \"Default\""));
        assert_eq!(out.preview_names, vec!["Default".to_string()]);
    }

    #[test]
    fn dollar_marks_reactive_reads_and_clones_closure_captures() {
        let src = "[logic]\nlet count = signal(0i32);\n[view]\ncol\n    text \"{$count}\"\n    button on_press:(|| $count.update(|n| *n += 1))\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("count.get()"),
            "interpolation read missing:\n{code}"
        );
        assert!(
            code.contains("count.update(|n| *n += 1)"),
            "closure handle missing:\n{code}"
        );
        assert_eq!(
            code.matches("let count = count.clone();").count(),
            2,
            "expected one clone per capturing closure:\n{code}"
        );
        assert!(
            !code.contains('$'),
            "the `$` marker must not reach output:\n{code}"
        );
    }

    #[test]
    fn img_src_value_carries_an_expr_span() {
        let src = "[logic]\nlet hero = 1i32;\n[view]\ncol\n    img src:hero width:100\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        let spans: Vec<&str> = out
            .expr_spans
            .iter()
            .map(|s| &src[s.rsx_start as usize..(s.rsx_start + s.len) as usize])
            .collect();
        assert!(
            spans.contains(&"hero"),
            "img src value should map back to `hero`; got spans {spans:?}"
        );
        let span = out
            .expr_spans
            .iter()
            .find(|s| &src[s.rsx_start as usize..(s.rsx_start + s.len) as usize] == "hero")
            .unwrap();
        let gs = span.gen_start as usize;
        assert_eq!(&out.rust_code[gs..gs + span.len as usize], "hero");
    }

    #[test]
    fn svg_generates_src_color_and_layout() {
        let src = "[view]\ncol\n    svg src:props.icon color:Color::hex(\"#ff5722\") width:24 height:24\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(code.contains("Svg::new("), "missing Svg::new:\n{code}");
        assert!(
            code.contains("let __src = props.icon.clone();"),
            "missing src hoist:\n{code}"
        );
        assert!(
            code.contains("move || __src.clone(),"),
            "missing src closure:\n{code}"
        );
        assert!(
            code.contains("move || Some(Color::hex(\"#ff5722\")),"),
            "missing colour closure:\n{code}"
        );
        assert!(
            code.contains(".width(24.0)") && code.contains(".height(24.0)"),
            "missing layout dims:\n{code}"
        );
    }

    #[test]
    fn svg_color_signal_reads_reactively_and_clones_into_the_closure() {
        let src = "[logic]\nlet accent = signal(Color::WHITE);\n[view]\ncol\n    svg src:props.icon color:$accent width:24 height:24\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            !code.contains("compile_error!"),
            "a signal colour must not error:\n{code}"
        );
        assert!(
            code.contains("{ let accent = accent.clone(); move || Some(accent.get()) }"),
            "the colour should reactively read the cloned signal:\n{code}"
        );
    }

    /// `VirtualList` was built, exported, and referenced by nothing in the toolchain, so every `.rsx` list built a widget per row up front and long lists were capped instead. It is opt-in rather than inferred: it needs a fixed row height and an enclosing viewport, and neither is safe to assume of a loop.
    #[test]
    fn a_virtual_loop_builds_only_the_rows_its_scroll_shows() {
        let src = "[logic]\nlet rows = signal(Vec::<Row>::new());\n[view]\nscroll height:400\n    for row in $rows key row.id virtual row_height:32\n        text \"{row.name}\"\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("VirtualList::new("),
            "the loop builds a virtual list:\n{code}"
        );
        assert!(
            code.contains("LayoutScrollArea::new_with(") && code.contains("__viewport"),
            "and the scroll hands its viewport over for it:\n{code}"
        );
        assert!(
            code.contains("move |__index: usize, row|"),
            "with the row's index alongside the item:\n{code}"
        );
    }

    /// A scroll with no virtual loop under it keeps the cheaper constructor: the viewport is handed over only where something asked for it.
    #[test]
    fn an_ordinary_scroll_does_not_expose_a_viewport() {
        let src = "[logic]\nlet rows = signal(Vec::<Row>::new());\n[view]\nscroll height:400\n    for row in $rows\n        text \"{row.name}\"\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        assert!(
            !out.rust_code.contains("__viewport"),
            "no viewport is bound:\n{}",
            out.rust_code
        );
    }

    /// Outside a scroll there is no viewport to ask what is visible, so this is refused where it is written rather than quietly building every row.
    #[test]
    fn a_virtual_loop_outside_a_scroll_is_refused() {
        let src = "[logic]\nlet rows = signal(Vec::<Row>::new());\n[view]\ncol\n    for row in $rows virtual row_height:32\n        text \"x\"\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        assert!(
            out.rust_code.contains("needs an enclosing `scroll`"),
            "the requirement is named:\n{}",
            out.rust_code
        );
    }

    /// A reactive branch with several children was wrapped in a hard-coded `flex_column`, so a tab strip written as `row > if $ws > icon icon icon` stacked its icons and overflowed the 30px row it lived in. The cell now runs the way its host does.
    #[test]
    fn a_reactive_branch_cell_inherits_its_host_axis() {
        let body = "\n    if $open\n        text \"a\"\n        text \"b\"\n";
        let in_row = transpile_source(&format!("[view]\nrow{body}"), "demo", None, None).unwrap();
        assert!(
            in_row.rust_code.contains("LayoutStyle::new().flex_row()"),
            "a branch inside a row lays its children out horizontally:\n{}",
            in_row.rust_code
        );

        let in_col = transpile_source(&format!("[view]\ncol{body}"), "demo", None, None).unwrap();
        assert!(
            in_col
                .rust_code
                .contains("LayoutStyle::new().flex_column()"),
            "and stacks them inside a column, as before:\n{}",
            in_col.rust_code
        );
    }

    /// A layout prop reading a signal makes the whole style reactive: the node keeps an effect that re-resolves it, because a `LayoutStyle` is a value handed to the tree once — unlike paint, which is a closure the renderer re-runs. Without this a dragged panel width changed the signal and nothing moved.
    #[test]
    fn a_signal_sized_container_follows_its_signal() {
        let out = transpile_source(
            "[view]\ncol width:$panel_w\n    text \"body\"\n",
            "demo",
            None,
            None,
        )
        .unwrap();
        assert!(
            out.rust_code.contains(".styled_by("),
            "the container keeps a style effect:\n{}",
            out.rust_code
        );
        assert!(
            out.rust_code.contains("panel_w.get()"),
            "and the effect reads the signal:\n{}",
            out.rust_code
        );

        let constant = transpile_source(
            "[view]\ncol width:300\n    text \"body\"\n",
            "demo",
            None,
            None,
        )
        .unwrap();
        assert!(
            !constant.rust_code.contains(".styled_by("),
            "a constant size costs no effect:\n{}",
            constant.rust_code
        );
    }

    /// An attribute a tag does not accept used to map to a builder call that did nothing, or to nothing at all — `cols:` on a plain `box` compiled and had no effect. The table it checks is the one the analyzer completes from, so what the editor never suggests is what the build now refuses.
    #[test]
    fn an_attribute_a_tag_does_not_take_is_named() {
        let out =
            transpile_source("[view]\nbox nonsense:4 width:10\n", "demo", None, None).unwrap();
        assert!(
            out.rust_code
                .contains("`nonsense` is not an attribute of `box`"),
            "the offending key and tag are both named:\n{}",
            out.rust_code
        );

        let component =
            transpile_source("[view]\nmy_widget anything:4\n", "demo", None, None).unwrap();
        assert!(
            !component.rust_code.contains("is not an attribute"),
            "a component tag is exempt:\n{}",
            component.rust_code
        );
    }

    /// The idiom `track_layout` is used for in Rust, reachable from the view: read where a node ended up so a sibling can be drawn from it. With a transform transition it is the whole of a sliding indicator, which is why the two landed together.
    ///
    /// **The copy is guarded**, and that is the difference between a rect somebody can build on and one nothing can afford to read: the layout runs on every frame something moves and a signal notifies on every write, so an unguarded mirror wakes each of its readers whether or not the rectangle moved.
    #[test]
    fn track_rect_mirrors_a_node_into_a_signal_and_keeps_the_effect() {
        let src = "[logic]\nlet active = signal(Rect::ZERO);\n[view]\ncol\n    box track_rect:$active width:20\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("track_layout(__tracked.layout_node())"),
            "the node's own rect signal is the source:\n{code}"
        );
        assert!(
            code.contains("active.set(__now);"),
            "and it is mirrored into the author's signal:\n{code}"
        );
        assert!(
            code.contains("if active.peek() != __now"),
            "only when it moved, or every reader of it wakes on every frame:\n{code}"
        );
        assert!(
            code.contains("effect(move || {"),
            "as a bare effect, owned by the scope that built the widget rather than parked on it:\n{code}"
        );
    }

    /// `cursor:` names a variant or works one out, because a box's shape is as often picked as it is written: a strip that could run either way would otherwise have to be two components with one attribute between them.
    #[test]
    fn a_cursor_is_a_keyword_or_the_expression_that_answers_with_one() {
        let named =
            transpile_source("[view]\nbox cursor:col_resize\n", "demo", None, None).unwrap();
        assert!(
            named.rust_code.contains(".cursor(Cursor::ColResize)"),
            "a name in the table is the variant it names:\n{}",
            named.rust_code
        );
        let worked_out =
            transpile_source("[view]\nbox cursor:(along(axis))\n", "demo", None, None).unwrap();
        assert!(
            worked_out.rust_code.contains(".cursor(along(axis))"),
            "and anything else is the expression the author wrote, without its delimiters:\n{}",
            worked_out.rust_code
        );
    }

    /// And a shape that reads something follows it, like a colour: the box takes a `Reactive<Cursor>`, so what is written once is written once and what is read is read again.
    #[test]
    fn a_cursor_that_reads_a_signal_keeps_reading_it() {
        let out = transpile_source(
            "[logic]\nlet busy = signal(false);\n[view]\nbox cursor:(shape($busy))\n",
            "demo",
            None,
            None,
        )
        .unwrap();
        assert!(
            out.rust_code
                .contains("Reactive::of(move || shape(busy.get()))"),
            "the read is kept as one:\n{}",
            out.rust_code
        );
    }

    /// A number that is not a literal used to be dropped where it stood — no cursor call, no threshold, no diagnostic — which is the one failure a value grammar must not have.
    #[test]
    fn a_computed_number_reaches_the_property_it_was_written_on() {
        let out = transpile_source(
            "[logic]\nconst REACH: f32 = 18.0;\n[view]\nbox drag_threshold:(REACH * 0.5) line_height:(UNIT / 11.0)\n    text \"long\" lines:(rows + 1)\n",
            "demo",
            None,
            None,
        )
        .unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains(".drag_threshold(REACH * 0.5)"),
            "a threshold is an expression:\n{code}"
        );
        assert!(
            code.contains(".with_line_height(UNIT / 11.0)"),
            "so is a line height:\n{code}"
        );
        assert!(
            code.contains(".with_clamp(rows + 1,"),
            "and a count stays a count, not a float:\n{code}"
        );
    }

    /// The parens a `when:` needs to hold its spaces are the markup's delimiters, like everywhere else: left on, they warn `unused_parens` in generated code the author cannot edit.
    #[test]
    fn a_lazy_condition_leaves_its_delimiters_behind() {
        let out = transpile_source(
            "[logic]\nlet shown = signal(false);\n[view]\ncol\n    lazy when:(ready(&held, 2))\n        text \"there\"\n",
            "demo",
            None,
            None,
        )
        .unwrap();
        assert!(
            out.rust_code.contains("move || ready(&held, 2)"),
            "the condition is spliced without them:\n{}",
            out.rust_code
        );
    }

    /// `shown:` is a layout property and not a block, which is the whole point of it: the subtree it takes out of flow is still there, with its scroll where it was and its canvas measured — and it comes back because the style says so, where a `display_none` written once could never be undone.
    #[test]
    fn shown_is_a_layout_value_that_re_resolves() {
        let out = transpile_source(
            "[logic]\nlet open = signal(true);\n[view]\nbox shown:$open width:20\n",
            "demo",
            None,
            None,
        )
        .unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains(".shown(open.get())"),
            "the flag is read where it is written:\n{code}"
        );
        assert!(
            code.contains(".styled_by("),
            "and a value that reads something makes the whole style follow it:\n{code}"
        );
    }

    /// A field that opens holding the keyboard, says who holds it, and can be given up — the three things a field that stands in for something else needs, and the three that used to keep one in hand-written Rust. `focus_id:` in particular is not something `[logic]` could do: it runs before the widget exists.
    #[test]
    fn a_field_can_open_focused_say_so_and_be_given_up() {
        let out = transpile_source(
            "[logic]\nlet typed = signal(String::new());\nlet holds = signal(None);\n[view]\ninput value:$typed autofocus focus_id:$holds on_cancel:(|| typed.set(String::new()))\n",
            "demo",
            None,
            None,
        )
        .unwrap();
        let code = &out.rust_code;
        assert!(code.contains(".autofocus()"), "it opens focused:\n{code}");
        assert!(
            code.contains(".on_cancel(move || typed.set(String::new()))"),
            "escape is answerable:\n{code}"
        );
        assert!(
            code.contains("holds.set(Some(__field.focus_id()));"),
            "and it publishes the id it holds the keyboard under:\n{code}"
        );
        assert!(
            code.contains("on_cleanup(move || holds.set(None));"),
            "withdrawn when the field goes:\n{code}"
        );
    }

    /// A transform is read per frame from a closure the renderer already re-runs, so animating one costs a repaint and no relayout — which is what lets `transition(…)` reach past paint without breaking the invariant the whole design rests on. It is also the half of a sliding indicator that is not `track_rect`.
    #[test]
    fn a_transform_can_be_transitioned() {
        let src = "[logic]\nlet x = signal(0.0f32);\n[view]\nbox translate_x:$x transition(translate_x 200ms)\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("motion::Animated::new") && code.contains(".retarget("),
            "the value is retargeted through a persistent Animated:\n{code}"
        );
        assert!(
            code.contains("with_transform("),
            "and still lands on the transform closure:\n{code}"
        );
    }

    /// The layout box stays out: animating it would put a layout pass in every frame of every transition, which is a separate decision from this one.
    #[test]
    fn transitioning_the_layout_box_is_still_refused() {
        let src = "[view]\nbox width:40 transition(width 200ms)\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        assert!(
            out.rust_code.contains("compile_error!"),
            "an unsupported property is named, not ignored:\n{}",
            out.rust_code
        );
    }

    /// The shape `if` could never express, and the reason 14 of hyprshell's 16 `widget "…"` escapes are icons: three arms of different structure, a payload bound out of the matched variant, and a key that is the payload's own identity rather than the variant — so re-arriving at the same picture does not rebuild.
    #[test]
    fn a_reactive_match_extracts_a_payload_and_keys_on_it() {
        let src = "[logic]\nlet state = signal(AssetState::Loading);\n[view]\ncol\n    match $state as s key s.as_ready().map(|svg| svg.id())\n        AssetState::Ready(svg)\n            svg src:svg\n        AssetState::Failed\n            box width:16 height:16\n        _\n            text \"…\"\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("AssetState::Ready(svg) =>"),
            "the payload is bound by the arm's own pattern:\n{code}"
        );
        assert!(
            code.contains("s.as_ready().map(|svg| svg.id())"),
            "the key is the payload's identity, not the variant:\n{code}"
        );
        assert!(
            code.contains("state.get()"),
            "and the scrutinee is read reactively:\n{code}"
        );
    }

    /// Without a key the fallback must be hashable whatever the matched type is, so it reconciles on the variant — rebuilding when the shape changes and not when the payload does.
    #[test]
    fn a_keyless_reactive_match_reconciles_on_the_variant() {
        let src = "[logic]\nlet state = signal(Mode::A);\n[view]\ncol\n    match $state\n        Mode::A\n            text \"a\"\n        _\n            text \"b\"\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        assert!(
            out.rust_code.contains("::std::mem::discriminant"),
            "the variant is the default key:\n{}",
            out.rust_code
        );
    }

    /// A scrutinee with no `$` chooses its arm once, so it stays an ordinary Rust `match` — the same split `if` and `for` already make between a construction-time branch and a reconciled one.
    #[test]
    fn a_match_without_a_signal_stays_a_construction_time_branch() {
        let src = "[view]\ncol\n    match props.kind\n        Kind::One\n            text \"one\"\n        _\n            text \"other\"\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(code.contains("match props.kind {"), "plain match:\n{code}");
        assert!(
            !code.contains("discriminant") && !code.contains("ReactiveList::new"),
            "and nothing reactive is built for it:\n{code}"
        );
    }

    /// `Svg::with_stroke` is how a theme draws every icon at one weight without editing the assets, and it was reachable only from Rust — which is one of the two reasons a themed icon could not be a `.rsx` component.
    #[test]
    fn svg_stroke_overrides_the_documents_own_weight() {
        let literal = transpile_source(
            "[view]\ncol\n    svg src:props.icon stroke:1.5\n",
            "demo",
            None,
            None,
        )
        .unwrap();
        assert!(
            literal.rust_code.contains(".with_stroke("),
            "stroke reaches the builder:\n{}",
            literal.rust_code
        );

        let live = transpile_source(
            "[logic]\nlet weight = signal(2.0f32);\n[view]\ncol\n    svg src:props.icon stroke:$weight\n",
            "demo",
            None,
            None)
        .unwrap();
        assert!(
            live.rust_code.contains(".with_stroke(") && live.rust_code.contains("weight.get()"),
            "and a signal is read inside the closure:\n{}",
            live.rust_code
        );

        let none =
            transpile_source("[view]\ncol\n    svg src:props.icon\n", "demo", None, None).unwrap();
        assert!(
            !none.rust_code.contains(".with_stroke("),
            "an svg that asks for no stroke keeps the document's own:\n{}",
            none.rust_code
        );
    }

    #[test]
    fn svg_without_color_generates_none() {
        let src = "[view]\ncol\n    svg src:props.icon\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        assert!(
            out.rust_code.contains("|| None,"),
            "missing default colour closure:\n{}",
            out.rust_code
        );
    }

    #[test]
    fn lazy_defers_its_subtree_behind_a_when_condition() {
        let src = "[logic]\nlet show = signal(false);\nlet count = signal(0i32);\n[view]\ncol\n    lazy when:$show\n        text \"count {$count}\"\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(!code.contains("compile_error!"), "{code}");
        assert!(
            code.contains("Lazy::new("),
            "no Lazy widget emitted:\n{code}"
        );
        assert!(
            code.contains("{ let show = show.clone(); move || show.get() }"),
            "the condition must read the signal through its own clone:\n{code}"
        );
        assert!(
            code.contains("let count = count.clone();"),
            "the deferred subtree's signals must be cloned into the build closure:\n{code}"
        );
        assert!(
            code.contains("move || -> Result<Vec<Box<dyn LayoutItem>>, LayoutError>"),
            "missing deferred build closure:\n{code}"
        );
    }

    // Regression: a signal read inside a reactive branch was moved into the branch closure, breaking later reads.
    #[test]
    fn a_reactive_if_clones_the_signals_its_branches_read() {
        let src = "[logic]\nlet show = signal(true);\nlet count = signal(0i32);\n[view]\ncol\n    if $show\n        text \"in branch {$count}\"\n    text \"outside {$count}\"\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(!code.contains("compile_error!"), "{code}");
        assert!(
            code.contains("let count = count.clone();"),
            "the branch closure must clone the signal it reads, not move it:\n{code}"
        );
        assert!(
            code.contains("{ let show = show.clone(); move || vec![show.get()] }"),
            "the condition still clones separately:\n{code}"
        );
    }

    #[test]
    fn a_reactive_for_clones_the_signals_its_body_reads() {
        let src = "[logic]\nlet items = signal(vec![1i32, 2]);\nlet scale = signal(2i32);\n[view]\ncol\n    for n in $items\n        text \"{$scale}\"\n    text \"outside {$scale}\"\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(!code.contains("compile_error!"), "{code}");
        assert!(
            code.contains("let scale = scale.clone();"),
            "the item builder must clone the signal its body reads:\n{code}"
        );
    }

    // The loop variable is the closure parameter, so a prelude clone would name a binding that does not exist.
    #[test]
    fn a_reactive_for_never_clones_its_own_loop_variable() {
        let src = "[logic]\nlet items = signal(vec![1i32, 2]);\n[view]\ncol\n    for n in $items\n        text \"{n}\"\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        // Only the line above the builder matters; the clone inside it is the leaf emitter cloning the parameter.
        let lines: Vec<&str> = code.lines().collect();
        let builder = lines
            .iter()
            .position(|l| l.contains("move |n| ->"))
            .expect("item builder closure");
        assert!(
            !lines[builder - 1].contains("let n = n.clone();"),
            "the loop variable must not be cloned above its own closure:\n{code}"
        );
    }

    #[test]
    fn a_signal_free_reactive_branch_gets_no_clone_prelude() {
        let src = "[logic]\nlet show = signal(true);\n[view]\ncol\n    if $show\n        text \"static\"\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains("move |__cond: bool|"),
            "missing branch closure:\n{code}"
        );
        assert!(
            !code.contains("let show = show.clone();\n        move |__cond"),
            "a branch that reads nothing must not be wrapped:\n{code}"
        );
    }

    #[test]
    fn lazy_without_a_condition_is_a_compile_error() {
        let src = "[view]\ncol\n    lazy\n        text \"hi\"\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains("compile_error!(\"lazy: needs a `when:` condition"),
            "a `lazy` with nothing to defer until must not silently build eagerly:\n{code}"
        );
    }

    #[test]
    fn svg_missing_src_falls_back_to_undefined_placeholder() {
        let src = "[view]\ncol\n    svg width:24 height:24\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        assert!(
            out.rust_code.contains("__svg_data"),
            "missing placeholder identifier:\n{}",
            out.rust_code
        );
    }

    #[test]
    fn svg_src_value_carries_an_expr_span() {
        let src = "[logic]\nlet icon = 1i32;\n[view]\ncol\n    svg src:icon width:24\n";
        let out = transpile_source(src, "demo", None, None).unwrap();
        let spans: Vec<&str> = out
            .expr_spans
            .iter()
            .map(|s| &src[s.rsx_start as usize..(s.rsx_start + s.len) as usize])
            .collect();
        assert!(
            spans.contains(&"icon"),
            "svg src value should map back to `icon`; got spans {spans:?}"
        );
    }

    #[test]
    fn svg_color_token_resolves_through_theme() {
        let src = "[view]\ncol\n    svg src:props.icon color:$theme.accent width:18 height:18\n";
        let code = transpile_source(src, "demo", Some("NordTheme"), None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("move || Some(theme.get().accent) }"),
            "a tint re-reads the theme each frame:\n{code}"
        );
    }

    #[test]
    fn svg_src_signal_is_reactive_and_clones_the_handle() {
        let src = "[logic]\nlet glyph = signal(props.icon.clone());\n[view]\ncol\n    svg src:$glyph color:theme.accent width:18 height:18\n";
        let code = transpile_source(src, "demo", Some("NordTheme"), None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("{ let glyph = glyph.clone(); move || glyph.get() }"),
            "reactive src should clone + read the signal each view:\n{code}"
        );
        assert!(
            !code.contains("let __src = glyph"),
            "reactive src must not freeze the handle at construction:\n{code}"
        );
    }

    #[test]
    fn svg_src_constant_expr_is_still_captured_once() {
        let src = "[view]\ncol\n    svg src:icon(\"bell\") width:18 height:18\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains("let __src = icon(\"bell\").clone();")
                && code.contains("move || __src.clone(),"),
            "constant src should be captured once:\n{code}"
        );
    }

    #[test]
    fn transition_opacity_hoists_animated_and_wraps_reactive_read() {
        let src = "[logic]\nlet fade = signal(1.0f32);\n[view]\nbox opacity:$fade transition(opacity 200ms ease-out)\n    text \"hi\"\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains("let __transition_0 = motion::Animated::new(fade.get(), motion::tween(std::time::Duration::from_millis(200), motion::Easing::EaseOut));"),
            "missing hoisted Animated:\n{code}"
        );
        assert!(
            code.contains(".with_opacity({ let fade = fade.clone(); move || { __transition_0.retarget(fade.get()); __transition_0.get() } })"),
            "missing opacity retarget+get:\n{code}"
        );
    }

    #[test]
    fn transition_fill_with_theme_color_and_cubic_bezier() {
        let src = "[view]\nbox fill:$theme.primary transition(fill 150ms cubic-bezier(0.4,0,0.2,1))\n    text \"x\"\n";
        let code = transpile_source(src, "demo", Some("SandboxTheme"), None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("let __transition_0 = motion::Animated::new(theme.get().primary, motion::tween(std::time::Duration::from_millis(150), motion::Easing::CubicBezier(0.4, 0.0, 0.2, 1.0)));"),
            "missing cubic-bezier Animated:\n{code}"
        );
        assert!(
            code.contains(
                ".with_fill({ __transition_0.retarget(theme.get().primary); __transition_0.get() })"
            ),
            "missing fill retarget+get:\n{code}"
        );
    }

    #[test]
    fn transition_fill_spring() {
        let src = "[view]\nbox fill:#3d78fa transition(fill spring(170,26))\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains("motion::spring(170.0, 26.0)"),
            "missing spring curve:\n{code}"
        );
        assert!(
            code.contains("motion::Animated::new(Color::rgba(61.0 / 255.0, 120.0 / 255.0, 250.0 / 255.0, 255.0 / 255.0), motion::spring(170.0, 26.0))"),
            "spring Animated should seed from the fill color:\n{code}"
        );
    }

    #[test]
    fn transition_multiple_properties_comma_separated() {
        let src = "[logic]\nlet fade = signal(1.0f32);\n[view]\nbox fill:#3d78fa opacity:$fade transition(opacity 200ms, fill 150ms linear)\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains(
                "motion::tween(std::time::Duration::from_millis(150), motion::Easing::Linear)"
            ),
            "missing fill linear tween:\n{code}"
        );
        assert!(
            code.contains(
                "motion::tween(std::time::Duration::from_millis(200), motion::Easing::EaseOut)"
            ),
            "missing opacity default-easing tween:\n{code}"
        );
        assert!(
            code.contains("let __transition_0 =") && code.contains("let __transition_1 ="),
            "expected two hoisted animations:\n{code}"
        );
    }

    #[test]
    fn transition_unsupported_property_emits_compile_error() {
        let src = "[view]\nbox fill:#3d78fa transition(radius 200ms)\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains("compile_error!(\"transition: unsupported property `radius`"),
            "unsupported prop should emit a compile_error:\n{code}"
        );
    }

    #[test]
    fn transition_invalid_duration_emits_compile_error() {
        let src = "[view]\nbox opacity:0.5 transition(opacity 200)\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains("compile_error!(\"transition `opacity` has an invalid duration `200`"),
            "invalid duration should emit a compile_error:\n{code}"
        );
    }

    #[test]
    fn transition_inside_for_loop_hoists_animated_per_iteration() {
        let src = "[logic]\nlet items = vec![1,2,3];\n[view]\ncol\n    for item in items.iter()\n        box fill:#3d78fa transition(fill 200ms)\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            !code.contains("compile_error!"),
            "transition inside a for must be accepted:\n{code}"
        );
        assert!(
            code.contains("let __transition_0 = motion::Animated::new(Color::rgba(61.0 / 255.0, 120.0 / 255.0, 250.0 / 255.0, 255.0 / 255.0), motion::tween(std::time::Duration::from_millis(200), motion::Easing::EaseOut));"),
            "missing hoisted Animated:\n{code}"
        );
        let for_pos = code
            .find("for item in items.iter() {")
            .expect("for loop emitted");
        let hoist_pos = code
            .find("let __transition_0 =")
            .expect("hoisted Animated present");
        let ctor_pos = code
            .find("StyledContainer::new")
            .expect("styled container emitted");
        assert!(
            for_pos < hoist_pos && hoist_pos < ctor_pos,
            "Animated hoist must sit inside the loop body, before its widget's constructor:\n{code}"
        );
    }

    #[test]
    fn transition_inside_for_loop_uses_distinct_counters_per_element() {
        let src = "[logic]\nlet items = vec![1,2,3];\n[view]\ncol\n    for item in items.iter()\n        box fill:#3d78fa transition(fill 150ms)\n        box stroke:#111111 transition(stroke 150ms)\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            !code.contains("compile_error!"),
            "both transitions inside the for must be accepted:\n{code}"
        );
        assert!(
            code.contains("let __transition_0 =") && code.contains("let __transition_1 ="),
            "expected two distinct hoisted animations, one per element in the loop body:\n{code}"
        );
    }

    #[test]
    fn reactive_opacity_without_transition_reads_signal_each_run() {
        let src = "[logic]\nlet fade = signal(0.5f32);\n[view]\nbox opacity:$fade\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains(".with_opacity({ let fade = fade.clone(); move || fade.get() })"),
            "reactive opacity should read the signal in a closure:\n{code}"
        );
        assert!(
            !code.contains("motion::"),
            "no transition means no motion usage:\n{code}"
        );
    }

    #[test]
    fn static_opacity_still_supported_as_closure() {
        let src = "[view]\nbox fill:#3d78fa opacity:0.5\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains(".with_opacity(|| 0.5)"),
            "static opacity should emit a capture-free closure:\n{code}"
        );
    }

    #[test]
    fn transition_fill_from_class_is_wired_without_false_error() {
        let src = "[style]\n@card\n    fill: #3d78fa\n    radius: 12\n[view]\ncol @card transition(fill 150ms)\n    text \"x\"\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            !code.contains("compile_error!"),
            "class-provided fill should be animatable:\n{code}"
        );
        assert!(
            code.contains("motion::Animated::new"),
            "class fill transition should be wired:\n{code}"
        );
    }

    #[test]
    fn dynamic_radius_forwards_the_expression_not_zero() {
        let code = transpile_source(
            "[logic]\nlet accent = signal(Color::WHITE);\n[view]\nbox fill:$accent radius:rad\n",
            "demo",
            None,
            None,
        )
        .unwrap()
        .rust_code;
        assert!(
            code.contains("with_radius(BorderRadius::all(rad))"),
            "a variable radius should forward verbatim:\n{code}"
        );
        let lit = transpile_source(
            "[logic]\nlet accent = signal(Color::WHITE);\n[view]\nbox fill:$accent radius:8\n",
            "demo",
            None,
            None,
        )
        .unwrap()
        .rust_code;
        assert!(
            lit.contains("with_radius(BorderRadius::all(8.0))"),
            "a literal radius still works:\n{lit}"
        );
    }

    fn paint_code(view: &str) -> String {
        transpile_source(&format!("[view]\n{view}\n"), "demo", None, None)
            .unwrap()
            .rust_code
    }

    /// The shorthand every `border-b` in a ported app turns into, and the one attribute it has to stay.
    #[test]
    fn the_stroke_width_shorthand_names_one_side() {
        let code = paint_code("box stroke:#ff0000 stroke_width:\"0 0 1 0\"");
        assert!(
            code.contains("widths: [0.0, 0.0, 1.0, 0.0]"),
            "the four values reach the style in CSS order:\n{code}"
        );
    }

    /// The other 22 components' worth of existing markup: a plain width still emits nothing per-side, so the stroke keeps deciding its own thickness.
    #[test]
    fn a_plain_stroke_width_stays_uniform() {
        let code = paint_code("box stroke:#ff0000 stroke_width:2");
        assert!(
            code.contains("paint: Paint::Solid(Color::rgba(255.0 / 255.0, 0.0 / 255.0, 0.0 / 255.0, 255.0 / 255.0)), widths: [2.0; 4]"),
            "a uniform border puts its one width on all four sides:\n{code}"
        );
    }

    /// `key(…)` is the only spelling left that admits a space, so every value needing one has to arrive through it. A consumer that only ever saw the quoted or comma-joined spelling would drop the parenthesized one silently, which is the one failure this grammar cannot afford.
    #[test]
    fn the_parenthesized_form_reaches_every_multi_token_value() {
        let stroke = paint_code("box stroke:#ff0000 stroke_width(0 0 1 0)");
        assert!(
            stroke.contains("widths: [0.0, 0.0, 1.0, 0.0]"),
            "stroke_width:\n{stroke}"
        );
        let cols = paint_code("grid cols(1fr 2fr)");
        assert!(
            cols.contains(".display_grid().grid_template_columns("),
            "cols:\n{cols}"
        );
        let drag = paint_code("box drag_button(secondary auxiliary)");
        assert!(
            drag.contains(
                ".drag_button(PointerButton::Secondary).drag_button(PointerButton::Auxiliary)"
            ),
            "drag_button:\n{drag}"
        );
    }

    #[test]
    fn a_named_side_needs_no_shorthand() {
        let code = paint_code("box stroke:#ff0000 stroke_bottom:1");
        assert!(
            code.contains("widths: [0.0, 0.0, 1.0, 0.0]"),
            "an unnamed side is not drawn, the way CSS leaves it styleless:\n{code}"
        );
    }

    /// A logical side cannot be resolved here — which edge it lands on is a runtime question — so it goes out through the helper that reads the writing direction inside the paint closure.
    #[test]
    fn a_logical_side_defers_to_the_writing_direction() {
        let code = paint_code("box stroke:#ff0000 stroke_end:1");
        assert!(
            code.contains("logical_border_widths(0.0, 0.0, 0.0, 0.0, None, Some(1.0))"),
            "start/end reach the runtime helper:\n{code}"
        );
    }

    /// `radius` had four corners in `BorderRadius` all along; only the DSL flattened them.
    #[test]
    fn the_radius_shorthand_reaches_all_four_corners() {
        let code = paint_code("box fill:#ff0000 stroke:#00ff00 radius:\"8 8 0 0\"");
        assert!(
            code.contains(
                "radius: BorderRadius { top_left: 8.0, top_right: 8.0, bottom_right: 0.0, bottom_left: 0.0 }"
            ),
            "the shorthand expands in CSS corner order:\n{code}"
        );
    }

    #[test]
    fn a_named_corner_pair_rounds_one_edge() {
        let code = paint_code("box fill:#ff0000 stroke:#00ff00 radius:8 radius_bottom:0");
        assert!(
            code.contains(
                "radius: BorderRadius { top_left: 8.0, top_right: 8.0, bottom_right: 0.0, bottom_left: 0.0 }"
            ),
            "the named edge overrides the shorthand that seeded it:\n{code}"
        );
    }

    /// And a plain one still emits what it always did, so no existing `.rsx` changes shape.
    #[test]
    fn a_plain_radius_stays_the_one_value_form() {
        let code = paint_code("box fill:#ff0000 radius:8");
        assert!(
            code.contains("with_radius(BorderRadius::all(8.0))"),
            "a single value keeps the shorthand constructor:\n{code}"
        );
    }

    /// A picture rounds its corners through the same resolver a box does, rather than through a narrower parser that would emit `.with_radius(8 8 0 0)` and not compile.
    #[test]
    fn a_picture_takes_the_same_radius_forms_a_box_does() {
        let code = paint_code("img src:\"a.png\" radius:\"8 8 0 0\"");
        assert!(
            code.contains(
                ".with_border_radius(BorderRadius { top_left: 8.0, top_right: 8.0, bottom_right: 0.0, bottom_left: 0.0 })"
            ),
            "an img takes the per-corner form:\n{code}"
        );
    }

    #[test]
    fn transition_color_on_text_wraps_text_style() {
        let src = "[view]\ntext \"hi\" color:$theme.primary transition(color 120ms)\n";
        let code = transpile_source(src, "demo", Some("SandboxTheme"), None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("let __transition_0 = motion::Animated::new(theme.get().primary, motion::tween(std::time::Duration::from_millis(120), motion::Easing::EaseOut));"),
            "missing hoisted color Animated:\n{code}"
        );
        assert!(
            code.contains(".with_color({ __transition_0.retarget(theme.get().primary); __transition_0.get() })"),
            "text color should be wrapped in the transition block:\n{code}"
        );
    }

    /// A compound callee gets the *recipe* for its children, not the children: they are built inside its context, which is the whole reason a row can ask which menu it is in.
    #[test]
    fn a_compound_component_receives_its_children_as_a_recipe() {
        let code = paint_code("menu label:\"Edit\"\n    item label:\"Undo\"\n    separator");
        assert!(
            code.contains("Children::new(") && code.contains("Ok(__slots)"),
            "the children are a closure returning the slots:\n{code}"
        );
        assert!(
            code.contains("menu(MenuProps::props().label(\"Edit\").build(), __deferred)"),
            "and the recipe is what the callee is handed:\n{code}"
        );
    }

    /// A `.rsx` can be compound too, and the markup names its context type by declaring one: a `Context` struct in `[logic]` is what turns the component's children from arguments into a recipe. It was the last thing keeping compound components to the built-in catalogue.
    #[test]
    fn a_context_struct_makes_an_rsx_component_compound() {
        let src = "[logic]\n#[derive(Clone)]\npub struct Context {\n    pub pick: u32,\n}\n\nlet ctx = Context { pick: 1 };\n\n[view]\ncol\n    children in:ctx\n";
        let code = transpile_source(src, "picker", None, None)
            .unwrap()
            .rust_code;

        assert!(
            code.contains("pub struct PickerContext"),
            "the type is renamed and lifted to module scope, so children in other files can name it:\n{code}"
        );
        assert!(
            code.contains("pub fn picker(props: PickerProps, children: Children)"),
            "and the component takes the recipe rather than built children:\n{code}"
        );
        assert!(
            code.contains("let mut __slots = children.build_with(ctx)?;"),
            "run inside the context `[logic]` built, before the view drains it:\n{code}"
        );
        // An alias is rewritten, not renamed: renaming in place would shift every column rustc reports for the line.
        assert!(
            code.contains("use PickerContext as Context;")
                && code.contains("    let ctx = Context { pick: 1 };"),
            "the body is untouched and an injected alias carries the name:\n{code}"
        );
    }

    /// The recipe runs again on every open, so a signal it reads is cloned into it rather than moved — the same treatment a reactive `if`/`for` branch gets, and for the same reason.
    #[test]
    fn a_recipe_clones_the_signals_it_reads_instead_of_moving_them() {
        let code = transpile_source(
            "[logic]\nlet busy = signal(false);\n[view]\nmenu label:\"Edit\"\n    item label:\"Undo\" disabled:$busy\n",
            "demo",
            None,
            None)
        .unwrap()
        .rust_code;
        assert!(
            code.contains("let busy = busy.clone();"),
            "the recipe owns its own handle:\n{code}"
        );
        assert!(
            code.contains(".disabled(busy.clone())"),
            "and a reactive predicate reaches the row as a closure, not a resolved bool:\n{code}"
        );
    }

    /// A childless compound call still passes the second argument, or it would not match the callee's arity.
    #[test]
    fn a_childless_compound_call_still_passes_a_recipe() {
        let code = paint_code("menu label:\"Edit\"");
        assert!(
            code.contains("Children::default()"),
            "an empty recipe, not an empty Slots:\n{code}"
        );
    }

    #[test]
    fn fill_signal_reads_reactively_and_clones_into_the_closure() {
        let src = "[logic]\nlet accent = signal(Color::WHITE);\n[view]\nbox fill:$accent\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            !code.contains("compile_error!"),
            "a signal fill must not error:\n{code}"
        );
        assert!(
            code.contains("{ let accent = accent.clone(); move |_| RectStyle::default().with_fill(accent.get()).with_radius(BorderRadius::zero()) }"),
            "fill should reactively read the cloned signal:\n{code}"
        );
    }

    #[test]
    fn fill_signal_with_spring_transition_seeds_and_retargets_from_the_same_read() {
        let src = "[logic]\nlet accent = signal(Color::WHITE);\n[view]\nbox fill:$accent transition(fill spring(170, 26))\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains(
                "let __transition_0 = motion::Animated::new(accent.get(), motion::spring(170.0, 26.0));"
            ),
            "missing hoisted Animated seeded from accent.get():\n{code}"
        );
        assert!(
            code.contains(
                "{ let accent = accent.clone(); move |_| RectStyle::default().with_fill({ __transition_0.retarget(accent.get()); __transition_0.get() }).with_radius(BorderRadius::zero()) }"
            ),
            "fill retarget+get should read accent.get() through the cloned signal:\n{code}"
        );
    }

    #[test]
    fn stroke_signal_reads_reactively_and_clones_into_the_closure() {
        let src = "[logic]\nlet accent = signal(Color::WHITE);\n[view]\nbox stroke:$accent\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains(
                "{ let accent = accent.clone(); move |_| RectStyle { fill: None, border: Some(Border { paint: Paint::Solid(accent.get()), widths: [1.0; 4] }), shadow: None, radius: BorderRadius::zero() } }"
            ),
            "stroke should reactively read the cloned signal:\n{code}"
        );
    }

    #[test]
    fn text_color_signal_reads_reactively_and_clones_into_the_closure() {
        let src =
            "[logic]\nlet accent = signal(Color::WHITE);\n[view]\ntext \"hi\" color:$accent\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains(
                "{ let accent = accent.clone(); move |__inherited: TextStyle| __inherited.with_color(accent.get()) }"
            ),
            "text color should reactively read the cloned signal:\n{code}"
        );
    }

    #[test]
    fn hex_theme_and_keyword_colors_are_unaffected_by_signal_support() {
        let src =
            "[view]\nbox fill:#3d78fa stroke:transparent\n    text \"x\" color:$theme.primary\n";
        let code = transpile_source(src, "demo", Some("SandboxTheme"), None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("Color::rgba(61.0 / 255.0, 120.0 / 255.0, 250.0 / 255.0, 255.0 / 255.0)")
        );
        assert!(code.contains("Color::TRANSPARENT"));
        assert!(code.contains("theme.get().primary"));
    }

    #[test]
    fn quoted_svg_src_bakes_static_asset_at_build_time() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let src = "[view]\ncol\n    svg src:\"icon.svg\" color:Color::WHITE width:24 height:24\n";
        let code = transpile_source(src, "demo", None, Some(base.as_path()))
            .unwrap()
            .rust_code;

        assert!(
            code.contains("SvgData::from_baked_vector("),
            "quoted src should bake to a vector SvgData:\n{code}"
        );
        assert!(
            code.contains(
                "static BAKED_SVG_0: std::sync::LazyLock<std::sync::Arc<SvgData>> = std::sync::LazyLock::new(|| std::sync::Arc::new("
            ),
            "missing baked LazyLock static:\n{code}"
        );
        assert!(
            code.contains("move || std::sync::Arc::clone(&BAKED_SVG_0)"),
            "data_fn should clone the shared Arc:\n{code}"
        );
        assert!(
            !code.contains("::renderer_core::") && !code.contains("::geometry_core::"),
            "baked expression must use bare type names:\n{code}"
        );
        assert!(
            code.contains("move || Some(Color::WHITE)"),
            "tint should stay on its dynamic closure path:\n{code}"
        );
        assert!(
            !code.contains("let __src ="),
            "baked asset must not hoist a dynamic __src:\n{code}"
        );
    }

    #[test]
    fn quoted_svg_src_missing_file_emits_compile_error() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let src = "[view]\nsvg src:\"does_not_exist.svg\" width:24\n";
        let code = transpile_source(src, "demo", None, Some(base.as_path()))
            .unwrap()
            .rust_code;
        assert!(
            code.contains("compile_error!(")
                && code.contains("does_not_exist.svg")
                && code.contains("not found"),
            "a missing asset should surface a compile_error:\n{code}"
        );
    }

    #[test]
    fn reactive_for_key_and_gap_is_transparent_gap_fragment() {
        let src = "[logic]\nlet items = signal(vec![1i32, 2, 3]);\n[view]\ncol\n    for n in $items key *n gap:8\n        text \"x\"\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains("fragment(")
                && code.contains("(8) as f32,")
                && !code.contains("ReactiveList"),
            "a keyed `for … gap:N` in a slot host is a transparent gap fragment, not a boxed list:\n{code}"
        );
        assert!(code.contains("|n| *n"), "key closure preserved:\n{code}");
        assert!(
            code.contains("(8) as f32,"),
            "the gap clause is threaded through as the trailing f32 arg:\n{code}"
        );
    }

    #[test]
    fn reactive_for_without_key_compiles_positionally() {
        let src = "[logic]\nlet items = signal(vec![1i32, 2, 3]);\n[view]\ncol\n    for n in $items\n        text \"x\"\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains("fragment_positional("),
            "a keyless reactive for should build via positional:\n{code}"
        );
        assert!(
            !code.contains("compile_error!"),
            "a keyless reactive for must compile, not error:\n{code}"
        );
    }

    #[test]
    fn reactive_for_without_key_with_gap_is_transparent_positional_gap_fragment() {
        let src = "[logic]\nlet items = signal(vec![1i32, 2, 3]);\n[view]\ncol\n    for n in $items gap:8\n        text \"x\"\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains("fragment_positional(") && !code.contains("ReactiveList"),
            "a keyless `for … gap:N` in a slot host is a transparent positional gap fragment:\n{code}"
        );
        assert!(
            code.contains("(8) as f32,"),
            "the gap clause is threaded through as the trailing f32 arg:\n{code}"
        );
    }

    #[test]
    fn reactive_for_gap_outside_slot_host_falls_back_to_a_boxed_list() {
        let keyed = "[logic]\nlet items = signal(vec![1i32, 2, 3]);\n[view]\noverlay\n    for n in $items key *n gap:8\n        text \"x\"\n";
        let code = transpile_source(keyed, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("ReactiveList::new(") && code.contains("(8) as f32,"),
            "a keyed `for … gap` in an overlay falls back to the boxed keyed list:\n{code}"
        );

        let keyless = "[logic]\nlet items = signal(vec![1i32, 2, 3]);\n[view]\noverlay\n    for n in $items gap:8\n        text \"x\"\n";
        let code = transpile_source(keyless, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("ReactiveList::positional(") && code.contains("(8) as f32,"),
            "a keyless `for … gap` in an overlay falls back to the boxed positional list:\n{code}"
        );
    }

    #[test]
    fn text_line_height_and_letter_spacing() {
        let src = "[view]\ntext \"Hi\" line_height:1.5 letter_spacing:2\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains(".with_line_height(1.5)"),
            "line_height:\n{code}"
        );
        assert!(
            code.contains(".with_letter_spacing(2.0)"),
            "letter_spacing:\n{code}"
        );
    }

    #[test]
    fn text_raster_selects_the_glyph_grid() {
        let code = transpile_source("[view]\ntext \"Hi\" raster:pixel\n", "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains(".with_raster(Raster::Pixel)"),
            "raster:pixel:\n{code}"
        );
        let smooth = transpile_source("[view]\ntext \"Hi\"\n", "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            !smooth.contains(".with_raster"),
            "the default must not emit the axis at all:\n{smooth}"
        );
    }

    #[test]
    fn path_tag_emits_pathdata_builder_and_widget() {
        let src = "[view]\npath d:\"M0,0 L10,0 Z\" fill:#ff0000 stroke:#000000 stroke_width:2 width:10 height:10\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains("PathData::new().move_to(Point::new(0.0, 0.0)).line_to(Point::new(10.0, 0.0)).close()"),
            "d: compiles to a PathData builder chain:\n{code}"
        );
        assert!(
            code.contains("Path::static_data(LayoutStyle::new()"),
            "draws a Path widget from the baked path data:\n{code}"
        );
        assert!(!code.contains("Canvas::new("), "{code}");
        assert!(
            code.contains("fill: Some(Paint::Solid(")
                && code.contains("stroke: Some(Stroke::new(")
                && code.contains("Stroke::new(Color::rgba(0.0 / 255.0, 0.0 / 255.0, 0.0 / 255.0, 255.0 / 255.0), 2.0)"),
            "fill/stroke/stroke_width reach the PathStyle:\n{code}"
        );
        assert!(
            code.contains(".width(10") && code.contains(".height(10"),
            "width/height size the path's own box:\n{code}"
        );
    }

    #[test]
    fn path_tag_relative_and_curves() {
        let src = "[view]\npath d:\"m10,10 l10,0 q5,-5 10,0 c1,1 2,2 3,0\" stroke:#111111 width:40 height:40\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains(".move_to(Point::new(10.0, 10.0)).line_to(Point::new(20.0, 10.0))"),
            "relative moveto/lineto resolve to absolute:\n{code}"
        );
        assert!(
            code.contains(".quad_to(Point::new(25.0, 5.0), Point::new(30.0, 10.0))"),
            "relative quad resolves to absolute:\n{code}"
        );
        assert!(
            code.contains(
                ".cubic_to(Point::new(31.0, 11.0), Point::new(32.0, 12.0), Point::new(33.0, 10.0))"
            ),
            "relative cubic resolves to absolute:\n{code}"
        );
        assert!(
            code.contains("fill: None"),
            "no fill attr means PathStyle.fill is None:\n{code}"
        );
    }

    #[test]
    fn path_tag_signal_fill_is_cloned() {
        let src = "[logic]\nlet c = signal(Color::WHITE);\n[view]\npath d:\"M0,0 L10,10 Z\" fill:$c width:10 height:10\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains("let c = c.clone();"),
            "the signal fill is cloned into the closure:\n{code}"
        );
        assert!(
            code.contains("Paint::Solid(c.get())"),
            "the fill re-reads the signal inside the style closure:\n{code}"
        );
    }

    #[test]
    fn path_tag_invalid_d_is_compile_error() {
        let src = "[view]\npath d:\"L10,10\" width:10 height:10\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains("compile_error!"),
            "a `d` that does not start with a moveto is a compile_error:\n{code}"
        );
    }

    /// The `for` that reads reactive state without the sigil that makes the loop follow it. It compiles to a one-shot Rust loop, so the rows are built from whatever the memo held at construction and never hear about the next value — a list that silently stops updating, with nothing in the source to point at.
    ///
    /// The clause diagnostics next to this one did not catch it: they need a `key`/`gap`/`virtual` to fire, and this shape carries none.
    #[test]
    fn a_for_that_reads_a_signal_without_the_sigil_is_a_compile_error() {
        let src = "[logic]\nlet rows = memo(move || vec![1, 2]);\n\n[view]\ncolumn\n    for r in rows.get().to_vec()\n        text \"{r}\"\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains("compile_error!") && code.contains("reads `rows`"),
            "the error names the signal the loop reads:\n{code}"
        );
    }

    /// The counterweight, and the reason this cannot simply reject every `for` without a `$`: a loop over a genuine constant is correctly non-reactive and must stay silent.
    #[test]
    fn a_for_over_a_static_iterable_is_left_alone() {
        let src = "[logic]\nconst HINTS: [&str; 2] = [\"a\", \"b\"];\nlet rows = memo(move || vec![1]);\n\n[view]\ncolumn\n    for h in HINTS\n        text \"{h}\"\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            !code.contains("compile_error!"),
            "a constant iterable names no signal, even with one declared beside it:\n{code}"
        );
    }

    /// `disabled:$signal` is threaded as a closure, not as a layout value: `width:$sig` re-runs the whole `LayoutStyle` through `styled_by`, and whether a control is usable is not a layout property. The closure is what lets the box re-read the flag instead of sampling it once at construction.
    #[test]
    fn a_reactive_disabled_flag_is_re_read_rather_than_sampled() {
        let src = "[logic]\nlet ready = signal(false);\n\n[view]\nbox width:20 disabled:$ready on_press:(|| ())\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains(".disabled(") && code.contains("ready.get()"),
            "the flag is read inside the closure:\n{code}"
        );
        assert!(
            code.contains("let ready = ready.clone();"),
            "and cloned in, so the caller's binding stays usable:\n{code}"
        );
        assert!(
            !code.contains(".styled_by"),
            "`disabled` is not a layout prop and must not drag the style into an effect:\n{code}"
        );
    }

    /// The HTML spelling: an attribute with no value is the assertion itself, as `absolute` and `click_through` already are.
    #[test]
    fn a_bare_disabled_flag_means_always() {
        let src = "[view]\nbox width:20 disabled on_press:(|| ())\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(code.contains(".disabled(|| true)"), "{code}");
    }

    /// `on_alt_press` was reachable from hand-written Rust but had no attribute key, so a `.rsx` author could not arm a right- or middle-click at all. The library was ahead of the grammar; the grammar caught up.
    #[test]
    fn an_rsx_box_can_arm_a_non_primary_press() {
        let src = "[view]\nbox width:20 on_alt_press:(|b| println!(\"{b:?}\"))\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(code.contains(".on_alt_press("), "{code}");
        assert!(
            code.contains("StyledContainer::"),
            "the handler lives on StyledContainer, so it must force the upgrade:\n{code}"
        );
    }

    /// A forwarded (non-literal) value wires the `maybe_` form, so a wrapper component can pass an `Option` through without a no-op stand-in swallowing every right-click.
    #[test]
    fn a_forwarded_alt_press_wires_the_maybe_form() {
        let src = "[logic]\nlet alt: Option<Box<dyn Fn(PointerButton)>> = None;\n\n[view]\nbox width:20 on_alt_press:alt\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(code.contains(".maybe_on_alt_press("), "{code}");
    }

    /// The one escape hatch a 1069-line application needed: a node paints wherever its commands say, so without a clip an axis line runs straight over the header. `ClippedItem` existed the whole time; the markup had no way to ask for it, so the `.rsx` dropped into Rust and spliced the widget back in.
    #[test]
    fn a_clip_attribute_cuts_a_node_to_its_own_rect() {
        let src = "[view]\ncol\n    box width:100 height:100 clip\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains("ClippedItem::new(box_item(") && code.contains("Clip::both())"),
            "{code}"
        );
    }

    /// A clip is a shape, and the shape is a Rust value: the one-way cut CSS cannot say (`overflow: hidden` on one axis forces the other out of `visible`), and the radius the renderer's clip node always took while the markup had no word for it.
    #[test]
    fn a_clip_is_the_shape_its_value_names() {
        let code = transpile_source(
            "[view]\nrow width:100 clip:Clip::x()\n    text \"a\"\n",
            "demo",
            None,
            None,
        )
        .unwrap()
        .rust_code;
        assert!(code.contains("Clip::x())"), "{code}");

        let rounded = transpile_source(
            "[view]\nbox radius:8 clip:(Clip::both().rounded(8.0).inset(1.0))\n    text \"a\"\n",
            "demo",
            None,
            None,
        )
        .unwrap()
        .rust_code;
        assert!(
            rounded.contains("Clip::both().rounded(8.0).inset(1.0))"),
            "{rounded}"
        );
    }

    /// The ring, spelled like the state paints beside it but composed rather than swapped — and declaring one is what makes the box focusable, or it would be a style nothing could ever satisfy.
    #[test]
    fn a_focus_style_reaches_the_box() {
        let src = "[view]\nbox width:20 fill:#ffffff focus_style(stroke:#0066ff)\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(code.contains(".focus_style("), "{code}");
    }

    /// And the paint for the state, spelled like the two states beside it.
    #[test]
    fn a_disabled_style_reaches_the_box() {
        let src = "[view]\nbox width:20 fill:#ffffff disabled_style(fill:#808080)\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(code.contains(".disabled_style("), "{code}");
    }

    /// An effect bound in `[logic]` used to need the root widget to hold its handle, or it deregistered when the component function returned. It belongs to an owner now, so the wrapper that did the holding is gone — and so is the `compile_error!` that refused an effect nobody bound.
    #[test]
    fn an_effect_in_logic_needs_nothing_kept_for_it() {
        let src = "[logic]\nlet count = signal(0);\neffect(move || { let _ = count.get(); });\n\n[view]\ntext \"hi\"\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(!code.contains("Holding::new"), "{code}");
        assert!(!code.contains("compile_error!"), "{code}");
    }

    /// A prop takes its value by ownership, so a `[logic]` binding named at two call sites used to be moved by the first and unavailable to the second — answered, in every `.rsx` that hit it, by the author writing `.clone()` at each one. The `$signal` arm has always done this for them; a binding without the sigil is the same situation and now gets the same answer.
    #[test]
    fn a_logic_binding_passed_to_a_component_is_cloned_for_the_author() {
        let src = "[logic]\nlet items = vec![\"a\"];\n\n[view]\ncolumn\n    menu label:\"File\" items:items\n    menu label:\"Edit\" items:items\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert_eq!(
            code.matches(".items(items.clone())").count(),
            2,
            "both call sites get their own copy:\n{code}"
        );
    }

    /// The call site itself still moves: a binding named once is handed over once, and cloning there would demand `Clone` of everything a `.rsx` forwards.
    ///
    /// **What now clones is the region around it.** A component's children are a recipe that may run again, so the recipe takes its own copy of every binding its subtree names — the one at the call site is still a move, out of the recipe's copy. The consequence is worth stating: a binding that is not `Clone` cannot be forwarded through a component's children at all, and must become an `Rc` or a `Reactive` to cross that line.
    #[test]
    fn a_logic_binding_is_moved_at_the_call_site_and_cloned_by_the_region() {
        let src = "[logic]\nlet save: Box<dyn Fn()> = Box::new(|| {});\n\n[view]\ncolumn\n    save_row on_press:save\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains(".on_press(save)"),
            "the call site takes the binding by value:\n{code}"
        );
        assert!(
            code.contains("let save = save.clone();"),
            "and the recipe around it keeps its own, because it can run again:\n{code}"
        );
    }

    /// Inside one it must still be cloned: the builder closure runs again on every re-render and cannot consume its capture. `captured_idents` does not cover a plain binding — it collects `$signal`s and loop variables — so nothing else keeps it alive.
    #[test]
    fn a_logic_binding_inside_a_reactive_branch_is_still_cloned() {
        let src = "[logic]\nlet open = signal(true);\nlet save: Box<dyn Fn()> = Box::new(|| {});\n\n[view]\ncolumn\n    if $open\n        save_row on_press:save\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(
            code.contains("save.clone()"),
            "a re-runnable branch cannot move its capture:\n{code}"
        );
    }

    /// And only bindings: a name the logic zone never bound is a path, a constant or an ambient token, and cloning it would be inventing a value rather than copying one.
    #[test]
    fn a_name_the_logic_zone_never_bound_is_still_passed_verbatim() {
        let src = "[logic]\nconst ITEMS: [&str; 1] = [\"a\"];\n\n[view]\ncolumn\n    menu label:\"File\" items:ITEMS\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(!code.contains("ITEMS.clone()"), "{code}");
        assert!(code.contains("ITEMS"), "{code}");
    }

    /// And the form the diagnostic is asking for compiles clean.
    #[test]
    fn a_reactive_for_is_not_flagged() {
        let src = "[logic]\nlet rows = memo(move || vec![1, 2]);\n\n[view]\ncolumn\n    for r in $rows\n        text \"{r}\"\n";
        let code = transpile_source(src, "demo", None, None).unwrap().rust_code;
        assert!(!code.contains("compile_error!"), "{code}");
        assert!(code.contains("ReactiveList::"), "{code}");
    }
}
