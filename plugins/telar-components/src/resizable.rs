//! [`resizable`]: content with handles on its inline-end edge, its bottom edge and the corner between them, dragged, or stepped with the arrow keys, Home and End, to resize it; and [`resize_handle`], one such handle for content whose size is kept elsewhere.

use std::rc::Rc;

use telar::{
    Accessible, Children, Clip, ClippedItem, Color, Container, Cursor, Direction, Key, LayoutError,
    LayoutItem, LayoutStyle, NamedKey, NumericValue, Orientation, Props, Reactive, RectStyle,
    RwSignal, ShapeStyle, Size, StyledContainer, SurfaceAmendment, Transaction, amend_surface,
    box_item, current_direction, focus::Role, signal, step_factor, track_layout,
};

use crate::edit::{Stroke, write_through};
use crate::resize::Latch;
use crate::shared;
use crate::split_pane::DRAG_THRESHOLD;
use crate::strings;

const DEFAULT_SIZE: Size = Size::new(240.0, 160.0);
const DEFAULT_STEP: f32 = 16.0;

/// A resizable's own tokens, which an application's theme supplies through a `#[theme(extension)]` field. A [`resize_handle`] reads them too.
///
/// Every field left `None` follows the role named beside it, read when a handle paints.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResizableTokens {
    /// Thickness of an edge handle, and the side of the corner one, at the regular control size, scaled with the control size in force. `None` is [`metrics::spacing`](crate::metrics::spacing).
    pub thickness: Option<f32>,
    /// The corner handle at rest. An edge handle is bare until a pointer finds it. `None` is the theme's `border`.
    pub grip: Option<Color>,
    /// A handle under the pointer or being dragged. `None` is the theme's accent.
    pub active: Option<Color>,
}

impl ResizableTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn thickness(&self) -> f32 {
        self.thickness.map_or_else(shared::spacing, |thickness| {
            thickness * telar::control_scale()
        })
    }

    pub fn grip(&self) -> Color {
        self.grip.unwrap_or_else(shared::border)
    }

    pub fn active(&self) -> Color {
        self.active.unwrap_or_else(shared::accent)
    }
}

/// Where a [`resize_handle`] sits on the content it resizes, and so what it resizes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, telar::PreviewArg)]
pub enum ResizeEdge {
    /// The inline end, the right in a left-to-right layout and the left in a right-to-left one: the width. A splitter, focusable and stepped with the keys.
    #[default]
    End,
    /// The bottom: the height. A splitter, focusable and stepped with the keys.
    Bottom,
    /// The corner between the two: both at once. A pointer's shortcut to the sizes the two edges reach from the keyboard, so it is not focusable: no role a reader knows moves along two axes.
    Corner,
}

impl ResizeEdge {
    fn widens(self) -> bool {
        self != Self::Bottom
    }

    fn deepens(self) -> bool {
        self != Self::End
    }

    fn orientation(self) -> Option<Orientation> {
        match self {
            Self::End => Some(Orientation::Vertical),
            Self::Bottom => Some(Orientation::Horizontal),
            Self::Corner => None,
        }
    }

    fn cursor(self) -> Cursor {
        match (self, current_direction()) {
            (Self::End, _) => Cursor::EwResize,
            (Self::Bottom, _) => Cursor::NsResize,
            (Self::Corner, Direction::Ltr) => Cursor::NwseResize,
            (Self::Corner, Direction::Rtl) => Cursor::NeswResize,
        }
    }

    fn default_label(self) -> &'static str {
        match self {
            Self::Bottom => strings::RESIZE_HEIGHT,
            _ => strings::RESIZE_WIDTH,
        }
    }

    fn layout(self, thickness: f32) -> LayoutStyle {
        match self {
            Self::End => LayoutStyle::new().width(thickness).flex_shrink(0.0),
            Self::Bottom => LayoutStyle::new()
                .height(thickness)
                .flex_grow(1.0)
                .min_width(0.0),
            Self::Corner => LayoutStyle::new()
                .width(thickness)
                .height(thickness)
                .flex_shrink(0.0),
        }
    }
}

