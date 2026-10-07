use super::*;
use crate::codegen::{TranspileInput, TranspiledSource, transpile};

fn transpiled(source: &str, theme_type: Option<&str>, library: bool) -> TranspiledSource {
    let document = telar_parser::parse(source).expect("the fixture parses");
    let out = transpile(TranspileInput {
        document: &document,
        component_name: "kit",
        theme_type,
        assets: None,
        prelude: &[],
        library,
        hot_reload: false,
        previews: true,
    })
    .expect("the fixture transpiles");
    if let Err(e) = syn::parse_file(&out.rust_code) {
        panic!("generated code does not parse: {e}\n{}", out.rust_code);
    }
    out
}

fn library(source: &str) -> TranspiledSource {
    transpiled(source, None, true)
}

fn application(source: &str) -> TranspiledSource {
    transpiled(source, Some("app::AppTheme"), false)
}

/// The 1-based `.rsx` line the generated line holding `needle` was written for.
fn rsx_line_of(out: &TranspiledSource, needle: &str) -> Option<u32> {
    let generated = out
        .rust_code
        .lines()
        .position(|line| line.contains(needle))?;
    out.source_map[generated].map(|line| line + 1)
}

#[test]
fn a_library_reads_each_token_through_the_shared_vocabulary() {
    let out = library("[view]\nbox fill:$theme.primary radius:$theme.radius\n    text \"x\"\n");
    assert!(
        out.rust_code
            .contains("::telar::use_theme_tokens().primary()"),
        "{}",
        out.rust_code
    );
    assert!(
        out.rust_code
            .contains("::telar::use_theme_tokens().radius()"),
        "{}",
        out.rust_code
    );
    assert!(!out.rust_code.contains("let theme"), "{}", out.rust_code);
    assert!(!out.rust_code.contains("Theme::<"), "{}", out.rust_code);
}

/// The same markup in an application still reads its own theme type, so the library path is a second spelling of `$theme`, not a change to the first.
#[test]
fn an_application_reads_the_same_markup_through_its_own_theme_type() {
    let out = application("[view]\nbox fill:$theme.primary radius:$theme.radius\n    text \"x\"\n");
    assert!(
        out.rust_code.contains("theme.get().primary"),
        "{}",
        out.rust_code
    );
    assert!(
        out.rust_code
            .contains("let theme = telar::Theme::<app::AppTheme>::default();"),
        "{}",
        out.rust_code
    );
    assert!(
        !out.rust_code.contains("use_theme_tokens"),
        "{}",
        out.rust_code
    );
}

/// A theme type handed to a library transpile is one no consumer has, so it is not what `$theme` compiles against.
#[test]
fn a_library_is_not_transpiled_against_a_theme_type_it_was_handed() {
    let out = transpiled(
        "[view]\nbox fill:$theme.primary\n",
        Some("kit::KitTheme"),
        true,
    );
    assert!(!out.rust_code.contains("KitTheme"), "{}", out.rust_code);
    assert!(
        out.rust_code
            .contains("::telar::use_theme_tokens().primary()"),
        "{}",
        out.rust_code
    );
}

#[test]
fn a_token_written_as_a_call_is_called_once() {
    let out = library("[view]\nbox fill:($theme.primary().with_alpha(0.5))\n");
    assert!(
        out.rust_code
            .contains("::telar::use_theme_tokens().primary().with_alpha(0.5)"),
        "{}",
        out.rust_code
    );
}

#[test]
fn a_name_that_is_not_a_token_is_an_error_on_the_line_that_wrote_it() {
    let out = library("[view]\ncol\n    box fill:$theme.accent\n");
    let message = "`$theme.accent`: `accent` is not a `ThemeTokens` token";
    assert!(
        out.rust_code.contains("compile_error!("),
        "{}",
        out.rust_code
    );
    assert!(out.rust_code.contains(message), "{}", out.rust_code);
    assert!(
        out.rust_code.contains("primary, on_primary, radius"),
        "the message lists the tokens a library may read:\n{}",
        out.rust_code
    );
    assert!(!out.rust_code.contains("accent()"), "{}", out.rust_code);
    assert_eq!(rsx_line_of(&out, message), Some(3));
}

/// `[style]` classes are generated outside the view and read the theme the same way.
#[test]
fn a_style_class_reads_tokens_too() {
    let out = library("[style]\n@card\n    pad: $theme.spacing\n[view]\nbox @card\n");
    assert!(
        out.rust_code
            .contains(".padding_all(::telar::use_theme_tokens().spacing())"),
        "{}",
        out.rust_code
    );
    assert!(!out.rust_code.contains("let theme"), "{}", out.rust_code);
}

/// A library's `$theme` names a function, not a binding, so a closure that reads one has nothing to clone in. Cloning a `theme` that was never bound is the error this guards against.
#[test]
fn a_read_inside_a_rebuilding_region_clones_no_theme() {
    let source =
        "[logic]\nlet open = signal(false);\n[view]\nif $open\n    box fill:$theme.primary\n";
    let app = application(source);
    let lib = library(source);
    assert!(
        app.rust_code.contains("theme.clone()"),
        "the fixture captures the theme in an application:\n{}",
        app.rust_code
    );
    assert!(
        !lib.rust_code.contains("theme.clone()"),
        "{}",
        lib.rust_code
    );
    assert!(
        lib.rust_code
            .contains("::telar::use_theme_tokens().primary()"),
        "{}",
        lib.rust_code
    );
}

/// In an interpolation the token's name is what a cursor lands on, so it is the part mapped back to the `.rsx`.
#[test]
fn an_interpolated_token_maps_its_name_back_to_the_rsx() {
    let source = "[view]\ntext \"{$theme.spacing}\"\n";
    let out = library(source);
    let span = out
        .expr_spans
        .iter()
        .find(|span| &source[span.rsx_start as usize..][..span.len as usize] == "spacing")
        .unwrap_or_else(|| panic!("no span covers the token: {:?}", out.expr_spans));
    assert_eq!(
        &out.rust_code[span.gen_start as usize..][..span.len as usize],
        "spacing"
    );
    assert!(
        out.rust_code
            .contains("::telar::use_theme_tokens().spacing()"),
        "{}",
        out.rust_code
    );
}

#[test]
fn every_shared_token_is_a_read_and_nothing_else_is() {
    for token in telar_project::theme_tokens::all() {
        let after = format!(".{token}");
        assert!(
            matches!(TokensRead::parse(&after), TokensRead::Token { name, called: false } if name == token),
            "{token}"
        );
    }
    assert!(matches!(
        TokensRead::parse(".accent + 1"),
        TokensRead::Unknown { name: "accent" }
    ));
    assert!(matches!(TokensRead::parse(" + 1"), TokensRead::Tokens));
}
