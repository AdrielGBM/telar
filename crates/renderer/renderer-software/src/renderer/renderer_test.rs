use std::iter;
use std::sync::Arc;

use geometry_core::{Point, Rect};
use platform_headless::HeadlessWindow;
use renderer_core::dirty::FrameDiff;
use renderer_core::dirty_scenarios::{self, Plan, Scenario};
use renderer_core::{
    BorderRadius, Color, DrawCommand, Element, ElementId, FontMetrics, PathData, PathStyle,
    RectStyle, RenderBackend, Semantics, Shadow, ShapeStyle, Stroke,
};

use super::SoftwareRenderer;
use super::pixels::{PixelBounds, clamp_to_pixels};
use super::present::FrameOp;
use crate::SoftwareRendererConfig;

// The caches are a thread-local, so which thread builds them decides whether the renderer sees its own fonts. Building them in the constructor furnished the UI thread for a renderer drawing on another. Run on a fresh thread, because the test binary's main thread may already have caches from another case.
#[test]
fn the_drawing_thread_builds_the_caches_not_the_constructing_one() {
    std::thread::spawn(|| {
        assert!(!crate::caches::initialised(), "a fresh thread starts empty");

        let mut renderer = SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(
            8,
            8,
            SoftwareRendererConfig::default(),
        );
        assert!(
            !crate::caches::initialised(),
            "constructing must not load fonts on this thread"
        );

        renderer.begin_frame(8, 8, 1.0, 0).unwrap();
        assert!(
            crate::caches::initialised(),
            "the first frame builds them, on the thread that draws"
        );
    })
    .join()
    .unwrap();
}

const UNTOUCHED: [u8; 4] = [255, 0, 255, 255];
const BACKGROUND: [u8; 4] = [10, 10, 10, 255];
const SMALL: (u32, u32) = (640, 400);

struct Drawn {
    pixels: Vec<u8>,
    frame_op: FrameOp,
}

// With `poison`, every frame after the first starts from a colour no frame draws, so the pixels still wearing it are the ones the renderer chose not to repaint.
fn draw(size: (u32, u32), frames: &[&[DrawCommand]], poison: bool) -> Drawn {
    let (width, height) = size;
    let mut renderer = SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(
        width,
        height,
        SoftwareRendererConfig::default(),
    );
    let [r, g, b, _] = BACKGROUND;
    let mut frame_op = FrameOp::NoChange;
    for (generation, frame) in frames.iter().enumerate() {
        if poison && generation > 0 {
            let [r, g, b, a] = UNTOUCHED;
            renderer
                .pixmap
                .as_mut()
                .expect("a frame was drawn")
                .fill(tiny_skia::Color::from_rgba8(r, g, b, a));
        }
        renderer
            .begin_frame(width, height, 1.0, generation as u64)
            .unwrap();
        frame_op = renderer.render(frame, Some(Color::from_rgb_u8(r, g, b)));
    }
    Drawn {
        pixels: renderer
            .pixmap()
            .expect("a frame was drawn")
            .data()
            .to_vec(),
        frame_op,
    }
}

fn pixel(drawn: &Drawn, width: u32, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * width + x) * 4) as usize;
    drawn.pixels[at..at + 4].try_into().unwrap()
}

fn on_pixels(rects: &[Rect], (width, height): (u32, u32)) -> Vec<PixelBounds> {
    let mut bounds: Vec<PixelBounds> = rects
        .iter()
        .filter_map(|rect| clamp_to_pixels(*rect, width, height))
        .collect();
    bounds.sort_unstable();
    bounds
}

// A blit changes its whole clip but repaints only the band it exposed.
fn regions_of(plan: &Plan) -> (Vec<Rect>, Vec<Rect>) {
    match plan {
        Plan::Damage(rects) => (rects.clone(), rects.clone()),
        Plan::Scroll {
            clip,
            exposed,
            extra,
            ..
        } => (
            iter::once(*exposed).chain(extra.iter().copied()).collect(),
            iter::once(*clip).chain(extra.iter().copied()).collect(),
        ),
    }
}

fn diffed(old: &[DrawCommand], new: &[DrawCommand]) -> (Vec<Rect>, Vec<Rect>) {
    let change = FrameDiff::default().compare(new, old, |cmd, matrix| {
        renderer_core::culling::command_visual_rect(cmd, matrix, &FontMetrics::default())
    });
    assert!(change.scroll.is_none(), "these frames do not scroll");
    let damage = change.damage.expect("a bounded change").to_vec();
    (damage.clone(), damage)
}

