//! The stage a canvas sits on: centred in its pane inside a viewport frame, at the size the handles or the preview ask for or else filling the pane, with handles that resize it; or, for a fullscreen preview, edge to edge with no frame.

use std::cell::{Cell, OnceCell};
use std::rc::Rc;

use telar::preview::{Globals, Layout};
use telar::{
    Accessible, AlignItems, Border, Canvas, Clip, ClippedItem, Color, Cursor, Direction,
    JustifyContent, Key, LayoutError, LayoutItem, LayoutStyle, NamedKey, NumericValue, Orientation,
    Point, Rect, RectStyle, RenderNode, Role, RwSignal, ShapeStyle, Size, SizeDimension,
    StyledContainer, SurfaceCanvas, SurfaceFrame, Text, box_item, current_direction, signal,
    track_layout,
};
use telar_devtools::{WORKBENCH_GRID, use_workbench_tokens};

use crate::state::WorkshopState;
use crate::strings::{self, VIEWPORT_HEIGHT, VIEWPORT_RESET, VIEWPORT_SIZE, VIEWPORT_WIDTH};

pub(super) const STAGE_MARGIN: f32 = WORKBENCH_GRID * 2.0;
pub(super) const HANDLE: f32 = WORKBENCH_GRID;
pub(super) const BORDER: f32 = 1.0;
pub(super) const VIEWPORT_MIN: f32 = WORKBENCH_GRID * 4.0;
const KEY_STEP: f32 = WORKBENCH_GRID;
const CHECKER: f32 = WORKBENCH_GRID * 2.0;

pub(super) struct Stage {
    pub(super) item: Box<dyn LayoutItem>,
    /// The frame's size, for the header to show; `None` when there is no frame.
    pub(super) size: Option<SizeLabel>,
}

pub(super) fn stage(
    state: &WorkshopState,
    layout: Layout,
    globals: Globals,
    canvas: Rc<SurfaceCanvas>,
) -> Result<Stage, LayoutError> {
    if layout == Layout::Fullscreen {
        let frame = SurfaceFrame::filling(canvas, growing().width(SizeDimension::Percent(1.0)))?;
        return Ok(Stage {
            item: box_item(frame),
            size: None,
        });
    }
    framed(state, globals, canvas)
}

/// `size` kept at least [`VIEWPORT_MIN`] each way and at most `room`.
pub(super) fn clamp_viewport(size: Size, room: Size) -> Size {
    let clamp = |value: f32, max: f32| value.clamp(VIEWPORT_MIN, max.max(VIEWPORT_MIN));
    Size::new(
        clamp(size.width, room.width),
        clamp(size.height, room.height),
    )
}

fn framed(
    state: &WorkshopState,
    globals: Globals,
    canvas: Rc<SurfaceCanvas>,
) -> Result<Stage, LayoutError> {
    let viewport = globals.viewport();
    let background = globals.background();
    let checker = Canvas::new(LayoutStyle::new().absolute_fill(), move |rect| {
        checker(rect, background.get())
    })?;
    let surface = SurfaceFrame::filling(canvas, growing().width(SizeDimension::Percent(1.0)))?;
    let box_style = move || {
        let style = LayoutStyle::new().flex_column().padding_all(BORDER);
        match viewport.get() {
            Some(size) => style
                .width(size.width + 2.0 * BORDER)
                .height(size.height + 2.0 * BORDER)
                .flex_shrink(0.0),
            None => style.flex_grow(1.0).min_width(0.0).min_height(0.0),
        }
    };
    let viewport_box = StyledContainer::new(
        box_style(),
        |_| {
            let tokens = use_workbench_tokens();
            RectStyle::default().with_border(Border::uniform(tokens.border_subtle, BORDER))
        },
        vec![box_item(checker), box_item(surface)],
    )?
    .styled_by(box_style);
    let box_rect =
        track_layout(viewport_box.layout_node()).expect("the viewport box is registered");

    let stage_rect: Rc<OnceCell<RwSignal<Rect>>> = Rc::default();
    let resize = Resize {
        chosen: state.viewport(),
        viewport: box_rect,
        stage: Rc::clone(&stage_rect),
    };
    let bottom_style = move || match viewport.get() {
        Some(size) => LayoutStyle::new().width(size.width + 2.0 * BORDER),
        None => LayoutStyle::new().flex_grow(1.0),
    };
    let top = row(
        move || match viewport.get() {
            Some(_) => LayoutStyle::new().flex_row(),
            None => LayoutStyle::new().flex_row().flex_grow(1.0).min_height(0.0),
        },
        vec![
            box_item(viewport_box),
            handle(Edge::Right, resize.clone(), || {
                LayoutStyle::new().width(HANDLE).flex_shrink(0.0)
            })?,
        ],
    )?;
    let bottom = row(
        || {
            LayoutStyle::new()
                .flex_row()
                .height(HANDLE)
                .flex_shrink(0.0)
        },
        vec![
            handle(Edge::Bottom, resize.clone(), bottom_style)?,
            handle(Edge::Corner, resize, || {
                LayoutStyle::new()
                    .width(HANDLE)
                    .height(HANDLE)
                    .flex_shrink(0.0)
            })?,
        ],
    )?;
    let column_style = move || match viewport.get() {
        Some(_) => LayoutStyle::new().flex_column().flex_shrink(0.0),
        None => growing().width(SizeDimension::Percent(1.0)),
    };
    let column = StyledContainer::new(
        column_style(),
        |_| RectStyle::default(),
        vec![box_item(top), box_item(bottom)],
    )?
    .styled_by(column_style);
    let stage = StyledContainer::new(
        growing()
            .padding_all(STAGE_MARGIN)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        |_| RectStyle::default(),
        vec![box_item(column)],
    )?;
    let _ = stage_rect.set(track_layout(stage.layout_node()).expect("the stage is registered"));
    Ok(Stage {
        item: box_item(ClippedItem::new(box_item(stage), Clip::both().paint_only())),
        size: Some(SizeLabel { viewport: box_rect }),
    })
}