/// Which handles a [`resizable`] draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, telar::PreviewArg)]
pub enum ResizeEdges {
    /// The inline-end edge alone: the width.
    End,
    /// The bottom edge alone: the height.
    Bottom,
    /// Both edges, and the corner between them.
    #[default]
    All,
}

impl ResizeEdges {
    fn has_end(self) -> bool {
        self != Self::Bottom
    }
}

/// How the content grows as a handle moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, telar::PreviewArg)]
pub enum ResizeGrowth {
    /// Its inline start and its top stay put, and the edge dragged follows the pointer.
    #[default]
    Anchored,
    /// It is centred in its parent, so it grows on both sides at once: by twice the pointer's travel, for the edge dragged to stay under the pointer.
    Centred,
}

impl ResizeGrowth {
    fn factor(self) -> f32 {
        match self {
            Self::Anchored => 1.0,
            Self::Centred => 2.0,
        }
    }
}

/// Content in a box of the size the user gives it, with handles to give it one: dragged, or stepped with the arrow keys, Home and End once an edge handle has focus. Children: the content, laid out in a column of that size that clips what overflows it. Every drag is a [`Transaction`] on the size, so Escape puts it back, and every key step is one of its own.
#[derive(Props)]
pub struct ResizableProps {
    /// Bound size of the content, in px. `None` (the default) is uncontrolled: the widget owns a signal starting at [`default_size`](Self::default_size).
    #[props(some, into, default)]
    pub size: Option<RwSignal<Size>>,
    /// Starting size when [`size`](Self::size) is not bound. `Size::ZERO` (the default) means 240 × 160.
    #[props(default)]
    pub default_size: Size,
    /// Smallest size of the content, in px.
    #[props(default)]
    pub min: Size,
    /// Largest size of the content, in px. A side left `0.0` (the default) is bounded only by [`room`](Self::room).
    #[props(default)]
    pub max: Size,
    /// The most the content may take right now, such as the space its parent leaves, read whenever a resize is bounded. It narrows [`max`](Self::max), never below [`min`](Self::min). `None` (the default) leaves `max` alone.
    #[props(some, into, default)]
    pub room: Option<Reactive<Size>>,
    /// The handles drawn.
    #[props(default)]
    pub edges: ResizeEdges,
    /// Whether the content stays put at its start or is centred as it grows.
    #[props(default)]
    pub growth: ResizeGrowth,
    /// One unscaled arrow-key step in px. Non-positive means 16; Shift makes it larger.
    #[props(default)]
    pub step: f32,
    /// What a reader calls the width handle. `None` (the default) uses the localized `telar_components.resize_width` message, which an application can override in its catalog.
    #[props(some, into, default)]
    pub width_label: Option<Reactive<String>>,
    /// What a reader calls the height handle. `None` (the default) uses the localized `telar_components.resize_height` message.
    #[props(some, into, default)]
    pub height_label: Option<Reactive<String>>,
    /// Runs as a drag takes hold of a handle, before its first size is asked for. Key steps are not drags and do not run it.
    #[props(some, default)]
    pub on_start: Option<Rc<dyn Fn()>>,
    /// Runs once for every drag [`on_start`](Self::on_start) ran for, with whether it was kept: `true` when it was let go, `false` when it was cancelled (Escape, another pointer button, the window losing focus) or the handle went away mid-drag. A cancelled drag has put the size back by the time this runs.
    #[props(some, default)]
    pub on_end: Option<Rc<dyn Fn(bool)>>,
    /// Amends the paint of every handle, the resizable's **principal surface**, at rest, under the pointer and while dragged. See [`telar::SurfaceStyle`] for when a theme token or [`ResizableTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// One handle of a [`resizable`], for content whose size is kept elsewhere, such as a canvas its settings size or a surface the window system sizes, and laid out by its caller: [`End`](ResizeEdge::End) after the content in a row, whose height it spans; [`Bottom`](ResizeEdge::Bottom) in a row under the content, whose width it takes beside the corner; [`Corner`](ResizeEdge::Corner) at the end of that row.
#[derive(Props)]
pub struct ResizeHandleProps {
    #[props(default)]
    pub edge: ResizeEdge,
    /// The size the content has now, in the units [`on_resize`](Self::on_resize) takes: what a drag and a key step start from, and what a reader hears.
    #[props(into, default)]
    pub size: Reactive<Size>,
    /// Takes every size a drag or a key step asks for, kept within [`min`](Self::min) and [`max`](Self::max). A cancelled drag asks once more for the size it started from, before [`on_end`](Self::on_end) runs.
    #[props(some, default)]
    pub on_resize: Option<Rc<dyn Fn(Size)>>,
    /// Smallest size.
    #[props(default)]
    pub min: Size,
    /// Largest size, read whenever a resize is bounded, and never below [`min`](Self::min). `None` (the default) is unbounded.
    #[props(some, into, default)]
    pub max: Option<Reactive<Size>>,
    /// Whether the content stays put at its start or is centred as it grows.
    #[props(default)]
    pub growth: ResizeGrowth,
    /// Pointer px per unit of size: content drawn zoomed passes its zoom, so the edge dragged stays under the pointer. Default 1.
    #[props(into, default = Reactive::from(1.0_f32))]
    pub scale: Reactive<f32>,
    /// One unscaled arrow-key step, in units of size. Non-positive means 16; Shift makes it larger.
    #[props(default)]
    pub step: f32,
    /// Thickness in px. `0.0` (the default) is the [`ResizableTokens`] thickness.
    #[props(default)]
    pub thickness: f32,
    /// What a reader calls an edge handle. `None` (the default) uses the localized `telar_components.resize_width` or `telar_components.resize_height` message. The corner has no name, since a reader never meets it.
    #[props(some, into, default)]
    pub label: Option<Reactive<String>>,
    /// Runs as a drag takes hold of the handle, before its first size is asked for.
    #[props(some, default)]
    pub on_start: Option<Rc<dyn Fn()>>,
    /// Runs once for every drag [`on_start`](Self::on_start) ran for, with whether it was kept.
    #[props(some, default)]
    pub on_end: Option<Rc<dyn Fn(bool)>>,
    /// Amends the paint of the handle at rest, under the pointer and while dragged.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

struct Target {
    size: Reactive<Size>,
    resize: Rc<dyn Fn(Size)>,
    min: Size,
    max: Box<dyn Fn() -> Size>,
    growth: ResizeGrowth,
    scale: Reactive<f32>,
    step: f32,
}

impl Target {
    fn bound(&self) -> Size {
        let max = (self.max)();
        Size::new(
            max.width.max(self.min.width),
            max.height.max(self.min.height),
        )
    }

