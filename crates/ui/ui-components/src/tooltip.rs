use std::rc::Rc;
use telar_macros::Props;

use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use reactive_core::{Reactive, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, TextStyle, TextWrap};
#[cfg(test)]
use ui_core::Slots;
use ui_core::{
    Children, Container, LayoutItem, Overlay, Placement, ReactiveList, StyledContainer, Text,
    box_item, track_layout,
};

use crate::shared;

/// Fallback bubble surface when `color` is unset — an opaque dark chip.
const DEFAULT_BUBBLE: Color = Color::rgba(0.12, 0.12, 0.16, 0.96);
/// Bubble text colour (always light, on the dark chip).
const BUBBLE_INK: Color = Color::rgba(0.98, 0.98, 1.0, 1.0);
/// How wide a bubble may get before its text wraps. A hint is read at a glance, and a sentence stretched
/// across the window is not: 240px is about a dozen words, which is as much as a hint should ever say.
const BUBBLE_MAX_WIDTH: f32 = 240.0;
/// Leading for the description line, the only one that wraps (`leading-snug`).
const DESCRIPTION_LEADING: f32 = 1.375;
/// A bubble is the smallest surface in the app and a corner is read against the size it turns, so it takes
/// the middle step of the theme's radius scale, and moves when a theme moves its base radius. It was one and
/// a half times the *base* radius, half as round again as the button that
/// opened it, and a literal here besides: a theme could change how round everything was and this would not
/// have moved.
fn bubble_radius() -> f32 {
    shared::radius_md()
}
fn bubble_pad_x() -> f32 {
    shared::spacing()
}
fn bubble_pad_y() -> f32 {
    shared::spacing() * 0.6
}
/// A bubble's share of the text around whatever it is describing. The name line takes it whole; the shortcut
/// and the sentence step down from there, because a bubble that says three things has to rank them.
const BUBBLE_RATIO: f32 = 0.85;
const SHORTCUT_RATIO: f32 = BUBBLE_RATIO * 0.9;
const DESCRIPTION_RATIO: f32 = BUBBLE_RATIO * 0.92;

/// A hover popup: wraps its slot (the trigger content) and, while the mouse is over it, shows a small `text`
/// bubble anchored just below the trigger. Built on the `overlay` primitive's anchored variant (the bubble is
/// portalled to the top layer and translated to the trigger's rect, so it escapes clipping and only itself
/// blocks). High-level sugar; lives in `ui-components`, not the kernel.
#[derive(Props)]
pub struct TooltipProps {
    #[props(into, default)]
    pub text: Reactive<String>,
    /// The binding that does the same thing, pushed to the far side of the first line. Empty means none.
    ///
    /// A hint that names its own shortcut is how a keyboard gets learned — the pointer finds the control, and
    /// the bubble says which key would have got there first. Separate from `text` because it is *placed*, not
    /// worded: folded into the sentence it wraps with it and stops lining up down a toolbar.
    #[props(into, default)]
    pub shortcut: Reactive<String>,
    /// A sentence under the name, saying what the control does rather than what it is called. Empty means
    /// none, which is the right shape for a control whose name already says everything.
    #[props(into, default)]
    pub description: Reactive<String>,
    /// Which side of the trigger the bubble takes: `"bottom"` (the default), `"top"`, `"start"`/`"left"` or
    /// `"end"`/`"right"`. It still flips when that side has no room.
    #[props(default = "")]
    pub side: &'static str,
    /// Bubble surface colour. `Color::TRANSPARENT` (the default) means "unset" -> `DEFAULT_BUBBLE`. A closure
    /// (re-read every frame) so a theme token or `$signal` colour re-colours live.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Amends the paint of the bubble — this component's **principal surface**, the thing a caller means when
    /// they point at a tooltip. See [`shared::SurfaceStyle`] for why it takes the finished style rather than
    /// naming one property, and for when a theme token is the right instrument instead.
    #[props(some, default)]
    pub style: Option<Rc<dyn Fn(RectStyle) -> RectStyle>>,
    /// Let the trigger take the space its parent offers instead of hugging its content.
    ///
    /// The wrapper the tooltip puts around the trigger is a real node in the parent's flow, so without this
    /// a tooltipped child cannot be a `flex-1` cell: wrapping it collapses the row it was sharing. Set on a
    /// tab, a toolbar segment, or anything else whose whole point is to divide the space evenly.
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
    let trigger_content = slots.take_default();
    // Reflects whether the mouse is over the trigger; drives the bubble's show/hide.
    let hovered = signal(false);

