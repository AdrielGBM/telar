use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use geometry_core::{Rect, Transform};
use layout_core::{
    AlignItems, AvailableSpace, JustifyContent, LayoutError, LayoutStyle, NodeId, SizeDimension,
};
use motion_core::{Animated, Easing, tween};
use platform_core::{Event, Key, NamedKey, PointerButton};
use reactive_core::RwSignal;
use renderer_core::{Color, RectStyle, TextStyle};
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::{compute_layout, mark_dirty, new_container, track_layout};
use crate::layout_item::{LayoutItem, box_item};
use crate::styled_container::StyledContainer;
use crate::text::Text;

/// What kind of secondary surface a placement describes. A backend maps the role to its own surface
/// primitives (a layer-shell backend picks a layer + namespace; a windowed backend a child window or an
/// in-window portal). Roles carry no behaviour of their own — the explicit [`SurfacePlacement`] fields do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceRole {
    /// A panel that slides off a bar/edge, dimming what's behind it.
    Drawer,
    /// A transient, positioned popup (a notification, a menu detached from its trigger).
    Popup,
    /// A brief, non-interactive status flash (volume/brightness), auto-dismissed.
    Osd,
    /// A free-floating window with its own title/close affordances.
    Float,
    /// A modal that owns the screen while it is up: a launcher, a command palette, a session menu. Unlike a
    /// [`Drawer`](Self::Drawer) it isn't anchored to an edge, and unlike a [`Float`](Self::Float) it expects to
    /// take the keyboard outright — the user is typing into it, not at whatever is behind it.
    Overlay,
}

/// How much of the keyboard a surface needs.
///
/// The distinction matters because it decides who receives a keystroke *before* any click. A panel with a text
/// field can wait to be clicked into ([`OnDemand`](Self::OnDemand)); a launcher cannot — it opens on a keybind
/// and the next keystroke is already its first search character, so it has to hold the keyboard from the moment
/// it maps ([`Exclusive`](Self::Exclusive)). Asking for more than is needed is not free: a surface holding the
/// keyboard takes it from the focused window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyboardMode {
    /// Display-only; never takes keyboard focus.
    #[default]
    None,
    /// May be given focus on interaction, e.g. a click into a text field.
    OnDemand,
    /// Holds the keyboard for as long as it is mapped.
    Exclusive,
}

/// The screen edge (or centre) a surface hugs. The cross axis is aligned by [`SurfaceAlign`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceAnchor {
    Top,
    Bottom,
    Left,
    Right,
    Center,
}

/// Cross-axis alignment along the anchored edge (e.g. left/centre/right for a top-anchored surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceAlign {
    Start,
    Center,
    End,
}

/// A surface's size: a fixed logical pixel box, or derived from its content.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SurfaceSize {
    Fixed(u32, u32),
    Auto,
}

/// A backend-agnostic description of a secondary surface: where it sits, how big it is, and how it
/// behaves (scrim, outside-dismiss, auto-timeout). The intent lives here; a backend derives its own
/// surface config from it. Reusable by a windowed app (as an in-window portal) and by a shell (as a real
/// layer-shell surface) alike.
#[derive(Debug, Clone)]
pub struct SurfacePlacement {
    pub role: SurfaceRole,
    pub anchor: SurfaceAnchor,
    pub align: SurfaceAlign,
    pub size: SurfaceSize,
    /// Gap from the screen edges, as `(top, right, bottom, left)`. A full-screen scrim scaffold applies the
    /// whole tuple as padding (so the panel floats off every edge, not just the anchored one); a
    /// directly-anchored surface applies it as the compositor margin.
    pub margin: (i32, i32, i32, i32),
    /// Dim (and, with `dismiss_on_outside`, capture) the area behind the panel.
    pub scrim: bool,
    /// A press outside the panel dismisses the surface.
    pub dismiss_on_outside: bool,
    /// Auto-dismiss after this long; `None` keeps it until closed explicitly.
    pub timeout: Option<Duration>,
    /// The surface passes pointer input through to whatever is beneath it (a click-through OSD).
    pub input_transparent: bool,
    /// How much of the keyboard the surface needs; a backend maps this to its own focus model (e.g. layer-shell
    /// keyboard interactivity). Defaults to [`KeyboardMode::None`], so a panel is display-only and never steals
    /// the keyboard.
    pub keyboard: KeyboardMode,
    /// The monitor to place the surface on by name; `None` = the active/default output.
    pub output: Option<String>,
}

