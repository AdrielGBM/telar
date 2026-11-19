//! [`tooltip`]: a bubble anchored to its trigger while the pointer is over it or the keyboard is on it.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use telar::{
    AlignItems, BorderRadius, Children, Color, Container, DismissRegistration, JustifyContent,
    KeySequence, LayoutError, LayoutItem, LayoutStyle, NodeId, Overlay, Props, Reactive,
    ReactiveList, RectStyle, RwSignal, ShapeStyle, Side, StyledContainer, SurfaceAmendment,
    SurfaceStyle, Text, TextStyle, TextWrap, Timer, amend_surface, box_item, children, effect,
    focus, on_cleanup, pointer_position, run_after, signal, use_theme_tokens, visible_rect,
};

use crate::shared::{self, shown_chord};
use telar::ControlSize;

/// How long the bubble stays while the pointer stands in the gap between it and the trigger: long enough to cross it without the bubble closing under the pointer, short enough that a pointer that stops there does not hold it open.
const LINGER: Duration = Duration::from_millis(150);

/// Leading for the description line, the only one that wraps (`leading-snug`).
const DESCRIPTION_LEADING: f32 = 1.375;
/// How many ems wide a bubble may get before its text wraps, when its tokens say nothing. A hint is read at a glance, and a sentence stretched across the window is not: about a dozen words is as much as a hint should ever say.
const MAX_WIDTH_EMS: f32 = 17.0;

/// A tooltip's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role named beside it, read when the bubble is laid out or painted. Its colours are the theme's `inverse_surface` and `inverse_ink`, which every transient label shares.
#[derive(Clone, Copy, Debug, Default, PartialEq, telar::ComponentTokens)]
pub struct TooltipTokens {
    /// Corner of the bubble. `None` is the theme's `radius_surface`.
    #[token(or = shared::radius_surface)]
    pub radius: Option<f32>,
    /// Space between the bubble's sides and its text at the regular size, scaled with the tooltip's own. `None` is [`metrics::gap_md`](crate::metrics::gap_md).
    #[token(or = shared::gap_md, scaled)]
    pub pad_x: Option<f32>,
    /// Space above and below the text, likewise. `None` is [`metrics::gap_sm`](crate::metrics::gap_sm).
    #[token(or = shared::gap_sm, scaled)]
    pub pad_y: Option<f32>,
    /// How wide the bubble may get before its text wraps, likewise. `None` is 17 ems of the document's text.
    pub max_width: Option<f32>,
}

impl TooltipTokens {
    /// How wide the bubble of a tooltip of `size` may get before its text wraps.
    pub fn max_width(&self, size: ControlSize) -> f32 {
        shared::width_or_measure(size, self.max_width, MAX_WIDTH_EMS)
    }
}

/// A bubble's share of the text around whatever it is describing. The name line takes it whole; the shortcut and the sentence step down from there, because a bubble that says three things has to rank them.
const BUBBLE_RATIO: f32 = 0.85;
const SHORTCUT_RATIO: f32 = BUBBLE_RATIO * 0.9;
const DESCRIPTION_RATIO: f32 = BUBBLE_RATIO * 0.92;