fn assert_exact_repaint(
    what: &str,
    size: (u32, u32),
    old: &[DrawCommand],
    new: &[DrawCommand],
    (repainted, declared): (Vec<Rect>, Vec<Rect>),
) {
    let width = size.0;
    let incremental = draw(size, &[old, new], false);
    let fresh = draw(size, &[new], false);
    let stale = incremental
        .pixels
        .chunks_exact(4)
        .zip(fresh.pixels.chunks_exact(4))
        .position(|(a, b)| a != b)
        .map(|i| (i as u32 % width, i as u32 / width));
    assert!(
        stale.is_none(),
        "{what}: pixel {stale:?} differs from the new frame drawn from scratch"
    );

    let regions = on_pixels(&repainted, size);
    let poisoned = draw(size, &[old, new], true);
    let disagreement = poisoned
        .pixels
        .chunks_exact(4)
        .enumerate()
        .find_map(|(i, px)| {
            let (x, y) = (i as u32 % width, i as u32 / width);
            let planned = regions
                .iter()
                .any(|&(x0, y0, x1, y1)| (x0..x1).contains(&x) && (y0..y1).contains(&y));
            let touched = px != UNTOUCHED;
            (touched != planned).then_some((x, y, touched))
        });
    assert!(
        disagreement.is_none(),
        "{what}: (x, y, repainted) {disagreement:?} disagrees with the planned regions {repainted:?}"
    );

    let FrameOp::Regions(changed) = &incremental.frame_op else {
        panic!(
            "{what}: an incremental frame declares regions, not {:?}",
            incremental.frame_op
        );
    };
    assert_eq!(
        on_pixels(changed, size),
        on_pixels(&declared, size),
        "{what}: the present declares what the plan changed"
    );
}

fn scenario(name: &str) -> Scenario {
    dirty_scenarios::all()
        .into_iter()
        .find(|scenario| scenario.name == name)
        .expect("a shared scenario by that name")
}

fn open(id: u64) -> DrawCommand {
    DrawCommand::PushElement {
        element: Arc::new(Element::new(
            ElementId(id),
            Semantics::group(),
            "",
            Rect::default(),
        )),
    }
}

fn boxed(x: f32, y: f32, width: f32, height: f32, style: RectStyle) -> DrawCommand {
    DrawCommand::Rect {
        rect: Rect::new(x, y, width, height),
        style: Arc::new(style),
    }
}

fn circle(cx: f32, cy: f32, r: f32) -> PathData {
    let k = r * 0.552_284_8;
    PathData::new()
        .move_to(Point::new(cx + r, cy))
        .cubic_to(
            Point::new(cx + r, cy + k),
            Point::new(cx + k, cy + r),
            Point::new(cx, cy + r),
        )
        .cubic_to(
            Point::new(cx - k, cy + r),
            Point::new(cx - r, cy + k),
            Point::new(cx - r, cy),
        )
        .cubic_to(
            Point::new(cx - r, cy - k),
            Point::new(cx - k, cy - r),
            Point::new(cx, cy - r),
        )
        .cubic_to(
            Point::new(cx + k, cy - r),
            Point::new(cx + r, cy - k),
            Point::new(cx + r, cy),
        )
        .close()
}

#[test]
fn every_shared_scenario_repaints_exactly_its_plan_and_matches_a_fresh_frame() {
    for scenario in dirty_scenarios::all() {
        assert_exact_repaint(
            scenario.name,
            scenario.size,
            &scenario.old,
            &scenario.new,
            regions_of(&scenario.plan),
        );
    }
}

#[test]
fn a_translucent_layer_draws_only_inside_the_clip_it_is_in() {
    let scenario = scenario("a translucent layer spilling past its clip");
    let drawn = draw(scenario.size, &[&scenario.new], false);
    let at = |x, y| pixel(&drawn, scenario.size.0, x, y);
    assert_ne!(at(700, 680), BACKGROUND, "inside the clip the layer shows");
    assert_eq!(at(820, 680), BACKGROUND, "right of the clip");
    assert_eq!(at(700, 720), BACKGROUND, "below the clip");
}

