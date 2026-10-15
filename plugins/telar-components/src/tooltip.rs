//! [`tooltip`]: a bubble anchored to its trigger while the pointer is over it.

use telar::{
    AlignItems, BorderRadius, Children, Color, Container, JustifyContent, LayoutError, LayoutItem,
    LayoutStyle, Overlay, Placement, Props, Reactive, ReactiveList, RectStyle, ShapeStyle,
    StyledContainer, SurfaceAmendment, SurfaceStyle, Text, TextStyle, TextWrap, amend_surface,
    box_item, children, signal, use_theme_tokens,
};

#[cfg(test)]
use telar::Slots;

use crate::shared::{self, shown_chord};

/// Leading for the description line, the only one that wraps (`leading-snug`).
const DESCRIPTION_LEADING: f32 = 1.375;
/// How many ems wide a bubble may get before its text wraps, when its tokens say nothing. A hint is read at a glance, and a sentence stretched across the window is not: about a dozen words is as much as a hint should ever say.
const MAX_WIDTH_EMS: f32 = 17.0;

/// A tooltip's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role named beside it, read when the bubble is laid out or painted. Its colours are the theme's `inverse_surface` and `inverse_ink`, which every transient label shares.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TooltipTokens {
    /// Corner of the bubble. `None` is the theme's `radius_surface`.
    pub radius: Option<f32>,
    /// Space between the bubble's sides and its text at the regular control size, scaled with the control size in force. `None` is [`metrics::gap_md`](crate::metrics::gap_md).
    pub pad_x: Option<f32>,
    /// Space above and below the text, likewise. `None` is [`metrics::gap_sm`](crate::metrics::gap_sm).
    pub pad_y: Option<f32>,
    /// How wide the bubble may get before its text wraps, likewise. `None` is 17 ems of the document's text.
    pub max_width: Option<f32>,
}

impl TooltipTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_surface)
    }

    pub fn pad_x(&self) -> f32 {
        self.pad_x
            .map_or_else(shared::gap_md, |pad| pad * telar::control_scale())
    }

    pub fn pad_y(&self) -> f32 {
        self.pad_y
            .map_or_else(shared::gap_sm, |pad| pad * telar::control_scale())
    }

    pub fn max_width(&self) -> f32 {
        shared::width_or_measure(self.max_width, MAX_WIDTH_EMS)
    }
}

/// A bubble's share of the text around whatever it is describing. The name line takes it whole; the shortcut and the sentence step down from there, because a bubble that says three things has to rank them.
const BUBBLE_RATIO: f32 = 0.85;
const SHORTCUT_RATIO: f32 = BUBBLE_RATIO * 0.9;
const DESCRIPTION_RATIO: f32 = BUBBLE_RATIO * 0.92;

/// A hover popup: wraps its slot (the trigger content) and, while the mouse is over it, shows a small `text` bubble anchored just below the trigger. Built on the `overlay` primitive's anchored variant (the bubble is portalled to the top layer and translated to the trigger's rect, so it escapes clipping and only itself blocks). High-level sugar; lives in `telar-components`, not the kernel.
#[derive(Props)]
pub struct TooltipProps {
    #[props(into, default)]
    pub text: Reactive<String>,
    /// The binding that does the same thing, written portably as [`telar::Chord`] reads it (`"Mod+S"`) and shown as the platform writes it (`⌘S` on a Mac, `Ctrl+S` elsewhere), pushed to the far side of the first line. Empty means none.
    ///
    /// A hint that names its own shortcut is how a keyboard gets learned — the pointer finds the control, and the bubble says which key would have got there first. Separate from `text` because it is *placed*, not worded: folded into the sentence it wraps with it and stops lining up down a toolbar.
    #[props(into, default)]
    pub shortcut: Reactive<String>,
    /// A sentence under the name, saying what the control does rather than what it is called. Empty means none, which is the right shape for a control whose name already says everything.
    #[props(into, default)]
    pub description: Reactive<String>,
    /// Which side of the trigger the bubble takes: `"bottom"` (the default), `"top"`, `"start"`/`"left"` or `"end"`/`"right"`. It still flips when that side has no room.
    #[props(default = "")]
    pub side: &'static str,
    /// Bubble surface colour, its text inked in whichever of the theme's inks reads on it. `Color::TRANSPARENT` (the default) is the theme's `inverse_surface`, under its `inverse_ink`. A closure (re-read every frame) so a theme token or `$signal` colour re-colours live.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Amends the paint of the bubble — this component's **principal surface**, the thing a caller means when they point at a tooltip. See [`telar::SurfaceStyle`] for when a theme token or [`TooltipTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
    /// Let the trigger take the space its parent offers instead of hugging its content.
    ///
    /// The wrapper the tooltip puts around the trigger is a real node in the parent's flow, so without this a tooltipped child cannot be a `flex-1` cell: wrapping it collapses the row it was sharing. Set on a tab, a toolbar segment, or anything else whose whole point is to divide the space evenly.
    #[props(default = false)]
    pub stretch: bool,
}

