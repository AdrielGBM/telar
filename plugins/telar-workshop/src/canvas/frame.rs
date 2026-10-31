//! The stage a canvas sits on: centred in its pane inside a viewport frame, or a device's bezel, at the size the toolbar, the handles or the preview ask for or else filling the pane, zoomed as the toolbar asks, with handles that resize it; or, for a fullscreen preview, edge to edge with no frame.

use std::cell::{OnceCell, RefCell};
use std::rc::{Rc, Weak};

use telar::preview::{Globals, Layout};
use telar::{
    Accessible, AlignItems, Border, BorderRadius, Canvas, Clip, ClippedItem, Color, Container,
    JustifyContent, LayoutError, LayoutItem, LayoutStyle, Memo, Reactive, ReactiveList, Rect,
    RectStyle, RenderNode, Role, RwSignal, ShapeStyle, Size, SizeDimension, StyledContainer,
    SurfaceCanvas, SurfaceFrame, Text, TextWrap, box_item, children, effect, memo, signal,
    track_layout,
};
use telar_components::{ResizeEdge, ResizeGrowth, ResizeHandleProps, resize_handle};
use telar_devtools::{
    WORKBENCH_GRID, inspection_marks, use_workbench_tokens, workbench_mono, workbench_muted,
};

use super::guides;
use crate::interactions;
use crate::settings::{CanvasSettings, CanvasSize, fit_scale};
use crate::state::WorkshopState;
use crate::strings::{self, VIEWPORT_HEIGHT, VIEWPORT_RESET, VIEWPORT_WIDTH};