/// A hint: wraps its `children` (the trigger) and shows a small `text` bubble anchored beside it while the pointer is over the trigger or the bubble, and while the keyboard is on a control inside the trigger. Escape hides it without moving focus, and the keyboard moving to another control hides one the pointer raised, so the hint of the control just reached is not drawn over by it; either way it comes back once the pointer and the keyboard have left and return. What it says is the description a reader hears on the controls inside the trigger. Built on the `overlay` primitive's anchored variant (the bubble is portalled to the top layer and translated to the trigger's rect, so it escapes clipping and only itself blocks). High-level sugar; lives in `telar-components`, not the kernel.
#[derive(Props)]
pub struct TooltipProps {
    #[props(into, default)]
    pub text: Reactive<String>,
    /// Whether `text` is already what a reader calls the trigger, as on a control that is only a picture named by the same words, so only [`description`](Self::description) is said as the trigger's description. Off by default: the trigger is described by `text`, then by `description`.
    #[props(default = false)]
    pub names_trigger: bool,
    /// The binding that does the same thing, written portably as [`KeySequence`] reads it (`"Mod+S"`, `"Mod+K S"`) and shown as the platform writes it (`⌘S` on a Mac, `Ctrl+S` elsewhere), pushed to the far side of the first line. `None` (the default) names none.
    ///
    /// A hint that names its own shortcut is how a keyboard gets learned — the pointer finds the control, and the bubble says which key would have got there first. Separate from `text` because it is *placed*, not worded: folded into the sentence it wraps with it and stops lining up down a toolbar.
    #[props(some, into, default)]
    pub shortcut: Option<KeySequence>,
    /// A sentence under the name, saying what the control does rather than what it is called. `None` (the default) is the right shape for a control whose name already says everything.
    #[props(some, into, default)]
    pub description: Option<Reactive<String>>,
    /// Which side of the trigger the bubble takes, [`Side::Below`] by default; [`Side::Start`] and [`Side::End`] follow the way the surface reads. It still flips when that side has no room.
    #[props(default = Side::Below)]
    pub side: Side,
    /// How large the bubble is drawn: its padding, text and the width it wraps at, as [`ControlSize`] works them out. The trigger is the caller's, sized where it is written.
    #[props(default)]
    pub size: ControlSize,
    /// Bubble surface colour, its text inked in whichever of the theme's inks reads on it. `None` (the default) is the theme's `inverse_surface`, under its `inverse_ink`. A closure (re-read every frame) so a theme token or `$signal` colour re-colours live.
    #[props(some, into, default)]
    pub color: Option<Reactive<Color>>,
    /// Amends the paint of the bubble — this component's **principal surface**, the thing a caller means when they point at a tooltip. See [`telar::SurfaceStyle`] for when a theme token or [`TooltipTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
    /// The trigger the bubble describes.
    #[props(default)]
    pub children: Children,
}

/// A bubble anchored to its trigger while the pointer is over either or the keyboard is on the trigger, describing the controls inside the trigger to a reader.
pub fn tooltip(props: TooltipProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let TooltipProps {
        text,
        names_trigger,
        shortcut,
        description,
        side,
        size,
        color,
        style,
        children,
    } = props;
    let said = Rc::new(described_as(
        text.clone(),
        description.clone(),
        names_trigger,
    ));
    let shortcut = shortcut.map(|shortcut| Reactive::of(move || shown_chord(&shortcut)));
    let trigger_content = children.build()?;

    let trigger = StyledContainer::new(
        LayoutStyle::row(),
        |_r| RectStyle::default(),
        trigger_content,
    )?;
    let trigger_node = trigger.layout_node();
    let pointer = Pointer::new(trigger_node);
    let trigger = trigger.on_hover({
        let pointer = pointer.clone();
        move |over| pointer.over_trigger(over)
    });
    for control in focus::controls_within(trigger_node) {
        let said = said.clone();
        focus::set_description(control, move || said());
    }

    let near = pointer.near;
    let keyboard_on = move || focus::shows_focus_within(trigger_node);
    let dismissed = signal(false);
    effect(move || {
        if !near.get() && !keyboard_on() && dismissed.peek() {
            dismissed.set(false);
        }
    });
    // Follows focus only while the pointer is near: every hint on a page following it woke them all at each Tab.
    let last_focus = Cell::new(None);
    effect(move || {
        if !near.get() {
            last_focus.set(None);
            return;
        }
        let now = focus::current();
        let Some(before) = last_focus.replace(Some(now)) else {
            return;
        };
        let moved_elsewhere =
            now != before && now.is_some_and(focus::is_focus_visible) && !keyboard_on();
        if moved_elsewhere {
            dismissed.set(true);
        }
    });
    {
        let pointer = pointer.clone();
        on_cleanup(move || pointer.forget());
    }

    // The bubble is a fresh `text` each time it shows, with no children of its own to preserve, so keying on whether it shows mounts and disposes the anchored overlay like a reactive `if`.
    let style: SurfaceStyle = style;
    let bubble = ReactiveList::new(
        move || vec![!dismissed.get() && (near.get() || keyboard_on())],
        |showing: &bool| *showing,
        move |showing| -> Result<Box<dyn LayoutItem>, LayoutError> {
            if !showing {
                return Ok(box_item(Container::new(
                    LayoutStyle::new().width(0.0).height(0.0),
                    vec![],
                )?));
            }
            let dismissal = DismissRegistration::within(trigger_node, move || dismissed.set(true));
            let left = pointer.clone();
            on_cleanup(move || {
                drop(dismissal);
                left.bubble.set(None);
                left.over_bubble(false);
            });
            build_bubble(
                size,
                Content {
                    text: text.clone(),
                    shortcut: shortcut.clone(),
                    description: description.clone(),
                },
                color.clone(),
                style.clone(),
                side,
                trigger_node,
                pointer.clone(),
            )
        },
        0.0,
    )?;

    // The bubble node is a 0-size portal placeholder, so it never shifts the trigger.
    Ok(box_item(Container::new(
        LayoutStyle::column(),
        children![trigger, bubble],
    )?))
}

