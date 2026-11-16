[logic]
use std::time::Duration;

use crate::shared::{self, GlyphProps, Mark, glyph};
use telar::motion::{Easing, Keyframes, Repeat};
use telar::{ControlSize, Tone};

/// One full rotation's duration — fast enough to read as "busy", slow enough not to strobe.
const ROTATION: Duration = Duration::from_millis(900);

/// An icon that turns on its centre for as long as it is shown, for work with no bound value to show. It stands in a box an icon's size, so it takes an icon's place beside a label without moving it.
///
/// What turns is an open arc, drawn like every other mark of the catalogue, or whatever the call nests instead: an `icon`, an `svg`, which sizes the spinner when it is larger than an icon.
///
/// A reader hears it as a progress bar with no reading, which says the work is under way and not how far along it is, called what the call's `a11y_label` names it.
pub struct Props {
    /// What the spinner's colour says. `ink`, the default, is the ink of the text around it, as an icon's is; a named tone or a colour of the caller's marks the work it stands for.
    #[props(into)]
    pub tone: Tone = Tone::Ink,
    /// How large the spinner is drawn: the theme's icon size, as [`ControlSize`] scales it.
    #[props(default)]
    pub size: ControlSize,
    /// What turns in place of the arc, in the spinner's ink.
    #[props(default)]
    pub children: Children,
}

let size = props.size;
let tone = props.tone.clone();
let own = props.children.clone();
let turning = Children::new(move || {
    let own = own.build()?;
    match own.is_empty() {
        true => Ok(vec![glyph(GlyphProps::props().shape(Mark::Arc).size(size).build())?]),
        false => Ok(own),
    }
});
let angle = Keyframes::<f32>::new(0.0)
    .then(360.0, ROTATION, Easing::Linear)
    .start(Repeat::Loop);

[view]
row role:progressbar
    // Sized by what it holds and never stretched, so it turns as a square whatever room the spinner is given.
    row self:center shrink:0 align:center justify:center min_width:(shared::icon_size(size)) min_height:(shared::icon_size(size)) color:(tone.color($text.ink)) rotate:(angle.get())
        children turning