fn placement_of(side: &str) -> Placement {
    match side {
        "top" | "above" => Placement::Above,
        "start" | "left" => Placement::Start,
        "end" | "right" => Placement::End,
        _ => Placement::Below,
    }
}

/// A bubble anchored to its trigger while the pointer is over it.
pub fn tooltip(
    props: TooltipProps,
    children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut slots = children.build()?;
    let TooltipProps {
        text,
        shortcut,
        description,
        side,
        color,
        style,
        stretch,
    } = props;
    let placement = placement_of(side);
    let shortcut = Reactive::of(move || shown_chord(&shortcut.get()));
    let trigger_content = slots.take_default();
    let hovered = signal(false);

    let mut trigger_style = LayoutStyle::row();
    if stretch {
        trigger_style = trigger_style.flex_grow(1.0).align_self_stretch();
    }
    let hover_sink = hovered;
    let trigger = StyledContainer::new(trigger_style, |_r| RectStyle::default(), trigger_content)?
        .on_hover(move |over| hover_sink.set(over));
    let trigger_node = trigger.layout_node();

    // The bubble is a fresh `text` each hover, with no slot children to preserve, so keying on `hovered` mounts and disposes the anchored overlay like a reactive `if`. Both closures are re-erased to `Rc` so each remount can clone them into a fresh bubble.
    let style: SurfaceStyle = style;
    let key_hovered = hovered;
    let bubble = ReactiveList::new(
        move || vec![key_hovered.get()],
        |is_hovered: &bool| *is_hovered,
        move |is_hovered| -> Result<Box<dyn LayoutItem>, LayoutError> {
            if !is_hovered {
                return Ok(box_item(Container::new(
                    LayoutStyle::new().width(0.0).height(0.0),
                    vec![],
                )?));
            }
            build_bubble(
                Content {
                    text: text.clone(),
                    shortcut: shortcut.clone(),
                    description: description.clone(),
                },
                color.clone(),
                style.clone(),
                placement,
                trigger_node,
            )
        },
        0.0,
    )?;

    // The bubble node is a 0-size portal placeholder, so it never shifts the trigger. `stretch` has to reach this root as well: the root is what the parent lays out, so growing only the inner node would leave the pair hugging its content.
    let mut root_style = LayoutStyle::column();
    if stretch {
        root_style = root_style.flex_grow(1.0).align_self_stretch();
    }
    Ok(box_item(Container::new(
        root_style,
        children![trigger, bubble],
    )?))
}

/// Builds the bubble for the hovered state: a padded rounded chip with the tooltip `text`, anchored to the trigger where it is drawn, inside a NON-blocking overlay (a tooltip must not eat clicks on the page).
fn build_bubble(
    content: Content,
    color: Reactive<Color>,
    style: SurfaceStyle,
    placement: Placement,
    trigger_node: telar::NodeId,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    // Laid out at the origin and translated to the trigger by the anchored overlay, rather than pushed there with a left margin: a margin eats the width the bubble had to lay out in, so a trigger near the right edge left it a 50px column. The translate happens after layout, when the real size is known, which is also what lets it flip and slide to stay on screen.
    let bubble = move || {
        let tokens = TooltipTokens::current();
        LayoutStyle::column()
            .max_width(tokens.max_width())
            .padding_horizontal(tokens.pad_x())
            .padding_vertical(tokens.pad_y())
    };
    let ink = color.clone();
    let chip = StyledContainer::new(
        bubble(),
        move |_r| {
            amend_surface(
                RectStyle::default()
                    .with_fill(bubble_fill(&color))
                    .with_radius(BorderRadius::all(TooltipTokens::current().radius())),
                &style,
            )
        },
        content.rows(ink)?,
    )?
    .styled_by(bubble);
    // A flex row, for the chip to hug: a `LayoutStyle::new()` is a CSS block, and a block child fills its containing block whatever `align_items` says — so every bubble came out at its 240px cap. Click-through, because a tooltip must not eat clicks on the page it is describing.
    let overlay = Overlay::anchored_click_through(
        LayoutStyle::row().align_items(AlignItems::START),
        children![chip],
        trigger_node,
        placement,
    )?;
    Ok(box_item(overlay))
}

