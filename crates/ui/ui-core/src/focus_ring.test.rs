use super::*;
use geometry_core::NON_TEXT_CONTRAST;
use preferences_core::ColorScheme;
use theme_core::{ThemeTokens, register_mode, set_mode, set_mode_scheme, set_theme};

#[derive(Clone)]
struct Defaults;

impl ThemeTokens for Defaults {}

fn in_mode(scheme: ColorScheme) {
    let id = format!("{scheme:?}");
    register_mode(id.clone(), || {});
    set_mode_scheme(id.clone(), scheme);
    set_mode(id);
}

fn better_band(ring: FocusRing, against: Color) -> f32 {
    ring.band
        .contrast_ratio(against)
        .max(ring.hairline.contrast_ratio(against))
}

/// The accent is both the ring's colour and a filled control's, so the default ring has to show on the accent and on the surface around it alike.
#[test]
fn the_default_ring_stands_out_from_the_accent_fill_and_the_surface_in_either_mode() {
    set_theme(Defaults);
    for scheme in [ColorScheme::Light, ColorScheme::Dark] {
        in_mode(scheme);
        let tokens = use_theme_tokens();
        let ring = focus_ring();
        for (what, against) in [("accent", tokens.primary()), ("surface", tokens.surface())] {
            let ratio = better_band(ring, against);
            assert!(
                ratio >= NON_TEXT_CONTRAST,
                "{scheme:?}: the better band is {ratio:.2}:1 against the {what}"
            );
        }
    }
}

#[test]
fn the_ring_drawn_over_content_is_the_band_and_the_hairline_inside_it() {
    let ring = FocusRing {
        band: Color::rgba(0.1, 0.2, 0.9, 1.0),
        hairline: Color::WHITE,
        width: 2.0,
    };
    let borders: Vec<Option<Border>> = ring
        .nodes(Rect::new(0.0, 0.0, 80.0, 30.0), BorderRadius::zero(), 0.0)
        .into_iter()
        .map(|node| match node {
            RenderNode::Primitive(renderer_core::DrawCommand::Rect { style, .. }) => style.border,
            _ => None,
        })
        .collect();
    assert_eq!(
        borders,
        [
            Some(Border::uniform(ring.band, 2.0)),
            Some(Border::uniform(Color::WHITE, 1.0)),
        ]
    );
}
