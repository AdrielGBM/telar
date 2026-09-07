use super::*;

fn migrated(source: &str) -> String {
    migrate(source, &BTreeMap::new(), "demo", &BTreeMap::new())
}

#[test]
fn a_value_loses_its_parens_and_a_directive_keeps_them() {
    let out = migrated(
        "[view]\nbtn label(\"Save\") gap(8) on_press(|| f()) transition(fill 250ms ease-out)\n",
    );
    assert_eq!(
        out,
        "[view]\nbtn label:\"Save\" gap:8 on_press:(|| f()) transition(fill 250ms ease-out)\n"
    );
}

/// A `.rsx` line holds prose, and a codemod that walks it byte by byte splits an em dash in three.
#[test]
fn a_line_of_prose_survives_the_walk() {
    let source = "[view]\n// Un guion largo — y un acento: canción\ncol gap(8)\n";
    assert_eq!(
        migrated(source),
        "[view]\n// Un guion largo — y un acento: canción\ncol gap:8\n"
    );
}

/// A control-flow line is Rust: `if shown($seen)` is a call, `for … in options()` is a call, and a `[view]`-level `let` holds one. None of the three has an attribute in it.
#[test]
fn a_control_flow_line_is_left_alone() {
    let source = "[view]\nif shown($seen)\n    let over = signal(false)\n    for (i, x) in options()\n        row gap(4)\n";
    assert_eq!(
        migrated(source),
        "[view]\nif shown($seen)\n    let over = signal(false)\n    for (i, x) in options()\n        row gap:4\n"
    );
}

/// A leading `::` against the colon reads as `key::…`, which is a key nobody wrote.
#[test]
fn a_value_that_opens_with_a_path_separator_is_parenthesised() {
    assert_eq!(
        migrated("[view]\nrow gap(::ui::scale::space::md())\n"),
        "[view]\nrow gap:(::ui::scale::space::md())\n"
    );
}

#[test]
fn a_call_inside_a_value_is_not_an_attribute() {
    let out = migrated("[view]\ncol gap(space::lg()) pad(scale(2, 3))\n");
    assert_eq!(out, "[view]\ncol gap:space::lg() pad:scale(2, 3)\n");
}

#[test]
fn a_catalog_key_in_a_value_becomes_the_macro_and_content_keeps_the_literal() {
    let out = migrated("[view]\nbtn label:t\"buttons.save\"\ntext t\"nav.title\"\n");
    assert_eq!(
        out,
        "[view]\nbtn label:t!(\"buttons.save\")\ntext t\"nav.title\"\n"
    );
}

#[test]
fn a_theme_read_gains_the_sigil_and_prose_does_not() {
    let out = migrated(
        "[logic]\nlet c = theme().primary;\n\n[view]\nbox fill:theme.surface\n    text \"switch the theme.\" color:theme.ink\n",
    );
    assert_eq!(
        out,
        "[logic]\nlet c = theme.get().primary;\n\n[view]\nbox fill:$theme.surface\n    text \"switch the theme.\" color:$theme.ink\n"
    );
}

/// A qualified call still names the crate's own accessor — which is what a nested `fn` inside `[logic]` needs, since it cannot see the view's binding.
#[test]
fn a_qualified_theme_call_is_not_the_views_binding() {
    let source = "[logic]\nfn draw() {\n    let t = crate::core::theme::theme();\n}\n";
    assert_eq!(migrated(source), source);
}