    let mut trigger_style = LayoutStyle::new().flex_row();
    if stretch {
        trigger_style = trigger_style.flex_grow(1.0).align_self_stretch();
    }
    let hover_sink = hovered;
    let trigger = StyledContainer::new(trigger_style, |_r| RectStyle::default(), trigger_content)?
        .on_hover(move |over| hover_sink.set(over));
    let trigger_node = trigger.layout_node();
    // The trigger's laid-out rect (a fresh runtime handle, not the borrowed `ctx`): the anchored overlay
    // reads it to position the bubble below the trigger.
    let trigger_rect = track_layout(trigger_node).expect("trigger container is registered");

    // The bubble is a fresh `text` each hover (rebuildable — no slot children to preserve), so no take-once
    // cell here; keying on `hovered` mounts/disposes the anchored overlay like a reactive `if`. Both `text`
    // and `color` are re-erased to `Rc` so each remount can clone them into a fresh bubble.
    let style: shared::SurfaceStyle =
        style.map(|f| -> Rc<dyn Fn(RectStyle) -> RectStyle> { Rc::from(f) });
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
                trigger_rect,
            )
        },
        0.0,
    )?;

    // The trigger sits in flow; the bubble node is a 0-size portal placeholder, so it never shifts the trigger.
    // `stretch` has to reach this root as well as the trigger — the root is what the parent actually lays out, so growing only the inner node would leave the pair hugging its content anyway.
    let mut root_style = LayoutStyle::new().flex_column();
    if stretch {
        root_style = root_style.flex_grow(1.0).align_self_stretch();
    }
    Ok(box_item(Container::new(
        root_style,
        vec![box_item(trigger), box_item(bubble)],
    )?))
}