impl SurfacePlacement {
    pub fn new(role: SurfaceRole, anchor: SurfaceAnchor) -> Self {
        Self {
            role,
            anchor,
            align: SurfaceAlign::Center,
            size: SurfaceSize::Auto,
            margin: (0, 0, 0, 0),
            scrim: false,
            dismiss_on_outside: false,
            timeout: None,
            input_transparent: false,
            keyboard: KeyboardMode::None,
            output: None,
        }
    }

    /// A modal that owns the screen: centred, scrimmed, dismissed by a press outside, and holding the keyboard
    /// from the moment it maps so the first keystroke after the keybind is already typed into it.
    pub fn overlay() -> Self {
        Self {
            scrim: true,
            dismiss_on_outside: true,
            keyboard: KeyboardMode::Exclusive,
            ..Self::new(SurfaceRole::Overlay, SurfaceAnchor::Center)
        }
    }

    pub fn drawer(anchor: SurfaceAnchor) -> Self {
        Self {
            scrim: true,
            dismiss_on_outside: true,
            ..Self::new(SurfaceRole::Drawer, anchor)
        }
    }

    pub fn osd() -> Self {
        Self {
            input_transparent: true,
            ..Self::new(SurfaceRole::Osd, SurfaceAnchor::Top)
        }
    }

    pub fn float() -> Self {
        Self::new(SurfaceRole::Float, SurfaceAnchor::Center)
    }

    pub fn align(mut self, align: SurfaceAlign) -> Self {
        self.align = align;
        self
    }

    pub fn size(mut self, size: SurfaceSize) -> Self {
        self.size = size;
        self
    }

    pub fn margin(mut self, margin: (i32, i32, i32, i32)) -> Self {
        self.margin = margin;
        self
    }

    pub fn inset(mut self, px: i32) -> Self {
        let (t, r, b, l) = self.margin;
        self.margin = match self.anchor {
            SurfaceAnchor::Top => (px, r, b, l),
            SurfaceAnchor::Bottom => (t, r, px, l),
            SurfaceAnchor::Left => (t, r, b, px),
            SurfaceAnchor::Right => (t, px, b, l),
            SurfaceAnchor::Center => (t, r, b, l),
        };
        self
    }

    pub fn scrim(mut self, scrim: bool) -> Self {
        self.scrim = scrim;
        self
    }

    pub fn dismiss_on_outside(mut self, dismiss: bool) -> Self {
        self.dismiss_on_outside = dismiss;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn input_transparent(mut self, transparent: bool) -> Self {
        self.input_transparent = transparent;
        self
    }

    /// Opt the surface into focus-on-interaction, for panels that host editable text (a search box, a note
    /// title). Sugar for [`keyboard_mode`](Self::keyboard_mode) with
    /// [`OnDemand`](KeyboardMode::OnDemand)/[`None`](KeyboardMode::None).
    pub fn keyboard(mut self, wants_keyboard: bool) -> Self {
        self.keyboard = if wants_keyboard {
            KeyboardMode::OnDemand
        } else {
            KeyboardMode::None
        };
        self
    }

    /// Sets exactly how much of the keyboard the surface takes.
    pub fn keyboard_mode(mut self, mode: KeyboardMode) -> Self {
        self.keyboard = mode;
        self
    }

    /// Whether the surface takes keyboard focus at all.
    pub fn wants_keyboard(&self) -> bool {
        self.keyboard != KeyboardMode::None
    }

    pub fn output(mut self, output: Option<String>) -> Self {
        self.output = output;
        self
    }

    /// Whether the surface needs a full-viewport scaffold (to draw a scrim or catch outside presses)
    /// rather than being anchored directly at its content size.
    pub fn needs_scaffold(&self) -> bool {
        self.scrim || self.dismiss_on_outside
    }
}

/// The default scrim wash: ~35 % black over the content behind a drawer/modal. Rendered as a fill (not an
/// opacity layer) so the panel above it stays fully opaque.
pub const DEFAULT_SCRIM: Color = Color::rgba(0.0, 0.0, 0.0, 0.35);

fn cross_align(align: SurfaceAlign) -> AlignItems {
    match align {
        SurfaceAlign::Start => AlignItems::START,
        SurfaceAlign::Center => AlignItems::CENTER,
        SurfaceAlign::End => AlignItems::END,
    }
}

/// Enter-animation duration and slide travel.
const ENTER_MS: u64 = 200;
const SLIDE_DISTANCE: f32 = 24.0;
const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

#[derive(Clone, Copy)]
enum EnterMotion {
    Slide(SurfaceAnchor),
    Fade,
}

/// The transform matrix and opacity for an enter animation at `progress` (0 = just opened, 1 = settled).
/// A slide starts `SLIDE_DISTANCE` off its edge and eases to rest; both forms fade in.
fn enter_transform(motion: EnterMotion, progress: f32) -> ([f32; 6], f32) {
    let p = progress.clamp(0.0, 1.0);
    let opacity = p;
    match motion {
        EnterMotion::Fade => (IDENTITY, opacity),
        EnterMotion::Slide(anchor) => {
            let d = SLIDE_DISTANCE * (1.0 - p);
            let (dx, dy) = match anchor {
                SurfaceAnchor::Top => (0.0, -d),
                SurfaceAnchor::Bottom => (0.0, d),
                SurfaceAnchor::Left => (-d, 0.0),
                SurfaceAnchor::Right => (d, 0.0),
                SurfaceAnchor::Center => (0.0, 0.0),
            };
            (Transform::translate(dx, dy).to_array(), opacity)
        }
    }
}

fn apply_enter(node: RenderNode, matrix: [f32; 6], opacity: f32) -> RenderNode {
    let faded = if opacity < 1.0 {
        RenderNode::layer(opacity, 0.0, [node])
    } else {
        node
    };
    if matrix == IDENTITY {
        faded
    } else {
        RenderNode::transform_with(matrix, [faded])
    }
}

/// The one progress value a surface's arrival and departure share: 0 is off its edge and transparent, 1 is
/// settled. Opening runs it to 1; [`leave`](Self::leave) runs it back to 0, so the exit is the entrance
/// reversed rather than a second animation that has to be kept in step with the first.
///
/// A backend that can hold a closing surface on screen for [`duration`](Self::duration) is what makes the exit
/// half visible; one that cannot simply never calls `leave`, and the surface disappears as it always did.
#[derive(Clone)]
pub struct SurfaceTransition {
    progress: Animated<f32>,
    duration: Duration,
}

impl SurfaceTransition {
    /// A transition already on its way in. Constructed away from its goal and retargeted at once, never *at*
    /// it: an `Animated` born settled registers with no ticker, so nothing would schedule the frames that carry
    /// it in.
    pub fn enter() -> Self {
        let duration = Duration::from_millis(ENTER_MS);
        let progress = Animated::new(0.0, tween(duration, Easing::EaseOut));
        progress.retarget(1.0);
        Self { progress, duration }
    }

