//! Where Taffy put a box, against where the browser puts it from the same style.
//!
//! This backend's whole claim is that the two agree: Taffy computes the rects that hit-testing, scrolling and every anchored overlay read, while CSS is what actually positions the elements a person sees. Nothing forces them to be the same answer — `LayoutStyle::to_css` is a second statement of the style, and every property it forgets, spells differently or resolves the other way is a box drawn somewhere its own framework does not believe it is.
//!
//! So the tree here is built twice from one style: once in the layout engine, and once in a document. The rects Taffy computed ride into the frame on each element and the reconciler writes them out beside the elements it creates; what is compared is that attribute against `getBoundingClientRect`.
//!
//! Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test` — see the crate README.

#![cfg(target_arch = "wasm32")]

use std::sync::Arc;

use geometry_core::Rect;
use layout_core::{
    AlignItems, AvailableSpace, Direction, JustifyContent, LayoutEngine, LayoutStyle, NodeId,
    SizeDimension, TemplateTrack,
};
use renderer_core::{DrawCommand, Element, ElementId, RenderBackend, Semantics};
use telar_renderer_dom::DomRenderer;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

/// How far apart the two answers may be.
///
/// Taffy rounds a finished layout to whole pixels; a browser lays out in fractions of one and reports them. A third of a pixel of disagreement over a `1fr` split is that rounding and nothing else. Every mistake this test exists to catch is off by the whole of whatever was dropped — a padding counted twice, a gap the browser never heard about, an absolute child that escaped its parent — and none of them lands under a pixel.
const TOLERANCE: f32 = 1.0;

const SURFACE: (f32, f32) = (800.0, 600.0);

/// A box and what is inside it, before either engine has seen it.
struct Spec {
    style: LayoutStyle,
    children: Vec<Spec>,
}

fn a_box(style: LayoutStyle) -> Spec {
    Spec {
        style,
        children: Vec::new(),
    }
}

fn holding(style: LayoutStyle, children: Vec<Spec>) -> Spec {
    Spec { style, children }
}

fn sized(width: f32, height: f32) -> Spec {
    a_box(LayoutStyle::new().width(width).height(height))
}

/// The tree as the engine now knows it, kept because the engine tells nobody who a node's children are.
struct Built {
    node: NodeId,
    children: Vec<Built>,
}

fn build(engine: &mut LayoutEngine, spec: Spec) -> Built {
    let children: Vec<Built> = spec
        .children
        .into_iter()
        .map(|child| build(engine, child))
        .collect();
    let ids: Vec<NodeId> = children.iter().map(|child| child.node).collect();
    let node = engine
        .new_container(spec.style, &ids)
        .expect("the engine took the style");
    Built { node, children }
}

/// The frame a widget tree would have emitted for these boxes: an element apiece, carrying the CSS it asked for and the rect Taffy gave it.
fn frame(engine: &LayoutEngine, built: &Built, origin: (f32, f32), out: &mut Vec<DrawCommand>) {
    let local = engine.layout(built.node).expect("the node was laid out");
    let rect = Rect::new(
        origin.0 + local.x,
        origin.1 + local.y,
        local.width,
        local.height,
    );
    let css = engine
        .declared_style(built.node)
        .expect("the node kept its style")
        .to_css(engine.direction());
    out.push(DrawCommand::PushElement {
        element: Arc::new(Element::new(
            ElementId(built.node.into()),
            Semantics::group(),
            css.into_string(),
            rect,
        )),
    });
    for child in &built.children {
        frame(engine, child, (rect.x, rect.y), out);
    }
    out.push(DrawCommand::PopElement);
}

fn document() -> web_sys::Document {
    web_sys::window()
        .and_then(|window| window.document())
        .expect("a document")
}

/// An element to mount the frame in, sized to the surface the layout was computed for.
fn host() -> web_sys::HtmlElement {
    let host: web_sys::HtmlElement = document()
        .create_element("div")
        .expect("a host element")
        .dyn_into()
        .expect("a div is an HTML element");
    let style = host.style();
    let _ = style.set_property("width", &format!("{}px", SURFACE.0));
    let _ = style.set_property("height", &format!("{}px", SURFACE.1));
    let _ = style.set_property("overflow", "hidden");
    // What the reconciler reads to write each box's computed rect out beside it, which is the whole of what this test compares against.
    let _ = host.set_attribute("data-telar-audit", "");
    document()
        .body()
        .expect("a body")
        .append_child(host.as_ref())
        .expect("the host went into the page");
    host
}

