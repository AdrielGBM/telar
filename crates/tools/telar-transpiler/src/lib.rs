//! RSX transpiler: converts a parsed [`RsxDocument`] AST into compilable Rust source code that depends on `telar::*`.

mod codegen;
mod discovery;
mod error;
mod i18n;
pub mod naming;
mod registry;
mod signal_scan;
mod style;
mod transition;
mod view;

pub use codegen::{
    ComponentRegistry, ComponentSig, ExprSpan, TranspiledSource, external_component_sigs,
    scan_component_sig, source_map_to_json, transpile_source_full, transpile_source_with_theme,
};
pub use discovery::{
    assets_root, auto_modules_enabled, collect_files_by_ext, component_paths,
    discover_rust_modules, find_rsx_files, find_rsx_files_in_tree, relative_output_path,
    relative_stem,
};
pub use error::TranspileError;
pub use i18n::{
    CatalogModel, I18N_CATALOG_PATH, I18N_MODULE, MessageModel, PartModel, catalog_files,
    parse_catalog, parse_message, to_source as bake_catalog_to_source,
};
pub use registry::{
    TAG_REFERENCES_VARIABLE, builtin_tags, color_attr_keys, color_keywords, is_builtin_tag,
    is_control_flow_keyword, keyword_color_rgba, layout_attr_keys, tag_attr_keys,
};
pub use signal_scan::{SignalInfo, scan_locals, scan_signals};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    /// `keep:` is what turns a viewport into one whose position survives its tree being rebuilt; without it
    /// the emission must not change at all, since every scroll that never asked to be kept is one whose
    /// position belongs to the tree it was built with.
    #[test]
    fn a_scroll_keeps_its_position_only_when_it_is_asked_to() {
        let plain =
            transpile_source_with_theme("[view]\nscroll\n    text \"x\"\n", "demo", None, None)
                .unwrap();
        assert!(
            plain.rust_code.contains("LayoutScrollArea::new(")
                && !plain.rust_code.contains("new_kept"),
            "unkeyed scrolls compile exactly as before:\n{}",
            plain.rust_code
        );

        let kept = transpile_source_with_theme(
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

    /// A theme makes every bare lowercase name a candidate token, and that guess used to beat the file's own
    /// bindings — so a `let` three lines above was unreachable from the view, and a binding named after a real
    /// token read the theme instead of itself without any diagnostic. The binding wins now; a name the logic
    /// zone never bound still reaches the theme.
    #[test]
    fn a_logic_binding_beats_a_theme_token_of_the_same_name() {
        let logic = "let muted = telar::Color::WHITE;\nlet size = 16.0;\n";
        let out = transpile_source_with_theme(
            &format!(
                "[logic]\n{logic}\n[view]\nbox fill:muted\n    spinner color:accent size:size\n"
            ),
            "demo",
            Some("crate::Theme"),
            None,
        )
        .unwrap();
        assert!(
            out.rust_code.contains("with_fill(muted)"),
            "a bound colour is itself, not a token lookup:\n{}",
            out.rust_code
        );
        assert!(
            out.rust_code.contains("size: size"),
            "a bound number reaches a prop that is not a colour at all:\n{}",
            out.rust_code
        );
        assert!(
            out.rust_code.contains("use_theme::<crate::Theme>().accent"),
            "a name the file never bound still resolves through the theme:\n{}",
            out.rust_code
        );
    }

    /// Only the zone's own bindings are in scope where the view is emitted: a `let` inside a nested `fn` body
    /// is not, and claiming it would shadow a token the author does mean.
    #[test]
    fn a_binding_nested_inside_a_fn_is_not_in_view_scope() {
        let locals = scan_locals("let outer = 1.0;\nfn helper() {\n    let inner = 2.0;\n}\n");
        assert_eq!(locals, vec!["outer".to_string()]);
        assert_eq!(
            scan_locals("let (a, b) = pair();\nlet mut c: f32 = 0.0;\n"),
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
            "a destructuring pattern contributes every name it binds, and a type annotation none"
        );
    }

    /// An unrecognised first word in the view is a component call, so a `//` note used to compile into a call
    /// to a component named `//`, with the words after it read as its attributes. It builds nothing now, and
    /// it is carried into the generated file — which is what a diagnostic points at.
    #[test]
    fn a_note_in_the_view_builds_nothing_and_is_carried_through() {
        let src = "[view]\ncol\n    // why this box is here\n    text \"hi\"\n";
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("// why this box is here"),
            "the note reaches the generated file:\n{code}"
        );
        assert!(
            !code.contains("Props {"),
            "the note is not a component call:\n{code}"
        );
        assert_eq!(
            code.matches("Text::auto").count(),
            1,
            "and it adds no widget of its own:\n{code}"
        );
    }

    #[test]
    fn i18n_markup_text_emits_catalog_lookup() {
        // A `t"key"` text node compiles to a reactive catalog lookup, not a literal string.
        let out = transpile_source_with_theme("[view]\ntext t\"nav.title\"\n", "demo", None, None)
            .unwrap();
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
        // A plain (non-`t`) text node stays a literal, unaffected.
        let plain =
            transpile_source_with_theme("[view]\ntext \"Hi\"\n", "demo", None, None).unwrap();
        assert!(
            !plain.rust_code.contains("i18n::translate"),
            "{}",
            plain.rust_code
        );
    }

    #[test]
    fn i18n_component_label_emits_reactive_lookup() {
        // A built-in component's text prop written as `t"key"` becomes a reactive catalog lookup inside the
        // `Box<dyn Fn() -> String>` the prop now takes; a plain quoted label becomes a static string closure.
        let out =
            transpile_source_with_theme("[view]\nbutton label:t\"btn.save\"\n", "demo", None, None)
                .unwrap();
        assert!(
            out.rust_code
                .contains("label: Box::new(move || telar::i18n::translate"),
            "{}",
            out.rust_code
        );
        assert!(out.rust_code.contains("\"btn.save\""), "{}", out.rust_code);
        let plain =
            transpile_source_with_theme("[view]\nbutton label:\"Save\"\n", "demo", None, None)
                .unwrap();
        assert!(
            plain
                .rust_code
                .contains("label: Box::new(move || \"Save\".to_string())"),
            "{}",
            plain.rust_code
        );
    }

    #[test]
    fn source_map_points_generated_logic_back_to_rsx() {
        // rsx lines (1-based): 1 [logic], 2 derive, 3 struct, 4 body field, 5 close, 6 let, 8 [view], 9 col.
        let src = "[logic]\n#[derive(Props)]\npub struct Props {\n    pub body: &'static st,\n}\nlet count = signal(0i32);\n\n[view]\ncol\n";
        let result = transpile_source_with_theme(src, "demo", None, None).unwrap();
        let lines: Vec<&str> = result.rust_code.lines().collect();
        assert_eq!(lines.len(), result.source_map.len());

        // The field with the `st` typo maps back to its `.rsx` line (4 -> 0-based 3).
        let body_idx = lines
            .iter()
            .position(|l| l.contains("&'static st"))
            .expect("generated struct field");
        assert_eq!(result.source_map[body_idx], Some(3));

        // The logic binding maps back to its `.rsx` line (6 -> 0-based 5).
        let let_idx = lines
            .iter()
            .position(|l| l.contains("signal"))
            .expect("generated logic line");
        assert_eq!(result.source_map[let_idx], Some(5));

        // Boilerplate (the prelude `use`) has no source line.
        let use_idx = lines
            .iter()
            .position(|l| l.contains("use telar::*"))
            .unwrap();
        assert_eq!(result.source_map[use_idx], None);
    }

    #[test]
    fn source_map_points_generated_view_back_to_rsx() {
        // rsx lines (1-based): 1 [view], 2 col, 3 text, 4 row, 5 button (with closure).
        let src =
            "[view]\ncol\n    text \"hi\"\n    row\n        button on_press(|| missing.set(1))\n";
        let result = transpile_source_with_theme(src, "demo", None, None).unwrap();
        let lines: Vec<&str> = result.rust_code.lines().collect();
        assert_eq!(lines.len(), result.source_map.len());

        // No source marker leaks into the generated output.
        assert!(!result.rust_code.contains("@RSX@"));

        // The button's closure line (where `missing` would error) maps to the `btn` line (5 -> 4).
        let btn_idx = lines
            .iter()
            .position(|l| l.contains("missing.set(1)"))
            .expect("generated button closure");
        assert_eq!(result.source_map[btn_idx], Some(4));

        // The text leaf maps to its own line (3 -> 2).
        let text_idx = lines
            .iter()
            .position(|l| l.contains("\"hi\""))
            .expect("generated text leaf");
        assert_eq!(result.source_map[text_idx], Some(2));

        // A container's own closing constructor maps back to the container, not its last child: the row's `Container::new` resolves to the `row` line (4 -> 3) even though the btn nested inside.
        let row_ctor = lines
            .iter()
            .position(|l| l.contains("flex_row()"))
            .expect("generated row container");
        assert_eq!(result.source_map[row_ctor], Some(3));
    }

    // COUNTER declares [style] colors: with no theme they become local COLOR_* consts; with a theme_type they resolve through use_theme instead (see the theme tests below).
    const COUNTER: &str = r#"[logic]
let count = signal(0i32);

[style]
primary: #3d78fa
dark: #141424

@card
    width: 240
    padding: 20
    gap: 12
    direction: col
    align: center

[view]
col @card
    text "Count: {$count}" size:14 color:dark
    button label:"Increment" fill:primary on_press(|| $count.update(|n| *n += 1))
"#;

    // COUNTER_THEMED has no [style] color declarations — colors flow through the live theme so they react to `set_theme(...)` calls at runtime.
    const COUNTER_THEMED: &str = r#"[logic]
let count = signal(0i32);

[style]
@card
    width: 240
    padding: 20
    gap: 12
    direction: col
    align: center

[view]
col @card
    text "Count: {$count}" size:14 color:dark
    button label:"Increment" fill:primary on_press(|| $count.update(|n| *n += 1))
"#;

    #[test]
    fn relative_output_path_mirrors_tree_and_rejects_out_of_src() {
        let src = Path::new("/proj/src");
        // Nested file mirrors its location relative to src/.
        assert_eq!(
            relative_output_path(Path::new("/proj/src/sections/cards.rsx"), src),
            Some(PathBuf::from("sections/cards.rs"))
        );
        // Root-level file stays at the output root.
        assert_eq!(
            relative_output_path(Path::new("/proj/src/counter.rsx"), src),
            Some(PathBuf::from("counter.rs"))
        );
        // Out-of-src files have no mirror — flattening their absolute path would escape the output root.
        assert_eq!(
            relative_output_path(Path::new("/proj/examples/foo.rsx"), src),
            None
        );
        // src/ itself is not a file.
        assert_eq!(relative_output_path(src, src), None);
    }

    #[test]
    fn generates_counter() {
        let out = transpile_source_with_theme(COUNTER, "counter", None, None).unwrap();
        let code = out.rust_code;
        assert!(code.contains("pub fn counter()"));
        // [style]-declared colors become local constants.
        assert!(code.contains("const COLOR_PRIMARY: Color = Color::rgba(61.0 / 255.0"));
        assert!(code.contains("fn style_card() -> LayoutStyle"));
        assert!(code.contains("move || format!(\"Count: {}\", { count.get() })"));
        // `button` is now a widget component call, not the removed `Button::new` builtin.
        assert!(code.contains("button(ButtonProps {"));
        assert!(code.contains("label: Box::new(move || \"Increment\".to_string())"));
        // Its `fill` is a reactive colour closure; with no theme it reads the [style] const.
        assert!(code.contains("fill: Box::new(move || COLOR_PRIMARY)"));
        assert!(code.contains("count.update(|n| *n += 1)"));
        assert!(code.contains("Container::new(style_card(), children!["));
        assert!(code.contains("Ok(Box::new(__col_0))"));
    }

    // Regression for the F13 fix: a type-annotated `let count: RwSignal<i32> = signal(...)` was skipped by the old `let count =` prefix match, so the later `move` closure's clone was never emitted and it captured `count` by move instead — breaking any later use of `count` in the view.
    #[test]
    fn move_clone_emitted_for_type_annotated_signal() {
        let src = "[logic]\nlet count: RwSignal<i32> = signal(0i32);\nlet doubled = memo(move || count.get() * 2);\n[view]\ntext \"hi\"\n";
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = out.rust_code;
        assert!(code.contains("let count_rsx_mv = count.clone();"));
        assert!(code.contains("let doubled = memo(move || count_rsx_mv.get() * 2);"));
    }

    // A `move` closure that is a call argument on a continuation line (inside an unclosed `(`) must have its
    // signal clone wrapped in a block, NOT injected as a preceding `let` — that would land inside the argument
    // list and be invalid Rust (regression from the hyprshell `watch(..)` migration).
    #[test]
    fn move_clone_in_call_arg_closure_is_block_wrapped() {
        let src = "[logic]\nlet s = signal(0i32);\nsetup(\n    move || s.set(1),\n);\n[view]\ntext \"x\"\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("{ let s_rsx_mv = s.clone(); move || s_rsx_mv.set(1) }"),
            "a call-arg closure's clone must be block-wrapped, not a preceding let:\n{code}"
        );
        assert!(
            !code.contains("\n    let s_rsx_mv = s.clone();\n    move"),
            "the clone must not be emitted as a preceding statement inside the call args:\n{code}"
        );
    }

    // Regression (hyprshell battery module): a `move` closure whose line contains a signal's name ONLY
    // inside a string literal must not trip the `[logic]` clone pass — no spurious clone (which would
    // borrow a value already moved into an earlier closure), and the string left byte-for-byte intact
    // (the old textual rewrite corrupted `"battery-charging"` into `"battery-charging_rsx_mv"`).
    #[test]
    fn logic_clone_pass_ignores_signal_names_inside_string_literals() {
        let src = "[logic]\nlet charging = signal(false);\nlet view = charging.read_only();\nlet glyph = memo(move || if view.get() { \"battery-charging\" } else { \"battery\" });\n[view]\ntext \"{$glyph}\" size:12\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("\"battery-charging\""),
            "the string literal must survive intact:\n{code}"
        );
        assert!(
            !code.contains("charging_rsx_mv"),
            "a name appearing only inside a string must not be cloned/rewritten:\n{code}"
        );
    }

    // `self:center|start|end|stretch` maps to the matching per-child cross-axis override, so a fixed-size
    // child (e.g. a square icon chip) can stay centered instead of stretching to the parent's cross size.
    #[test]
    fn self_alignment_maps_to_align_self() {
        let src = "[view]\ncol\n    box self:center width:20 height:20\n    box self:stretch\n    text \"x\"\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains(".align_self_center()"),
            "self:center should emit align_self_center:\n{code}"
        );
        assert!(
            code.contains(".align_self_stretch()"),
            "self:stretch should still emit align_self_stretch:\n{code}"
        );
    }

    // `has_props` lets the app macro alias a nested component's `Props` type by its base name only when it actually has one.
    #[test]
    fn scan_component_sig_detects_default_fields_and_slot() {
        let src = "[logic]\n#[derive(Default)]\npub struct Props {\n    pub gap: f32,\n    pub title: &'static str,\n}\n[view]\nbox\n    children\n";
        let sig = scan_component_sig(src);
        assert!(sig.has_props && sig.props_default && sig.has_slot);
        assert_eq!(
            sig.prop_fields,
            vec!["gap".to_string(), "title".to_string()]
        );
    }

    /// A preview of a component that reads ambient state needs that state seeded first, and the seeding is not
    /// the same for every preview of the same component — that is the whole point of naming one per block.
    #[test]
    fn a_preview_fixture_runs_before_the_component_is_built() {
        let src = "[view]\ntext \"x\"\n\n[preview \"Charging\" fixture:mock_battery]\ndemo\n\n[preview \"Plain\"]\ndemo\n";
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
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

    /// A `.rsx` component that declares a closure-typed prop gets the same live text and colour the built-in
    /// catalogue gets. Without this a user component could only take `&'static str`, so it could never show a
    /// value that changes — and anything with live text had to stay hand-written Rust.
    #[test]
    fn a_closure_typed_prop_takes_live_text_and_colour() {
        let src = "[logic]\n#[derive(Default)]\npub struct Props {\n    pub label: Box<dyn Fn() -> String>,\n    pub tint: Box<dyn Fn() -> telar::Color>,\n    pub dense: bool,\n}\n[view]\ntext \"x\"\n";
        let sig = scan_component_sig(src);
        assert_eq!(sig.text_fields, vec!["label".to_string()]);
        assert_eq!(sig.color_fields, vec!["tint".to_string()]);
        assert!(
            !sig.text_fields.contains(&"dense".to_string()),
            "a plain field is not a text prop"
        );
    }

    #[test]
    fn scan_component_sig_no_default_no_slot() {
        let src =
            "[logic]\npub struct Props {\n    pub title: &'static str,\n}\n[view]\ntext \"hi\"\n";
        let sig = scan_component_sig(src);
        assert!(sig.has_props && !sig.props_default && !sig.has_slot);
        assert_eq!(sig.prop_fields, vec!["title".to_string()]);
    }

    #[test]
    fn optional_prop_wraps_provided_value_and_defaults_when_omitted() {
        // A widget whose `checked` prop is `Option<RwSignal<bool>>` (Default = None): the scan marks it
        // optional, the caller wraps a provided `$signal` in `Some(...)`, and an omitted `checked` falls to
        // `..Default::default()` — the ergonomics a signal/closure field could not get from `#[derive(Default)]`.
        let widget = "[logic]\n#[derive(Default)]\npub struct Props {\n    pub checked: Option<RwSignal<bool>>,\n    pub label: &'static str,\n}\n[view]\nbox\n";
        let sig = scan_component_sig(widget);
        assert_eq!(sig.optional_fields, vec!["checked".to_string()]);
        assert_eq!(
            sig.prop_fields,
            vec!["checked".to_string(), "label".to_string()]
        );

        let mut reg = ComponentRegistry::new();
        reg.insert("checkbox".to_string(), sig);

        // Provided: the `$flag` signal is wrapped `Some(flag.clone())`; the omitted `label` defaults.
        let with = transpile_source_full(
            "[logic]\nlet flag = signal(false);\n[view]\ncheckbox checked:$flag\n",
            "demo",
            None,
            None,
            Some(&reg),
        )
        .unwrap();
        assert!(
            with.rust_code
                .contains("CheckboxProps { checked: Some(flag.clone()), ..Default::default() }"),
            "optional signal prop should be Some-wrapped and other fields defaulted:\n{}",
            with.rust_code
        );

        // Omitted: no `checked:` field is emitted; `..Default::default()` supplies its `None`.
        let without = transpile_source_full(
            "[view]\ncheckbox label:\"Agree\"\n",
            "demo",
            None,
            None,
            Some(&reg),
        )
        .unwrap();
        assert!(
            without
                .rust_code
                .contains("CheckboxProps { label: \"Agree\", ..Default::default() }"),
            "an omitted optional prop should rely on Default (None):\n{}",
            without.rust_code
        );
        assert!(
            !without.rust_code.contains("checked:"),
            "an omitted optional prop must not be emitted:\n{}",
            without.rust_code
        );
    }

    #[test]
    fn reports_has_props() {
        let with = transpile_source_with_theme(
            "[logic]\npub struct Props {\n    pub title: &'static str,\n}\n[view]\ntext \"{props.title}\"\n",
            "shared_components_card",
            None,
            None,
        )
        .unwrap();
        assert!(with.has_props);
        assert!(with.rust_code.contains("SharedComponentsCardProps"));

        let without = transpile_source_with_theme(
            "[view]\ntext \"hi\"\n",
            "shared_components_note",
            None,
            None,
        )
        .unwrap();
        assert!(!without.has_props);
    }

    #[test]
    fn section_and_heading_resolve_as_widget_components() {
        // `section`/`heading` are no longer built-in tags: they resolve as component calls into the
        // component catalogue (their bodies live in `ui-components`, not the transpiler).
        let src = "[view]\nsection title:\"Cards\"\n    heading text:\"Subtitle\"\n    text \"Body\" size:14 color:dark\n";
        let code = transpile_source_with_theme(src, "cards", None, None)
            .unwrap()
            .rust_code;
        // No inlined library components remain.
        assert!(
            !code.contains("Section::new") && !code.contains("Heading::new"),
            "section/heading must not reference removed components in:\n{code}"
        );
        // `section` is a slotted component call carrying its title...
        assert!(
            code.contains(
                "section(SectionProps { title: Box::new(move || \"Cards\".to_string()) }"
            ),
            "expected section component call in:\n{code}"
        );
        // ...and `heading` a plain component call carrying its text.
        assert!(
            code.contains(
                "heading(HeadingProps { text: Box::new(move || \"Subtitle\".to_string()) })"
            ),
            "expected heading component call in:\n{code}"
        );
    }

    #[test]
    fn theme_type_resolves_colors_via_use_theme() {
        // Colors not declared in [style] resolve reactively through the theme.
        let out =
            transpile_source_with_theme(COUNTER_THEMED, "counter", Some("SandboxTheme"), None)
                .unwrap();
        let code = out.rust_code;
        assert!(code.contains("use telar::use_theme;"));
        assert!(code.contains("use_theme::<SandboxTheme>().dark"));
        assert!(code.contains("use_theme::<SandboxTheme>().primary"));
        // No COLOR_* consts should be referenced inside the function body.
        let fn_start = code.find("pub fn counter").unwrap();
        assert!(!code[fn_start..].contains("COLOR_DARK"));
        assert!(!code[fn_start..].contains("COLOR_PRIMARY"));
    }

    #[test]
    fn style_declared_colors_resolve_via_theme_when_active() {
        // With a theme configured, [style]-declared colors resolve reactively through use_theme like undeclared ones, and their now-dead COLOR_* consts are omitted.
        let out =
            transpile_source_with_theme(COUNTER, "counter", Some("SandboxTheme"), None).unwrap();
        let code = out.rust_code;
        assert!(code.contains("use_theme::<SandboxTheme>().primary"));
        assert!(code.contains("use_theme::<SandboxTheme>().dark"));
        // The now-unused color consts are not emitted.
        assert!(!code.contains("const COLOR_PRIMARY"));
        assert!(!code.contains("const COLOR_DARK"));
        let fn_start = code.find("pub fn counter").unwrap();
        assert!(!code[fn_start..].contains("COLOR_PRIMARY"));
    }

    #[test]
    fn extracts_props_struct_to_file_scope() {
        let src = "[logic]\n#[derive(Props)]\npub struct Props { pub title: &'static str }\n[view]\ntext \"hi\"\n";
        let out = transpile_source_with_theme(src, "card", None, None).unwrap();
        let code = &out.rust_code;
        // Props struct is renamed and lifted before the fn declaration.
        assert!(
            code.contains("pub struct CardProps"),
            "struct should be renamed CardProps"
        );
        assert!(
            code.contains("pub fn card(props: CardProps)"),
            "fn signature must use CardProps"
        );
        // The struct must appear before the fn, not inside it.
        let struct_pos = code.find("pub struct CardProps").unwrap();
        let fn_pos = code.find("pub fn card").unwrap();
        assert!(
            struct_pos < fn_pos,
            "Props struct must precede the function"
        );
        // The derive attribute is preserved as-is (it references the macro, not the struct name).
        assert!(
            code.contains("#[derive(Props)]"),
            "derive attribute must be preserved"
        );
    }

    /// A props struct is emitted as a sibling of the component function, so everything it needs has to reach
    /// file scope with it: the imports its field types name, and the comment that says what it is. Both used to
    /// stay behind in the function body — the first as a compile error at the struct, the second as a doc
    /// comment describing whichever statement happened to follow it.
    #[test]
    fn a_props_struct_takes_its_imports_and_its_comment_with_it() {
        let src = "[logic]\nuse crate::model::Item;\n\n/// What the card shows.\npub struct Props {\n    pub item: Option<Item> = None,\n}\n\nlet item = props.item;\n[view]\ntext \"hi\"\n";
        let out = transpile_source_with_theme(src, "card", None, None).unwrap();
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

    /// A field's own comment is prose, and prose has commas in it. Splitting the struct body on every comma
    /// tore such a field away from its type and dropped it silently — the first sign was a missing field at a
    /// call site, nowhere near the comment that caused it.
    #[test]
    fn a_comma_inside_a_comment_or_string_does_not_split_a_props_field() {
        let src = "[logic]\npub struct Props {\n    // One meter, never two, because a card is a glance.\n    pub meter: Option<f32> = None,\n    pub label: String = \"a, b\".to_string(),\n    pub tail: bool = false,\n}\n[view]\ntext \"hi\"\n";
        let out = transpile_source_with_theme(src, "card", None, None).unwrap();
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
        // `[logic]` line 2 declares `count`; `[view]` line 4 interpolates it.
        let src = "[logic]\nlet count = signal(0i32);\n[view]\ntext \"Count: {count}\"\n";
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();

        // No marker leaks into the generated Rust.
        assert!(!out.rust_code.contains("@RSX@"), "markers must be stripped");

        // One span, for the `{count}` interpolation.
        assert_eq!(out.expr_spans.len(), 1, "expected one expression span");
        let span = &out.expr_spans[0];

        // The span's source range is exactly `count` in the original `.rsx`.
        let rsx_frag = &src[span.rsx_start as usize..(span.rsx_start + span.len) as usize];
        assert_eq!(rsx_frag, "count");

        // It maps byte-for-byte onto `count` in the generated Rust (char-boundary safe).
        let gen_frag =
            &out.rust_code[span.gen_start as usize..(span.gen_start + span.len) as usize];
        assert_eq!(gen_frag, "count");
        assert!(out.rust_code.is_char_boundary(span.gen_start as usize));
    }

    #[test]
    fn expr_spans_are_char_boundary_safe_with_multibyte() {
        // A multi-byte literal precedes the interpolation; the span must still land on char boundaries in both source and generated (the byte-wise affine map would otherwise mis-slice / panic).
        let src = "[logic]\nlet count = signal(0i32);\n[view]\ntext \"caf\u{e9} {count}\"\n";
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
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
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
        assert!(!out.rust_code.contains("@RSX@"));

        // Each span must point at the identical fragment in both source and generated output.
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
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("my_widget(MyWidgetProps {"),
            "should call component fn with Props"
        );
        // `my_widget` is unregistered (no sig), so its `label` is not a known text prop and stays a literal.
        assert!(
            code.contains("label: \"hello\""),
            "an unknown component's quoted attr stays a string literal"
        );
        assert!(code.contains("size: 16.0"), "numeric attr must become f32");
    }

    #[test]
    fn preview_section_generates_build_fn_and_entry() {
        let src = "[logic]\n[view]\ncol\n    text \"x\"\n\n[preview \"Default\"]\ncounter\n";
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        // A dedicated build fn per preview (so prop-taking components can be previewed)...
        assert!(
            code.contains("pub fn demo_preview_0() -> Result<Box<dyn LayoutItem>, LayoutError>"),
            "missing preview build fn:\n{code}"
        );
        // ...whose body builds the preview's markup (here a bare component call)...
        assert!(
            code.contains("counter()?"),
            "preview body should call the component:\n{code}"
        );
        // ...and a PreviewEntry pointing at that fn, not the component fn.
        assert!(
            code.contains("build: demo_preview_0"),
            "entry should point at the preview fn:\n{code}"
        );
        assert!(code.contains("preview_name: \"Default\""));
        assert_eq!(out.preview_names, vec!["Default".to_string()]);
    }

    #[test]
    fn dollar_marks_reactive_reads_and_clones_closure_captures() {
        let src = "[logic]\nlet count = signal(0i32);\n[view]\ncol\n    text \"{$count}\"\n    button on_press(|| $count.update(|n| *n += 1))\n";
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        // `$count` in interpolation is a read.
        assert!(
            code.contains("count.get()"),
            "interpolation read missing:\n{code}"
        );
        // `$count` in a closure is the handle (the `$` is stripped).
        assert!(
            code.contains("count.update(|n| *n += 1)"),
            "closure handle missing:\n{code}"
        );
        // The text and the button each clone `count`, so the two `move` closures own it independently.
        assert_eq!(
            code.matches("let count = count.clone();").count(),
            2,
            "expected one clone per capturing closure:\n{code}"
        );
        // No `$` sigil leaks into the generated Rust.
        assert!(
            !code.contains('$'),
            "the `$` marker must not reach output:\n{code}"
        );
    }

    #[test]
    fn img_src_value_carries_an_expr_span() {
        // The `src` attr is a verbatim Rust expression, so the analyzer must get an expr-span that maps back onto the `hero` identifier in source (this is what makes refs/rename precise in `[view]`).
        let src = "[logic]\nlet hero = 1i32;\n[view]\ncol\n    img src:hero width:100\n";
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
        let spans: Vec<&str> = out
            .expr_spans
            .iter()
            .map(|s| &src[s.rsx_start as usize..(s.rsx_start + s.len) as usize])
            .collect();
        assert!(
            spans.contains(&"hero"),
            "img src value should map back to `hero`; got spans {spans:?}"
        );
        // And the matching span must point at byte-identical text in the generated code.
        let span = out
            .expr_spans
            .iter()
            .find(|s| &src[s.rsx_start as usize..(s.rsx_start + s.len) as usize] == "hero")
            .unwrap();
        let gs = span.gen_start as usize;
        assert_eq!(&out.rust_code[gs..gs + span.len as usize], "hero");
    }

    #[test]
    fn svg_generates_src_tint_and_layout() {
        let src =
            "[view]\ncol\n    svg src:props.icon tint:Color::hex(\"#ff5722\") width:24 height:24\n";
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
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
            "missing tint closure:\n{code}"
        );
        assert!(
            code.contains(".width(24.0)") && code.contains(".height(24.0)"),
            "missing layout dims:\n{code}"
        );
    }

    #[test]
    fn svg_tint_signal_reads_reactively_and_clones_into_the_closure() {
        // `tint:$accent` must share `fill`/`stroke`'s `$ident` resolution (via `color_expr`) and clone the signal into the tint closure so the outer binding stays usable elsewhere, matching `box fill:$sig`.
        let src = "[logic]\nlet accent = signal(Color::WHITE);\n[view]\ncol\n    svg src:props.icon tint:$accent width:24 height:24\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            !code.contains("compile_error!"),
            "a signal tint must not error:\n{code}"
        );
        assert!(
            code.contains("{ let accent = accent.clone(); move || Some(accent.get()) }"),
            "tint should reactively read the cloned signal:\n{code}"
        );
    }

    /// `VirtualList` was built, exported, and referenced by nothing in the toolchain, so every `.rsx` list built
    /// a widget per row up front and long lists were capped instead. It is opt-in rather than inferred: it needs
    /// a fixed row height and an enclosing viewport, and neither is safe to assume of a loop.
    #[test]
    fn a_virtual_loop_builds_only_the_rows_its_scroll_shows() {
        let src = "[logic]\nlet rows = signal(Vec::<Row>::new());\n[view]\nscroll height:400\n    for row in $rows key row.id virtual row_height:32\n        text \"{row.name}\"\n";
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
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

    /// A scroll with no virtual loop under it keeps the cheaper constructor: the viewport is handed over only
    /// where something asked for it.
    #[test]
    fn an_ordinary_scroll_does_not_expose_a_viewport() {
        let src = "[logic]\nlet rows = signal(Vec::<Row>::new());\n[view]\nscroll height:400\n    for row in $rows\n        text \"{row.name}\"\n";
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
        assert!(
            !out.rust_code.contains("__viewport"),
            "no viewport is bound:\n{}",
            out.rust_code
        );
    }

    /// Outside a scroll there is no viewport to ask what is visible, so this is refused where it is written
    /// rather than quietly building every row.
    #[test]
    fn a_virtual_loop_outside_a_scroll_is_refused() {
        let src = "[logic]\nlet rows = signal(Vec::<Row>::new());\n[view]\ncol\n    for row in $rows virtual row_height:32\n        text \"x\"\n";
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
        assert!(
            out.rust_code.contains("needs an enclosing `scroll`"),
            "the requirement is named:\n{}",
            out.rust_code
        );
    }

    /// An attribute a tag does not accept used to map to a builder call that did nothing, or to nothing at all —
    /// `cols:` on a plain `box` compiled and had no effect. The table it checks is the one the analyzer completes
    /// from, so what the editor never suggests is what the build now refuses.
    #[test]
    fn an_attribute_a_tag_does_not_take_is_named() {
        let out =
            transpile_source_with_theme("[view]\nbox nonsense:4 width:10\n", "demo", None, None)
                .unwrap();
        assert!(
            out.rust_code
                .contains("`nonsense` is not an attribute of `box`"),
            "the offending key and tag are both named:\n{}",
            out.rust_code
        );

        // A component's keys are its `Props` fields, which rustc checks — the gate must not guess at them.
        let component =
            transpile_source_with_theme("[view]\nmy_widget anything:4\n", "demo", None, None)
                .unwrap();
        assert!(
            !component.rust_code.contains("is not an attribute"),
            "a component tag is exempt:\n{}",
            component.rust_code
        );
    }

    /// The idiom `track_layout` is used for in Rust, reachable from the view: read where a node ended up so a
    /// sibling can be drawn from it. With a transform transition it is the whole of a sliding indicator, which
    /// is why the two landed together.
    #[test]
    fn track_rect_mirrors_a_node_into_a_signal_and_keeps_the_effect() {
        let src = "[logic]\nlet active = signal(Rect::ZERO);\n[view]\ncol\n    box track_rect:$active width:20\n";
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("track_layout(__tracked.layout_node())"),
            "the node's own rect signal is the source:\n{code}"
        );
        assert!(
            code.contains("active.set(__rect.get())"),
            "and it is mirrored into the author's signal:\n{code}"
        );
        assert!(
            code.contains(".keeping(effect("),
            "with the effect owned by the widget it belongs to:\n{code}"
        );
    }

    /// A transform is read per frame from a closure the renderer already re-runs, so animating one costs a
    /// repaint and no relayout — which is what lets `transition:` reach past paint without breaking the
    /// invariant the whole design rests on. It is also the half of a sliding indicator that is not `track_rect`.
    #[test]
    fn a_transform_can_be_transitioned() {
        let src = "[logic]\nlet x = signal(0.0f32);\n[view]\nbox translate_x:$x transition(translate_x 200ms)\n";
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
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

    /// The layout box stays out: animating it would put a layout pass in every frame of every transition, which
    /// is a separate decision from this one.
    #[test]
    fn transitioning_the_layout_box_is_still_refused() {
        let src = "[view]\nbox width:40 transition(width 200ms)\n";
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
        assert!(
            out.rust_code.contains("compile_error!"),
            "an unsupported property is named, not ignored:\n{}",
            out.rust_code
        );
    }

    /// The shape `if` could never express, and the reason 14 of hyprshell's 16 `widget "…"` escapes are icons:
    /// three arms of different structure, a payload bound out of the matched variant, and a key that is the
    /// payload's own identity rather than the variant — so re-arriving at the same picture does not rebuild.
    #[test]
    fn a_reactive_match_extracts_a_payload_and_keys_on_it() {
        let src = "[logic]\nlet state = signal(AssetState::Loading);\n[view]\ncol\n    match $state as s key s.as_ready().map(|svg| svg.id())\n        AssetState::Ready(svg)\n            svg src:svg\n        AssetState::Failed\n            box width:16 height:16\n        _\n            text \"…\"\n";
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
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

    /// Without a key the fallback must be hashable whatever the matched type is, so it reconciles on the variant
    /// — rebuilding when the shape changes and not when the payload does.
    #[test]
    fn a_keyless_reactive_match_reconciles_on_the_variant() {
        let src = "[logic]\nlet state = signal(Mode::A);\n[view]\ncol\n    match $state\n        Mode::A\n            text \"a\"\n        _\n            text \"b\"\n";
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
        assert!(
            out.rust_code.contains("::std::mem::discriminant"),
            "the variant is the default key:\n{}",
            out.rust_code
        );
    }

    /// A scrutinee with no `$` chooses its arm once, so it stays an ordinary Rust `match` — the same split `if`
    /// and `for` already make between a construction-time branch and a reconciled one.
    #[test]
    fn a_match_without_a_signal_stays_a_construction_time_branch() {
        let src = "[view]\ncol\n    match props.kind\n        Kind::One\n            text \"one\"\n        _\n            text \"other\"\n";
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(code.contains("match props.kind {"), "plain match:\n{code}");
        assert!(
            !code.contains("discriminant") && !code.contains("ReactiveList::new"),
            "and nothing reactive is built for it:\n{code}"
        );
    }

    /// `Svg::with_stroke` is how a theme draws every icon at one weight without editing the assets, and it was
    /// reachable only from Rust — which is one of the two reasons a themed icon could not be a `.rsx` component.
    #[test]
    fn svg_stroke_overrides_the_documents_own_weight() {
        let literal = transpile_source_with_theme(
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

        let live = transpile_source_with_theme(
            "[logic]\nlet weight = signal(2.0f32);\n[view]\ncol\n    svg src:props.icon stroke:$weight\n",
            "demo",
            None,
            None,
        )
        .unwrap();
        assert!(
            live.rust_code.contains(".with_stroke(") && live.rust_code.contains("weight.get()"),
            "and a signal is read inside the closure:\n{}",
            live.rust_code
        );

        let none = transpile_source_with_theme(
            "[view]\ncol\n    svg src:props.icon\n",
            "demo",
            None,
            None,
        )
        .unwrap();
        assert!(
            !none.rust_code.contains(".with_stroke("),
            "an svg that asks for no stroke keeps the document's own:\n{}",
            none.rust_code
        );
    }

    #[test]
    fn svg_without_tint_generates_none() {
        let src = "[view]\ncol\n    svg src:props.icon\n";
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
        assert!(
            out.rust_code.contains("|| None,"),
            "missing default tint closure:\n{}",
            out.rust_code
        );
    }

    #[test]
    fn lazy_defers_its_subtree_behind_a_when_condition() {
        let src = "[logic]\nlet show = signal(false);\nlet count = signal(0i32);\n[view]\ncol\n    lazy when:$show\n        text \"count {$count}\"\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(!code.contains("compile_error!"), "{code}");
        assert!(
            code.contains("Lazy::new("),
            "no Lazy widget emitted:\n{code}"
        );
        assert!(
            code.contains("{ let show = show.clone(); move || show.get() }"),
            "the condition must read the signal through its own clone:\n{code}"
        );
        // The build closure is `move`, so every signal its subtree reads has to be cloned in above it or the outer binding would be moved out of the rest of the view.
        assert!(
            code.contains("let count = count.clone();"),
            "the deferred subtree's signals must be cloned into the build closure:\n{code}"
        );
        assert!(
            code.contains("move || -> Result<Vec<Box<dyn LayoutItem>>, LayoutError>"),
            "missing deferred build closure:\n{code}"
        );
    }

    // A signal read INSIDE a reactive branch used to be moved into the branch closure, so the same signal stopped being usable in the rest of the view — a move error in generated code the author never wrote.
    #[test]
    fn a_reactive_if_clones_the_signals_its_branches_read() {
        let src = "[logic]\nlet show = signal(true);\nlet count = signal(0i32);\n[view]\ncol\n    if $show\n        text \"in branch {$count}\"\n    text \"outside {$count}\"\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(!code.contains("compile_error!"), "{code}");
        assert!(
            code.contains("let count = count.clone();"),
            "the branch closure must clone the signal it reads, not move it:\n{code}"
        );
        // The condition keeps its own clone in the source closure; the two must not be conflated.
        assert!(
            code.contains("{ let show = show.clone(); move || vec![show.get()] }"),
            "the condition still clones separately:\n{code}"
        );
    }

    #[test]
    fn a_reactive_for_clones_the_signals_its_body_reads() {
        let src = "[logic]\nlet items = signal(vec![1i32, 2]);\nlet scale = signal(2i32);\n[view]\ncol\n    for n in $items\n        text \"{$scale}\"\n    text \"outside {$scale}\"\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(!code.contains("compile_error!"), "{code}");
        assert!(
            code.contains("let scale = scale.clone();"),
            "the item builder must clone the signal its body reads:\n{code}"
        );
    }

    // The loop variable is the closure's own parameter, so it must never appear in the prelude above it — that would name a binding which does not exist in the enclosing scope.
    #[test]
    fn a_reactive_for_never_clones_its_own_loop_variable() {
        let src = "[logic]\nlet items = signal(vec![1i32, 2]);\n[view]\ncol\n    for n in $items\n        text \"{n}\"\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        // Only the line directly above the builder matters: `let n = n.clone();` *inside* it is the leaf emitter cloning the parameter into its own reactive closure, which is correct and long-standing.
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

    // A branch reading no signal needs no prelude at all, so the common case stays unwrapped.
    #[test]
    fn a_signal_free_reactive_branch_gets_no_clone_prelude() {
        let src = "[logic]\nlet show = signal(true);\n[view]\ncol\n    if $show\n        text \"static\"\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
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
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("compile_error!(\"lazy: needs a `when:` condition"),
            "a `lazy` with nothing to defer until must not silently build eagerly:\n{code}"
        );
    }

    #[test]
    fn svg_missing_src_falls_back_to_undefined_placeholder() {
        // No `src` attr: falls back to an undefined `__svg_data` identifier, so rustc's "cannot find value" error lands on this `.rsx` line via the source map — the same diagnostic strategy `img` uses for a missing `src`.
        let src = "[view]\ncol\n    svg width:24 height:24\n";
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
        assert!(
            out.rust_code.contains("__svg_data"),
            "missing placeholder identifier:\n{}",
            out.rust_code
        );
    }

    #[test]
    fn svg_src_value_carries_an_expr_span() {
        let src = "[logic]\nlet icon = 1i32;\n[view]\ncol\n    svg src:icon width:24\n";
        let out = transpile_source_with_theme(src, "demo", None, None).unwrap();
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
    fn svg_tint_token_resolves_through_theme() {
        // `tint:accent` must resolve the bare token through `use_theme` exactly like `color:`/`fill:`, so
        // an icon tints from a theme token without a verbose `use_theme::<T>()` expression — and re-reads
        // it each frame so a runtime theme switch recolors the glyph.
        let src = "[view]\ncol\n    svg src:props.icon tint:accent width:18 height:18\n";
        let code = transpile_source_with_theme(src, "demo", Some("NordTheme"), None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("move || Some(use_theme::<NordTheme>().accent),"),
            "tint token should resolve through use_theme:\n{code}"
        );
    }

    #[test]
    fn svg_src_signal_is_reactive_and_clones_the_handle() {
        // `src:$glyph` must re-read the signal on every `view()` so an adaptive icon swaps its glyph when
        // the bound state changes — not freeze the handle captured at construction (`let __src = …`).
        let src = "[logic]\nlet glyph = signal(props.icon.clone());\n[view]\ncol\n    svg src:$glyph tint:accent width:18 height:18\n";
        let code = transpile_source_with_theme(src, "demo", Some("NordTheme"), None)
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
        // Regression: a `$`-free `src:expr` (a constant handle like `icon("bell")`) keeps the
        // capture-once path so a plain asset is not needlessly re-read.
        let src = "[view]\ncol\n    svg src:icon(\"bell\") width:18 height:18\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("let __src = icon(\"bell\").clone();")
                && code.contains("move || __src.clone(),"),
            "constant src should be captured once:\n{code}"
        );
    }

    #[test]
    fn transition_opacity_hoists_animated_and_wraps_reactive_read() {
        // A `transition:opacity` over a reactive `opacity:$sig`: the Animated is hoisted into setup (built once), and the opacity closure re-targets it to the current value and reads it.
        let src = "[logic]\nlet fade = signal(1.0f32);\n[view]\nbox opacity:$fade transition:opacity 200ms ease-out\n    text \"hi\"\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
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
        let src = "[view]\nbox fill:primary transition:fill 150ms cubic-bezier(0.4,0,0.2,1)\n    text \"x\"\n";
        let code = transpile_source_with_theme(src, "demo", Some("SandboxTheme"), None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("let __transition_0 = motion::Animated::new(use_theme::<SandboxTheme>().primary, motion::tween(std::time::Duration::from_millis(150), motion::Easing::CubicBezier(0.4, 0.0, 0.2, 1.0)));"),
            "missing cubic-bezier Animated:\n{code}"
        );
        assert!(
            code.contains(".with_fill({ __transition_0.retarget(use_theme::<SandboxTheme>().primary); __transition_0.get() })"),
            "missing fill retarget+get:\n{code}"
        );
    }

    #[test]
    fn transition_fill_spring() {
        let src = "[view]\nbox fill:#3d78fa transition:fill spring(170,26)\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
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
        let src = "[logic]\nlet fade = signal(1.0f32);\n[view]\nbox fill:#3d78fa opacity:$fade transition:opacity 200ms, fill 150ms linear\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        // The fill transition uses linear...
        assert!(
            code.contains(
                "motion::tween(std::time::Duration::from_millis(150), motion::Easing::Linear)"
            ),
            "missing fill linear tween:\n{code}"
        );
        // ...and the opacity transition uses the default ease-out (no easing given).
        assert!(
            code.contains(
                "motion::tween(std::time::Duration::from_millis(200), motion::Easing::EaseOut)"
            ),
            "missing opacity default-easing tween:\n{code}"
        );
        // Two distinct Animated handles are hoisted.
        assert!(
            code.contains("let __transition_0 =") && code.contains("let __transition_1 ="),
            "expected two hoisted animations:\n{code}"
        );
    }

    #[test]
    fn transition_unsupported_property_emits_compile_error() {
        let src = "[view]\nbox fill:#3d78fa transition:radius 200ms\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("compile_error!(\"transition: unsupported property `radius`"),
            "unsupported prop should emit a compile_error:\n{code}"
        );
    }

    #[test]
    fn transition_invalid_duration_emits_compile_error() {
        let src = "[view]\nbox opacity:0.5 transition:opacity 200\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("compile_error!(\"transition:opacity has an invalid duration `200`"),
            "invalid duration should emit a compile_error:\n{code}"
        );
    }

    #[test]
    fn transition_inside_for_loop_hoists_animated_per_iteration() {
        // `for` is a construction loop (runs once per component instance, pushing one widget per item into `__children`), not a reactive list needing key-based identity; the `Animated` for a `transition:` inside its body must sit inside the loop's own per-iteration `let __sbox_N = { .. }` block, so a fresh, persistent handle is installed for every item.
        let src = "[logic]\nlet items = vec![1,2,3];\n[view]\ncol\n    for item in items.iter()\n        box fill:#3d78fa transition:fill 200ms\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            !code.contains("compile_error!"),
            "transition inside a for must be accepted:\n{code}"
        );
        assert!(
            code.contains("let __transition_0 = motion::Animated::new(Color::rgba(61.0 / 255.0, 120.0 / 255.0, 250.0 / 255.0, 255.0 / 255.0), motion::tween(std::time::Duration::from_millis(200), motion::Easing::EaseOut));"),
            "missing hoisted Animated:\n{code}"
        );
        // The hoist must be textually nested inside the `for` body (between the loop header and its own widget's `StyledContainer::new`), so it re-installs once per iteration at runtime.
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
        // Two elements with `transition:` in the same loop body are two distinct code sites, so the global `transition_count` must still hand out unique names for each — not one shared name reused per iteration.
        let src = "[logic]\nlet items = vec![1,2,3];\n[view]\ncol\n    for item in items.iter()\n        box fill:#3d78fa transition:fill 150ms\n        box stroke:#111111 transition:stroke 150ms\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
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
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
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
        // T-3.1: opacity is now a closure; a static value becomes a capture-free `|| 0.5`.
        let src = "[view]\nbox fill:#3d78fa opacity:0.5\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains(".with_opacity(|| 0.5)"),
            "static opacity should emit a capture-free closure:\n{code}"
        );
    }

    #[test]
    fn transition_fill_from_class_is_wired_without_false_error() {
        // The fill comes from the `@card` class, not an inline attribute; it must still be animatable (no spurious "no matching value").
        let src = "[style]\n@card\n    fill: #3d78fa\n    radius: 12\n[view]\ncol @card transition:fill 150ms\n    text \"x\"\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
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
        // A variable radius must reach the RectStyle (like fill/pad do), not silently collapse to zero.
        let code = transpile_source_with_theme(
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
        // A numeric literal still renders as a float.
        let lit = transpile_source_with_theme(
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

    #[test]
    fn transition_color_on_text_wraps_text_style() {
        let src = "[view]\ntext \"hi\" color:primary transition:color 120ms\n";
        let code = transpile_source_with_theme(src, "demo", Some("SandboxTheme"), None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("let __transition_0 = motion::Animated::new(use_theme::<SandboxTheme>().primary, motion::tween(std::time::Duration::from_millis(120), motion::Easing::EaseOut));"),
            "missing hoisted color Animated:\n{code}"
        );
        assert!(
            code.contains("TextStyle::new(14.0, { __transition_0.retarget(use_theme::<SandboxTheme>().primary); __transition_0.get() })"),
            "text color should be wrapped in the transition block:\n{code}"
        );
    }

    #[test]
    fn fill_signal_reads_reactively_and_clones_into_the_closure() {
        // No `transition:`: `fill:$accent` must still re-evaluate every time the styling closure runs, and must clone `accent` into that closure so the outer binding (declared in `[logic]`) stays usable elsewhere.
        let src = "[logic]\nlet accent = signal(Color::WHITE);\n[view]\nbox fill:$accent\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
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
        // The `Animated`'s initial value and every `retarget` call must both read through the same `accent.get()` expression — the transition mechanism wraps a `$signal` fill exactly like it already does theme colors.
        let src = "[logic]\nlet accent = signal(Color::WHITE);\n[view]\nbox fill:$accent transition:fill spring(170, 26)\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
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
        // `stroke:` shares `color_expr`/`rect_style_pieces` with `fill:`, so `$ident` must work identically.
        let src = "[logic]\nlet accent = signal(Color::WHITE);\n[view]\nbox stroke:$accent\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains(
                "{ let accent = accent.clone(); move |_| RectStyle { fill: None, stroke: Some(Stroke::new(accent.get(), 1.0)), shadow: None, radius: BorderRadius::zero() } }"
            ),
            "stroke should reactively read the cloned signal:\n{code}"
        );
    }

    #[test]
    fn text_color_signal_reads_reactively_and_clones_into_the_closure() {
        // `text`'s `color:` also shares `color_expr` (via `text_style`), confirming the `$ident` branch and its clone-wrapping generalize beyond fill/stroke.
        let src =
            "[logic]\nlet accent = signal(Color::WHITE);\n[view]\ntext \"hi\" color:$accent\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains(
                "{ let accent = accent.clone(); move || TextStyle::new(14.0, accent.get()) }"
            ),
            "text color should reactively read the cloned signal:\n{code}"
        );
    }

    #[test]
    fn hex_theme_and_keyword_colors_are_unaffected_by_signal_support() {
        // Regression guard: adding the `$ident` branch to `color_expr` must not touch the pre-existing hex/theme/keyword paths.
        let src = "[view]\nbox fill:#3d78fa stroke:white\n    text \"x\" color:primary\n";
        let code = transpile_source_with_theme(src, "demo", Some("SandboxTheme"), None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("Color::rgba(61.0 / 255.0, 120.0 / 255.0, 250.0 / 255.0, 255.0 / 255.0)")
        );
        assert!(code.contains("Color::WHITE"));
        assert!(code.contains("use_theme::<SandboxTheme>().primary"));
        assert!(
            !code.contains(".clone()"),
            "no signal clone should appear for static/theme colors:\n{code}"
        );
    }

    #[test]
    fn quoted_svg_src_bakes_static_asset_at_build_time() {
        // A quoted `src:"path"` resolves against `base_dir` and bakes the SVG into a shared `static LazyLock<Arc<SvgData>>`; `tint` still flows through its own dynamic closure.
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let src = "[view]\ncol\n    svg src:\"icon.svg\" tint:Color::WHITE width:24 height:24\n";
        let code = transpile_source_with_theme(src, "demo", None, Some(base.as_path()))
            .unwrap()
            .rust_code;

        // The fixture is a solid-fill path, so it bakes to a vector display list, not a raster.
        assert!(
            code.contains("SvgData::from_baked_vector("),
            "quoted src should bake to a vector SvgData:\n{code}"
        );
        // Baked once into a shared static, cloned per reactive call.
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
        // The baked expression uses bare type names (resolved via `use telar::*`), never qualified paths.
        assert!(
            !code.contains("::renderer_core::") && !code.contains("::geometry_core::"),
            "baked expression must use bare type names:\n{code}"
        );
        // `tint` keeps its dynamic path.
        assert!(
            code.contains("move || Some(Color::WHITE)"),
            "tint should stay on its dynamic closure path:\n{code}"
        );
        // No dynamic `__src` hoist for a baked asset.
        assert!(
            !code.contains("let __src ="),
            "baked asset must not hoist a dynamic __src:\n{code}"
        );
    }

    #[test]
    fn quoted_svg_src_missing_file_emits_compile_error() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let src = "[view]\nsvg src:\"does_not_exist.svg\" width:24\n";
        let code = transpile_source_with_theme(src, "demo", None, Some(base.as_path()))
            .unwrap()
            .rust_code;
        assert!(
            code.contains("compile_error!(")
                && code.contains("does_not_exist.svg")
                && code.contains("not found"),
            "a missing asset should surface a compile_error:\n{code}"
        );
    }

    // `for … key … gap:N` inside a container is a transparent gap fragment (spacing via a per-item margin),
    // not a boxed list — the gap is threaded to `fragment_gap` as a trailing `f32` arg.
    #[test]
    fn reactive_for_key_and_gap_is_transparent_gap_fragment() {
        let src = "[logic]\nlet items = signal(vec![1i32, 2, 3]);\n[view]\ncol\n    for n in $items key *n gap:8\n        text \"x\"\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("fragment_gap(") && !code.contains("ReactiveList"),
            "a keyed `for … gap:N` in a slot host is a transparent gap fragment, not a boxed list:\n{code}"
        );
        assert!(code.contains("|n| *n"), "key closure preserved:\n{code}");
        assert!(
            code.contains("(8) as f32,"),
            "the gap clause is threaded through as the trailing f32 arg:\n{code}"
        );
    }

    // A keyless reactive `for` (no `key` clause) compiles by reconciling positionally instead of erroring.
    // Inside a container with no `gap:`, it is a transparent fragment (`fragment_positional`).
    #[test]
    fn reactive_for_without_key_compiles_positionally() {
        let src = "[logic]\nlet items = signal(vec![1i32, 2, 3]);\n[view]\ncol\n    for n in $items\n        text \"x\"\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("fragment_positional("),
            "a keyless reactive for should build via positional:\n{code}"
        );
        assert!(
            !code.contains("compile_error!"),
            "a keyless reactive for must compile, not error:\n{code}"
        );
    }

    // A keyless reactive `for` with a `gap:N` clause inside a container is a transparent positional gap fragment.
    #[test]
    fn reactive_for_without_key_with_gap_is_transparent_positional_gap_fragment() {
        let src = "[logic]\nlet items = signal(vec![1i32, 2, 3]);\n[view]\ncol\n    for n in $items gap:8\n        text \"x\"\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("fragment_positional_gap(") && !code.contains("ReactiveList"),
            "a keyless `for … gap:N` in a slot host is a transparent positional gap fragment:\n{code}"
        );
        assert!(
            code.contains("(8) as f32,"),
            "the gap clause is threaded through as the trailing f32 arg:\n{code}"
        );
    }

    // Outside a slot host — here inside an `overlay`, which takes a plain child vec — a reactive `for … gap:N`
    // can't attach as a transparent fragment, so it falls back to the boxed `ReactiveList` that carries the gap
    // on its own node (`with_gap` keyed, `positional_with_gap` keyless). This keeps the boxed gap path covered.
    #[test]
    fn reactive_for_gap_outside_slot_host_falls_back_to_boxed_with_gap() {
        let keyed = "[logic]\nlet items = signal(vec![1i32, 2, 3]);\n[view]\noverlay\n    for n in $items key *n gap:8\n        text \"x\"\n";
        let code = transpile_source_with_theme(keyed, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("ReactiveList::with_gap(") && code.contains("(8) as f32,"),
            "a keyed `for … gap` in an overlay falls back to the boxed with_gap list:\n{code}"
        );

        let keyless = "[logic]\nlet items = signal(vec![1i32, 2, 3]);\n[view]\noverlay\n    for n in $items gap:8\n        text \"x\"\n";
        let code = transpile_source_with_theme(keyless, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("ReactiveList::positional_with_gap(") && code.contains("(8) as f32,"),
            "a keyless `for … gap` in an overlay falls back to the boxed positional_with_gap list:\n{code}"
        );
    }

    // `line_height:N` and `letter_spacing:N` become the matching TextStyle builder calls.
    #[test]
    fn text_line_height_and_letter_spacing() {
        let src = "[view]\ntext \"Hi\" line_height:1.5 letter_spacing:2\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains(".with_line_height(1.5)"),
            "line_height:\n{code}"
        );
        assert!(
            code.contains(".with_letter_spacing(2.0)"),
            "letter_spacing:\n{code}"
        );
    }

    // A declarative `path d:"…"` compiles its SVG path-data into a `PathData` builder chain and draws it
    // as a `Path` inside a sized `Canvas` (the layout wrapper, since `Path` is not a `LayoutItem`).
    #[test]
    fn path_tag_emits_pathdata_builder_and_widget() {
        let src = "[view]\npath d:\"M0,0 L10,0 Z\" fill:#ff0000 stroke:#000000 stroke_width:2 width:10 height:10\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("PathData::new().move_to(Point::new(0.0, 0.0)).line_to(Point::new(10.0, 0.0)).close()"),
            "d: compiles to a PathData builder chain:\n{code}"
        );
        assert!(
            code.contains("Path::static_data(__path_data.clone(),"),
            "draws a Path widget from the baked path data:\n{code}"
        );
        assert!(
            code.contains("Canvas::new("),
            "wrapped in a Canvas so it lays out:\n{code}"
        );
        assert!(
            code.contains("fill: Some(Paint::Solid(")
                && code.contains("stroke: Some(Stroke::new(")
                && code.contains("Stroke::new(Color::rgba(0.0 / 255.0, 0.0 / 255.0, 0.0 / 255.0, 255.0 / 255.0), 2.0)"),
            "fill/stroke/stroke_width reach the PathStyle:\n{code}"
        );
        assert!(
            code.contains(".width(10") && code.contains(".height(10"),
            "width/height size the wrapping canvas:\n{code}"
        );
    }

    // Relative commands and Bézier curves resolve to absolute `PathData` builder calls at compile time.
    #[test]
    fn path_tag_relative_and_curves() {
        let src = "[view]\npath d:\"m10,10 l10,0 q5,-5 10,0 c1,1 2,2 3,0\" stroke:#111111 width:40 height:40\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
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
        // A stroke with no fill leaves the fill None.
        assert!(
            code.contains("fill: None"),
            "no fill attr means PathStyle.fill is None:\n{code}"
        );
    }

    // A `$signal` path fill is cloned into the reactive style closure so the outer handle stays usable.
    #[test]
    fn path_tag_signal_fill_is_cloned() {
        let src = "[logic]\nlet c = signal(Color::WHITE);\n[view]\npath d:\"M0,0 L10,10 Z\" fill:$c width:10 height:10\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("let c = c.clone();"),
            "the signal fill is cloned into the closure:\n{code}"
        );
        assert!(
            code.contains("Paint::Solid(c.get())"),
            "the fill re-reads the signal inside the style closure:\n{code}"
        );
    }

    // A malformed `d:` surfaces a compile_error! on the path's line rather than emitting broken code.
    #[test]
    fn path_tag_invalid_d_is_compile_error() {
        let src = "[view]\npath d:\"L10,10\" width:10 height:10\n";
        let code = transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("compile_error!"),
            "a `d` that does not start with a moveto is a compile_error:\n{code}"
        );
    }
}