/// The frame's size as `W × H`, which resets the viewport when pressed.
pub(super) struct SizeLabel {
    viewport: RwSignal<Rect>,
}

impl SizeLabel {
    pub(super) fn build(self, state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let rect = self.viewport;
        let text = Text::declaring(
            move || {
                let size = inner(rect.get());
                format!("{} × {}", size.width.round(), size.height.round())
            },
            LayoutStyle::new(),
            |text| {
                let tokens = use_workbench_tokens();
                text.with_color(tokens.text_muted)
                    .with_font_family(tokens.mono_family)
                    .with_font_size(tokens.mono_size)
            },
        )?;
        let chosen = state.viewport();
        let label = StyledContainer::new(
            LayoutStyle::new()
                .flex_row()
                .padding_horizontal(WORKBENCH_GRID / 2.0),
            |_| RectStyle::default(),
            vec![box_item(text)],
        )?
        .control(Role::Button)
        .a11y_label(|| strings::text(VIEWPORT_RESET))
        .on_press(move || chosen.set(None));
        Ok(box_item(label))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Edge {
    Right,
    Bottom,
    Corner,
}

impl Edge {
    fn widens(self) -> bool {
        self != Self::Bottom
    }

    fn deepens(self) -> bool {
        self != Self::Right
    }

    fn label(self) -> &'static str {
        match self {
            Self::Right => VIEWPORT_WIDTH,
            Self::Bottom => VIEWPORT_HEIGHT,
            Self::Corner => VIEWPORT_SIZE,
        }
    }

    fn cursor(self) -> Cursor {
        match self {
            Self::Right => Cursor::EwResize,
            Self::Bottom => Cursor::NsResize,
            Self::Corner => Cursor::NwseResize,
        }
    }
}

/// The viewport a drag started from, and where the pointer was then.
#[derive(Clone, Copy)]
struct Grab {
    from: Size,
    origin: Point,
}

/// What a handle reads and writes: the viewport the handles chose, the frame's box and the stage's.
#[derive(Clone)]
struct Resize {
    chosen: RwSignal<Option<(f32, f32)>>,
    viewport: RwSignal<Rect>,
    stage: Rc<OnceCell<RwSignal<Rect>>>,
}

impl Resize {
    fn current(&self) -> Size {
        inner(self.viewport.peek())
    }

    /// The largest viewport the stage shows whole.
    fn room(&self) -> Size {
        let stage = self.stage.get().map_or(Rect::default(), |rect| rect.peek());
        let chrome = 2.0 * STAGE_MARGIN + HANDLE + 2.0 * BORDER;
        Size::new(stage.width - chrome, stage.height - chrome)
    }

    fn set(&self, size: Size) {
        let size = clamp_viewport(size, self.room());
        self.chosen.set(Some((size.width, size.height)));
    }