/// Every box the frame left in the document, as (what Taffy computed, what the browser laid out) in the host's own coordinates.
fn placements(host: &web_sys::HtmlElement) -> Vec<(Rect, Rect)> {
    let origin = host.get_bounding_client_rect();
    let boxes = host
        .query_selector_all("[data-telar-rect]")
        .expect("the audit attribute is a valid selector");
    (0..boxes.length())
        .filter_map(|index| boxes.item(index))
        .filter_map(|node| node.dyn_into::<web_sys::Element>().ok())
        .map(|element| {
            let declared = element
                .get_attribute("data-telar-rect")
                .expect("the node matched on having one");
            let numbers: Vec<f32> = declared
                .split_whitespace()
                .map(|value| value.parse().expect("the audit writes four numbers"))
                .collect();
            let computed = Rect::new(numbers[0], numbers[1], numbers[2], numbers[3]);
            let laid_out = element.get_bounding_client_rect();
            let shown = Rect::new(
                (laid_out.x() - origin.x()) as f32,
                (laid_out.y() - origin.y()) as f32,
                laid_out.width() as f32,
                laid_out.height() as f32,
            );
            (computed, shown)
        })
        .collect()
}

fn agree(case: &str, placements: &[(Rect, Rect)]) {
    assert!(
        !placements.is_empty(),
        "{case}: the frame put no boxes in the document"
    );
    for (index, (computed, shown)) in placements.iter().enumerate() {
        for (edge, expected, got) in [
            ("x", computed.x, shown.x),
            ("y", computed.y, shown.y),
            ("width", computed.width, shown.width),
            ("height", computed.height, shown.height),
        ] {
            assert!(
                (expected - got).abs() <= TOLERANCE,
                "{case}: box {index} {edge} — taffy says {expected}, the browser says {got} \
                 (taffy {computed:?}, browser {shown:?})"
            );
        }
    }
}

/// Lays `root` out both ways and asserts every box came out in the same place.
fn parity(case: &str, direction: Direction, root: Spec) {
    parity_over(case, direction, root, 1);
}

/// The same, over `frames` renders of one unchanged frame — so the reconcile that *reuses* an element is held to the placement the one that created it got.
fn parity_over(case: &str, direction: Direction, root: Spec, frames: usize) {
    let mut engine = LayoutEngine::new();
    engine.set_direction(direction);
    let built = build(&mut engine, root);
    engine
        .compute_layout(
            built.node,
            AvailableSpace::Definite(SURFACE.0),
            AvailableSpace::Definite(SURFACE.1),
        )
        .expect("the tree laid out");
    let mut commands = Vec::new();
    frame(&engine, &built, (0.0, 0.0), &mut commands);

    let host = host();
    let mut renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    for pass in 1..=frames {
        renderer
            .render_frame(&commands, None)
            .expect("the frame reconciled");
        agree(&format!("{case} (frame {pass})"), &placements(&host));
    }
    host.remove();
}

/// The surface's own box, which an application computes and places itself.
fn surface(style: LayoutStyle, children: Vec<Spec>) -> Spec {
    holding(style.width(SURFACE.0).height(SURFACE.1), children)
}

/// Taffy's size is the border box and CSS's default is not, so a padded box comes out wider than it was computed unless `box-sizing` is stated. Left out, every box below the first drifted further down the page than the rect hit-testing reads.
#[wasm_bindgen_test]
fn padding_stays_inside_the_box_it_was_asked_for() {
    parity(
        "a padded row of fixed boxes",
        Direction::Ltr,
        surface(
            LayoutStyle::new().flex_row().padding_all(24.0).gap(12.0),
            vec![sized(120.0, 80.0), sized(200.0, 80.0), sized(60.0, 80.0)],
        ),
    );
}

#[wasm_bindgen_test]
fn growth_splits_the_leftover_space() {
    parity(
        "a row of grown boxes",
        Direction::Ltr,
        surface(
            LayoutStyle::new().flex_row().gap(16.0),
            vec![
                a_box(LayoutStyle::new().flex_grow(1.0).height(40.0)),
                a_box(LayoutStyle::new().flex_grow(2.0).height(40.0)),
                a_box(
                    LayoutStyle::new()
                        .width(100.0)
                        .flex_shrink(0.0)
                        .height(40.0),
                ),
            ],
        ),
    );
}

#[wasm_bindgen_test]
fn a_column_stacks_by_its_gap() {
    parity(
        "a column",
        Direction::Ltr,
        surface(
            LayoutStyle::new().flex_column().gap_y(20.0).gap_x(4.0),
            vec![sized(300.0, 50.0), sized(300.0, 90.0), sized(300.0, 10.0)],
        ),
    );
}