/// A props struct is `Clone` now, so a unique box in one is a struct that cannot reach a region that rebuilds. Only the declaration is rewritten — a `Box<dyn Fn>` elsewhere in `[logic]` is the author's own.
#[test]
fn a_handler_prop_becomes_a_shared_one() {
    let out = migrated(
        "[logic]\npub struct Props {\n    pub act: Box<dyn Fn()>,\n    pub label: Box<dyn Fn() -> String>,\n    pub tint: Option<Box<dyn Fn() -> Color>>,\n}\n\nlet held: Box<dyn Fn()> = Box::new(|| {});\n\n[view]\ncol\n",
    );
    assert!(out.starts_with("[logic]\nuse std::rc::Rc;\n"), "{out}");
    assert!(out.contains("pub act: Rc<dyn Fn()>,"), "{out}");
    assert!(
        out.contains("pub label: Reactive<String>,"),
        "a value, not a handler: {out}"
    );
    assert!(out.contains("pub tint: Option<Reactive<Color>>,"), "{out}");
    // A closure that takes and returns is a callback with no framework shape: left as written, since reading it as a value would drop the argument it is handed.
    let callback = migrated(
        "[logic]\npub struct Props {\n    pub style: Box<dyn Fn(RectStyle) -> RectStyle>,\n}\n\n[view]\ncol\n",
    );
    assert!(
        callback.contains("pub style: Rc<dyn Fn(RectStyle) -> RectStyle>,"),
        "{callback}"
    );

    // A prop already moved to `Rc` by hand is still a value if it returns one and a handler if it does not, which is what makes running the codemod twice safe.
    let again = migrated(&out);
    assert_eq!(again, out, "the rewrite is its own fixed point");
    let by_hand = migrated(
        "[logic]\nuse std::rc::Rc;\n\npub struct Props {\n    pub label: Rc<dyn Fn() -> String>,\n    pub act: Rc<dyn Fn()>,\n}\n\n[view]\ncol\n",
    );
    assert!(
        by_hand.contains("pub label: Reactive<String>,"),
        "{by_hand}"
    );
    assert!(by_hand.contains("pub act: Rc<dyn Fn()>,"), "{by_hand}");
    assert!(
        out.contains("let held: Box<dyn Fn()> = Box::new(|| {});"),
        "a binding outside the declaration is left alone: {out}"
    );
}

/// The inline `= Box::new(…)` default is a closure too, and the type it defaults no longer takes one.
#[test]
fn a_boxed_default_follows_the_type_it_defaults() {
    let out = migrated(
        "[logic]\npub struct Props {\n    pub text: Box<dyn Fn() -> String> = Box::new(String::new),\n}\n\n[view]\ncol\n",
    );
    assert!(
        out.contains("pub text: Reactive<String> = Reactive::of(String::new),"),
        "{out}"
    );

    // And the attribute form, which defaults a handler.
    let handler = migrated(
        "[logic]\npub struct Props {\n    #[props(default = Box::new(|_| {}))]\n    pub on_pick: Box<dyn Fn(bool)>,\n}\n\n[view]\ncol\n",
    );
    assert!(
        handler.contains("#[props(default = Rc::new(|_| {}))]"),
        "{handler}"
    );
    assert!(
        handler.contains("pub on_pick: Rc<dyn Fn(bool)>,"),
        "{handler}"
    );
}

/// The binding that shadows the view's handle is a top-level one. A `let theme = use_theme::<T>()` inside a nested `fn` is that function's own and shadows nothing in the view.
#[test]
fn a_theme_bound_inside_a_fn_shadows_nothing() {
    let out = migrated(
        "[logic]\nfn tint() -> Color {\n    let theme = use_theme::<NordTheme>();\n    theme.muted\n}\n\n[view]\ntext \"x\" font_size:theme.body\n",
    );
    assert!(out.contains("font_size:$theme.body"), "{out}");
    assert!(
        out.contains("    let theme = use_theme::<NordTheme>();"),
        "the nested binding is left alone: {out}"
    );
}

/// A file that binds `theme` itself means its own binding, not the view's handle — `$theme.base` on a `NordTheme` is a `.get()` the type does not have.
#[test]
fn a_file_that_binds_theme_keeps_meaning_its_own() {
    let source = "[logic]\nlet theme = use_theme::<NordTheme>();\n\n[view]\nbox fill:theme.base\n";
    assert_eq!(migrated(source), source);
}

