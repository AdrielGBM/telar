//! [`Look`]: what a component's `tone` and `variant` paint, in every state, worked out once for every component that has both.

use telar::{Color, RwSignal, Tone, Variant};

use crate::shared;

/// A tone in a variant, as a component carrying it paints: its fill, its frame, the wash a part the pointer operates lays over it, and the ink of what sits on it.
///
/// Every colour is worked out from `around`, the ink of the place the component sits in, which [`Tone::Ink`] is.
#[derive(Clone)]
pub(crate) struct Look {
    pub tone: Tone,
    pub variant: Variant,
    /// A toggle's state: off draws a neutral frame instead of the tone. `None` is always on.
    pub selected: Option<RwSignal<bool>>,
    /// Whether a part answers the pointer, so the ink has to read on its hover and press washes too.
    pub interactive: bool,
}

impl Look {
    /// A component that only shows its tone: no toggle, nothing the pointer operates.
    pub fn shown(tone: Tone, variant: Variant) -> Self {
        Self {
            tone,
            variant,
            selected: None,
            interactive: false,
        }
    }

    fn hue(&self, around: Color) -> Color {
        self.tone.color(around)
    }

    fn lit(&self) -> bool {
        self.selected.is_none_or(|on| on.get())
    }

    pub fn fill(&self, around: Color) -> Color {
        let hue = self.hue(around);
        match (self.lit(), self.variant) {
            (true, Variant::Solid) => hue,
            // Scaled by the colour's own alpha, so a transparent colour washes nothing in.
            (true, Variant::Soft) => hue.with_alpha(hue.a * shared::held_wash().a),
            _ => Color::TRANSPARENT,
        }
    }

    pub fn frame(&self, around: Color) -> Color {
        match (self.lit(), self.variant) {
            (false, _) => shared::border(),
            (true, Variant::Outline) => self.hue(around),
            (true, _) => Color::TRANSPARENT,
        }
    }

    /// What a part the pointer operates lays over the component while the pointer rests on it, or while it is pressed: a filled one's fill stepped away from its ink, a wash of the colour over a soft or outline one, and the theme's own wash where it has no surface of its own, which is how a ghost's surface appears.
    fn state(&self, around: Color, pressed: bool) -> Color {
        let hue = self.hue(around);
        let wash = match pressed {
            true => shared::pressed_wash(),
            false => shared::hover_wash(),
        };
        match (self.lit(), self.variant) {
            (true, Variant::Solid) => {
                let ink = self.ink(around);
                match pressed {
                    true => shared::pressed_fill_under(hue, ink),
                    false => shared::hovered_fill_under(hue, ink),
                }
            }
            (true, Variant::Soft | Variant::Outline) => hue.with_alpha(hue.a * wash.a),
            _ => wash,
        }
    }

    pub fn hovered(&self, around: Color) -> Color {
        self.state(around, false)
    }

    pub fn pressed(&self, around: Color) -> Color {
        self.state(around, true)
    }

    /// The ink of what sits on the component.
    ///
    /// A solid fill's ink is picked for the fill alone, since its states step away from that ink. Any other ink is held to every state the component can show, because a wash only deepens the colour the label is drawn in, against the theme's surface it is assumed to sit on, except the ink of the place itself: that one already reads on whatever the place is, which may be no theme surface at all.
    pub fn ink(&self, around: Color) -> Color {
        let surface = shared::surface();
        let rest = shared::over(self.fill(around), surface);
        let wanted = match (self.lit(), self.variant, &self.tone) {
            (true, Variant::Solid, _) => {
                return shared::readable_on(shared::ink_on(rest), rest);
            }
            (true, _, Tone::Ink) => return around,
            (true, _, _) => self.hue(around),
            (false, _, _) => shared::ink_on(surface),
        };
        let mut under = vec![rest];
        if self.interactive {
            under.extend([
                shared::over(self.state(around, false), rest),
                shared::over(self.state(around, true), rest),
            ]);
        }
        under
            .into_iter()
            .fold(shared::over(wanted, surface), |ink, page| {
                shared::readable_on(ink, page)
            })
    }
}