/// Builds the bubble for the hovered state: a padded rounded chip with the tooltip `text`, positioned just
/// below the trigger (via `absolute_rect`, so it works even when the trigger is in a separately-computed
/// sub-root) inside a NON-blocking overlay (a tooltip must not eat clicks on the page).
fn build_bubble(
    content: Content,
    color: Reactive<Color>,
    style: shared::SurfaceStyle,
    placement: Placement,
    trigger_node: ui_core::NodeId,
    trigger_rect: reactive_core::RwSignal<geometry_core::Rect>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let _ = trigger_node;
    // Laid out at the origin and *translated* to the trigger by the anchored overlay, rather than pushed
    // there with a left margin: a margin eats the width the bubble had to lay out in, so a trigger near the
    // right edge left it a 50px column and the text wrapped one word per line. The translate happens after
    // layout, when the bubble's real size is known — which is also what lets it flip and slide to stay on
    // screen (see `anchor_translate`).
    let bubble = move || {
        LayoutStyle::new()
            .flex_column()
            .max_width(BUBBLE_MAX_WIDTH)
            .padding_horizontal(bubble_pad_x())
            .padding_vertical(bubble_pad_y())
    };
    let chip = StyledContainer::new(
        bubble(),
        move |_r| {
            shared::amend(
                RectStyle::default()
                    .with_fill(shared::resolve(&color, || DEFAULT_BUBBLE))
                    .with_radius(BorderRadius::all(bubble_radius())),
                &style,
            )
        },
        content.rows()?,
    )?
    .styled_by(bubble);
    // The content layer has to be a FLEX row for the chip to hug: a `LayoutStyle::new()` is a CSS block, and
    // a block child fills its containing block whatever `align_items` says — so every bubble came out at its
    // 240px cap however little it had to say, and the `align_items(START)` here was doing nothing at all.
    // Click-through because a tooltip must not eat clicks on the page it is describing.
    let overlay = Overlay::anchored_click_through(
        LayoutStyle::new().flex_row().align_items(AlignItems::START),
        vec![box_item(chip)],
        trigger_rect,
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
    /// The two optional parts are mounted reactively rather than decided here, because the strings are
    /// closures: a hint whose shortcut arrives with a signal would otherwise be built once, empty, and stay
    /// that way.
    fn rows(self) -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {
        let Content {
            text,
            shortcut,
            description,
        } = self;
        let name = Text::declaring(
            move || text.get(),
            LayoutStyle::new(),
            |t| bubble_text(t, BUBBLE_RATIO, 1.0).with_text_wrap(TextWrap::NoWrap),
        )?;
        let key = optional_line(shortcut, |t| {
            bubble_text(t, SHORTCUT_RATIO, 0.6).with_text_wrap(TextWrap::NoWrap)
        })?;
        // The shortcut is pushed apart with `SPACE_BETWEEN` and not with a growing spacer: a spacer wants all
        // the width there is, so the bubble took its 240px maximum whatever it said. This way the row
        // stretches to the column — which is as wide as the longest line — and the key lands on that edge.
        let title = Container::new(
            LayoutStyle::new()
                .flex_row()
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::SPACE_BETWEEN)
                .align_self_stretch()
                .gap(bubble_pad_x() * 1.5),
            vec![box_item(name), box_item(key)],
        )?;
        // The only line in a bubble that wraps, so the only one whose leading is set: at the shaper's default 1.2 a two-line hint reads as squashed *text* rather than as short leading.
        let body = optional_line(description, |t| {
            bubble_text(t, DESCRIPTION_RATIO, 0.72).with_line_height(DESCRIPTION_LEADING)
        })?;
        Ok(vec![box_item(title), box_item(body)])
    }
}

/// A line that is there only while its text is non-empty. Zero-sized otherwise, so the bubble keeps the
/// height of what it actually says.
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