    /// Sends the surface back the way it came. The caller is responsible for keeping it on screen for
    /// [`duration`](Self::duration) — otherwise this animates a surface that has already been torn down.
    pub fn leave(&self) {
        self.progress.retarget(0.0);
    }

    /// How long either half takes, and therefore how long a closing surface has to stay mapped.
    pub fn duration(&self) -> Duration {
        self.duration
    }

    fn get(&self) -> f32 {
        self.progress.get()
    }
}

/// A full-viewport scaffold that positions a panel against a screen edge, optionally dims the area behind
/// it, and dismisses on a press outside the panel. It is the reusable body of a drawer/modal: a shell
/// mounts it as the root of a full-screen layer-shell surface, and a windowed app can mount it in-tree as
/// an in-window portal — both get the same positioning and dismiss behaviour.
///
/// Like other full-surface roots it (re)computes its own layout on `WindowResized`; the runner synthesizes
/// an initial one on resume, so the scaffold is laid out before its first frame.
pub struct SurfaceScaffold {
    root: NodeId,
    panel_rect: Option<RwSignal<Rect>>,
    root_rect: Option<RwSignal<Rect>>,
    content: Box<dyn LayoutItem>,
    scrim: Option<Color>,
    dismiss: Option<Rc<dyn Fn()>>,
    anchor: SurfaceAnchor,
    transition: Option<SurfaceTransition>,
}

impl SurfaceScaffold {
    pub fn new(
        placement: &SurfacePlacement,
        content: Box<dyn LayoutItem>,
        dismiss: Option<Rc<dyn Fn()>>,
    ) -> Result<Self, LayoutError> {
        let panel_node = content.layout_node();
        let (mt, mr, mb, ml) = placement.margin;
        let cross = cross_align(placement.align);
        // The full margin becomes padding so the panel floats off every screen edge, not just the anchored one;
        // the per-anchor direction/justify then pins it to its edge within that padded box.
        let base = LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0))
            .padding_top(mt as f32)
            .padding_right(mr as f32)
            .padding_bottom(mb as f32)
            .padding_left(ml as f32);
        let style = match placement.anchor {
            SurfaceAnchor::Top => base
                .flex_column()
                .justify_content(JustifyContent::START)
                .align_items(cross),
            SurfaceAnchor::Bottom => base
                .flex_column()
                .justify_content(JustifyContent::END)
                .align_items(cross),
            SurfaceAnchor::Left => base
                .flex_row()
                .justify_content(JustifyContent::START)
                .align_items(cross),
            SurfaceAnchor::Right => base
                .flex_row()
                .justify_content(JustifyContent::END)
                .align_items(cross),
            SurfaceAnchor::Center => base
                .flex_column()
                .justify_content(JustifyContent::CENTER)
                .align_items(AlignItems::CENTER),
        };
        let root = new_container(style, &[panel_node])?;
        let panel_rect = track_layout(panel_node);
        let root_rect = track_layout(root);
        let dismiss = if placement.dismiss_on_outside {
            dismiss
        } else {
            None
        };
        Ok(Self {
            root,
            panel_rect,
            root_rect,
            content,
            scrim: placement.scrim.then_some(DEFAULT_SCRIM),
            dismiss,
            anchor: placement.anchor,
            transition: None,
        })
    }

    pub fn animate_in(self) -> Self {
        self.animate(SurfaceTransition::enter())
    }

    /// Drives the scaffold from a transition the *caller* owns, so it can also send the surface back out — see
    /// [`SurfaceTransition::leave`].
    pub fn animate(mut self, transition: SurfaceTransition) -> Self {
        self.transition = Some(transition);
        self
    }
}