pub(super) const STAGE_MARGIN: f32 = WORKBENCH_GRID * 2.0;
pub(super) const HANDLE: f32 = WORKBENCH_GRID;
pub(super) const BORDER: f32 = 1.0;
pub(super) const BEZEL: f32 = WORKBENCH_GRID * 1.5;
pub(super) const VIEWPORT_MIN: f32 = WORKBENCH_GRID * 4.0;
const BEZEL_RADIUS: f32 = WORKBENCH_GRID * 3.0;
const BEZEL_COLOR: Color = Color::rgb(0.11, 0.11, 0.12);
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
        let grid = guides::grid(state.canvas_settings(), Rc::downgrade(&canvas), || 0.0)?;
        let frame = SurfaceFrame::filling(canvas, growing().width(SizeDimension::Percent(1.0)))?;
        let edge_to_edge = Container::new(
            growing().width(SizeDimension::Percent(1.0)),
            vec![
                box_item(frame),
                interactions::frame_layer(state)?,
                grid,
                inspection_marks(state.inspection())?,
            ],
        )?;
        return Ok(Stage {
            item: box_item(edge_to_edge),
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
    let settings = state.canvas_settings();
    let device = memo(move || settings.with(|settings| settings.device().is_some()));
    let scale = signal(1.0_f32);
    let checker = Canvas::new(LayoutStyle::new().absolute_fill(), move |rect| {
        checker(rect, edge(device.get()), background.get())
    })?;
    let grid = guides::grid(settings, Rc::downgrade(&canvas), move || edge(device.get()))?;
    let shown = Rc::clone(&canvas);
    let surface = ReactiveList::with_style(
        growing().width(SizeDimension::Percent(1.0)),
        move || vec![viewport.get().is_some()],
        |fixed: &bool| *fixed,
        move |fixed| surface_frame(Rc::clone(&shown), fixed),
    )?;
    let surface = Container::new(
        growing().width(SizeDimension::Percent(1.0)),
        vec![box_item(surface), interactions::frame_layer(state)?],
    )?;
    let box_style = move || {
        let edge = edge(device.get());
        let style = LayoutStyle::column().padding_all(edge);
        match viewport.get() {
            Some(size) => style
                .width(size.width * scale.get() + 2.0 * edge)
                .height(size.height * scale.get() + 2.0 * edge)
                .flex_shrink(0.0),
            None => style.flex_grow(1.0).min_width(0.0).min_height(0.0),
        }
    };
    let viewport_box = StyledContainer::new(
        box_style(),
        move |_| frame_paint(device.get()),
        vec![
            box_item(checker),
            box_item(surface),
            grid,
            inspection_marks(state.inspection())?,
        ],
    )?
    .styled_by(box_style);
    let box_rect =
        track_layout(viewport_box.layout_node()).expect("the viewport box is registered");

    let stage_rect: Rc<OnceCell<RwSignal<Rect>>> = Rc::default();
    let resize = Resize {
        settings,
        viewport: box_rect,
        stage: Rc::clone(&stage_rect),
        scale,
    };
    let top = row(
        move || match viewport.get() {
            Some(_) => LayoutStyle::row(),
            None => LayoutStyle::row().flex_grow(1.0).min_height(0.0),
        },
        vec![box_item(viewport_box), handle(ResizeEdge::End, &resize)?],
    )?;
    let bottom = row(
        || LayoutStyle::row().height(HANDLE).flex_shrink(0.0),
        vec![
            handle(ResizeEdge::Bottom, &resize)?,
            handle(ResizeEdge::Corner, &resize)?,
        ],
    )?;
    let column_style = move || match viewport.get() {
        Some(_) => LayoutStyle::column().flex_shrink(0.0),
        None => growing().width(SizeDimension::Percent(1.0)),
    };
    let column = Container::new(column_style(), children![top, bottom])?.styled_by(column_style);
    let stage = Container::new(
        growing()
            .padding_all(STAGE_MARGIN)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        children![column],
    )?;
    let stage_rect = *stage_rect
        .get_or_init(|| track_layout(stage.layout_node()).expect("the stage is registered"));
    let rulers = guides::rulers(
        settings,
        Rc::downgrade(&canvas),
        guides::Measured {
            frame: box_rect,
            stage: stage_rect,
            edge: move || edge(device.get()),
        },
        STAGE_MARGIN,
    )?;
    let stage = Container::new(growing(), vec![box_item(stage), rulers])?;
    follow_zoom(settings, viewport, stage_rect, device, scale, &canvas);
    Ok(Stage {
        item: box_item(ClippedItem::new(box_item(stage), Clip::both().paint_only())),
        size: Some(SizeLabel {
            canvas: Rc::downgrade(&canvas),
            scale,
        }),
    })
}

/// Keeps the canvas at the zoom the toolbar asks for: a fixed one, or, to fit, actual size unless the stage cannot show the whole canvas at that.
fn follow_zoom(
    settings: RwSignal<CanvasSettings>,
    viewport: RwSignal<Option<Size>>,
    stage: RwSignal<Rect>,
    device: Memo<bool>,
    scale: RwSignal<f32>,
    canvas: &Rc<SurfaceCanvas>,
) {
    let zoom = memo(move || settings.with(|settings| settings.zoom));
    let canvas = Rc::downgrade(canvas);
    effect(move || {
        let next = zoom.get().fixed_scale().unwrap_or_else(|| {
            viewport.get().map_or(1.0, |size| {
                fit_scale(size, room(stage.get(), edge(device.get())))
            })
        });
        scale.set_if_changed(next);
        if let Some(canvas) = canvas.upgrade() {
            canvas.set_scale(next);
        }
    });
}

/// The canvas inside the frame's box: sized by the canvas where it has a fixed viewport, so the zoom never rounds its logical size, and filling the box where it takes the stage's.
fn surface_frame(
    canvas: Rc<SurfaceCanvas>,
    fixed: bool,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let style = growing().width(SizeDimension::Percent(1.0));
    let frame = if fixed {
        SurfaceFrame::sized(canvas, LayoutStyle::new().flex_shrink(0.0))?
    } else {
        SurfaceFrame::filling(canvas, style)?
    };
    Ok(box_item(frame))
}

/// How far the frame's box sits outside the canvas on each side: a bezel around a device, a hairline otherwise.
fn edge(device: bool) -> f32 {
    if device { BEZEL } else { BORDER }
}

fn frame_paint(device: bool) -> RectStyle {
    if device {
        return RectStyle::default()
            .with_fill(BEZEL_COLOR)
            .with_radius(BorderRadius::all(BEZEL_RADIUS));
    }
    let tokens = use_workbench_tokens();
    RectStyle::default().with_border(Border::uniform(tokens.border_subtle, BORDER))
}

/// The largest canvas a stage of `stage` shows whole inside a frame `edge` thick, in the stage's px.
fn room(stage: Rect, edge: f32) -> Size {
    let chrome = 2.0 * STAGE_MARGIN + HANDLE + 2.0 * edge;
    Size::new(stage.width - chrome, stage.height - chrome)
}

/// The canvas's size as `W × H`, in its own logical px whatever the zoom, and the zoom where it is not actual size, which resets the viewport when pressed.
#[derive(Clone)]
pub(super) struct SizeLabel {
    canvas: Weak<SurfaceCanvas>,
    scale: RwSignal<f32>,
}

impl SizeLabel {
    pub(super) fn build(self, state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let (canvas, scale) = (self.canvas, self.scale);
        let text = Text::declaring(
            move || {
                let size = canvas.upgrade().map_or(Size::ZERO, |canvas| canvas.size());
                let size = format!("{} × {}", size.width.round(), size.height.round());
                let percent = (scale.get() * 100.0).round();
                if percent == 100.0 {
                    size
                } else {
                    format!("{size} · {percent}%")
                }
            },
            LayoutStyle::new(),
            |text| workbench_muted(workbench_mono(text)).with_text_wrap(TextWrap::NoWrap),
        )?;
        let settings = state.canvas_settings();
        let label = StyledContainer::new(
            LayoutStyle::row().padding_horizontal(WORKBENCH_GRID / 2.0),
            |_| RectStyle::default(),
            children![text],
        )?
        .control(Role::Button)
        .a11y_label(|| strings::text(VIEWPORT_RESET))
        .on_press(move || {
            settings.update(|settings| {
                settings.size = CanvasSize::Preview;
                settings.rotated = false;
            })
        });
        Ok(box_item(label))
    }
}

/// What a handle reads and writes: the settings it sets a custom size in, the frame's box, the stage's and the zoom the canvas is shown at.
#[derive(Clone)]
struct Resize {
    settings: RwSignal<CanvasSettings>,
    viewport: RwSignal<Rect>,
    stage: Rc<OnceCell<RwSignal<Rect>>>,
    scale: RwSignal<f32>,
}

impl Resize {
    /// The canvas's size, in its own logical px.
    fn current(&self) -> Size {
        let device = self
            .settings
            .peek_with(|settings| settings.device().is_some());
        logical(inner(self.viewport.peek(), edge(device)), self.scale.peek())
    }

    /// The largest canvas the stage shows whole at the zoom the toolbar asks for, in the canvas's logical px. A custom size has no bezel, so it is measured inside a hairline frame.
    fn room(&self) -> Size {
        let stage = self.stage.get().map_or(Rect::default(), |rect| rect.peek());
        let zoom = self
            .settings
            .peek_with(|settings| settings.zoom.fixed_scale().unwrap_or(1.0));
        logical(room(stage, BORDER), zoom)
    }

    fn set(&self, size: Size) {
        let size = clamp_viewport(size, self.room());
        self.settings.update(|settings| {
            settings.size = CanvasSize::Custom {
                width: size.width,
                height: size.height,
            };
            settings.rotated = false;
        });
    }
}

/// A handle on `edge` of the centred frame, in the canvas's logical px whatever the zoom. A cancelled drag puts back the settings it started from, a preset or the preview's own size included.
fn handle(edge: ResizeEdge, resize: &Resize) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let settings = resize.settings;
    let before: Rc<RefCell<Option<CanvasSettings>>> = Rc::default();
    let (measured, bounded, resized) = (resize.clone(), resize.clone(), resize.clone());
    let mut props = ResizeHandleProps::props()
        .edge(edge)
        .size(Reactive::of(move || measured.current()))
        .on_resize(Rc::new(move |size| resized.set(size)))
        .min(Size::new(VIEWPORT_MIN, VIEWPORT_MIN))
        .max(Reactive::of(move || bounded.room()))
        .growth(ResizeGrowth::Centred)
        .scale(resize.scale)
        .step(KEY_STEP)
        .thickness(HANDLE)
        .on_start(Rc::new({
            let before = Rc::clone(&before);
            move || *before.borrow_mut() = Some(settings.peek())
        }))
        .on_end(Rc::new(move |kept| {
            if let Some(before) = before.take()
                && !kept
            {
                settings.set(before);
            }
        }))
        .build();
    props.label = match edge {
        ResizeEdge::End => Some(Reactive::of(|| strings::text(VIEWPORT_WIDTH))),
        ResizeEdge::Bottom => Some(Reactive::of(|| strings::text(VIEWPORT_HEIGHT))),
        ResizeEdge::Corner => None,
    };
    resize_handle(props)
}