/// What a reader hears after the trigger's name: `text` unless it already is that name, then `description`, a line each. `None` for a hint that names its trigger and has no `description`.
fn described_as(
    text: Reactive<String>,
    description: Option<Reactive<String>>,
    names_trigger: bool,
) -> impl Fn() -> Option<String> {
    move || {
        let lines: Vec<String> = (!names_trigger)
            .then(|| text.get())
            .into_iter()
            .chain(description.as_ref().map(Reactive::get))
            .collect();
        (!lines.is_empty()).then(|| lines.join("\n"))
    }
}

/// Whether the pointer is near the hint: over the trigger or the bubble, or crossing the space between the two for less than [`LINGER`].
struct Pointer {
    trigger: NodeId,
    bubble: Cell<Option<NodeId>>,
    over_trigger: Cell<bool>,
    over_bubble: Cell<bool>,
    near: RwSignal<bool>,
    linger: Cell<Option<Timer>>,
    gone: Cell<bool>,
}

impl Pointer {
    fn new(trigger: NodeId) -> Rc<Self> {
        Rc::new(Self {
            trigger,
            bubble: Cell::new(None),
            over_trigger: Cell::new(false),
            over_bubble: Cell::new(false),
            near: signal(false),
            linger: Cell::new(None),
            gone: Cell::new(false),
        })
    }

    fn over_trigger(&self, over: bool) {
        self.over_trigger.set(over);
        self.settle();
    }

    fn over_bubble(&self, over: bool) {
        self.over_bubble.set(over);
        self.settle();
    }

    fn settle(&self) {
        if self.gone.get() {
            return;
        }
        let over = self.over_trigger.get() || self.over_bubble.get();
        if over || !self.crossing() {
            self.linger.take();
            if self.near.peek() != over {
                self.near.set(over);
            }
        } else if self.near.peek() {
            let near = self.near;
            self.linger
                .set(Some(run_after(LINGER, move || near.set(false))));
        }
    }

    /// Whether the pointer stands between the trigger and the bubble, off both: where it passes on its way from one to the other. One that left the surface is still where it last moved, inside what it left, so it is not crossing.
    fn crossing(&self) -> bool {
        let rects = (
            visible_rect(self.trigger),
            self.bubble.get().and_then(visible_rect),
        );
        let (Some((x, y)), (Some(trigger), Some(bubble))) = (pointer_position(), rects) else {
            return false;
        };
        trigger.union(bubble).contains(x, y) && !trigger.contains(x, y) && !bubble.contains(x, y)
    }

    fn forget(&self) {
        self.gone.set(true);
        self.linger.take();
    }
}