impl LayoutItem for SurfaceScaffold {
    fn layout_node(&self) -> NodeId {
        self.root
    }
}

impl Component for SurfaceScaffold {
    fn view(&self) -> RenderNode {
        let content = self.content.view();
        let (matrix, opacity) = match &self.transition {
            Some(transition) => enter_transform(EnterMotion::Slide(self.anchor), transition.get()),
            None => (IDENTITY, 1.0),
        };
        let panel = apply_enter(content, matrix, opacity);
        match (self.scrim, self.root_rect.as_ref()) {
            (Some(color), Some(rect)) => {
                let scrim = apply_enter(
                    RenderNode::rect(rect.get(), RectStyle::filled(color, 0.0)),
                    IDENTITY,
                    opacity,
                );
                RenderNode::group([scrim, panel])
            }
            _ => panel,
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::WindowResized { width, height } => {
                mark_dirty(self.root).ok();
                compute_layout(
                    self.root,
                    AvailableSpace::Definite(*width as f32),
                    AvailableSpace::Definite(*height as f32),
                )
                .ok();
                EventResult::Handled
            }
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
                ..
            } => {
                let inside = self
                    .panel_rect
                    .as_ref()
                    .map(|r| r.get().contains(*x as f32, *y as f32))
                    .unwrap_or(false);
                match (inside, &self.dismiss) {
                    (false, Some(dismiss)) => {
                        dismiss();
                        EventResult::Handled
                    }
                    _ => self.content.on_event(event),
                }
            }
            // Escape is the keyboard's version of a press outside, so a surface that answers to one answers to
            // the other. Same rule as the in-window dismiss stack (see `dispatch_overlays`): the content gets
            // first refusal, so a focused field blurs on the first press and the surface closes on the second,
            // and backing out of an armed confirmation never takes the surface with it.
            Event::KeyPressed {
                key: Key::Named(NamedKey::Escape),
                ..
            } => match (self.content.on_event(event), &self.dismiss) {
                (EventResult::Ignored, Some(dismiss)) => {
                    dismiss();
                    EventResult::Handled
                }
                (result, _) => result,
            },
            _ => self.content.on_event(event),
        }
    }

    fn debug_name(&self) -> &'static str {
        "SurfaceScaffold"
    }
}

pub struct SurfaceRoot {
    root: NodeId,
    content: Box<dyn LayoutItem>,
    transition: Option<SurfaceTransition>,
}

impl SurfaceRoot {
    pub fn new(content: Box<dyn LayoutItem>) -> Result<Self, LayoutError> {
        let root = new_container(
            LayoutStyle::new()
                .flex_row()
                .width(SizeDimension::Percent(1.0))
                .height(SizeDimension::Percent(1.0)),
            &[content.layout_node()],
        )?;
        Ok(Self {
            root,
            content,
            transition: None,
        })
    }

    pub fn animate_in(self) -> Self {
        self.animate(SurfaceTransition::enter())
    }

    /// Drives the root from a transition the *caller* owns, so it can also send the surface back out — see
    /// [`SurfaceTransition::leave`].
    pub fn animate(mut self, transition: SurfaceTransition) -> Self {
        self.transition = Some(transition);
        self
    }
}

impl LayoutItem for SurfaceRoot {
    fn layout_node(&self) -> NodeId {
        self.root
    }
}