/// A bubble line: sized from the text around the control it describes, inked against the chip it is drawn on
/// rather than against the page.
///
/// The ink is the one thing here that does not inherit, and deliberately: the bubble paints its own dark
/// surface, so a page that declared black text would hand this line black-on-black. Size still follows the
/// region — a hint in a compact panel is a hint at that panel's scale.
fn bubble_text(inherited: TextStyle, ratio: f32, strength: f32) -> TextStyle {
    shared::control_text(inherited, ratio).with_color(BUBBLE_INK.with_alpha(strength))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::moved;
    use layout_core::AvailableSpace;
    use renderer_core::LineHeight;

    use renderer_core::DrawCommand;
    use ui_core::{ComponentList, compute_layout, new_container, relayout_if_dirty};

    fn find_text(cmds: &[DrawCommand], needle: &str) -> bool {
        cmds.iter()
            .any(|c| matches!(c, DrawCommand::Text { text, .. } if text.as_ref() == needle))
    }

    // A trigger slot child with a definite size so the trigger has a rect to hover and anchor against.
    fn slot_with_trigger() -> Slots {
        let inner = Container::new(LayoutStyle::new().width(80.0).height(30.0), vec![]).unwrap();
        let mut slots = Slots::new();
        slots.push(None, box_item(inner));
        slots
    }

    /// A hint is sized from the text it is describing, not from the theme: a compact panel that says its
    /// region is 11px gets a bubble at that panel's scale, and `TooltipProps` has no size to correct it with.
    #[test]
    fn a_bubble_takes_the_size_the_region_around_it_declared() {
        crate::test_support::fresh_layout_runtime();
        let tooltip = tooltip(
            TooltipProps::props().text("Move").build(),
            Children::from(slot_with_trigger()),
        )
        .unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            &[tooltip.layout_node()],
        )
        .unwrap();
        ui_core::declare(
            root,
            renderer_core::Declared::default().with_font_size(11.0),
        );
        compute_layout(
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();

        let mut tree = ComponentList::new(tooltip);
        let _ = tree.commands();
        tree.on_event(&moved(40.0, 15.0));
        relayout_if_dirty();

        let size = tree
            .commands()
            .iter()
            .find_map(|c| match c {
                DrawCommand::Text { text, style, .. } if text.as_ref() == "Move" => {
                    Some(style.font_size)
                }
                _ => None,
            })
            .expect("the bubble drew its name");
        assert_eq!(size, 11.0 * BUBBLE_RATIO);
    }

    // Hovering the trigger shows the bubble; leaving hides it. Driven through the full component tree, like
    // a real app: a mouse move onto the trigger fires its on_hover, mounting the anchored overlay.
    #[test]
    fn hover_shows_bubble_and_leave_hides_it() {
        crate::test_support::fresh_layout_runtime();
        let slots = slot_with_trigger();
        let tooltip = tooltip(
            TooltipProps::props().text("Helpful hint").build(),
            Children::from(slots),
        )
        .unwrap();

        // A parent-less root computed against the window registers the overlay host the bubble anchors into.
        let root = new_container(
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            &[tooltip.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut tree = ComponentList::new(tooltip);
        let _ = tree.commands();

        assert!(!find_text(&tree.commands(), "Helpful hint"));

        // Move onto the trigger (its rect is ~0,0,80,30): on_hover(true) mounts the bubble.
        tree.on_event(&moved(40.0, 15.0));
        relayout_if_dirty();
        assert!(
            find_text(&tree.commands(), "Helpful hint"),
            "bubble shows on hover"
        );

        // Move far away: on_hover(false) disposes the bubble.
        tree.on_event(&moved(9999.0, 9999.0));
        relayout_if_dirty();
        assert!(
            !find_text(&tree.commands(), "Helpful hint"),
            "bubble hidden on leave"
        );
    }

    /// The shape every hint in an application takes: the name, its binding pushed to the far edge, and a
    /// sentence under both. All three have to reach the bubble, because the reason the shortcut is a prop of
    /// its own is that folding it into the name loses exactly this arrangement.
    #[test]
    fn a_hint_shows_its_name_its_shortcut_and_its_description() {
        crate::test_support::fresh_layout_runtime();
        let tooltip = tooltip(
            TooltipProps::props()
                .text("Move")
                .shortcut("G")
                .description("Drag the selection along the ground")
                .build(),
            Children::from(slot_with_trigger()),
        )
        .unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            &[tooltip.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut tree = ComponentList::new(tooltip);
        tree.on_event(&moved(40.0, 15.0));
        relayout_if_dirty();

        let cmds = tree.commands();
        assert!(find_text(&cmds, "Move"), "the name");
        assert!(find_text(&cmds, "G"), "its binding");
        assert!(
            find_text(&cmds, "Drag the selection along the ground"),
            "and what it does"
        );
    }

    /// The description line is set with room to breathe, because it is the only part of a bubble that wraps.
    ///
    /// At the default 1.2 the two lines of a long hint sit almost on top of each other, and it reads as the
    /// *text* being squashed rather than as the leading being short — which is why it only showed up on the
    /// hints long enough to need a second line, and looked like a placement bug rather than a type one.
    /// The looser leading is set on this line and on no other: the name and the shortcut never wrap.
    #[test]
    fn the_description_line_is_set_with_room_to_wrap_into() {
        crate::test_support::fresh_layout_runtime();
        let tooltip = tooltip(
            TooltipProps::props()
                .text("Setup")
                .description("Name regions and say what it is made of")
                .build(),
            Children::from(slot_with_trigger()),
        )
        .unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            &[tooltip.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut tree = ComponentList::new(tooltip);
        tree.on_event(&moved(40.0, 15.0));
        relayout_if_dirty();

        // Asserted on the style, not on the drawn height: whether this sentence needs a second line depends on the system font, so measuring it tests the CI runner's font and not the leading.
        let leading_of = |needle: &str| {
            tree.commands()
                .iter()
                .find_map(|c| match c {
                    DrawCommand::Text { text, style, .. } if text.starts_with(needle) => {
                        Some(style.line_height)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("`{needle}` is drawn"))
        };
        assert_eq!(
            leading_of("Name regions"),
            LineHeight::Times(DESCRIPTION_LEADING)
        );
        assert_eq!(
            leading_of("Setup"),
            LineHeight::Natural,
            "the name never wraps"
        );
    }

    /// A bubble is as wide as what it says, up to its cap. It was taking the cap whatever it had to say,
    /// because the layer its chip sits in was a `LayoutStyle::new()` — a CSS **block**, where a child fills
    /// its containing block and `align_items` means nothing. A two-word hint in a 240px box does not read as
    /// a hint, and nothing about it looks like a layout mode being wrong.
    #[test]
    fn a_bubble_is_as_wide_as_what_it_says() {
        crate::test_support::fresh_layout_runtime();
        let tooltip = tooltip(
            TooltipProps::props()
                .text("Object")
                .shortcut("1")
                .description("Pick whole bodies")
                .build(),
            Children::from(slot_with_trigger()),
        )
        .unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            &[tooltip.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut tree = ComponentList::new(tooltip);
        tree.on_event(&moved(40.0, 15.0));
        relayout_if_dirty();

        let widest = tree
            .commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Text { rect, .. } => Some(rect.width),
                _ => None,
            })
            .fold(0.0f32, f32::max);
        assert!(
            widest > 0.0 && widest < BUBBLE_MAX_WIDTH * 0.75,
            "the bubble hugged its text instead of taking its cap, got {widest}"
        );
    }

    /// A hint with nothing but a name is the common case, and it must not pay for the parts it left out with
    /// a taller bubble than it has words for.
    #[test]
    fn an_absent_shortcut_and_description_take_no_room() {
        crate::test_support::fresh_layout_runtime();
        let height_of = |props: TooltipProps| {
            let tooltip = tooltip(props, Children::from(slot_with_trigger())).unwrap();
            let root = new_container(
                LayoutStyle::new().flex_column().width(400.0).height(400.0),
                &[tooltip.layout_node()],
            )
            .unwrap();
            compute_layout(
                root,
                AvailableSpace::Definite(400.0),
                AvailableSpace::Definite(400.0),
            )
            .unwrap();
            let mut tree = ComponentList::new(tooltip);
            tree.on_event(&moved(40.0, 15.0));
            relayout_if_dirty();
            tree.commands()
                .iter()
                .filter_map(|c| match c {
                    DrawCommand::Text { rect, .. } => Some(rect.height),
                    _ => None,
                })
                .sum::<f32>()
        };
        let bare = height_of(TooltipProps::props().text("Move").build());
        crate::test_support::fresh_layout_runtime();
        let full = height_of(
            TooltipProps::props()
                .text("Move")
                .description("Drag the selection")
                .build(),
        );
        assert!(bare > 0.0, "the name is drawn either way");
        assert!(
            full > bare,
            "a described hint is taller than a bare one: {bare} vs {full}"
        );
    }

    // Construction succeeds headless with an empty trigger and no hover.
    #[test]
    fn builds_without_hover() {
        crate::test_support::fresh_layout_runtime();
        let slots = slot_with_trigger();
        let tooltip = tooltip(TooltipProps::props().build(), Children::from(slots)).unwrap();
        let tree = ComponentList::new(tooltip);
        assert!(!find_text(&tree.commands(), "Helpful hint"));
    }
}
