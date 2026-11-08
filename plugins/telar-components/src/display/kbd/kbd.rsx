[logic]
use crate::look::Look;
use crate::shared::{self, ChordStep, shown_chord};
use telar::{Chord, ControlSize, Tone, Variant, keyboard_convention};

/// The key caps' own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role named beside it, read when a cap paints, so a theme that moves the role still moves every cap it has not given a value of its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KbdTokens {
    /// Corner radius of a cap. `None` is the theme's `radius_item`: a cap sits inside a row or a tooltip.
    pub radius: Option<f32>,
    /// Width of a cap's frame. `None` is the theme's `border_width`.
    pub border_width: Option<f32>,
}

impl KbdTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    /// The corner radius of a cap.
    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_item)
    }

    /// The width of a cap's frame.
    pub fn border_width(&self) -> f32 {
        self.border_width.unwrap_or_else(shared::border_width)
    }
}

/// The keys of a shortcut, each on its own cap, for hints beside a command.
///
/// A reader hears the chord once, as the platform writes it (`Ctrl+K, then S`), as keyboard input: text, never a stop, the caps silent. A document writes it as `<kbd>`. It is the hint a person sees; the control that runs the command says the shortcut to a reader itself, through `keyboard_shortcut`, where it can be used.
pub struct Props {
    /// The chord, written portably as [`Chord`] reads it: `"Mod+K"`, `"Alt+Shift+P"`, `"Ctrl++"`, or a sequence of them step after step, `"Mod+K S"`. Its caps are the keys the platform names, `⌘` `K` on a Mac and `Ctrl` `K` elsewhere; the steps of a sequence stand further apart than the keys of one, and it is heard as `Ctrl+K, then S`. Text that names no chord is one cap, as written.
    #[props(into, default)]
    pub chord: Reactive<String>,
    /// How large the caps are drawn: their padding, the gap between them and their text, as [`ControlSize`] works them out.
    #[props(default)]
    pub size: ControlSize,
    /// How the keys of a step are put on caps: each on its own, linked by the platform's separator, or joined on one.
    #[props(default)]
    pub caps: KbdCaps,
    /// What the caps' colour says. `ink`, the default, is the ink of the place the chord sits in, so a cap reads in a menu row, on the page and in a dark tooltip alike; a named tone or a colour of the caller's marks a chord as more than a hint.
    #[props(into)]
    pub tone: Tone = Tone::Ink,
    /// How the caps carry their colour.
    pub variant: Variant = Variant::Soft,
    /// Amends the paint of every cap — the chord's **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`KbdTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// How the keys of one step of a chord are put on caps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, telar::PreviewArg)]
pub enum KbdCaps {
    /// A cap per key: `[Ctrl] [Shift] [P]`.
    #[default]
    Each,
    /// A cap per key, with what the platform writes between keys standing between the caps: `[Ctrl] + [Shift] + [P]`. A Mac writes nothing there, so its caps stand as [`Each`](Self::Each) sets them.
    Linked,
    /// One cap per step, its keys as the platform writes them, spaced round the separator where it has one: `[Ctrl + Shift + P]`, `[⇧⌘P]`.
    Joined,
}

/// The caps of each step of `chord`, the way the platform names its keys, each with the separator standing before it, if any. Text that names no [`Chord`] is one cap, as written.
fn shown_steps(chord: &str, caps: KbdCaps) -> Vec<Vec<(Option<&'static str>, String)>> {
    let convention = keyboard_convention();
    let separator = Some(convention.separator()).filter(|separator| !separator.is_empty());
    let spaced = separator.map_or_else(String::new, |separator| format!(" {separator} "));
    let alone = |cap: String| vec![(None, cap)];
    match shared::chord_steps(chord) {
        Some(steps) => steps
            .iter()
            .map(|step| match (step, caps) {
                (ChordStep::Chord(chord), KbdCaps::Joined) => {
                    alone(chord.keys(convention).join(&spaced))
                }
                (ChordStep::Chord(chord), KbdCaps::Each) => {
                    chord.keys(convention).into_iter().map(|key| (None, key)).collect()
                }
                (ChordStep::Chord(chord), KbdCaps::Linked) => chord
                    .keys(convention)
                    .into_iter()
                    .enumerate()
                    .map(|(at, key)| (separator.filter(|_| at > 0), key))
                    .collect(),
                (ChordStep::Written(text), _) => alone(text.clone()),
            })
            .collect(),
        None if chord.trim().is_empty() => Vec::new(),
        None => vec![alone(chord.trim().to_string())],
    }
}

let size = props.size;
let chord = props.chord.clone();
let look = Look::shown(props.tone.clone(), props.variant);
let caps = props.caps;
let steps = memo(move || shown_steps(&chord.get(), caps).into_iter().enumerate().collect::<Vec<_>>());

[view]
row role:(Role::KeyboardInput) a11y_label:(shown_chord(&$props.chord))
    // The steps of a sequence stand as far apart as two controls do, well clear of the keys inside one.
    row align:center gap:(shared::gap_md(size))
        for (step, keys) in $steps key (*step, keys.clone())
            row align:center gap:(shared::gap_xs(size))
                for (separator, key) in keys.iter()
                    if let Some(separator) = separator
                        text "{separator}" font_size:(size.text_scale() * shared::CAPTION_RATIO)em text_wrap:nowrap
                    // Never narrower than it is tall, so a one-letter key is a square.
                    row align:center justify:center min_width:(1lh + shared::gap_xs(size) * 2.0) pad_x:(shared::gap_sm(size)) pad_y:(shared::gap_xs(size)) fill:(look.fill($text.ink)) stroke:(look.frame($text.ink)) color:(look.ink($text.ink)) stroke_width:(KbdTokens::current().border_width()) radius:(KbdTokens::current().radius()) font_size:(size.text_scale() * shared::CAPTION_RATIO)em surface:$props.style
                        text "{key}" text_wrap:nowrap