#[test]
fn a_layer_nested_in_a_rounded_clip_keeps_to_its_corners() {
    let scenario = scenario("a layer nested in a rounded clip");
    let drawn = draw(scenario.size, &[&scenario.new], false);
    let at = |x, y| pixel(&drawn, scenario.size.0, x, y);
    assert_ne!(
        at(1000, 300),
        BACKGROUND,
        "the outer layer shows in the clip"
    );
    assert_ne!(
        at(920, 260),
        at(1000, 300),
        "the nested layer shows over it"
    );
    assert_eq!(at(901, 201), BACKGROUND, "outside the top-left corner");
    assert_eq!(at(1098, 318), BACKGROUND, "outside the bottom-right corner");
    assert_eq!(at(890, 260), BACKGROUND, "the nested layer past the clip");
}

#[test]
fn antialiased_edges_crossing_a_repainted_region_match_a_fresh_frame() {
    let frame = |accent: Color| {
        vec![
            boxed(
                100.3,
                80.6,
                300.5,
                180.2,
                RectStyle::filled(Color::from_rgba_u8(90, 160, 220, 190), 37.7),
            ),
            DrawCommand::Path {
                data: Arc::new(circle(400.4, 100.7, 30.3)),
                style: Arc::new(
                    PathStyle::default()
                        .with_fill(Color::from_rgba_u8(240, 90, 60, 160))
                        .with_stroke(Stroke::new(Color::from_rgb_u8(240, 240, 240), 3.5)),
                ),
            },
            open(1),
            boxed(370.0, 70.0, 48.0, 60.0, RectStyle::filled(accent, 0.0)),
            DrawCommand::PopElement,
        ]
    };
    let old = frame(Color::from_rgba_u8(60, 200, 90, 120));
    let new = frame(Color::from_rgba_u8(200, 60, 160, 120));
    assert_exact_repaint(
        "antialiased edges crossing the region",
        SMALL,
        &old,
        &new,
        diffed(&old, &new),
    );
}

#[test]
fn a_shadow_crossing_a_repainted_region_matches_a_fresh_frame() {
    let card = RectStyle::filled(Color::from_rgb_u8(230, 230, 235), 12.0).with_shadow(
        Shadow::new(6.0, 9.0, 18.0, Color::from_rgba_u8(0, 0, 0, 140)).with_spread(2.0),
    );
    let frame = |accent: Color| {
        vec![
            boxed(120.0, 90.0, 260.0, 160.0, card),
            open(1),
            boxed(360.0, 230.0, 40.0, 50.0, RectStyle::filled(accent, 0.0)),
            DrawCommand::PopElement,
        ]
    };
    let old = frame(Color::from_rgba_u8(60, 200, 90, 90));
    let new = frame(Color::from_rgba_u8(200, 60, 160, 90));
    assert_exact_repaint(
        "a shadow crossing the region",
        SMALL,
        &old,
        &new,
        diffed(&old, &new),
    );
}

#[test]
fn a_change_beneath_a_backdrop_blur_repaints_the_whole_blur_exactly() {
    let frame = |accent: Color| {
        vec![
            boxed(
                0.0,
                0.0,
                640.0,
                400.0,
                RectStyle::filled(Color::from_rgb_u8(40, 44, 52), 0.0),
            ),
            open(1),
            boxed(300.0, 180.0, 40.0, 40.0, RectStyle::filled(accent, 0.0)),
            DrawCommand::PopElement,
            open(2),
            DrawCommand::PushLayer {
                opacity: 1.0,
                backdrop_blur: 8.0,
            },
            boxed(
                260.0,
                150.0,
                200.0,
                120.0,
                RectStyle::filled(Color::from_rgba_u8(255, 255, 255, 60), 16.0),
            ),
            DrawCommand::PopLayer,
            DrawCommand::PopElement,
        ]
    };
    let old = frame(Color::from_rgb_u8(220, 40, 40));
    let new = frame(Color::from_rgb_u8(40, 220, 90));
    let planned = diffed(&old, &new);
    let blur = Rect::new(260.0, 150.0, 200.0, 120.0);
    assert!(
        planned.0.iter().any(|region| region.x <= blur.x
            && region.y <= blur.y
            && region.x + region.width >= blur.x + blur.width
            && region.y + region.height >= blur.y + blur.height),
        "the plan grows to the whole blur the change reaches: {:?}",
        planned.0
    );
    assert_exact_repaint(
        "a change beneath a backdrop blur",
        SMALL,
        &old,
        &new,
        planned,
    );
}

