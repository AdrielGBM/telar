//! The workshop's own marks around a canvas, as the toolbar turns them on: a grid over it every [`GRID_STEP`] of its logical px, and rulers along the stage measuring it. The stage draws them, so the preview inside never sees them.

use std::rc::Weak;

use telar::{
    Canvas, Color, LayoutError, LayoutItem, LayoutStyle, Rect, RectStyle, RenderNode, RwSignal,
    ShapeStyle, StyledContainer, SurfaceCanvas, TextStyle, TextWrap, Transform, box_item,
    use_theme_tokens,
};
use telar_devtools::use_workbench_tokens;

use crate::settings::{CanvasSettings, GRID_STEP};

/// Every how many grid lines one is drawn stronger.
const MAJOR_EVERY: u32 = 8;
/// The closest two lines or ticks are drawn, in shown px; closer ones are left out.
const MIN_GAP: f32 = 4.0;
/// The closest two ruler labels are drawn, in shown px.
const LABEL_GAP: f32 = 48.0;
/// The canvas px a ruler labels, the smallest that leaves [`LABEL_GAP`] between labels.
const LABEL_STEPS: &[u32] = &[10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10_000];
/// How many ticks a ruler tries to draw between two labels, fewer where they would crowd.
const TICK_DIVISIONS: &[u32] = &[10, 5, 2];
const LINE: f32 = 1.0;
const LABEL_SIZE: f32 = 9.0;
const LABEL_INSET: f32 = 2.0;

/// One line of the grid: how far it sits from the canvas's origin in shown px, and whether it is a major one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct GridLine {
    pub(super) at: f32,
    pub(super) major: bool,
}

/// The lines inside a canvas `extent` logical px long shown at `scale`, its edges left to the frame. The minor lines are left out where they would crowd.
pub(super) fn grid_lines(extent: f32, scale: f32) -> Vec<GridLine> {
    if !(scale > 0.0 && extent > 0.0) {
        return Vec::new();
    }
    let minor = GRID_STEP * scale >= MIN_GAP;
    (1..)
        .map(|index: u32| (index, index as f32 * GRID_STEP))
        .take_while(|(_, at)| *at < extent)
        .filter_map(|(index, at)| {
            let major = index % MAJOR_EVERY == 0;
            (major || minor).then_some(GridLine {
                at: at * scale,
                major,
            })
        })
        .collect()
}

/// One mark of a ruler: how far from the canvas's origin it sits in shown px, and the logical px it labels if it is a labelled one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Tick {
    pub(super) at: f32,
    pub(super) label: Option<i32>,
}

/// The marks of a ruler running from `from` to `to` shown px either side of the canvas's origin, for a canvas shown at `scale`: labels as far apart as reads, and ticks between them as many as fit.
pub(super) fn ticks(from: f32, to: f32, scale: f32) -> Vec<Tick> {
    if !(scale > 0.0 && to > from) {
        return Vec::new();
    }
    let label = LABEL_STEPS
        .iter()
        .copied()
        .find(|&step| step as f32 * scale >= LABEL_GAP)
        .unwrap_or(LABEL_STEPS[LABEL_STEPS.len() - 1]);
    let tick = TICK_DIVISIONS
        .iter()
        .map(|&divisions| label / divisions)
        .find(|&step| step > 0 && step as f32 * scale >= MIN_GAP)
        .unwrap_or(label) as i32;
    let first = (from / scale / tick as f32).ceil() as i32;
    let last = (to / scale / tick as f32).floor() as i32;
    (first..=last)
        .map(|index| {
            let value = index * tick;
            Tick {
                at: value as f32 * scale,
                label: (value % label as i32 == 0).then_some(value),
            }
        })
        .collect()
}

/// The grid over a canvas whose frame's box this fills: the canvas sits `edge` px inside it, at the scale `canvas` is shown at.
pub(super) fn grid(
    settings: RwSignal<CanvasSettings>,
    canvas: Weak<SurfaceCanvas>,
    edge: impl Fn() -> f32 + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    see_through(Canvas::new(
        LayoutStyle::new().absolute_fill(),
        move |rect| {
            if !settings.with(|settings| settings.grid) {
                return RenderNode::Empty;
            }
            let scale = canvas.upgrade().map_or(1.0, |canvas| canvas.scale());
            let edge = edge();
            let area = Rect::new(
                rect.x + edge,
                rect.y + edge,
                (rect.width - 2.0 * edge).max(0.0),
                (rect.height - 2.0 * edge).max(0.0),
            );
            let accent = use_theme_tokens().primary();
            let paint = |line: GridLine| {
                RectStyle::default().with_fill(accent.with_alpha(if line.major {
                    0.35
                } else {
                    0.15
                }))
            };
            let across = grid_lines(area.width / scale, scale)
                .into_iter()
                .map(|line| {
                    RenderNode::rect(
                        Rect::new(area.x + line.at, area.y, LINE, area.height),
                        paint(line),
                    )
                });
            let down = grid_lines(area.height / scale, scale)
                .into_iter()
                .map(|line| {
                    RenderNode::rect(
                        Rect::new(area.x, area.y + line.at, area.width, LINE),
                        paint(line),
                    )
                });
            RenderNode::group(across.chain(down).collect::<Vec<_>>())
        },
    )?)
}

