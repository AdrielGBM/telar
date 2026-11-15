use std::cell::RefCell;

use preferences_core::{
    ColorScheme, SystemPreferences, set_high_contrast_override, set_system_preferences,
};

use super::*;
use crate::use_theme_tokens;

fn in_scheme(color_scheme: ColorScheme) {
    let light = crate::register_mode("light", || {});
    let dark = crate::register_mode("dark", || {});
    crate::follow_system(&light, &dark);
    set_system_preferences(SystemPreferences {
        color_scheme: Some(color_scheme),
        ..SystemPreferences::default()
    });
}

fn described(shortfalls: Vec<ContrastShortfall>) -> Vec<String> {
    shortfalls.iter().map(ToString::to_string).collect()
}

fn reaches(tokens: &dyn ThemeTokens, level: ContrastLevel) -> Vec<String> {
    CONTRAST_PAIRS
        .iter()
        .filter_map(|pair| {
            let ratio = pair.drawn.of(tokens).contrast_ratio(pair.on.of(tokens));
            (ratio < pair.used.minimum(level)).then(|| {
                format!(
                    "{} on {} is {ratio:.2}:1",
                    pair.drawn.name(),
                    pair.on.name()
                )
            })
        })
        .collect()
}

struct Defaults;
impl ThemeTokens for Defaults {}

#[test]
fn every_default_token_reaches_its_minimum_in_either_mode_at_either_contrast() {
    let mut shortfalls = Vec::new();
    for color_scheme in [ColorScheme::Light, ColorScheme::Dark] {
        in_scheme(color_scheme);
        shortfalls.extend(described(contrast_shortfalls(Defaults)));
    }
    assert!(shortfalls.is_empty(), "{shortfalls:#?}");
}

/// Status colours a designer might pick for their hue alone, too light to read as text on white, and a translucent surface tint.
#[derive(Clone)]
struct Pastel;
impl ThemeTokens for Pastel {
    fn surface(&self) -> Color {
        Color::WHITE
    }
    fn surface_alt(&self) -> Color {
        Color::rgba(0.2, 0.3, 0.6, 0.08)
    }
    fn muted(&self) -> Color {
        Color::rgba(0.62, 0.62, 0.66, 1.0)
    }
    fn success(&self) -> Color {
        Color::rgba(0.18, 0.69, 0.45, 1.0)
    }
    fn warning(&self) -> Color {
        Color::rgba(0.90, 0.62, 0.16, 1.0)
    }
    fn info(&self) -> Color {
        Color::rgba(0.24, 0.55, 0.90, 1.0)
    }
    fn primary(&self) -> Color {
        Color::rgba(0.45, 0.58, 1.0, 1.0)
    }
}

#[test]
fn the_high_contrast_set_lifts_a_theme_that_falls_short_at_the_standard_level() {
    in_scheme(ColorScheme::Light);
    let shortfalls = contrast_shortfalls(Pastel);
    assert!(
        shortfalls
            .iter()
            .any(|shortfall| shortfall.level == ContrastLevel::Standard
                && shortfall.pair.drawn == ColorToken::Warning),
        "the theme as written falls short: {:#?}",
        described(shortfalls.clone())
    );
    let high: Vec<_> = shortfalls
        .into_iter()
        .filter(|shortfall| shortfall.level == ContrastLevel::High)
        .collect();
    assert!(high.is_empty(), "{:#?}", described(high));
}

#[test]
fn the_tokens_in_force_follow_the_high_contrast_preference() {
    in_scheme(ColorScheme::Light);
    crate::set_theme(Pastel);
    let widths = std::rc::Rc::new(RefCell::new(Vec::new()));
    {
        let widths = std::rc::Rc::clone(&widths);
        reactive_core::effect(move || {
            widths
                .borrow_mut()
                .push(use_theme_tokens().focus_ring_width())
        });
    }

    assert!(!reaches(&*use_theme_tokens(), ContrastLevel::Standard).is_empty());
    set_high_contrast_override(Some(true));
    let tokens = use_theme_tokens();
    assert_eq!(reaches(&*tokens, ContrastLevel::High), Vec::<String>::new());
    assert_eq!(
        tokens.surface_alt().a,
        1.0,
        "the surfaces are opaque under high contrast"
    );
    set_high_contrast_override(Some(false));

    assert_eq!(
        *widths.borrow(),
        vec![2.0, 3.0, 2.0],
        "the ring widens and narrows back"
    );
}

#[test]
fn a_colour_that_already_reads_at_high_contrast_stays_the_themes_own() {
    in_scheme(ColorScheme::Light);
    set_high_contrast_override(Some(true));
    assert_eq!(use_theme_tokens().ink(), Defaults.ink());
}