#[test]
fn many_small_changes_repaint_a_bounded_set_of_regions_exactly() {
    let frame = |shade: u8| {
        let mut commands = vec![boxed(
            0.0,
            0.0,
            640.0,
            400.0,
            RectStyle::filled(Color::from_rgb_u8(20, 20, 24), 0.0),
        )];
        for row in 0..8u8 {
            for column in 0..8u8 {
                let accent = if (row + column) % 2 == 0 {
                    Color::from_rgb_u8(shade, 90, 200)
                } else {
                    Color::from_rgb_u8(90, shade, 120)
                };
                commands.extend([
                    open(u64::from(row) * 8 + u64::from(column) + 1),
                    boxed(
                        12.0 + f32::from(column) * 78.0,
                        10.0 + f32::from(row) * 48.0,
                        30.5,
                        20.25,
                        RectStyle::filled(accent, 6.0),
                    ),
                    DrawCommand::PopElement,
                ]);
            }
        }
        commands
    };
    let old = frame(60);
    let new = frame(220);
    let drawn = draw(SMALL, &[&old, &new], false);
    let FrameOp::Regions(changed) = &drawn.frame_op else {
        panic!("an incremental frame, not {:?}", drawn.frame_op);
    };
    assert!(
        !changed.spilled(),
        "sixty-four changes present as a bounded set of regions: {changed:?}"
    );
    assert_exact_repaint(
        "sixty-four small changes",
        SMALL,
        &old,
        &new,
        diffed(&old, &new),
    );
}

// A blur off the frame thread lands a frame or two later, and the frames in between plan nothing at all.
fn render_until_something_changes(
    renderer: &mut SoftwareRenderer<HeadlessWindow, HeadlessWindow>,
    (width, height): (u32, u32),
    commands: &[DrawCommand],
) -> FrameOp {
    let [r, g, b, _] = BACKGROUND;
    for _ in 0..200 {
        renderer.begin_frame(width, height, 1.0, 0).unwrap();
        match renderer.render(commands, Some(Color::from_rgb_u8(r, g, b))) {
            FrameOp::NoChange => std::thread::sleep(std::time::Duration::from_millis(5)),
            op => return op,
        }
    }
    panic!("nothing changed in two hundred frames, so this case proves nothing");
}

// Where a box and the shadow it casts paint, which is what the dirty tracker measures everywhere else.
fn shadow_footprint(rect: Rect, shadow: Shadow) -> Rect {
    renderer_core::culling::expand_for_shadow(
        rect,
        shadow.blur_radius,
        shadow.spread,
        shadow.offset_x,
        shadow.offset_y,
    )
}

#[test]
fn a_shadow_that_finished_blurring_repaints_its_own_footprint() {
    let rect = Rect::new(100.0, 40.0, 300.0, 300.0);
    // Its pixmap clears `ASYNC_SHADOW_THRESHOLD`, so the blur goes to a worker and its result lands on a frame carrying the very same commands as the one before it.
    let shadow = Shadow::new(0.0, 0.0, 12.0, Color::from_rgba_u8(9, 4, 13, 210));
    let commands = vec![boxed(
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        RectStyle::filled(Color::from_rgb_u8(230, 230, 235), 12.0).with_shadow(shadow),
    )];
    let mut renderer = SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(
        SMALL.0,
        SMALL.1,
        SoftwareRendererConfig::default(),
    );

    let first = render_until_something_changes(&mut renderer, SMALL, &commands);
    assert_eq!(
        first,
        FrameOp::Full,
        "the first frame has no previous one to diff against"
    );

    let landed = render_until_something_changes(&mut renderer, SMALL, &commands);
    let FrameOp::Regions(changed) = &landed else {
        panic!("a shadow arriving is a bounded change, not {landed:?}");
    };
    assert_eq!(
        on_pixels(changed, SMALL),
        on_pixels(&[shadow_footprint(rect, shadow)], SMALL),
        "the frame the blur lands on repaints where that shadow paints and nothing else"
    );
}

