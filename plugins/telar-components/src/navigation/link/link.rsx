[logic]
use std::rc::Rc;

use crate::look::Look;
use crate::shared::{self, GlyphProps, Mark, glyph};
use crate::strings;
use telar::{ControlSize, Destination, IntoDestination, Tone, Variant};

/// A link's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role named beside it, read when the link paints.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LinkTokens {
    /// Corner of the wash a pointer resting on the link lays behind it. `None` is the theme's `radius_item`.
    pub radius: Option<f32>,
}

impl LinkTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_item)
    }
}

/// Where a [`link`] goes, re-read whenever what it reads changes: anything a box's `to:` takes — a typed route, a [`Location`](telar::Location), an [`anchor`](telar::anchor) or an [`external`](telar::external) address — or an `Option` of one, `None` being nowhere to go right now.
#[derive(Clone)]
pub struct LinkTarget(Rc<dyn Fn() -> Option<Destination>>);

impl LinkTarget {
    /// A destination worked out from what `read` reads, so the link follows a signal.
    pub fn of<D: IntoDestination>(read: impl Fn() -> D + 'static) -> Self {
        Self(Rc::new(move || read().into_destination()))
    }

    fn read(&self) -> Option<Destination> {
        (self.0)()
    }

    fn leaves(&self) -> bool {
        matches!(self.read(), Some(Destination::External(_)))
    }
}

impl<D: IntoDestination + Clone + 'static> From<D> for LinkTarget {
    fn from(destination: D) -> Self {
        Self::of(move || destination.clone())
    }
}

/// Words that go somewhere when pressed, or when Enter is pressed on them: a route is pushed onto the application's history, an anchor is scrolled to, and an address outside the application is handed to the system.
///
/// Drawn in its tone, the theme's accent unless the call names one, made readable on the surface, at the size of the text around it and always underlined, so it is told from the words around it by more than its colour. One that leaves the application says so twice: a drawn arrow after it, and a reader hears it named an external link. Ctrl, Cmd or Shift on the press ask for a view beside this one where there is one, as on a web page.
///
/// It is one box sized to its words: for a link inside a wrapping paragraph, write the paragraph as [`Text::runs`](telar::Text::runs) with a [`TextRun::to`](telar::TextRun::to) run instead.
pub struct Props {
    /// What the link says.
    #[props(into)]
    pub label: Reactive<String>,
    /// Where it goes.
    #[props(into)]
    pub to: LinkTarget,
    /// Marks the link as the current one of its set — the page being shown, in a navigation's links — so it is drawn in the theme's strong weight and announced as current. Off by default.
    #[props(into, default)]
    pub current: Reactive<bool>,
    /// What the link's colour says: a named tone, or a colour of the caller's, taken just far enough toward the ink to read on the theme's surface. `ink` is the ink of the text around it, for a link told apart by its underline alone.
    #[props(into, default)]
    pub tone: Tone,
    /// What sits before the words, in the link's ink: an `icon`.
    #[props(default)]
    pub leading: Children,
    /// What sits after the words, in the link's ink, before the arrow of a link that leaves the application.
    #[props(default)]
    pub trailing: Children,
    /// Runs when the link is pressed, before it is followed.
    #[props(some, default)]
    pub on_press: Option<Rc<dyn Fn()>>,
    /// Amends the paint of the link's box — its **principal surface** — at rest and hovered. See [`telar::SurfaceStyle`] for when a theme token or [`LinkTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}


fn heard(label: String, leaves: bool) -> String {
    match leaves {
        true => strings::text_with(strings::EXTERNAL_LINK, &[("label", &label)]),
        false => label,
    }
}

let to = props.to.clone();
let look = Look {
    tone: props.tone.clone(),
    variant: Variant::Ghost,
    selected: None,
    interactive: true,
};
let on_press = props.on_press.clone().map(|run| move || run());

[view]
row align:center radius:(LinkTokens::current().radius()) color:(look.ink($text.ink)) text_wrap:nowrap hover_style(fill:(look.hovered($text.ink)) radius:(LinkTokens::current().radius())) to:to.read() current:$props.current on_press:on_press a11y_label:(heard($props.label, to.leaves())) surface:$props.style
    row align:center gap:(shared::gap_sm(ControlSize::Regular))
        children $props.leading
        if $props.current
            text "{$props.label}" underline font_weight:$theme.font_weight_strong
        else
            text "{$props.label}" underline
        children $props.trailing
    if to.leaves()
        row align:center pad_start:(shared::gap_sm(ControlSize::Regular))
            glyph shape:Mark::External
