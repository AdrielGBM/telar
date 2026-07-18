use std::rc::Rc;
use std::time::Duration;

use geometry_core::{Rect, Transform};
use layout_core::{
    AlignItems, AvailableSpace, JustifyContent, LayoutError, LayoutStyle, NodeId, SizeDimension,
};
use motion_core::{Animated, Easing, tween};
use platform_core::{Event, PointerButton};
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
    /// The surface hosts editable text and must be able to take keyboard focus. A backend maps this to
    /// its own focus model (e.g. layer-shell `on-demand` keyboard interactivity); left off, a panel is
    /// display-only and never steals the keyboard. Default `false`.
    pub wants_keyboard: bool,
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
            wants_keyboard: false,
            output: None,
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

    /// Opt the surface into keyboard focus, for panels that host editable text (a search box, a note
    /// title). A backend maps this to its focus model; the default is display-only.
    pub fn keyboard(mut self, wants_keyboard: bool) -> Self {
        self.wants_keyboard = wants_keyboard;
        self
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

fn enter_animation() -> Animated<f32> {
    let anim = Animated::new(0.0, tween(Duration::from_millis(ENTER_MS), Easing::EaseOut));
    anim.retarget(1.0);
    anim
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
    enter: Option<Animated<f32>>,
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
            enter: None,
        })
    }

    pub fn animate_in(mut self) -> Self {
        self.enter = Some(enter_animation());
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
        let (matrix, opacity) = match &self.enter {
            Some(anim) => enter_transform(EnterMotion::Slide(self.anchor), anim.get()),
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
    enter: Option<Animated<f32>>,
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
            enter: None,
        })
    }

    pub fn animate_in(mut self) -> Self {
        self.enter = Some(enter_animation());
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
        match &self.enter {
            Some(anim) => {
                let (_, opacity) = enter_transform(EnterMotion::Fade, anim.get());
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

pub fn surface_frame(
    title: impl Into<String>,
    style: SurfaceFrameStyle,
    close: std::rc::Rc<dyn Fn()>,
    body: Box<dyn LayoutItem>,
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

    let body_area = box_item(StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .flex_grow(1.0)
            .width(SizeDimension::Percent(1.0))
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .padding_all(12.0),
        |_| RectStyle::default(),
        vec![body],
    )?);

    let background = style.background;
    let radius = style.radius;
    let card = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0)),
        move |_| RectStyle::filled(background, radius),
        vec![title_bar, body_area],
    )?;
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