/// Where a ruler measures from: the box of the frame a canvas sits `edge` px inside, and the stage the rulers line.
pub(super) struct Measured<E> {
    pub(super) frame: RwSignal<Rect>,
    pub(super) stage: RwSignal<Rect>,
    pub(super) edge: E,
}

/// Rulers along the top and left of the stage this fills, `band` px deep, their zero at the canvas's origin.
pub(super) fn rulers(
    settings: RwSignal<CanvasSettings>,
    canvas: Weak<SurfaceCanvas>,
    measured: Measured<impl Fn() -> f32 + 'static>,
    band: f32,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    see_through(Canvas::new(
        LayoutStyle::new().absolute_fill(),
        move |rect| {
            if !settings.with(|settings| settings.rulers) {
                return RenderNode::Empty;
            }
            let scale = canvas.upgrade().map_or(1.0, |canvas| canvas.scale());
            let (frame, stage, edge) = (
                measured.frame.get(),
                measured.stage.get(),
                (measured.edge)(),
            );
            let origin_x = frame.x + edge - stage.x;
            let origin_y = frame.y + edge - stage.y;
            let tokens = use_workbench_tokens();
            let band_fill = RectStyle::default().with_fill(tokens.panel_background);
            let ink = tokens.text_muted;
            let label_style = TextStyle::new(LABEL_SIZE, ink)
                .with_font_family(tokens.mono_family)
                .with_text_wrap(TextWrap::NoWrap);
            let mut nodes = vec![
                RenderNode::rect(Rect::new(0.0, 0.0, rect.width, band), band_fill),
                RenderNode::rect(Rect::new(0.0, 0.0, band, rect.height), band_fill),
                RenderNode::rect(
                    Rect::new(band, band - LINE, rect.width - band, LINE),
                    line(ink),
                ),
                RenderNode::rect(
                    Rect::new(band - LINE, band, LINE, rect.height - band),
                    line(ink),
                ),
            ];
            for tick in ticks(band - origin_x, rect.width - origin_x, scale) {
                let x = origin_x + tick.at;
                let length = mark_length(tick, band);
                nodes.push(RenderNode::rect(
                    Rect::new(x, band - length, LINE, length),
                    line(ink),
                ));
                if let Some(value) = tick.label {
                    nodes.push(RenderNode::text(
                        value.to_string(),
                        Rect::new(x + LABEL_INSET, 0.0, LABEL_GAP, band),
                        label_style.clone(),
                    ));
                }
            }
            for tick in ticks(band - origin_y, rect.height - origin_y, scale) {
                let y = origin_y + tick.at;
                let length = mark_length(tick, band);
                nodes.push(RenderNode::rect(
                    Rect::new(band - length, y, length, LINE),
                    line(ink),
                ));
                if let Some(value) = tick.label {
                    // Read bottom to top, as a vertical ruler is: turned a quarter anticlockwise about where it starts.
                    let turned = Transform::rotate_around(-90.0, 0.0, y - LABEL_INSET);
                    nodes.push(RenderNode::transform_with(
                        turned.to_array(),
                        [RenderNode::text(
                            value.to_string(),
                            Rect::new(0.0, y - LABEL_INSET, LABEL_GAP, band),
                            label_style.clone(),
                        )],
                    ));
                }
            }
            RenderNode::group(nodes)
        },
    )?)
}

fn mark_length(tick: Tick, band: f32) -> f32 {
    if tick.label.is_some() {
        band
    } else {
        band * 0.3
    }
}

fn line(ink: Color) -> RectStyle {
    RectStyle::default().with_fill(ink)
}

/// `canvas` drawn over what is under it, leaving the pointer to that.
fn see_through(canvas: Canvas) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let layer = StyledContainer::new(
        LayoutStyle::new().absolute_fill(),
        |_| RectStyle::default(),
        vec![box_item(canvas)],
    )?
    .input_transparent();
    Ok(box_item(layer))
}