/// Builds the bubble for the showing state: a padded rounded chip with the tooltip `text`, anchored to the trigger where it is drawn, inside a NON-blocking overlay (a tooltip must not eat clicks on the page). The chip tells `pointer` when the pointer is over it, so moving onto the bubble keeps it open.
fn build_bubble(
    size: ControlSize,
    content: Content,
    color: Option<Reactive<Color>>,
    style: SurfaceStyle,
    side: Side,
    trigger_node: telar::NodeId,
    pointer: Rc<Pointer>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    // Laid out at the origin and translated to the trigger by the anchored overlay, rather than pushed there with a left margin: a margin eats the width the bubble had to lay out in, so a trigger near the right edge left it a 50px column. The translate happens after layout, when the real size is known, which is also what lets it flip and slide to stay on screen.
    let bubble = move || {
        let tokens = TooltipTokens::current();
        LayoutStyle::column()
            .max_width(tokens.max_width(size))
            .padding_horizontal(tokens.pad_x(size))
            .padding_vertical(tokens.pad_y(size))
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
        content.rows(size, ink)?,
    )?
    .styled_by(bubble);
    pointer.bubble.set(Some(chip.layout_node()));
    let chip = chip.on_hover(move |over| pointer.over_bubble(over));
    // A flex row, for the chip to hug: a `LayoutStyle::new()` is a CSS block, and a block child fills its containing block whatever `align_items` says — so every bubble came out at its 240px cap. Click-through, because a tooltip must not eat clicks on the page it is describing.
    let overlay = Overlay::anchored_click_through(
        LayoutStyle::row().align_items(AlignItems::START),
        children![chip],
        trigger_node,
        side,
    )?;
    Ok(box_item(overlay))
}

/// What the bubble says, in the one shape every hint in an application takes.
struct Content {
    text: Reactive<String>,
    shortcut: Option<Reactive<String>>,
    description: Option<Reactive<String>>,
}

impl Content {
    /// The name line, with its shortcut pushed to the far edge, over an optional sentence.
    fn rows(
        self,
        size: ControlSize,
        color: Option<Reactive<Color>>,
    ) -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {
        let Content {
            text,
            shortcut,
            description,
        } = self;
        let line = move |ratio: f32, strength: f32| {
            let color = color.clone();
            move |t: TextStyle| bubble_text(size, t, ratio, line_ink(&color, strength))
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
        let title_box = move || {
            LayoutStyle::row()
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::SPACE_BETWEEN)
                .align_self_stretch()
                .gap(shared::gap_lg(size))
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

/// A line where there is `text`, and a box of no size where there is none, so the bubble keeps the height of what it actually says.
fn optional_line(
    text: Option<Reactive<String>>,
    style: impl Fn(TextStyle) -> TextStyle + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    match text {
        Some(text) => Ok(box_item(Text::declaring(
            move || text.get(),
            LayoutStyle::new(),
            style,
        )?)),
        None => Ok(box_item(Container::new(
            LayoutStyle::new().width(0.0).height(0.0),
            vec![],
        )?)),
    }
}

/// The bubble's surface: the theme's inverse surface, or the colour the caller chose.
fn bubble_fill(color: &Option<Reactive<Color>>) -> Color {
    shared::resolve(color, || use_theme_tokens().inverse_surface())
}

/// The ink that reads on [`bubble_fill`]: the theme's inverse ink on its inverse surface, and on a colour the caller chose whichever ink reads on that.
fn bubble_ink(color: &Option<Reactive<Color>>) -> Color {
    match color {
        Some(chosen) => shared::ink_on(chosen.get()),
        None => use_theme_tokens().inverse_ink(),
    }
}

/// The ink of a line that ranks below the name: [`bubble_ink`] at `strength` over the bubble, taken back toward full strength only as far as it has to go to read on the bubble at 4.5:1. A shortcut faded to a fixed strength fell under that on the dark theme's light bubble.
fn line_ink(color: &Option<Reactive<Color>>, strength: f32) -> Color {
    let fill = bubble_fill(color);
    let faded = bubble_ink(color).with_alpha(strength).over(fill);
    shared::readable_on(faded, fill)
}

/// A bubble line: sized from the text around the control it describes, inked against the chip it is drawn on rather than against the page.
///
/// The ink is the one thing here that does not inherit, and deliberately: the bubble paints its own inverse surface, so a page that declared dark text would hand this line dark-on-dark. Size still follows the region — a hint in a compact panel is a hint at that panel's scale.
fn bubble_text(size: ControlSize, inherited: TextStyle, ratio: f32, ink: Color) -> TextStyle {
    shared::control_text(size, inherited, ratio).with_color(ink)
}

#[cfg(test)]
#[path = "tooltip.test.rs"]
mod tests;