#[wasm_bindgen_test]
fn alignment_puts_a_short_box_where_it_says() {
    parity(
        "centred and spaced",
        Direction::Ltr,
        surface(
            LayoutStyle::new()
                .flex_row()
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::SPACE_BETWEEN)
                .padding_all(16.0),
            vec![sized(80.0, 40.0), sized(80.0, 120.0), sized(80.0, 200.0)],
        ),
    );
}

#[wasm_bindgen_test]
fn a_grid_places_every_track() {
    parity(
        "a three-column grid",
        Direction::Ltr,
        surface(
            LayoutStyle::new()
                .display_grid()
                .grid_template_columns(vec![TemplateTrack::repeat(3, TemplateTrack::fr(1.0))])
                .gap(10.0)
                .padding_all(12.0),
            (0..6)
                .map(|_| a_box(LayoutStyle::new().height(70.0)))
                .collect(),
        ),
    );
}

/// What `grid cols:"fit 150"` compiles to, and the case that made the sandbox stack one card per row.
#[wasm_bindgen_test]
fn an_auto_fitting_grid_agrees_on_how_many_fitted() {
    parity(
        "an auto-fit grid",
        Direction::Ltr,
        surface(
            LayoutStyle::new()
                .display_grid()
                .grid_template_columns(vec![TemplateTrack::fit(TemplateTrack::minmax(
                    TemplateTrack::px(150.0),
                    TemplateTrack::fr(1.0),
                ))])
                .gap(16.0),
            (0..7)
                .map(|_| a_box(LayoutStyle::new().height(60.0)))
                .collect(),
        ),
    );
}

#[wasm_bindgen_test]
fn a_spanning_cell_covers_the_tracks_it_claims() {
    parity(
        "a grid with a span",
        Direction::Ltr,
        surface(
            LayoutStyle::new()
                .display_grid()
                .grid_template_columns(vec![TemplateTrack::repeat(4, TemplateTrack::fr(1.0))])
                .gap(8.0),
            vec![
                a_box(LayoutStyle::new().height(50.0).grid_column_span(2)),
                a_box(LayoutStyle::new().height(50.0)),
                a_box(LayoutStyle::new().height(50.0)),
            ],
        ),
    );
}

/// Taffy resolves an absolute child against its parent node; a document resolves it against the nearest *positioned* ancestor, and a `static` box is not one. Left alone, a slider's fill escaped to the page and came out window-height.
#[wasm_bindgen_test]
fn an_absolute_child_stays_inside_the_box_that_holds_it() {
    parity(
        "an overlay pinned to its parent",
        Direction::Ltr,
        surface(
            LayoutStyle::new().flex_column().padding_all(30.0),
            vec![holding(
                LayoutStyle::new().width(400.0).height(200.0),
                vec![
                    a_box(LayoutStyle::new().absolute_fill()),
                    a_box(
                        LayoutStyle::new()
                            .absolute()
                            .inset_top(12.0)
                            .inset_start(SizeDimension::Px(20.0))
                            .width(60.0)
                            .height(60.0),
                    ),
                ],
            )],
        ),
    );
}

#[wasm_bindgen_test]
fn percentages_resolve_against_the_same_parent() {
    parity(
        "percentage sizes",
        Direction::Ltr,
        surface(
            LayoutStyle::new().flex_column().padding_all(40.0),
            vec![holding(
                LayoutStyle::new()
                    .width(SizeDimension::Percent(0.5))
                    .height(SizeDimension::Percent(0.25))
                    .padding_all(24.0),
                vec![a_box(
                    LayoutStyle::new()
                        .width(SizeDimension::Percent(1.0))
                        .height(20.0),
                )],
            )],
        ),
    );
}

/// A divergence this test found and neither this crate nor the CSS it writes can fix: taffy places a **block** container's children at half its resolved percentage padding.
///
/// It reports the padding correctly — `Layout::padding` comes back 72 for `10%` of a 720px containing block, and the box's own content width is sized from that 72 — and then puts the first child at 36. Its flex and grid algorithms both place it at 72, which is what CSS says and what the browser does, so this is taffy disagreeing with itself rather than with the web.
///
/// Left as a failing case on purpose: writing the wrong offset into an assertion would make the bug the specification. It is upstream's to fix, and un-ignoring this is how the fix gets noticed. It is not a web problem either — `LayoutStyle::new()` is a block box, so `pad:10%` misplaces its content on the desktop and in the terminal too; the browser is only what made it visible.
#[wasm_bindgen_test]
#[ignore = "taffy places a block container's children at half its percentage padding"]
fn percentage_padding_puts_the_content_where_it_reserved_room_for_it() {
    parity(
        "percentage padding",
        Direction::Ltr,
        surface(
            LayoutStyle::new().flex_column().padding_all(40.0),
            vec![holding(
                LayoutStyle::new()
                    .width(SizeDimension::Percent(0.5))
                    .height(SizeDimension::Percent(0.25))
                    .padding_all(SizeDimension::Percent(0.1)),
                vec![a_box(
                    LayoutStyle::new()
                        .width(SizeDimension::Percent(1.0))
                        .height(20.0),
                )],
            )],
        ),
    );
}

