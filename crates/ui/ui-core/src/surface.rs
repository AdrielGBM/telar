//! [`SurfaceScaffold`]: a surface's own chrome — its scrim, its dismissal, and the content it frames.

use std::rc::Rc;
use std::time::Duration;

use geometry_core::{Rect, Transform};
use layout_core::{
    AlignItems, AvailableSpace, JustifyContent, LayoutError, LayoutStyle, NodeId, SizeDimension,
};
use motion_core::{Animated, Easing, tween};
use platform_core::{Event, Key, NamedKey, PointerButton};
use reactive_core::RwSignal;
use renderer_core::{Color, RectStyle};
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::{compute_layout, mark_dirty, new_container, track_layout};
use crate::layout_item::LayoutItem;

/// The default scrim wash: ~35 % black over the content behind a drawer/modal. Rendered as a fill (not an opacity layer) so the panel above it stays fully opaque. Kept as the value a caller reaches for rather than being folded into the scaffold, because [`SurfaceScaffold`] now takes the colour itself.
pub const DEFAULT_SCRIM: Color = Color::rgba(0.0, 0.0, 0.0, 0.35);

/// Which side of the viewport a [`SurfaceScaffold`] pins its panel to, and the direction it slides in from.
///
/// [`Center`](Edge::Center) is not an edge: it means the panel is centred on both axes and arrives by fading rather than sliding, which is what a launcher or a command palette wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
    Center,
}

/// Enter-animation duration and slide travel.
const ENTER_MS: u64 = 200;
const SLIDE_DISTANCE: f32 = 24.0;
pub(crate) const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

#[derive(Clone, Copy)]
pub(crate) enum EnterMotion {
    Slide(Edge),
    Fade,
}

/// The transform matrix and opacity for an enter animation at `progress` (0 = just opened, 1 = settled). A slide starts `SLIDE_DISTANCE` off its edge and eases to rest; both forms fade in.
pub(crate) fn enter_transform(motion: EnterMotion, progress: f32) -> ([f32; 6], f32) {
    let p = progress.clamp(0.0, 1.0);
    let opacity = p;
    match motion {
        EnterMotion::Fade => (IDENTITY, opacity),
        EnterMotion::Slide(anchor) => {
            let d = SLIDE_DISTANCE * (1.0 - p);
            let (dx, dy) = match anchor {
                Edge::Top => (0.0, -d),
                Edge::Bottom => (0.0, d),
                Edge::Left => (-d, 0.0),
                Edge::Right => (d, 0.0),
                Edge::Center => (0.0, 0.0),
            };
            (Transform::translate(dx, dy).to_array(), opacity)
        }
    }
}

pub(crate) fn apply_enter(node: RenderNode, matrix: [f32; 6], opacity: f32) -> RenderNode {
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

/// The one progress value a surface's arrival and departure share: 0 is off its edge and transparent, 1 is settled. Opening runs it to 1; [`leave`](Self::leave) runs it back to 0, so the exit is the entrance reversed rather than a second animation that has to be kept in step with the first.
///
/// A backend that can hold a closing surface on screen for [`duration`](Self::duration) is what makes the exit half visible; one that cannot simply never calls `leave`, and the surface disappears as it always did.
#[derive(Clone)]
pub struct SurfaceTransition {
    progress: Animated<f32>,
    duration: Duration,
}

impl SurfaceTransition {
    /// A transition already on its way in. Constructed away from its goal and retargeted at once, never *at* it: an `Animated` born settled registers with no ticker, so nothing would schedule the frames that carry it in.
    pub fn enter() -> Self {
        let duration = Duration::from_millis(ENTER_MS);
        let progress = Animated::new(0.0, tween(duration, Easing::EaseOut));
        progress.retarget(1.0);
        Self { progress, duration }
    }

    /// Sends the surface back the way it came. The caller is responsible for keeping it on screen for [`duration`](Self::duration) — otherwise this animates a surface that has already been torn down.
    pub fn leave(&self) {
        self.progress.retarget(0.0);
    }

    /// How long either half takes, and therefore how long a closing surface has to stay mapped.
    pub fn duration(&self) -> Duration {
        self.duration
    }

    pub(crate) fn get(&self) -> f32 {
        self.progress.get()
    }
}

/// A full-viewport scaffold that positions a panel against a screen edge, optionally dims the area behind it, and dismisses on a press outside the panel. It is the reusable body of a drawer/modal: a shell mounts it as the root of a full-screen layer-shell surface, and a windowed app can mount it in-tree as an in-window portal — both get the same positioning and dismiss behaviour.
///
/// Like other full-surface roots it (re)computes its own layout on `WindowResized`; the runner synthesizes an initial one on resume, so the scaffold is laid out before its first frame.
pub struct SurfaceScaffold {
    root: NodeId,
    panel_rect: Option<RwSignal<Rect>>,
    root_rect: Option<RwSignal<Rect>>,
    content: Box<dyn LayoutItem>,
    scrim: Option<Color>,
    dismiss: Option<Rc<dyn Fn()>>,
    edge: Edge,
    transition: Option<SurfaceTransition>,
}

impl SurfaceScaffold {
    /// `margin` is `(top, right, bottom, left)` and becomes padding on all four sides, so the panel floats off every viewport edge rather than only the one it is pinned to. `scrim` paints behind the panel when set (see [`DEFAULT_SCRIM`]); `dismiss` fires on a press outside it, and `None` means outside presses fall through.
    pub fn new(
        edge: Edge,
        align: AlignItems,
        margin: (i32, i32, i32, i32),
        scrim: Option<Color>,
        dismiss: Option<Rc<dyn Fn()>>,
        content: Box<dyn LayoutItem>,
    ) -> Result<Self, LayoutError> {
        let panel_node = content.layout_node();
        let (mt, mr, mb, ml) = margin;
        let base = LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0))
            .padding_top(mt as f32)
            .padding_right(mr as f32)
            .padding_bottom(mb as f32)
            .padding_left(ml as f32);
        let style = match edge {
            Edge::Top => base
                .flex_column()
                .justify_content(JustifyContent::START)
                .align_items(align),
            Edge::Bottom => base
                .flex_column()
                .justify_content(JustifyContent::END)
                .align_items(align),
            Edge::Left => base
                .flex_row()
                .justify_content(JustifyContent::START)
                .align_items(align),
            Edge::Right => base
                .flex_row()
                .justify_content(JustifyContent::END)
                .align_items(align),
            Edge::Center => base
                .flex_column()
                .justify_content(JustifyContent::CENTER)
                .align_items(AlignItems::CENTER),
        };
        let root = new_container(style, &[panel_node])?;
        let panel_rect = track_layout(panel_node);
        let root_rect = track_layout(root);
        Ok(Self {
            root,
            panel_rect,
            root_rect,
            content,
            scrim,
            dismiss,
            edge,
            transition: None,
        })
    }

    pub fn animate_in(self) -> Self {
        self.animate(SurfaceTransition::enter())
    }

    /// Drives the scaffold from a transition the *caller* owns, so it can also send the surface back out — see [`SurfaceTransition::leave`].
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
            Some(transition) => enter_transform(EnterMotion::Slide(self.edge), transition.get()),
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
            // Escape is the keyboard's press-outside, so a surface answering one answers the other. The content gets first refusal, so a focused field blurs on the first press and the surface closes on the second.
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

#[cfg(test)]
#[path = "surface_test.rs"]
mod tests;
