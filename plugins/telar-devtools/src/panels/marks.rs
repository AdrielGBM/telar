//! The marks the panels draw over what is inspected: every component's box, the selected one's box model with its size and distances, and the row chosen in the accessibility panel.

use std::cell::OnceCell;
use std::rc::Rc;

use telar::{
    Accessible, Border, BorderRadius, Canvas, Insets, LayoutError, LayoutItem, LayoutStyle, Rect,
    RectStyle, RenderNode, RwSignal, ShapeStyle, StyledContainer, TextStyle, TextWrap, Transform,
    box_item, track_layout, transform_clip_rect, use_theme_tokens,
};

use super::accessibility::chosen_rect;
use super::inspector::size;
use super::number;
use crate::inspect::{InspectedNode, Inspection};
use crate::workbench::use_workbench_tokens;

const OUTLINE_ALPHA: f32 = 0.45;
const BAND_ALPHA: f32 = 0.22;
const CONTENT_ALPHA: f32 = 0.12;
const STROKE: f32 = 1.5;
const HAIRLINE: f32 = 1.0;
const LABEL_SIZE: f32 = 11.0;
const LABEL_PAD: f32 = 4.0;
const LABEL_GAP: f32 = 4.0;
/// How wide a monospace glyph is for its size, to size a label's pill without shaping it.
const GLYPH_ASPECT: f32 = 0.62;

/// A layer that fills the box it is given and draws the inspection's marks over what is inspected, through the source's placement, clipped to that box. It answers no pointer and tells a reader nothing, so whatever it covers keeps both.
///
/// The selection is marked while an inspector panel is mounted, and the accessibility panel's chosen row while one of those is; the outline and the measure follow their toggles.
pub fn inspection_marks(inspection: Inspection) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let placed_at: Rc<OnceCell<RwSignal<Rect>>> = Rc::default();
    let origin = Rc::clone(&placed_at);
    let canvas = Canvas::new(LayoutStyle::new().absolute_fill(), move |area| {
        let offset = origin.get().map(|rect| rect.get()).unwrap_or_default();
        let to_local = inspection
            .placement
            .get()
            .then(Transform::translate(-offset.x, -offset.y));
        let marks = marks(inspection, to_local);
        if marks.is_empty() {
            return RenderNode::Empty;
        }
        RenderNode::clip(area, BorderRadius::zero(), marks)
    })?;
    if let Some(rect) = track_layout(canvas.layout_node()) {
        let _ = placed_at.set(rect);
    }
    let layer = StyledContainer::new(
        LayoutStyle::new().absolute_fill(),
        |_| RectStyle::default(),
        vec![box_item(canvas.a11y_hidden())],
    )?
    .input_transparent();
    Ok(box_item(layer))
}

fn marks(inspection: Inspection, to_local: Transform) -> Vec<RenderNode> {
    let place = |rect: Rect| transform_clip_rect(to_local.to_array(), rect);
    let tokens = use_theme_tokens();
    let accent = tokens.primary();
    let mut marks = Vec::new();
    if inspection.outline.get() {
        let line = RectStyle::default()
            .with_border(Border::uniform(accent.with_alpha(OUTLINE_ALPHA), HAIRLINE));
        inspection.nodes.with(|nodes| {
            marks.extend(
                nodes
                    .iter()
                    .filter(|node| drawn(node.rect))
                    .map(|node| RenderNode::rect(place(node.rect), line)),
            );
        });
    }
    let selected = (inspection.inspecting.get() > 0 || inspection.measure.get())
        .then(|| inspection.selected_node())
        .flatten()
        .filter(|node| drawn(node.rect));
    if let Some(node) = selected {
        if inspection.inspecting.get() > 0 {
            box_model(&node, &place, &mut marks);
        }
        if inspection.measure.get() {
            measure(inspection, &node, &place, &mut marks);
        }
    }
    if inspection.auditing.get() > 0
        && let Some(chosen) = inspection.access_selected.get()
        && let Some(rect) = inspection.audit.with(|audit| {
            audit
                .as_deref()
                .and_then(|audit| chosen_rect(audit, &chosen))
        })
        && drawn(rect)
    {
        let tone = if chosen.starts_with(super::VIOLATIONS_ROW) {
            tokens.error()
        } else {
            tokens.info()
        };
        marks.push(RenderNode::rect(
            place(rect),
            RectStyle::default()
                .with_fill(tone.with_alpha(CONTENT_ALPHA))
                .with_border(Border::uniform(tone, STROKE)),
        ));
    }
    marks
}

/// The selected box's margin, border, padding and content, each a band of its own colour, under the box's outline.
fn box_model(node: &InspectedNode, place: &impl Fn(Rect) -> Rect, marks: &mut Vec<RenderNode>) {
    let tokens = use_theme_tokens();
    let border_box = node.rect;
    let margin_box = outset(border_box, node.margin);
    let padding_box = inset(border_box, node.border);
    let content_box = inset(padding_box, node.padding);
    let bands = [
        (margin_box, border_box, tokens.warning()),
        (border_box, padding_box, tokens.muted()),
        (padding_box, content_box, tokens.success()),
    ];
    for (outer, inner, tone) in bands {
        let fill = RectStyle::default().with_fill(tone.with_alpha(BAND_ALPHA));
        marks.extend(
            ring(outer, inner)
                .into_iter()
                .filter(|strip| drawn(*strip))
                .map(|strip| RenderNode::rect(place(strip), fill)),
        );
    }
    let accent = tokens.primary();
    marks.push(RenderNode::rect(
        place(content_box),
        RectStyle::default().with_fill(accent.with_alpha(CONTENT_ALPHA)),
    ));
    marks.push(RenderNode::rect(
        place(border_box),
        RectStyle::default().with_border(Border::uniform(accent, STROKE)),
    ));
}