    fn range(&self, edge: Edge) -> NumericValue {
        let (now, room) = (self.current(), self.room());
        let (now, max) = match edge {
            Edge::Bottom => (now.height, room.height),
            _ => (now.width, room.width),
        };
        NumericValue {
            now: now as f64,
            min: VIEWPORT_MIN as f64,
            max: max.max(VIEWPORT_MIN) as f64,
        }
    }
}

fn handle(
    edge: Edge,
    resize: Resize,
    style: impl Fn() -> LayoutStyle + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let active = signal(false);
    let handle = StyledContainer::new(
        style(),
        move |_| {
            let tokens = use_workbench_tokens();
            match (active.get(), edge) {
                (true, _) => RectStyle::default().with_fill(tokens.selection),
                (false, Edge::Corner) => RectStyle::default().with_fill(tokens.border_subtle),
                (false, _) => RectStyle::default(),
            }
        },
        Vec::new(),
    )?
    .styled_by(style);
    let own = track_layout(handle.layout_node()).expect("the handle is registered");
    let grab: Rc<Cell<Option<Grab>>> = Rc::default();
    let (dragged, ended, cancelled, stepped, ranged) = (
        resize.clone(),
        Rc::clone(&grab),
        Rc::clone(&grab),
        resize.clone(),
        resize,
    );
    let handle = handle
        .control(Role::Splitter)
        .a11y_label(move || strings::text(edge.label()))
        .cursor(edge.cursor())
        .on_hover(move |hovered| active.set(hovered))
        .on_drag(move |x, y| {
            let rect = own.peek();
            let pointer = Point::new(rect.x + x, rect.y + y);
            let Grab { from, origin } = grab.get().unwrap_or_else(|| {
                let held = Grab {
                    from: dragged.current(),
                    origin: pointer,
                };
                grab.set(Some(held));
                held
            });
            // The frame is centred, so it grows by twice the travel for its edge to stay under the pointer.
            let dx = 2.0 * (pointer.x - origin.x) * outward();
            let dy = 2.0 * (pointer.y - origin.y);
            dragged.set(Size::new(
                from.width + if edge.widens() { dx } else { 0.0 },
                from.height + if edge.deepens() { dy } else { 0.0 },
            ));
        })
        .on_drag_end(move |_, _| ended.set(None))
        .on_drag_cancel(move || cancelled.set(None))
        .on_focused_key(move |key: &Key| -> bool {
            let Some((dx, dy)) = key_step(edge, key) else {
                return false;
            };
            let from = stepped.current();
            stepped.set(Size::new(from.width + dx, from.height + dy));
            true
        });
    let handle = match edge {
        Edge::Right => handle
            .oriented(Orientation::Vertical)
            .valued(move || ranged.range(edge)),
        Edge::Bottom => handle
            .oriented(Orientation::Horizontal)
            .valued(move || ranged.range(edge)),
        Edge::Corner => handle,
    };
    Ok(box_item(handle))
}

/// How far an arrow key moves `edge`, as `(width, height)`; `None` for a key that does not move it.
fn key_step(edge: Edge, key: &Key) -> Option<(f32, f32)> {
    let Key::Named(named) = key else {
        return None;
    };
    let across = KEY_STEP * outward();
    match named {
        NamedKey::ArrowRight if edge.widens() => Some((across, 0.0)),
        NamedKey::ArrowLeft if edge.widens() => Some((-across, 0.0)),
        NamedKey::ArrowDown if edge.deepens() => Some((0.0, KEY_STEP)),
        NamedKey::ArrowUp if edge.deepens() => Some((0.0, -KEY_STEP)),
        _ => None,
    }
}

/// `1` where the width handle sits on the right, and `-1` where a right-to-left layout puts it on the left.
fn outward() -> f32 {
    match current_direction() {
        Direction::Ltr => 1.0,
        Direction::Rtl => -1.0,
    }
}

/// A transparent background shows through to the checker; anything else covers it, so it is not drawn.
fn checker(rect: Rect, background: Option<Color>) -> RenderNode {
    if background.is_none_or(|color| color.a >= 1.0) {
        return RenderNode::Empty;
    }
    let tokens = use_workbench_tokens();
    let area = Rect::new(
        rect.x + BORDER,
        rect.y + BORDER,
        (rect.width - 2.0 * BORDER).max(0.0),
        (rect.height - 2.0 * BORDER).max(0.0),
    );
    let mut cells = vec![RenderNode::rect(
        area,
        RectStyle::default().with_fill(tokens.checker_a),
    )];
    let (columns, rows) = (
        (area.width / CHECKER).ceil() as usize,
        (area.height / CHECKER).ceil() as usize,
    );
    for row in 0..rows {
        for column in (row % 2..columns).step_by(2) {
            let (x, y) = (column as f32 * CHECKER, row as f32 * CHECKER);
            let cell = Rect::new(
                area.x + x,
                area.y + y,
                CHECKER.min(area.width - x),
                CHECKER.min(area.height - y),
            );
            cells.push(RenderNode::rect(
                cell,
                RectStyle::default().with_fill(tokens.checker_b),
            ));
        }
    }
    RenderNode::group(cells)
}

fn inner(rect: Rect) -> Size {
    Size::new(
        (rect.width - 2.0 * BORDER).max(0.0),
        (rect.height - 2.0 * BORDER).max(0.0),
    )
}

fn row(
    style: impl Fn() -> LayoutStyle + 'static,
    items: Vec<Box<dyn LayoutItem>>,
) -> Result<StyledContainer, LayoutError> {
    Ok(StyledContainer::new(style(), |_| RectStyle::default(), items)?.styled_by(style))
}

fn growing() -> LayoutStyle {
    LayoutStyle::new()
        .flex_column()
        .flex_grow(1.0)
        .min_width(0.0)
        .min_height(0.0)
}