#[wasm_bindgen_test]
fn bounds_hold_a_grown_box_back() {
    parity(
        "min and max",
        Direction::Ltr,
        surface(
            LayoutStyle::new().flex_row().gap(10.0),
            vec![
                a_box(
                    LayoutStyle::new()
                        .flex_grow(1.0)
                        .max_width(150.0)
                        .height(40.0),
                ),
                a_box(
                    LayoutStyle::new()
                        .flex_grow(1.0)
                        .min_width(400.0)
                        .height(40.0),
                ),
                a_box(LayoutStyle::new().flex_grow(1.0).height(40.0)),
            ],
        ),
    );
}

#[wasm_bindgen_test]
fn a_wrapped_row_breaks_on_the_same_box() {
    parity(
        "a wrapping row",
        Direction::Ltr,
        surface(
            LayoutStyle::new().flex_row().flex_wrap().gap(24.0),
            (0..9)
                .map(|_| a_box(LayoutStyle::new().width(180.0).height(60.0)))
                .collect(),
        ),
    );
}

#[wasm_bindgen_test]
fn an_aspect_ratio_gives_the_same_second_side() {
    parity(
        "an aspect ratio",
        Direction::Ltr,
        surface(
            LayoutStyle::new().flex_row().gap(12.0),
            vec![
                a_box(LayoutStyle::new().width(240.0).aspect_ratio(16.0 / 9.0)),
                a_box(LayoutStyle::new().height(120.0).aspect_ratio(1.0)),
            ],
        ),
    );
}

/// A logical edge is resolved once, by the same `resolve` the layout pass runs — so what the browser is told is already physical and needs no `dir` of its own. If the two ever resolved separately, this is the case that would come apart.
#[wasm_bindgen_test]
fn an_rtl_row_starts_at_the_right_in_both() {
    parity(
        "a right-to-left row",
        Direction::Rtl,
        surface(
            LayoutStyle::new()
                .flex_row()
                .gap(12.0)
                .padding_start(SizeDimension::Px(48.0))
                .padding_end(SizeDimension::Px(8.0)),
            vec![sized(100.0, 40.0), sized(140.0, 40.0), sized(60.0, 40.0)],
        ),
    );
}

/// Nothing here is exotic; what is being asked is whether four levels of padding, gap and growth accumulate the same way twice. A per-level error of one pixel is invisible at the top and obvious at the bottom.
#[wasm_bindgen_test]
fn a_deep_tree_does_not_drift() {
    let leaf = || a_box(LayoutStyle::new().flex_grow(1.0).height(30.0));
    let inner = || {
        holding(
            LayoutStyle::new().flex_row().gap(6.0).padding_all(6.0),
            vec![leaf(), leaf(), leaf()],
        )
    };
    let middle = || {
        holding(
            LayoutStyle::new().flex_column().gap(10.0).padding_all(14.0),
            vec![inner(), inner()],
        )
    };
    parity(
        "four levels deep",
        Direction::Ltr,
        surface(
            LayoutStyle::new().flex_column().gap(18.0).padding_all(22.0),
            vec![middle(), middle(), middle()],
        ),
    );
}

/// The reconcile reuses the element it made last frame and writes only what changed. A box that is right when it is created and wrong when it is kept is the failure this backend's whole design risks.
#[wasm_bindgen_test]
fn a_reused_element_stays_where_it_was_put() {
    parity_over(
        "three renders of one frame",
        Direction::Ltr,
        surface(
            LayoutStyle::new().flex_row().gap(12.0).padding_all(20.0),
            vec![
                a_box(LayoutStyle::new().flex_grow(1.0).height(80.0)),
                holding(
                    LayoutStyle::new().flex_column().gap(8.0).width(220.0),
                    vec![sized(180.0, 40.0), sized(180.0, 40.0)],
                ),
            ],
        ),
        3,
    );
}