impl Component for SurfaceRoot {
    fn view(&self) -> RenderNode {
        let content = self.content.view();
        match &self.transition {
            Some(transition) => {
                let (_, opacity) = enter_transform(EnterMotion::Fade, transition.get());
                apply_enter(content, IDENTITY, opacity)
            }
            None => content,
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if let Event::WindowResized { width, height } = event {
            mark_dirty(self.root).ok();
            compute_layout(
                self.root,
                AvailableSpace::Definite(*width as f32),
                AvailableSpace::Definite(*height as f32),
            )
            .ok();
            return EventResult::Handled;
        }
        self.content.on_event(event)
    }

    fn debug_name(&self) -> &'static str {
        "SurfaceRoot"
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SurfaceFrameStyle {
    pub background: Color,
    pub title_bar: Color,
    pub title_text: Color,
    pub close: Color,
    pub radius: f32,
    pub font_size: f32,
}

/// The smallest a frame will ask to become. A window dragged to nothing is a window the user cannot get hold of
/// again — its own grip goes with it.
pub const MIN_FRAME_SIZE: (f32, f32) = (180.0, 120.0);

/// The corner grip's side, in logical pixels. Big enough to hit without aiming, small enough not to read as
/// content.
const GRIP_SIZE: f32 = 14.0;

/// The rect a grip measures the frame against. It is a cell rather than a signal because the grip has to exist
/// before the row that holds it, and that row before the card that holds *both* — so the one rect the grip needs
/// is the one thing it cannot be handed at construction. Filled in as soon as the card exists.
type DeferredRect = Rc<RefCell<Option<RwSignal<Rect>>>>;

/// A resize grip for the bottom-right corner of a frame, reporting the size the *surface* should become.
///
/// The arithmetic is the whole of it. `on_drag` reports where the pointer is **inside the grip**, so the grip's
/// own laid-out origin has to be added back to reach surface space — and then the grab offset, the distance from
/// the pointer to the corner when the drag began, has to come off it, or the corner jumps to the cursor the
/// instant it is touched. The offset is latched once per drag rather than recomputed, because the card it was
/// measured against is resizing underneath the gesture.
fn resize_grip(
    color: Color,
    card_rect: DeferredRect,
    resize: Rc<dyn Fn(f32, f32)>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let grip = StyledContainer::new(
        LayoutStyle::new().width(GRIP_SIZE).height(GRIP_SIZE),
        move |_| RectStyle::filled(color, 2.0),
        vec![],
    )?;
    let grip_rect = track_layout(grip.layout_node());
    let grab: Rc<Cell<Option<(f32, f32)>>> = Rc::new(Cell::new(None));
    let release = Rc::clone(&grab);
    Ok(box_item(
        grip.on_drag(move |local_x, local_y| {
            let (Some(grip_rect), Some(card_rect)) = (&grip_rect, card_rect.borrow().clone())
            else {
                return;
            };
            let (grip, card) = (grip_rect.get(), card_rect.get());
            let (x, y) = (grip.x + local_x, grip.y + local_y);
            let (offset_x, offset_y) = match grab.get() {
                Some(offset) => offset,
                None => {
                    let offset = (x - (card.x + card.width), y - (card.y + card.height));
                    grab.set(Some(offset));
                    offset
                }
            };
            resize(
                (x - offset_x - card.x).max(MIN_FRAME_SIZE.0),
                (y - offset_y - card.y).max(MIN_FRAME_SIZE.1),
            );
        })
        .on_drag_end(move |_, _| release.set(None)),
    ))
}

/// A titled, closable window frame around `body`.
///
/// `resize` opts the frame into a corner grip: it is handed the size the surface should take, in logical
/// pixels, on every move of that grip. A backend that can renegotiate a surface's size wires it up; one that
/// cannot passes `None` and the grip is not drawn, rather than drawn and inert.
pub fn surface_frame(
    title: impl Into<String>,
    style: SurfaceFrameStyle,
    close: std::rc::Rc<dyn Fn()>,
    body: Box<dyn LayoutItem>,
    resize: Option<Rc<dyn Fn(f32, f32)>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let title = title.into();
    let title_color = style.title_text;
    let font_size = style.font_size;
    let title_label = box_item(Text::auto(
        move || title.clone(),
        LayoutStyle::new(),
        move || TextStyle::new(font_size, title_color),
    )?);

    let close_color = style.close;
    let close_label = box_item(Text::auto(
        || "\u{2715}".to_string(),
        LayoutStyle::new(),
        move || TextStyle::new(font_size, close_color),
    )?);
    let close_button = box_item(
        StyledContainer::new(
            LayoutStyle::new()
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::CENTER)
                .padding_horizontal(8.0)
                .padding_vertical(2.0),
            |_| RectStyle::default(),
            vec![close_label],
        )?
        .on_press(move || close()),
    );

    let title_bar_color = style.title_bar;
    let title_bar = box_item(StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::SPACE_BETWEEN)
            .width(SizeDimension::Percent(1.0))
            .padding_horizontal(12.0)
            .padding_vertical(8.0),
        move |_| RectStyle::filled(title_bar_color, 0.0),
        vec![title_label, close_button],
    )?);

    // A flex item may not shrink below its content unless you say so, and an application body sized to fill the window (the settings page area is a scroll leaf with a definite height) otherwise refuses to give up a single pixel and pushes the grip row off the bottom of the surface — a resize affordance that exists, lays out, and is never on screen.
    let body_area = box_item(StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .flex_grow(1.0)
            .min_height(0.0)
            .width(SizeDimension::Percent(1.0))
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .padding_all(12.0),
        |_| RectStyle::default(),
        vec![body],
    )?);

    let card_rect: DeferredRect = Rc::new(RefCell::new(None));
    let mut children = vec![title_bar, body_area];
    if let Some(resize) = resize {
        children.push(box_item(StyledContainer::new(
            LayoutStyle::new()
                .flex_row()
                .width(SizeDimension::Percent(1.0))
                .flex_shrink(0.0)
                .justify_content(JustifyContent::END)
                .padding_horizontal(4.0)
                .padding_bottom(4.0),
            |_| RectStyle::default(),
            vec![resize_grip(style.close, Rc::clone(&card_rect), resize)?],
        )?));
    }

    let background = style.background;
    let radius = style.radius;
    let card = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0)),
        move |_| RectStyle::filled(background, radius),
        children,
    )?;
    *card_rect.borrow_mut() = track_layout(card.layout_node());
    Ok(box_item(card))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    use platform_core::PointerSource;
    use renderer_core::RectStyle;

    use crate::StyledContainer;
    use crate::context::reset_layout_runtime;
    use crate::layout_item::box_item;

    fn panel() -> Box<dyn LayoutItem> {
        box_item(
            StyledContainer::new(
                LayoutStyle::new().width(100.0).height(40.0),
                |_r| RectStyle::default(),
                vec![],
            )
            .unwrap(),
        )
    }

    fn press(x: f64, y: f64) -> Event {
        Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        }
    }

    #[test]
    fn dismiss_fires_on_outside_press_only() {
        reset_layout_runtime();
        let fired = Rc::new(Cell::new(0u32));
        let f = fired.clone();
        let placement = SurfacePlacement::drawer(SurfaceAnchor::Top).inset(20);
        let mut scaffold = SurfaceScaffold::new(
            &placement,
            panel(),
            Some(Rc::new(move || f.set(f.get() + 1))),
        )
        .unwrap();
        scaffold.on_event(&Event::WindowResized {
            width: 200,
            height: 200,
        });

        scaffold.on_event(&press(100.0, 40.0));
        assert_eq!(fired.get(), 0, "a press inside the panel must not dismiss");
        scaffold.on_event(&press(10.0, 190.0));
        assert_eq!(fired.get(), 1, "a press outside the panel dismisses");
    }

    /// Escape is the keyboard's way out of a surface that a press outside would also close, and it only reaches
    /// the surface when nothing inside wanted it: a focused field, an armed confirmation and an open dropdown
    /// all cancel themselves first, and taking the whole surface down with them is the bug this guards.
    #[test]
    fn escape_dismisses_only_what_the_panel_left_alone() {
        reset_layout_runtime();
        let fired = Rc::new(Cell::new(0u32));
        let f = fired.clone();
        let placement = SurfacePlacement::drawer(SurfaceAnchor::Top).inset(20);
        let mut scaffold = SurfaceScaffold::new(
            &placement,
            panel(),
            Some(Rc::new(move || f.set(f.get() + 1))),
        )
        .unwrap();
        scaffold.on_event(&Event::WindowResized {
            width: 200,
            height: 200,
        });

        let escape = Event::KeyPressed {
            key: Key::Named(NamedKey::Escape),
            modifiers: Default::default(),
        };
        assert_eq!(scaffold.on_event(&escape), EventResult::Handled);
        assert_eq!(
            fired.get(),
            1,
            "a plain panel lets Escape close the surface"
        );

        reset_layout_runtime();
        let untouched = Rc::new(Cell::new(0u32));
        let u = untouched.clone();
        let field = crate::input::Input::new(
            reactive_core::signal(String::new()),
            LayoutStyle::new().width(100.0).height(30.0),
            || TextStyle::new(14.0, Color::BLACK),
        )
        .unwrap()
        .autofocus();
        let mut focused = SurfaceScaffold::new(
            &placement,
            box_item(field),
            Some(Rc::new(move || u.set(u.get() + 1))),
        )
        .unwrap();
        focused.on_event(&Event::WindowResized {
            width: 200,
            height: 200,
        });
        focused.on_event(&press(100.0, 40.0));
        focused.on_event(&escape);
        assert_eq!(
            untouched.get(),
            0,
            "a field that claims Escape to release its own focus keeps the surface up"
        );
    }

    #[test]
    fn cross_axis_margin_insets_the_panel_from_the_side_edges() {
        reset_layout_runtime();
        let fired = Rc::new(Cell::new(0u32));
        let f = fired.clone();
        // Start-aligned top drawer, floated 10px off every edge: the 100-wide panel sits at x in [10, 110].
        let placement = SurfacePlacement::drawer(SurfaceAnchor::Top)
            .align(SurfaceAlign::Start)
            .margin((30, 10, 10, 10));
        let mut scaffold = SurfaceScaffold::new(
            &placement,
            panel(),
            Some(Rc::new(move || f.set(f.get() + 1))),
        )
        .unwrap();
        scaffold.on_event(&Event::WindowResized {
            width: 200,
            height: 200,
        });

        scaffold.on_event(&press(50.0, 40.0));
        assert_eq!(
            fired.get(),
            0,
            "a press on the inset panel must not dismiss"
        );
        scaffold.on_event(&press(4.0, 40.0));
        assert_eq!(
            fired.get(),
            1,
            "a press in the left gap (before the inset) dismisses"
        );
        scaffold.on_event(&press(150.0, 40.0));
        assert_eq!(fired.get(), 2, "a press in the right gap dismisses");
    }

    #[test]
    fn no_dismiss_when_not_configured() {
        reset_layout_runtime();
        let fired = Rc::new(Cell::new(0u32));
        let f = fired.clone();
        let placement = SurfacePlacement::new(SurfaceRole::Drawer, SurfaceAnchor::Top).scrim(true);
        let mut scaffold = SurfaceScaffold::new(
            &placement,
            panel(),
            Some(Rc::new(move || f.set(f.get() + 1))),
        )
        .unwrap();
        scaffold.on_event(&Event::WindowResized {
            width: 200,
            height: 200,
        });

        scaffold.on_event(&press(10.0, 190.0));
        assert_eq!(
            fired.get(),
            0,
            "no dismiss must fire when dismiss_on_outside is off"
        );
    }

    /// The grip's whole job is arithmetic, and every part of it is invisible until it is wrong.
    ///
    /// `on_drag` reports a position *local to the grip*, so a grip that forgot to add its own origin back would
    /// resize the window to about 14×14 the moment it was touched. And the grab offset — the distance from the
    /// pointer to the corner when the drag began — is what stops the corner teleporting to the cursor on the
    /// first event: press the middle of the grip and the window must not change size at all.
    #[test]
    fn the_grip_resizes_by_the_distance_dragged_not_to_the_pointer() {
        use std::cell::RefCell;
        reset_layout_runtime();

        let asked: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&asked);
        let style = SurfaceFrameStyle {
            background: Color::TRANSPARENT,
            title_bar: Color::TRANSPARENT,
            title_text: Color::TRANSPARENT,
            close: Color::TRANSPARENT,
            radius: 0.0,
            font_size: 12.0,
        };
        let mut frame = surface_frame(
            "Settings",
            style,
            Rc::new(|| {}),
            panel(),
            Some(Rc::new(move |w, h| sink.borrow_mut().push((w, h)))),
        )
        .unwrap();
        compute_layout(
            frame.layout_node(),
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(300.0),
        )
        .unwrap();

        // The grip sits at the card's bottom-right, inset by the row's padding.
        let grip = Rect {
            x: 400.0 - 4.0 - GRIP_SIZE,
            y: 300.0 - 4.0 - GRIP_SIZE,
            width: GRIP_SIZE,
            height: GRIP_SIZE,
        };
        let (start_x, start_y) = (grip.x + GRIP_SIZE / 2.0, grip.y + GRIP_SIZE / 2.0);
        frame.on_event(&press(start_x as f64, start_y as f64));
        frame.on_event(&Event::PointerMoved {
            x: (start_x + 60.0) as f64,
            y: (start_y + 40.0) as f64,
            source: PointerSource::Mouse,
        });

        let asked = asked.borrow();
        assert_eq!(
            asked.first().copied(),
            Some((400.0, 300.0)),
            "grabbing the grip without moving must ask for the size the window already is"
        );
        assert_eq!(
            asked.last().copied(),
            Some((460.0, 340.0)),
            "the window grows by what the pointer travelled, not to where the pointer is"
        );
    }

    /// The grip has to be *on screen*, and a frame around an application is where it stops being.
    ///
    /// A settings-sized float hands `surface_frame` a body sized to fill the window — its page area is a scroll
    /// leaf with a definite height, computed from the surface height less the chrome that existed before there
    /// was a grip. A body that will not shrink below its content pushes the grip row past the bottom edge, and
    /// the affordance builds, lays out, and is never visible. Which is exactly what happened.
    #[test]
    fn the_grip_stays_inside_a_window_whose_body_wants_all_of_it() {
        reset_layout_runtime();

        const SURFACE: (f32, f32) = (920.0, 680.0);
        let style = SurfaceFrameStyle {
            background: Color::TRANSPARENT,
            title_bar: Color::TRANSPARENT,
            title_text: Color::TRANSPARENT,
            close: Color::TRANSPARENT,
            radius: 0.0,
            font_size: 12.0,
        };
        // Taller than the surface, the way an application body is once its own chrome is added on top.
        let hungry = box_item(
            StyledContainer::new(
                LayoutStyle::new().width(600.0).height(SURFACE.1),
                |_r| RectStyle::default(),
                vec![],
            )
            .unwrap(),
        );
        let asked: Rc<std::cell::RefCell<Vec<(f32, f32)>>> =
            Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = Rc::clone(&asked);
        let mut frame = surface_frame(
            "Settings",
            style,
            Rc::new(|| {}),
            hungry,
            Some(Rc::new(move |w, h| sink.borrow_mut().push((w, h)))),
        )
        .unwrap();
        compute_layout(
            frame.layout_node(),
            AvailableSpace::Definite(SURFACE.0),
            AvailableSpace::Definite(SURFACE.1),
        )
        .unwrap();

        // Pressing the bottom-right corner and dragging is the property the user actually has: a grip laid out past the bottom edge receives nothing, so nothing resizes.
        let (x, y) = (
            SURFACE.0 - 4.0 - GRIP_SIZE / 2.0,
            SURFACE.1 - 4.0 - GRIP_SIZE / 2.0,
        );
        frame.on_event(&press(x as f64, y as f64));
        frame.on_event(&Event::PointerMoved {
            x: (x + 40.0) as f64,
            y: (y + 30.0) as f64,
            source: PointerSource::Mouse,
        });

        let asked = asked.borrow();
        assert!(
            !asked.is_empty(),
            "nothing at the window's bottom-right corner answered a drag — a body that refuses to shrink \
             pushes the grip row off the surface, where it lays out perfectly and is never seen"
        );
        assert_eq!(
            asked.last().copied(),
            Some((SURFACE.0 + 40.0, SURFACE.1 + 30.0)),
            "and once it is on screen it still resizes by what the pointer travelled"
        );
    }

    #[test]
    fn a_frame_without_a_resize_callback_draws_no_grip() {
        reset_layout_runtime();
        let style = SurfaceFrameStyle {
            background: Color::TRANSPARENT,
            title_bar: Color::TRANSPARENT,
            title_text: Color::TRANSPARENT,
            close: Color::TRANSPARENT,
            radius: 0.0,
            font_size: 12.0,
        };
        // A grip a backend cannot act on must be absent rather than present and inert — an affordance that does nothing is worse than none.
        assert!(surface_frame("Clock", style, Rc::new(|| {}), panel(), None).is_ok());
    }

    #[test]
    fn enter_transform_fade_is_opacity_only() {
        assert_eq!(enter_transform(EnterMotion::Fade, 0.0), (IDENTITY, 0.0));
        assert_eq!(enter_transform(EnterMotion::Fade, 0.5), (IDENTITY, 0.5));
        assert_eq!(enter_transform(EnterMotion::Fade, 1.0), (IDENTITY, 1.0));
    }

    #[test]
    fn enter_transform_slide_offsets_from_edge_then_settles() {
        let (m, o) = enter_transform(EnterMotion::Slide(SurfaceAnchor::Top), 0.0);
        assert_eq!(o, 0.0);
        assert_eq!(m[5], -SLIDE_DISTANCE, "top slides down from above");
        assert_eq!(
            enter_transform(EnterMotion::Slide(SurfaceAnchor::Top), 1.0),
            (IDENTITY, 1.0)
        );
        assert_eq!(
            enter_transform(EnterMotion::Slide(SurfaceAnchor::Bottom), 0.0).0[5],
            SLIDE_DISTANCE
        );
        assert_eq!(
            enter_transform(EnterMotion::Slide(SurfaceAnchor::Left), 0.0).0[4],
            -SLIDE_DISTANCE
        );
        assert_eq!(
            enter_transform(EnterMotion::Slide(SurfaceAnchor::Right), 0.0).0[4],
            SLIDE_DISTANCE
        );
    }
}