/// A closure was how a call site said "a value that changes", and it fitted the `Box<dyn Fn() -> T>` the prop used to be. Only the props this sweep rewrote are wrapped; any other closure is a handler.
#[test]
fn a_closure_on_a_rewritten_prop_becomes_a_reactive() {
    let mut reactive = BTreeMap::new();
    reactive.insert("icon_glyph".to_string(), vec!["name".to_string()]);
    let out = migrate(
        "[view]\ncol\n    icon_glyph name:(|| \"cpu\".to_string()) on_press:(|| pick())\n",
        &BTreeMap::new(),
        "demo",
        &reactive,
    );
    assert!(
        out.contains("name:(Reactive::of(|| \"cpu\".to_string()))"),
        "{out}"
    );
    assert!(
        out.contains("on_press:(|| pick())"),
        "a handler stays one: {out}"
    );
}

#[test]
fn a_clip_axis_becomes_the_shape_it_named() {
    let out = migrated("[view]\nrow clip:x\ncol clip:y\nbox clip\n");
    assert_eq!(
        out,
        "[view]\nrow clip:Clip::x()\ncol clip:Clip::y()\nbox clip\n"
    );
}

#[test]
fn a_style_constant_moves_to_logic_and_takes_its_uses_with_it() {
    let out = migrated(
        "[logic]\nlet n = 1;\n\n[style]\nprimary: #4361ee\nradius: 6\n\n@card\n    width: 240\n\n[view]\nbox fill:primary radius:radius\n",
    );
    assert!(
        out.contains("const PRIMARY: Color = Color::rgba(0.263, 0.380, 0.933, 1.000);"),
        "{out}"
    );
    assert!(out.contains("const RADIUS: f32 = 6.0;"), "{out}");
    assert!(out.contains("box fill:PRIMARY radius:RADIUS"), "{out}");
    assert!(
        out.contains("@card\n    width: 240"),
        "a class stays: {out}"
    );
    assert!(!out.contains("primary: #4361ee"), "{out}");
}

#[test]
fn a_file_already_in_the_new_grammar_comes_out_unchanged() {
    let source = "[logic]\nlet n = 1;\n\n[style]\n@card\n    width: 240\n\n[view]\nbox @card fill:$theme.surface clip:Clip::x()\n    btn label:\"Save\" on_press:(|| f())\n";
    assert_eq!(migrated(source), source);
}

#[test]
fn a_component_tag_gains_the_use_line_the_crate_root_used_to_supply() {
    let mut modules = BTreeMap::new();
    modules.insert("card".to_string(), "crate::ui::card".to_string());
    let out = migrate(
        "[logic]\nlet n = 1;\n\n[view]\ncol\n    card gap:8\n",
        &modules,
        "demo",
        &BTreeMap::new(),
    );
    assert!(
        out.starts_with("[logic]\nuse crate::ui::card::{card, CardProps};\nlet n = 1;"),
        "{out}"
    );
}

/// A `[preview]` in a component's own file calls it as a sibling function, so importing it would be a module importing itself.
#[test]
fn a_file_never_imports_the_component_it_is() {
    let mut modules = BTreeMap::new();
    modules.insert("stat".to_string(), "crate::ui::stat".to_string());
    let source = "[logic]\nlet n = 1;\n\n[view]\ncol\n\n[preview \"Stat\"]\nstat value:\"60\"\n";
    assert_eq!(migrate(source, &modules, "stat", &BTreeMap::new()), source);
}

#[test]
fn an_escape_that_needs_names_is_reported_rather_than_guessed() {
    let found = escapes_needing_a_person(
        Path::new("a.rsx"),
        "[logic]\nlet x = 1;\n\n[view]\ncol\n    build \"tray(item, cfg)?\"\n    widget \"icon\"\n",
    );
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].1, 6);
    assert!(
        found[0].2.starts_with("build \"tray("),
        "the escape is reported verbatim for a human to name: {}",
        found[0].2
    );
    assert_eq!(found[1].1, 7);
}
