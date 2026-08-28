//! The caret's blink, and where it sits in a box wider than its text.
//!
//! Both are shared because both widgets that draw a caret — [`Input`](crate::Input) and
//! [`TextArea`](crate::TextArea) — had their own copy of the same 1.5px rectangle and neither of them moved.

use std::time::Duration;

use layout_core::Direction;
use layout_reactive::current_direction;
use motion_core::{Easing, Keyframes, Repeat};
use renderer_core::TextAlign;

/// How long the caret is lit, and how long it is out. The two together are one period, which lands on the
/// second every desktop toolkit settles on.
const HALF_PERIOD: Duration = Duration::from_millis(530);

/// The caret's opacity over time.
///
/// **A caret that does not blink is a caret nobody finds.** It is a hairline of ink that may be sitting in an
/// empty field, and what the eye goes to is what changes — which is the whole of why every text field ever
/// written blinks one.
///
/// A square wave and not a fade: a fading caret reads as an animation, and this one has to read as a caret.
/// It runs only while the field holds the keyboard, because a sequence registered with the ticker is a
/// standing request to redraw and an unfocused field has nothing to ask for.
#[derive(Clone)]
pub(crate) struct Blink(Keyframes<f32>);

impl Blink {
    /// Built stopped: [`follow`](Self::follow) is what starts it, and nothing off screen should be asking the
    /// window for frames.
    pub(crate) fn new() -> Self {
        let held = Keyframes::new(1.0f32)
            .hold(HALF_PERIOD)
            .then(0.0, Duration::ZERO, Easing::Linear)
            .hold(HALF_PERIOD)
            .then(1.0, Duration::ZERO, Easing::Linear)
            .start(Repeat::Loop);
        held.stop();
        Self(held)
    }

    /// What to draw the caret at, between 0 and 1. Reactive: reading it in a `view()` is what makes the blink
    /// arrive on screen.
    pub(crate) fn opacity(&self) -> f32 {
        self.0.get()
    }

    /// Lit again, from the top of the cycle.
    ///
    /// What a keystroke asks for: a caret that blinked out under the hand is a caret in the wrong place at the
    /// moment somebody is looking for it, and every editor answers a key by showing it.
    pub(crate) fn wake(&self) {
        self.0.restart();
    }

    /// Runs while the field has the keyboard and stands still otherwise. Called from a focus effect, so the
    /// widget pays for its animation exactly while the caret is on screen.
    pub(crate) fn follow(&self, focused: bool) {
        match focused {
            true => self.0.restart(),
            false => self.0.stop(),
        }
    }
}

/// Where the first glyph of a line sits in a box wider than it.
///
/// The shaper answers this for the *text*; the caret and the selection are drawn beside the text rather than
/// by it, so they have to ask the same question or they land somewhere the letters are not. A field that
/// inherits `text_align: center` from the region around it — which is what a centred column of chrome hands
/// down — drew its letters in the middle and its caret hard against the left edge.
pub(crate) fn align_origin(align: TextAlign, box_width: f32, text_width: f32) -> f32 {
    let slack = (box_width - text_width).max(0.0);
    let (start, end) = match current_direction() {
        Direction::Ltr => (0.0, slack),
        Direction::Rtl => (slack, 0.0),
    };
    match align {
        // Justify puts the slack between the words of a *wrapped* line and leaves the last one alone; a field is one line, and that line is the last one.
        TextAlign::Start | TextAlign::Justify => start,
        TextAlign::Center => slack / 2.0,
        TextAlign::End => end,
    }
}
