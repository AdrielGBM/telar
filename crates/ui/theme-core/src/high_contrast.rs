//! The token set a theme is read through while the person asked for more contrast: the theme's own answers, with every colour the catalogue draws text or a control's boundary in pushed far enough from what it is drawn on, the surfaces made opaque and the keyboard's ring widened.

use std::rc::Rc;

use geometry_core::{BODY_TEXT_CONTRAST, Color, ENHANCED_TEXT_CONTRAST, NON_TEXT_CONTRAST};
use renderer_core::Declared;

use crate::{ThemeExtensions, ThemeTokens};

/// How much wider the keyboard's ring is drawn under high contrast.
const RING_WIDENING: f32 = 1.5;

/// How many steps a colour may take toward black or white to reach the contrast it owes.
const STEPS: u8 = 20;

/// What a control's boundary owes what it sits on under high contrast. WCAG asks no more than 3:1 of a boundary at any level, so the set holds it to what body text needs.
pub(crate) const HIGH_CONTRAST_BOUNDARY: f32 = BODY_TEXT_CONTRAST;

/// `theme` read for a person who asked for more contrast. Every token this does not strengthen forwards to `theme`, so a theme still answers its own metrics, washes and extensions.
pub(crate) struct HighContrast(pub(crate) Rc<dyn ThemeTokens>);

impl HighContrast {
    fn grounds(&self) -> [Color; 2] {
        [self.surface(), self.surface_alt()]
    }

    fn text_on_surfaces(&self, color: Color) -> Color {
        strengthened(color, &self.grounds(), ENHANCED_TEXT_CONTRAST)
    }
}

/// `color` laid over the first ground, or the first step from it toward black or white that stands `minimum` from every ground; the far end when no step does.
fn strengthened(color: Color, grounds: &[Color], minimum: f32) -> Color {
    let color = color.over(grounds[0]);
    let reaches = |candidate: Color| {
        grounds
            .iter()
            .all(|ground| candidate.contrast_ratio(*ground) >= minimum)
    };
    if reaches(color) {
        return color;
    }
    let worst = |extreme: Color| {
        grounds
            .iter()
            .map(|ground| extreme.contrast_ratio(*ground))
            .fold(f32::INFINITY, f32::min)
    };
    let extreme = if worst(Color::BLACK) >= worst(Color::WHITE) {
        Color::BLACK
    } else {
        Color::WHITE
    };
    (1..=STEPS)
        .map(|step| color.mix(extreme, f32::from(step) / f32::from(STEPS)))
        .find(|candidate| reaches(*candidate))
        .unwrap_or(extreme)
}

fn opaque(color: Color) -> Color {
    color.with_alpha(1.0)
}

impl ThemeTokens for HighContrast {
    fn primary(&self) -> Color {
        self.text_on_surfaces(self.0.primary())
    }
    fn on_primary(&self) -> Color {
        strengthened(
            self.0.on_primary(),
            &[self.primary()],
            ENHANCED_TEXT_CONTRAST,
        )
    }
    fn radius(&self) -> f32 {
        self.0.radius()
    }
    fn radius_sm(&self) -> f32 {
        self.0.radius_sm()
    }
    fn radius_md(&self) -> f32 {
        self.0.radius_md()
    }
    fn radius_lg(&self) -> f32 {
        self.0.radius_lg()
    }
    fn radius_control(&self) -> f32 {
        self.0.radius_control()
    }
    fn radius_surface(&self) -> f32 {
        self.0.radius_surface()
    }
    fn radius_dialog(&self) -> f32 {
        self.0.radius_dialog()
    }
    fn radius_item(&self) -> f32 {
        self.0.radius_item()
    }
    fn border_width(&self) -> f32 {
        self.0.border_width()
    }
    fn border_width_strong(&self) -> f32 {
        self.0.border_width_strong()
    }
    fn focus_ring(&self) -> Color {
        strengthened(
            self.0.focus_ring(),
            &[self.surface()],
            HIGH_CONTRAST_BOUNDARY,
        )
    }
    fn focus_ring_inner(&self) -> Color {
        self.0.focus_ring_inner().over(self.surface())
    }
    fn focus_ring_width(&self) -> f32 {
        self.0.focus_ring_width() * RING_WIDENING
    }
    fn focus_ring_offset(&self) -> f32 {
        self.0.focus_ring_offset()
    }
    fn font_weight_strong(&self) -> u16 {
        self.0.font_weight_strong()
    }
    fn spacing(&self) -> f32 {
        self.0.spacing()
    }
    fn root(&self) -> Declared {
        self.0.root()
    }
    fn icon_size(&self) -> f32 {
        self.0.icon_size()
    }
    fn spacing_sm(&self) -> f32 {
        self.0.spacing_sm()
    }
    fn spacing_md(&self) -> f32 {
        self.0.spacing_md()
    }
    fn spacing_lg(&self) -> f32 {
        self.0.spacing_lg()
    }
    fn spacing_xl(&self) -> f32 {
        self.0.spacing_xl()
    }
    fn muted(&self) -> Color {
        self.text_on_surfaces(self.0.muted())
    }
    fn scrollbar(&self) -> Color {
        self.0.scrollbar()
    }
    fn scrollbar_width(&self) -> f32 {
        self.0.scrollbar_width()
    }
    fn scrollbar_inset(&self) -> f32 {
        self.0.scrollbar_inset()
    }
    fn scrollbar_radius(&self) -> f32 {
        self.0.scrollbar_radius()
    }
    fn ink(&self) -> Color {
        self.text_on_surfaces(self.0.ink())
    }
    fn surface(&self) -> Color {
        opaque(self.0.surface())
    }
    fn surface_alt(&self) -> Color {
        self.0.surface_alt().over(self.surface())
    }
    fn border(&self) -> Color {
        strengthened(self.0.border(), &self.grounds(), NON_TEXT_CONTRAST)
    }
    fn control_border(&self) -> Color {
        strengthened(
            self.0.control_border(),
            &self.grounds(),
            HIGH_CONTRAST_BOUNDARY,
        )
    }
    fn success(&self) -> Color {
        self.text_on_surfaces(self.0.success())
    }
    fn warning(&self) -> Color {
        self.text_on_surfaces(self.0.warning())
    }
    fn on_error(&self) -> Color {
        strengthened(self.0.on_error(), &[self.error()], ENHANCED_TEXT_CONTRAST)
    }
    fn error(&self) -> Color {
        self.text_on_surfaces(self.0.error())
    }
    fn info(&self) -> Color {
        self.text_on_surfaces(self.0.info())
    }
    fn highlight_low(&self) -> Color {
        self.0.highlight_low()
    }
    fn highlight_med(&self) -> Color {
        self.0.highlight_med()
    }
    fn highlight_high(&self) -> Color {
        self.0.highlight_high()
    }
    fn inverse_surface(&self) -> Color {
        self.0.inverse_surface().over(self.surface())
    }
    fn inverse_ink(&self) -> Color {
        strengthened(
            self.0.inverse_ink(),
            &[self.inverse_surface()],
            ENHANCED_TEXT_CONTRAST,
        )
    }
    fn overlay(&self) -> Color {
        self.0.overlay()
    }
    fn thumb(&self) -> Color {
        self.0.thumb()
    }
    fn register_extensions(&self, extensions: &mut ThemeExtensions) {
        self.0.register_extensions(extensions);
    }
}
