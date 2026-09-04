//! `cargo telar migrate` — rewrites a project's `.rsx` files into the one value grammar.
//!
//! Every rewrite here is mechanical: a spelling that used to mean something the language no longer has a second way to say. What is *not* mechanical is reported instead of guessed — a `build "…"` or `widget "…"` needs names for positional arguments, and only a person knows them.
//!
//! Run it once per project, then `cargo telar fmt` and the usual build. It is idempotent: a file already in the new grammar comes out byte-identical, which is what makes `--check` a CI answer.
//!
//! Quoted text is left alone throughout. A `"…"` is the author's data, and a documentation string showing the old spelling is prose about the language rather than a use of it — rewriting one would change what a sentence says.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::cli::MigrateArgs;
mod imports;
mod reactive;
mod text;
mod theme;
mod view;
mod zones;

use imports::{component_modules, imports_for_tags};
use reactive::{reactive_closures, reactive_props, shared_handlers};
use theme::{style_constants_to_logic, theme_calls, theme_reads};
use view::{clip_shapes, colonise, i18n_macro};
use zones::{Section, zones};

pub(crate) fn run_migrate_cmd(args: MigrateArgs) {
    let roots = match args.paths.is_empty() {
        true => vec![std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))],
        false => args.paths.clone(),
    };

    let mut sources = Vec::new();
    for root in &roots {
        collect_rsx(root, &mut sources);
    }
    sources.sort();
    sources.dedup();

    let modules = component_modules(&sources);
    let reactive = reactive_props(&sources);
    let (mut changed, mut failed, mut manual) = (Vec::new(), Vec::new(), Vec::new());
    for path in &sources {
        let Ok(source) = std::fs::read_to_string(path) else {
            failed.push(path.clone());
            continue;
        };
        manual.extend(escapes_needing_a_person(path, &source));
        let migrated = migrate(&source, &modules, own_stem(path), &reactive);
        if migrated == source {
            continue;
        }
        changed.push(path.clone());
        if !args.check && std::fs::write(path, &migrated).is_err() {
            failed.push(path.clone());
        }
    }

    for (path, line, text) in &manual {
        println!("{}:{line}: {text}", display(path));
    }
    if !manual.is_empty() {
        println!(
            "[cargo-telar] {} escape(s) need a component with named props — converting them is hand work",
            manual.len()
        );
    }
    for path in &changed {
        println!("{}", display(path));
    }
    for path in &failed {
        eprintln!(
            "[cargo-telar] {}: could not be read or written",
            display(path)
        );
    }
    let verb = match args.check {
        true => "would be rewritten",
        false => "rewritten",
    };
    println!(
        "[cargo-telar] {} of {} file(s) {verb}",
        changed.len(),
        sources.len()
    );
    if !failed.is_empty() || (args.check && !changed.is_empty()) {
        std::process::exit(1);
    }
}

/// Every rewrite, in the order the later ones depend on: the colon form first, so what follows reads one grammar rather than two.
fn migrate(
    source: &str,
    modules: &BTreeMap<String, String>,
    own: &str,
    reactive: &BTreeMap<String, Vec<String>>,
) -> String {
    // A file that binds `theme` itself is not talking about the view's handle: its own binding shadows it, so leaving it alone keeps exactly the behaviour the file had.
    let binds_own_theme = zones(source).iter().any(|z| {
        z.section == Section::Logic
            && z.body
                .lines()
                .any(|line| line.starts_with("let theme =") || line.starts_with("let theme:"))
    });
    let read_theme = |body: &str| match binds_own_theme {
        true => body.to_string(),
        false => theme_reads(body),
    };

    let mut out = String::with_capacity(source.len());
    for zone in zones(source) {
        let body = match zone.section {
            Section::View | Section::Preview => {
                let body = colonise(zone.body);
                let body = i18n_macro(&body);
                let body = read_theme(&body);
                reactive_closures(&clip_shapes(&body), reactive)
            }
            Section::Style => read_theme(zone.body),
            Section::Logic => match binds_own_theme {
                true => shared_handlers(zone.body),
                false => shared_handlers(&theme_calls(zone.body)),
            },
            Section::None => zone.body.to_string(),
        };
        out.push_str(zone.header);
        out.push_str(&body);
    }
    style_constants_to_logic(&imports_for_tags(&out, modules, own))
}

/// The `build "…"` and `widget "…"` sites, reported rather than guessed: turning `build "tray_icon(item, config, fg, size)?"` into a tag needs *names* for four positional arguments, and only a person knows them.
fn escapes_needing_a_person(path: &Path, source: &str) -> Vec<(PathBuf, usize, String)> {
    let mut out = Vec::new();
    for zone in zones(source) {
        if !matches!(zone.section, Section::View | Section::Preview) {
            continue;
        }
        let offset = source.len() - zone.body.len();
        let first_line = source[..offset].lines().count();
        for (i, line) in zone.body.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("build \"") || trimmed.starts_with("widget \"") {
                out.push((path.to_path_buf(), first_line + i + 1, trimmed.to_string()));
            }
        }
    }
    out
}

fn collect_rsx(root: &Path, out: &mut Vec<PathBuf>) {
    if root.is_file() {
        if root.extension().and_then(|e| e.to_str()) == Some("rsx") {
            out.push(root.to_path_buf());
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "target" {
            continue;
        }
        collect_rsx(&path, out);
    }
}

/// The component this file *is*, which it never imports: a `[preview]` calling it is a sibling function in the same generated module.
fn own_stem(path: &Path) -> &str {
    path.file_stem().and_then(|s| s.to_str()).unwrap_or("")
}

fn display(path: &Path) -> String {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok())
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
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
        let source =
            "[logic]\nlet theme = use_theme::<NordTheme>();\n\n[view]\nbox fill:theme.base\n";
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
        let source =
            "[logic]\nlet n = 1;\n\n[view]\ncol\n\n[preview \"Stat\"]\nstat value:\"60\"\n";
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
        assert!(found[0].2.starts_with("build \"tray("));
        assert_eq!(found[1].1, 7);
    }
}