/// What the bubble says, in the one shape every hint in an application takes.
struct Content {
    text: Reactive<String>,
    shortcut: Reactive<String>,
    description: Reactive<String>,
}

impl Content {
    /// The name line, with its shortcut pushed to the far edge, over an optional sentence.
    ///
    /// The two optional parts are mounted reactively rather than decided here, because the strings are closures: a hint whose shortcut arrives with a signal would otherwise be built once, empty, and stay that way.
    fn rows(self, color: Reactive<Color>) -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {
        let Content {
            text,
            shortcut,
            description,
        } = self;
        let line = move |ratio: f32, strength: f32| {
            let color = color.clone();
            move |t: TextStyle| bubble_text(t, ratio, line_ink(&color, strength))
        };
        let name = Text::declaring(move || text.get(), LayoutStyle::new(), {
            let line = line(BUBBLE_RATIO, 1.0);
            move |t| line(t).with_text_wrap(TextWrap::NoWrap)
        })?;
        let key = optional_line(shortcut, {
            let line = line(SHORTCUT_RATIO, 0.6);
            move |t| line(t).with_text_wrap(TextWrap::NoWrap)
        })?;
        // `SPACE_BETWEEN` rather than a growing spacer: a spacer wants all the width there is, so the bubble took its 240px maximum whatever it said. This way the row stretches to the column and the key lands on its edge.
        let title_box = || {
            LayoutStyle::row()
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::SPACE_BETWEEN)
                .align_self_stretch()
                .gap(shared::gap_lg())
        };
        let title = Container::new(title_box(), children![name, key])?.styled_by(title_box);
        // The only line in a bubble that wraps, so the only one whose leading is set: at the shaper's default 1.2 a two-line hint reads as squashed text rather than short leading.
        let body = optional_line(description, {
            let line = line(DESCRIPTION_RATIO, 0.72);
            move |t| line(t).with_line_height(DESCRIPTION_LEADING)
        })?;
        Ok(children![title, body])
    }
}

/// A line that is there only while its text is non-empty. Zero-sized otherwise, so the bubble keeps the height of what it actually says.
fn optional_line(
    text: Reactive<String>,
    style: impl Fn(TextStyle) -> TextStyle + Clone + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let present = text.clone();
    Ok(box_item(ReactiveList::new(
        move || vec![!present.get().is_empty()],
        |shown: &bool| *shown,
        move |shown| -> Result<Box<dyn LayoutItem>, LayoutError> {
            if !shown {
                return Ok(box_item(Container::new(
                    LayoutStyle::new().width(0.0).height(0.0),
                    vec![],
                )?));
            }
            let text = text.clone();
            Ok(box_item(Text::declaring(
                move || text.get(),
                LayoutStyle::new(),
                style.clone(),
            )?))
        },
        0.0,
    )?))
}

/// The bubble's surface: the theme's inverse surface, or the colour the caller chose.
fn bubble_fill(color: &Reactive<Color>) -> Color {
    shared::resolve(color, || use_theme_tokens().inverse_surface())
}

/// The ink that reads on [`bubble_fill`]: the theme's inverse ink on its inverse surface, and on a colour the caller chose whichever ink reads on that.
fn bubble_ink(color: &Reactive<Color>) -> Color {
    let chosen = color.get();
    if chosen == Color::TRANSPARENT {
        use_theme_tokens().inverse_ink()
    } else {
        shared::ink_on(chosen)
    }
}

/// The ink of a line that ranks below the name: [`bubble_ink`] at `strength` over the bubble, taken back toward full strength only as far as it has to go to read on the bubble at 4.5:1. A shortcut faded to a fixed strength fell under that on the dark theme's light bubble.
fn line_ink(color: &Reactive<Color>, strength: f32) -> Color {
    let fill = bubble_fill(color);
    let faded = shared::over(bubble_ink(color).with_alpha(strength), fill);
    shared::readable_on(faded, fill)
}

/// A bubble line: sized from the text around the control it describes, inked against the chip it is drawn on rather than against the page.
///
/// The ink is the one thing here that does not inherit, and deliberately: the bubble paints its own inverse surface, so a page that declared dark text would hand this line dark-on-dark. Size still follows the region — a hint in a compact panel is a hint at that panel's scale.
fn bubble_text(inherited: TextStyle, ratio: f32, ink: Color) -> TextStyle {
    shared::control_text(inherited, ratio).with_color(ink)
}

#[cfg(test)]
#[path = "tooltip_test.rs"]
mod tests;