#[test]
fn one_blur_waited_on_twice_repaints_both_places_it_is_drawn() {
    let size = (1024u32, 420u32);
    let shadow = Shadow::new(0.0, 0.0, 12.0, Color::from_rgba_u8(11, 5, 17, 205));
    let style = RectStyle::filled(Color::from_rgb_u8(235, 232, 228), 10.0).with_shadow(shadow);
    // Identical boxes, so one cache key and one worker serve both of them: the key holds the shadow's shape and not its position.
    let cards = [
        Rect::new(40.0, 40.0, 300.0, 300.0),
        Rect::new(500.0, 40.0, 300.0, 300.0),
    ];
    let commands: Vec<DrawCommand> = cards
        .iter()
        .map(|card| boxed(card.x, card.y, card.width, card.height, style))
        .collect();
    let mut renderer = SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(
        size.0,
        size.1,
        SoftwareRendererConfig::default(),
    );

    let first = render_until_something_changes(&mut renderer, size, &commands);
    assert_eq!(first, FrameOp::Full, "the first frame draws everything");

    let landed = render_until_something_changes(&mut renderer, size, &commands);
    let FrameOp::Regions(changed) = &landed else {
        panic!("a shadow arriving is a bounded change, not {landed:?}");
    };
    let reached = shadow_footprint(cards[0], shadow).union(shadow_footprint(cards[1], shadow));
    assert_eq!(
        on_pixels(changed, size),
        on_pixels(&[reached], size),
        "neither box is left with the stand-in its shared blur replaced"
    );
}

// The pixels of one box, laid out as the pixmap a layer over it would hold, so blurring them is the blur the renderer ran.
fn cropped(pixels: &[u8], width: u32, (x, y, w, h): (u32, u32, u32, u32)) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for row in y..y + h {
        let start = ((row * width + x) * 4) as usize;
        out.extend_from_slice(&pixels[start..start + (w * 4) as usize]);
    }
    out
}

#[test]
fn a_backdrop_blur_spreads_by_the_sigma_its_radius_converts_to() {
    const RADIUS: f32 = 12.0;
    let size = (192u32, 128u32);
    let layer_box = (48u32, 24u32, 96u32, 80u32);
    let (x, y, w, h) = layer_box;
    // Half the surface white against the clear colour, so the blur has one hard edge to spread.
    let backdrop = vec![boxed(
        0.0,
        0.0,
        96.0,
        128.0,
        RectStyle::filled(Color::WHITE, 0.0),
    )];
    let mut over = backdrop.clone();
    over.extend([
        DrawCommand::PushLayer {
            opacity: 1.0,
            backdrop_blur: RADIUS,
        },
        // Neither filled nor framed, so it draws nothing and only sizes the layer: what lands in the box is the blurred backdrop alone.
        boxed(x as f32, y as f32, w as f32, h as f32, RectStyle::default()),
        DrawCommand::PopLayer,
    ]);

    let plain = draw(size, &[&backdrop], false);
    let blurred = draw(size, &[&over], false);

    let mut scratch = Vec::new();
    let mut by_radius = cropped(&plain.pixels, size.0, layer_box);
    crate::primitives::gaussian_blur(
        &mut by_radius,
        w,
        h,
        renderer_core::blur_sigma(RADIUS),
        &mut scratch,
    );
    let mut as_sigma = cropped(&plain.pixels, size.0, layer_box);
    crate::primitives::gaussian_blur(&mut as_sigma, w, h, RADIUS, &mut scratch);

    assert_ne!(
        by_radius, as_sigma,
        "a radius and a deviation blur by visibly different amounts, so this case can tell them apart"
    );
    assert_eq!(
        cropped(&blurred.pixels, size.0, layer_box),
        by_radius,
        "a backdrop blurs by what the shared conversion makes of its radius, as shadows and the hardware backend do"
    );
}

#[test]
fn a_rounded_clip_cuts_the_corners_of_what_a_clip_inside_it_draws() {
    let inner = Rect::new(100.0, 100.0, 60.0, 60.0);
    let commands = vec![
        DrawCommand::PushClip {
            rect: Rect::new(100.0, 100.0, 200.0, 200.0),
            radius: BorderRadius::all(40.0),
        },
        DrawCommand::PushClip {
            rect: inner,
            radius: BorderRadius::zero(),
        },
        boxed(
            inner.x,
            inner.y,
            inner.width,
            inner.height,
            RectStyle::filled(Color::WHITE, 0.0),
        ),
        DrawCommand::PopClip,
        DrawCommand::PopClip,
    ];
    let drawn = draw(SMALL, &[&commands], false);
    let at = |x, y| pixel(&drawn, SMALL.0, x, y);

    // 53 px from the corner's centre, where a radius of 40 covers nothing.
    assert_eq!(
        at(102, 102),
        BACKGROUND,
        "the corner of the clip above cuts the square clip inside it"
    );
    assert_ne!(at(150, 150), BACKGROUND, "well inside both clips it paints");
}