/// A transparent background shows through to the checker; anything else covers it, so it is not drawn.
fn checker(rect: Rect, edge: f32, background: Option<Color>) -> RenderNode {
    if background.is_none_or(|color| color.a >= 1.0) {
        return RenderNode::Empty;
    }
    let tokens = use_workbench_tokens();
    let inside = inner(rect, edge);
    let area = Rect::new(rect.x + edge, rect.y + edge, inside.width, inside.height);
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

fn inner(rect: Rect, edge: f32) -> Size {
    Size::new(
        (rect.width - 2.0 * edge).max(0.0),
        (rect.height - 2.0 * edge).max(0.0),
    )
}

/// `shown` outer px, at `scale`, in the canvas's logical px.
fn logical(shown: Size, scale: f32) -> Size {
    Size::new(shown.width / scale, shown.height / scale)
}

fn row(
    style: impl Fn() -> LayoutStyle + 'static,
    items: Vec<Box<dyn LayoutItem>>,
) -> Result<StyledContainer, LayoutError> {
    Ok(StyledContainer::new(style(), |_| RectStyle::default(), items)?.styled_by(style))
}

fn growing() -> LayoutStyle {
    LayoutStyle::column()
        .flex_grow(1.0)
        .min_width(0.0)
        .min_height(0.0)
}