    fn request(&self, raw: Size) {
        let bound = self.bound();
        (self.resize)(Size::new(
            raw.width.clamp(self.min.width, bound.width),
            raw.height.clamp(self.min.height, bound.height),
        ));
    }

    fn value(&self, edge: ResizeEdge) -> NumericValue {
        let (now, bound) = (self.size.get(), self.bound());
        let (now, min, max) = match edge {
            ResizeEdge::Bottom => (now.height, self.min.height, bound.height),
            _ => (now.width, self.min.width, bound.width),
        };
        NumericValue {
            now: now as f64,
            min: min as f64,
            max: max as f64,
        }
    }

    fn dragged(&self, edge: ResizeEdge, from: Size, (dx, dy): (f32, f32)) -> Size {
        let reach = self.growth.factor() / self.scale.get();
        let (dx, dy) = (dx * outward() * reach, dy * reach);
        Size::new(
            from.width + if edge.widens() { dx } else { 0.0 },
            from.height + if edge.deepens() { dy } else { 0.0 },
        )
    }

    fn keyed(&self, edge: ResizeEdge, named: &NamedKey) -> Option<Size> {
        let now = self.size.get();
        let step = self.step * step_factor(telar::modifiers());
        let across = step * outward();
        let along = |value: f32| match edge {
            ResizeEdge::Bottom => Size::new(now.width, value),
            _ => Size::new(value, now.height),
        };
        match (edge, named) {
            (ResizeEdge::End, NamedKey::ArrowRight) => Some(along(now.width + across)),
            (ResizeEdge::End, NamedKey::ArrowLeft) => Some(along(now.width - across)),
            (ResizeEdge::Bottom, NamedKey::ArrowDown) => Some(along(now.height + step)),
            (ResizeEdge::Bottom, NamedKey::ArrowUp) => Some(along(now.height - step)),
            (_, NamedKey::Home) => Some(along(f32::NEG_INFINITY)),
            (_, NamedKey::End) => {
                let bound = self.value(edge).max as f32;
                bound.is_finite().then(|| along(bound))
            }
            _ => None,
        }
    }
}

struct Look {
    thickness: Rc<dyn Fn() -> f32>,
    label: Option<Reactive<String>>,
    style: Option<SurfaceAmendment>,
}

pub fn resizable(
    props: ResizableProps,
    children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ResizableProps {
        size,
        default_size,
        min,
        max,
        room,
        edges,
        growth,
        step,
        width_label,
        height_label,
        on_start,
        on_end,
        style,
    } = props;
    let default_size = if default_size == Size::ZERO {
        DEFAULT_SIZE
    } else {
        default_size
    };
    let size = size.unwrap_or_else(|| signal(default_size));
    let transaction = Transaction::new(size);
    let cap = |side: f32| if side > 0.0 { side } else { f32::INFINITY };
    let max = Size::new(cap(max.width), cap(max.height));
    let shown = move || {
        let size = size.get();
        Size::new(
            size.width.clamp(min.width, max.width.max(min.width)),
            size.height.clamp(min.height, max.height.max(min.height)),
        )
    };

    let content_box = move || {
        let shown = shown();
        LayoutStyle::column()
            .width(shown.width)
            .height(shown.height)
            .flex_shrink(0.0)
    };
    let content =
        Container::new(content_box(), children.build()?.take_default())?.styled_by(content_box);
    let laid_out = track_layout(content.layout_node()).expect("the content is registered");
    let content = box_item(ClippedItem::following(box_item(content), Clip::both));

    let handle = |edge: ResizeEdge| {
        let target = Target {
            size: Reactive::of(move || {
                let rect = laid_out.get();
                Size::new(rect.width, rect.height)
            }),
            resize: Rc::new(move |next| write_through(transaction, next)),
            min,
            max: Box::new({
                let room = room.clone();
                move || match &room {
                    Some(room) => {
                        let room = room.get();
                        Size::new(max.width.min(room.width), max.height.min(room.height))
                    }
                    None => max,
                }
            }),
            growth,
            scale: Reactive::from(1.0_f32),
            step,
        };
        let look = Look {
            thickness: Rc::new(|| ResizableTokens::current().thickness()),
            label: match edge {
                ResizeEdge::Bottom => height_label.clone(),
                _ => width_label.clone(),
            },
            style: style.clone(),
        };
        let stroke = Stroke::new(on_start.clone(), on_end.clone());
        build_handle(edge, target, look, stroke, Some(transaction))
    };

    let row = |items: Vec<Box<dyn LayoutItem>>| -> Result<Box<dyn LayoutItem>, LayoutError> {
        Ok(box_item(Container::new(LayoutStyle::row(), items)?))
    };
    let (flow, items) = match edges {
        ResizeEdges::End => (LayoutStyle::row(), vec![content, handle(ResizeEdge::End)?]),
        ResizeEdges::Bottom => (
            LayoutStyle::column(),
            vec![content, row(vec![handle(ResizeEdge::Bottom)?])?],
        ),
        ResizeEdges::All => (
            LayoutStyle::column(),
            vec![
                row(vec![content, handle(ResizeEdge::End)?])?,
                row(vec![
                    handle(ResizeEdge::Bottom)?,
                    handle(ResizeEdge::Corner)?,
                ])?,
            ],
        ),
    };
    let root_box = move || {
        let beside = match edges.has_end() {
            true => ResizableTokens::current().thickness(),
            false => 0.0,
        };
        flow.clone().width(shown().width + beside).flex_shrink(0.0)
    };
    let root = Container::new(root_box(), items)?.styled_by(root_box);
    Ok(box_item(root))
}

pub fn resize_handle(
    props: ResizeHandleProps,
    _children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ResizeHandleProps {
        edge,
        size,
        on_resize,
        min,
        max,
        growth,
        scale,
        step,
        thickness,
        label,
        on_start,
        on_end,
        style,
    } = props;
    let target = Target {
        size,
        resize: on_resize.unwrap_or_else(|| Rc::new(|_| {})),
        min,
        max: match max {
            Some(max) => Box::new(move || max.get()),
            None => Box::new(|| Size::new(f32::INFINITY, f32::INFINITY)),
        },
        growth,
        scale,
        step,
    };
    let look = Look {
        thickness: match thickness > 0.0 {
            true => Rc::new(move || thickness),
            false => Rc::new(|| ResizableTokens::current().thickness()),
        },
        label,
        style,
    };
    build_handle(edge, target, look, Stroke::new(on_start, on_end), None)
}

/// A handle on `edge` of `target`. A drag under `transaction` is decided by it, so a cancel reverts it there. Without one the drag runs in a transaction that holds nothing, only so Escape and the other cancels reach it, and a cancel asks for the size the drag started from.
fn build_handle(
    edge: ResizeEdge,
    target: Target,
    look: Look,
    stroke: Stroke,
    transaction: Option<Transaction<Size>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Look {
        thickness,
        label,
        style,
    } = look;
    let step = if target.step > 0.0 {
        target.step
    } else {
        DEFAULT_STEP
    };
    let target = Rc::new(Target { step, ..target });
    let stroke = Rc::new(stroke);
    let latch = Rc::new(Latch::default());
    let (hovered, dragging) = (signal(false), signal(false));

    let handle_box = move || edge.layout(thickness());
    let handle = StyledContainer::new(
        handle_box(),
        move |_| {
            let tokens = ResizableTokens::current();
            let fill = match (hovered.get() || dragging.get(), edge) {
                (true, _) => Some(tokens.active()),
                (false, ResizeEdge::Corner) => Some(tokens.grip()),
                (false, _) => None,
            };
            let rest = match fill {
                Some(fill) => RectStyle::default().with_fill(fill),
                None => RectStyle::default(),
            };
            amend_surface(rest, &style)
        },
        Vec::new(),
    )?
    .styled_by(handle_box);
    let own = track_layout(handle.layout_node()).expect("the handle is registered");
    let handle = match transaction {
        Some(transaction) => handle.drag_transaction(transaction),
        None => handle.drag_transaction(Transaction::new(signal(()))),
    };
    let restores = transaction.is_none();

    let handle = handle
        .cursor(Reactive::of(move || edge.cursor()))
        .drag_threshold(DRAG_THRESHOLD)
        .on_hover(move |now| hovered.set(now))
        .on_drag({
            let (target, latch, stroke) = (target.clone(), latch.clone(), stroke.clone());
            move |x, y| {
                let (grab, pointer) = latch.hold(own.peek(), (x, y), || {
                    stroke.start();
                    dragging.set(true);
                    target.size.get()
                });
                target.request(target.dragged(edge, grab.from, grab.travel(pointer)));
            }
        })
        .on_drag_end({
            let (latch, stroke) = (latch.clone(), stroke.clone());
            move |_, _| {
                latch.release();
                dragging.set(false);
                stroke.end(true);
            }
        })
        .on_drag_cancel({
            let target = target.clone();
            move || {
                if let Some(grab) = latch.release()
                    && restores
                {
                    (target.resize)(grab.from);
                }
                dragging.set(false);
                stroke.end(false);
            }
        });
    let Some(orientation) = edge.orientation() else {
        return Ok(box_item(handle));
    };
    let valued = target.clone();
    let handle = handle
        .control(Role::Splitter)
        .oriented(orientation)
        .a11y_label(move || match &label {
            Some(label) => label.get(),
            None => strings::text(edge.default_label()),
        })
        .valued(move || valued.value(edge))
        .on_focused_key(move |key: &Key| -> bool {
            let Key::Named(named) = key else {
                return false;
            };
            let Some(next) = target.keyed(edge, named) else {
                return false;
            };
            target.request(next);
            true
        });
    Ok(box_item(handle))
}

fn outward() -> f32 {
    match current_direction() {
        Direction::Ltr => 1.0,
        Direction::Rtl => -1.0,
    }
}

#[cfg(test)]
#[path = "resizable_test.rs"]
mod tests;