/// The selected box's size under it, and how far each of its edges stands from the same edge of the box it is mounted in.
fn measure(
    inspection: Inspection,
    node: &InspectedNode,
    place: &impl Fn(Rect) -> Rect,
    marks: &mut Vec<RenderNode>,
) {
    let accent = use_theme_tokens().primary();
    let shown = place(node.rect);
    marks.push(label(
        &size(telar::Size::new(node.rect.width, node.rect.height)),
        shown.x,
        shown.y + shown.height + LABEL_GAP,
    ));
    let Some(parent) = inspection.nodes.with(|nodes| enclosing(nodes, node)) else {
        return;
    };
    let (r, p) = (node.rect, parent.rect);
    let (mid_x, mid_y) = (r.x + r.width / 2.0, r.y + r.height / 2.0);
    let gaps = [
        (Rect::new(p.x, mid_y, r.x - p.x, 0.0), r.x - p.x),
        (
            Rect::new(r.x + r.width, mid_y, p.x + p.width - r.x - r.width, 0.0),
            p.x + p.width - r.x - r.width,
        ),
        (Rect::new(mid_x, p.y, 0.0, r.y - p.y), r.y - p.y),
        (
            Rect::new(mid_x, r.y + r.height, 0.0, p.y + p.height - r.y - r.height),
            p.y + p.height - r.y - r.height,
        ),
    ];
    let line = RectStyle::default().with_fill(accent);
    for (span, distance) in gaps {
        if distance <= 0.0 {
            continue;
        }
        let shown = place(span);
        let stroke = Rect::new(
            shown.x - HAIRLINE / 2.0,
            shown.y - HAIRLINE / 2.0,
            shown.width.max(HAIRLINE),
            shown.height.max(HAIRLINE),
        );
        marks.push(RenderNode::rect(stroke, line));
        marks.push(label(
            &number(distance),
            shown.x + shown.width / 2.0 + LABEL_GAP,
            shown.y + shown.height / 2.0 + LABEL_GAP,
        ));
    }
}

/// The nearest component before `node` in the walk that is shallower than it: the one it is mounted in.
fn enclosing(nodes: &[InspectedNode], node: &InspectedNode) -> Option<InspectedNode> {
    let at = nodes.iter().position(|candidate| candidate.id == node.id)?;
    nodes[..at]
        .iter()
        .rev()
        .find(|candidate| candidate.depth < node.depth && drawn(candidate.rect))
        .cloned()
}

/// A measurement in a pill of the accent, its top left at `x`, `y`.
fn label(text: &str, x: f32, y: f32) -> RenderNode {
    let tokens = use_theme_tokens();
    let mono = use_workbench_tokens().mono_family;
    let width = text.chars().count() as f32 * LABEL_SIZE * GLYPH_ASPECT + 2.0 * LABEL_PAD;
    let height = LABEL_SIZE + 2.0 * LABEL_PAD;
    let pill = Rect::new(x, y, width, height);
    RenderNode::group([
        RenderNode::rect(
            pill,
            RectStyle::default()
                .with_fill(tokens.primary())
                .with_radius(BorderRadius::all(height / 2.0)),
        ),
        RenderNode::text(
            text.to_owned(),
            Rect::new(
                x + LABEL_PAD,
                y + LABEL_PAD,
                width - 2.0 * LABEL_PAD,
                LABEL_SIZE,
            ),
            TextStyle::new(LABEL_SIZE, tokens.on_primary())
                .with_font_family(mono)
                .with_text_wrap(TextWrap::NoWrap),
        ),
    ])
}

fn drawn(rect: Rect) -> bool {
    rect.width > 0.0 && rect.height > 0.0
}

fn inset(rect: Rect, by: Insets) -> Rect {
    Rect::new(
        rect.x + by.left,
        rect.y + by.top,
        (rect.width - by.left - by.right).max(0.0),
        (rect.height - by.top - by.bottom).max(0.0),
    )
}

fn outset(rect: Rect, by: Insets) -> Rect {
    Rect::new(
        rect.x - by.left,
        rect.y - by.top,
        rect.width + by.left + by.right,
        rect.height + by.top + by.bottom,
    )
}

/// What `outer` covers outside `inner`, as four strips: the full-width top and bottom, and the sides between them.
pub(crate) fn ring(outer: Rect, inner: Rect) -> [Rect; 4] {
    let top = (inner.y - outer.y).max(0.0);
    let bottom = (outer.y + outer.height - inner.y - inner.height).max(0.0);
    let left = (inner.x - outer.x).max(0.0);
    let right = (outer.x + outer.width - inner.x - inner.width).max(0.0);
    let middle = (outer.height - top - bottom).max(0.0);
    [
        Rect::new(outer.x, outer.y, outer.width, top),
        Rect::new(
            outer.x,
            outer.y + outer.height - bottom,
            outer.width,
            bottom,
        ),
        Rect::new(outer.x, outer.y + top, left, middle),
        Rect::new(outer.x + outer.width - right, outer.y + top, right, middle),
    ]
}
