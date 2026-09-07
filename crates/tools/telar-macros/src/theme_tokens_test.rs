use super::*;

fn expand_str(source: &str) -> syn::Result<String> {
    Ok(expand(syn::parse_str::<DeriveInput>(source)?)?.to_string())
}

/// Every token answered by a field of the same name, which is the shape the boilerplate had.
const FULL: &str = "struct T {
        primary: Color, on_primary: Color, radius: f32, spacing: f32, font_size: f32,
        icon_size: f32, muted: Color, scrollbar: Color, ink: Color, surface: Color,
    surface_alt: Color, border: Color, success: Color, warning: Color, error: Color,
    info: Color, highlight_low: Color, highlight_med: Color, highlight_high: Color,
}";

#[test]
fn same_named_fields_answer_their_tokens() {
    let out = expand_str(FULL).expect("every token is answered");
    assert!(out.contains("fn primary"), "emits the token methods");
}

/// The regression this derive exists for: a fixed built-in that contradicts the theme around it has to stop the build rather than reach the screen.
#[test]
fn an_unanswered_token_is_a_compile_error() {
    let err = expand_str(&FULL.replace("radius: f32,", "")).expect_err("radius has no answer");
    let message = err.to_string();
    assert!(message.contains("radius"), "names the token: {message}");
    assert!(
        message.contains("#[theme(default(radius))]"),
        "offers the deliberate opt-out: {message}"
    );
}

#[test]
fn a_defaulted_token_is_accepted_and_left_to_the_trait() {
    let out = expand_str(&format!(
        "#[theme(default(radius))] {}",
        FULL.replace("radius: f32,", "")
    ))
    .expect("opting out answers it");
    assert!(
        !out.contains("fn radius ("),
        "no override, so the trait default stands"
    );
}

#[test]
fn an_alias_answers_a_token_the_field_is_not_named_after() {
    let out = expand_str(&FULL.replace("error: Color,", "#[token(error)] danger: Color,"))
        .expect("the alias answers `error`");
    assert!(out.contains("self . danger"), "reads the aliased field");
}

#[test]
fn an_expression_answers_a_token_no_field_holds() {
    let out = expand_str(&format!(
        "#[theme(scrollbar = self.muted.dim())] {}",
        FULL.replace("scrollbar: Color,", "")
    ))
    .expect("the expression answers `scrollbar`");
    assert!(out.contains("dim"), "emits the expression");
}

/// The three radius steps derive from `radius`, so silence there is the theme following its own base rather than a built-in contradicting it.
#[test]
fn the_radius_scale_is_not_required() {
    let out = expand_str(FULL).expect("valid");
    assert!(!out.contains("fn radius_md"), "left to derive from radius");
}

/// A metric step is an `f32`; answering one with the colour branch would not compile at the call site, which is a long way from the attribute that caused it.
#[test]
fn a_spacing_step_is_typed_as_a_metric() {
    let out = expand_str(&format!("#[theme(spacing_lg = 20.0)] {FULL}")).expect("valid");
    assert!(out.contains("fn spacing_lg (& self) -> f32"), "got: {out}");
}

#[test]
fn a_token_that_does_not_exist_is_rejected() {
    let err =
        expand_str(&format!("#[theme(default(nonesuch))] {FULL}")).expect_err("unknown token name");
    assert!(
        err.to_string().contains("not a ThemeTokens token"),
        "the error must name what went wrong: {}",
        err
    );
}
