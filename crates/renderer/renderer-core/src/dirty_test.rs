use super::*;
use crate::culling;
use crate::dirty_scenarios::{self, Plan, Scenario};
use crate::{
    Border, BorderRadius, Color, Element, ElementId, FontMetrics, Gradient, ImageData, Paint,
    Raster, RectStyle, Semantics, Shadow, ShapeStyle, TextStyle,
};
use geometry_core::{Point, Rect};
use std::sync::Arc;

fn rect_cmd(x: f32, y: f32, w: f32, h: f32) -> DrawCommand {
    DrawCommand::Rect {
        rect: Rect::new(x, y, w, h),
        style: Arc::new(RectStyle::default().with_fill(Color::from_rgb_u8(200, 60, 60))),
    }
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

fn close() -> DrawCommand {
    DrawCommand::PopElement
}

fn boxed(id: u64, x: f32, y: f32, w: f32, h: f32) -> [DrawCommand; 3] {
    [open(id), rect_cmd(x, y, w, h), close()]
}

fn clip(x: f32, y: f32, w: f32, h: f32, radius: f32) -> DrawCommand {
    DrawCommand::PushClip {
        rect: Rect::new(x, y, w, h),
        radius: BorderRadius::all(radius),
    }
}

fn translate(x: f32, y: f32) -> DrawCommand {
    DrawCommand::PushMatrix {
        matrix: [1.0, 0.0, 0.0, 1.0, x, y],
    }
}

fn layer(opacity: f32, backdrop_blur: f32) -> DrawCommand {
    DrawCommand::PushLayer {
        opacity,
        backdrop_blur,
    }
}

fn visual_rect(cmd: &DrawCommand, matrix: [f32; 6]) -> Option<Rect> {
    culling::command_visual_rect(cmd, matrix, &FontMetrics::default())
}

fn change(new: &[DrawCommand], old: &[DrawCommand]) -> FrameChange {
    FrameDiff::default().compare(new, old, visual_rect)
}

fn dirty(new: &[DrawCommand], old: &[DrawCommand]) -> Option<DirtyRects> {
    change(new, old).damage
}

fn blit(new: &[DrawCommand], old: &[DrawCommand]) -> Option<ScrollBlit> {
    change(new, old).scroll
}

fn union(rects: &[Rect]) -> Rect {
    rects
        .iter()
        .copied()
        .reduce(Rect::union)
        .expect("some damage")
}

fn contains(outer: Rect, inner: Rect) -> bool {
    outer.x <= inner.x
        && outer.y <= inner.y
        && outer.x + outer.width >= inner.x + inner.width
        && outer.y + outer.height >= inner.y + inner.height
}

fn covers(rects: &[Rect], x: f32, y: f32) -> bool {
    rects
        .iter()
        .any(|r| r.x <= x && r.x + r.width >= x && r.y <= y && r.y + r.height >= y)
}

fn assert_same_rects(actual: &[Rect], expected: &[Rect], what: &str) {
    let mut unmatched: Vec<Rect> = expected.to_vec();
    for rect in actual {
        let at = unmatched.iter().position(|e| e == rect);
        assert!(
            at.is_some(),
            "{what}: unexpected dirty rect {rect:?}; planned {actual:?}, expected {expected:?}"
        );
        unmatched.swap_remove(at.unwrap());
    }
    assert!(
        unmatched.is_empty(),
        "{what}: missing {unmatched:?}; planned {actual:?}"
    );
}

fn assert_planned(scenario: &Scenario, change: &FrameChange) {
    let name = scenario.name;
    match &scenario.plan {
        Plan::Damage(rects) => {
            assert!(change.scroll.is_none(), "{name}: nothing scrolled");
            let damage = change.damage.as_deref().expect("a bounded change");
            assert_same_rects(damage, rects, name);
        }
        Plan::Scroll {
            clip,
            delta,
            exposed,
            extra,
        } => {
            let blit = change
                .scroll
                .as_ref()
                .unwrap_or_else(|| panic!("{name}: a blit"));
            assert_eq!(blit.scroll_clip, *clip, "{name}");
            assert_eq!((blit.delta_x, blit.delta_y), *delta, "{name}");
            assert_eq!(blit.exposed_band, *exposed, "{name}");
            assert_same_rects(&blit.extra_dirty, extra, name);
        }
    }
}

fn assert_same_change(a: &FrameChange, b: &FrameChange, what: &str) {
    match (&a.damage, &b.damage) {
        (Some(a), Some(b)) => assert_same_rects(a, b, what),
        (a, b) => assert_eq!(a.is_none(), b.is_none(), "{what}"),
    }
    match (&a.scroll, &b.scroll) {
        (Some(a), Some(b)) => {
            assert_eq!(a.scroll_clip, b.scroll_clip, "{what}");
            assert_eq!((a.delta_x, a.delta_y), (b.delta_x, b.delta_y), "{what}");
            assert_eq!(a.exposed_band, b.exposed_band, "{what}");
            assert_same_rects(&a.extra_dirty, &b.extra_dirty, what);
        }
        (a, b) => assert_eq!(a.is_none(), b.is_none(), "{what}"),
    }
}

fn assert_scenario(name: &str) {
    let scenario = dirty_scenarios::all()
        .into_iter()
        .find(|s| s.name == name)
        .expect("a scenario by that name");
    assert_planned(&scenario, &change(&scenario.new, &scenario.old));
}

#[test]
fn every_shared_scenario_plans_what_it_expects() {
    for scenario in dirty_scenarios::all() {
        assert!(scenario.size.0 > 0 && scenario.size.1 > 0);
        assert_planned(&scenario, &change(&scenario.new, &scenario.old));
    }
}

#[test]
fn a_layer_opacity_change_dirties_only_its_bounds() {
    assert_scenario("a layer animating its opacity");
}

#[test]
fn a_rounded_clip_radius_change_dirties_its_old_and_new_bounds() {
    assert_scenario("a rounded clip changing its radius");
}

#[test]
fn a_card_inserted_mid_list_dirties_it_and_the_cards_it_moved() {
    assert_scenario("a card inserted mid-list");
}

#[test]
fn a_card_removed_mid_list_dirties_it_and_the_cards_it_moved() {
    assert_scenario("a card removed mid-list");
}

#[test]
fn a_rounded_chip_widening_dirties_its_old_and_new_clip() {
    assert_scenario("a rounded chip widening with its text");
}

#[test]
fn a_clip_resizing_with_its_content_dirties_what_either_clip_showed() {
    assert_scenario("a clip resizing with the content it cuts");
}

#[test]
fn a_card_over_a_full_window_background_dirties_only_the_card() {
    assert_scenario("a card changing over a full-window background");
}

#[test]
fn a_layer_spilling_past_its_clip_dirties_only_what_the_clip_shows() {
    assert_scenario("a translucent layer spilling past its clip");
}

#[test]
fn a_layer_nested_in_a_rounded_clip_dirties_only_what_the_clip_shows() {
    assert_scenario("a layer nested in a rounded clip");
}

#[test]
fn a_real_window_still_blits_while_a_tooltip_outside_the_list_is_dismissed() {
    assert_scenario("a list scrolling while a tooltip outside it is dismissed");
}

#[test]
fn a_blit_repaints_a_tooltip_moving_outside_the_list_where_it_was_and_is() {
    assert_scenario("a list scrolling while a tooltip outside it moves");
}

#[test]
fn one_frame_diff_compares_frame_after_frame_like_a_fresh_one() {
    let mut reused = FrameDiff::default();
    for scenario in dirty_scenarios::all() {
        for (new, old) in [
            (&scenario.new, &scenario.old),
            (&scenario.old, &scenario.new),
            (&scenario.new, &scenario.new),
        ] {
            let again = reused.compare(new, old, visual_rect);
            assert_same_change(&again, &change(new, old), scenario.name);
        }
        assert_planned(
            &scenario,
            &reused.compare(&scenario.new, &scenario.old, visual_rect),
        );
    }
}

#[test]
fn an_animating_layer_in_a_4k_frame_never_dirties_more_than_its_bounds_and_blur() {
    let frame = |opacity: f32, backdrop_blur: f32| {
        let mut commands = vec![
            open(1),
            rect_cmd(0.0, 0.0, 3840.0, 2160.0),
            open(2),
            layer(opacity, backdrop_blur),
            rect_cmd(1900.0, 1060.0, 40.0, 40.0),
            DrawCommand::PopLayer,
            close(),
        ];
        commands.extend(boxed(3, 10.0, 10.0, 300.0, 40.0));
        commands.push(close());
        commands
    };
    let max_blur = 8.0;
    let margin = blur_padding(blur_sigma(max_blur)) as f32;
    let limit = Rect::new(
        1900.0 - margin,
        1060.0 - margin,
        40.0 + margin * 2.0,
        40.0 + margin * 2.0,
    );
    let mut diff = FrameDiff::default();
    let mut previous = frame(0.0, 0.0);
    for step in 1..=30 {
        let t = step as f32 / 30.0;
        let next = frame(t, max_blur * t);
        let planned = diff
            .compare(&next, &previous, visual_rect)
            .damage
            .expect("a layer animation is always bounded");
        assert!(!planned.is_empty(), "frame {step} changed the layer");
        let damage = union(&planned);
        assert!(
            contains(limit, damage),
            "frame {step}: {damage:?} reaches past {limit:?}"
        );
        previous = next;
    }
}

#[test]
fn text_changing_inside_an_unchanged_rounded_clip_dirties_only_the_text() {
    let text = |content: &str, width: f32| DrawCommand::Text {
        text: Arc::from(content),
        spans: None,
        rect: Rect::new(20.0, 20.0, width, 20.0),
        style: Arc::new(TextStyle::new(14.0, Color::WHITE)),
    };
    let frame = |content: &str, width: f32| {
        vec![
            open(1),
            clip(0.0, 0.0, 300.0, 80.0, 16.0),
            rect_cmd(0.0, 0.0, 300.0, 80.0),
            text(content, width),
            DrawCommand::PopClip,
            close(),
        ]
    };
    let old = frame("hello", 40.0);
    let new = frame("hello there", 90.0);
    let planned = dirty(&new, &old).expect("bounded");
    let identity = geometry_core::Transform::IDENTITY.to_array();
    let text_rect = visual_rect(&old[3], identity)
        .unwrap()
        .union(visual_rect(&new[3], identity).unwrap());
    let damage = union(&planned);
    assert!(
        contains(text_rect, damage),
        "{damage:?} reaches past the text's old and new rect {text_rect:?}"
    );
    assert!(
        contains(
            damage,
            text_rect
                .intersect(Rect::new(0.0, 0.0, 300.0, 80.0))
                .unwrap()
        ),
        "{damage:?} misses part of the text inside its clip"
    );
}

#[test]
fn an_unchanged_list_dirties_nothing() {
    let a = vec![rect_cmd(0.0, 0.0, 10.0, 10.0)];
    assert_eq!(
        dirty(&a, &a).map(|d| d.len()),
        Some(0),
        "an unchanged list dirties nothing, and says so rather than asking for a full repaint"
    );
}

#[test]
fn a_shorter_list_dirties_only_what_it_lost() {
    let old = vec![
        rect_cmd(0.0, 0.0, 10.0, 10.0),
        rect_cmd(100.0, 100.0, 10.0, 10.0),
        rect_cmd(200.0, 200.0, 10.0, 10.0),
    ];
    let new = vec![
        rect_cmd(0.0, 0.0, 10.0, 10.0),
        rect_cmd(200.0, 200.0, 10.0, 10.0),
    ];
    let planned = dirty(&new, &old).expect("bounded");
    assert_same_rects(
        &planned,
        &[Rect::new(100.0, 100.0, 10.0, 10.0)],
        "a removed loose command",
    );
}

#[test]
fn a_moved_command_dirties_both_edges_of_its_change() {
    let old = vec![rect_cmd(0.0, 0.0, 10.0, 10.0)];
    let new = vec![rect_cmd(5.0, 0.0, 10.0, 10.0)];
    let damage = union(&dirty(&new, &old).unwrap());
    assert!(
        damage.x <= 0.0 && damage.x + damage.width >= 15.0,
        "the dirty rect must reach both edges of the change: {damage:?}"
    );
}

#[test]
fn disjoint_changes_stay_separate_rects() {
    let old = vec![
        rect_cmd(0.0, 0.0, 10.0, 10.0),
        rect_cmd(500.0, 500.0, 10.0, 10.0),
    ];
    let new = vec![
        rect_cmd(0.0, 0.0, 20.0, 20.0),
        rect_cmd(500.0, 500.0, 20.0, 20.0),
    ];
    let rects = dirty(&new, &old).unwrap();
    assert_eq!(rects.len(), 2);
    for r in &rects {
        assert!(
            r.width < 100.0 && r.height < 100.0,
            "two far-apart changes must stay two tight rects, not one spanning box: {r:?}"
        );
    }
}

#[test]
fn a_translated_command_dirties_where_it_was_and_went() {
    let old = vec![
        translate(0.0, 0.0),
        rect_cmd(0.0, 0.0, 10.0, 10.0),
        DrawCommand::PopMatrix,
    ];
    let new = vec![
        translate(5.0, 5.0),
        rect_cmd(0.0, 0.0, 10.0, 10.0),
        DrawCommand::PopMatrix,
    ];
    let damage = union(&dirty(&new, &old).unwrap());
    assert!(
        contains(damage, Rect::new(0.0, 0.0, 15.0, 15.0)),
        "a translated command dirties where it was and where it went: {damage:?}"
    );
}

#[test]
fn a_change_under_a_nested_matrix_dirties_both_positions() {
    let make = |outer_ty: f32| {
        vec![
            translate(0.0, outer_ty),
            translate(10.0, 10.0),
            rect_cmd(0.0, 0.0, 10.0, 10.0),
            DrawCommand::PopMatrix,
            DrawCommand::PopMatrix,
        ]
    };
    let damage = union(&dirty(&make(50.0), &make(0.0)).unwrap());
    assert!(
        damage.y <= 10.0 && damage.y + damage.height >= 70.0,
        "a change under a nested matrix dirties both composed positions: {damage:?}"
    );
}

#[test]
fn a_matrix_that_keeps_the_bounds_but_flips_the_content_still_dirties() {
    let flip = DrawCommand::PushMatrix {
        matrix: [-1.0, 0.0, 0.0, 1.0, 20.0, 0.0],
    };
    let old = vec![
        translate(0.0, 0.0),
        rect_cmd(5.0, 0.0, 10.0, 10.0),
        DrawCommand::PopMatrix,
    ];
    let new = vec![flip, rect_cmd(5.0, 0.0, 10.0, 10.0), DrawCommand::PopMatrix];
    let planned = dirty(&new, &old).unwrap();
    assert!(
        !planned.is_empty(),
        "the same bounds drawn mirrored still differ"
    );
}

#[test]
fn commands_outside_any_element_diff_around_an_insertion() {
    let old: Vec<_> = [rect_cmd(0.0, 0.0, 50.0, 10.0), open(1)]
        .into_iter()
        .chain(boxed(2, 0.0, 100.0, 50.0, 10.0))
        .chain([close(), rect_cmd(0.0, 500.0, 50.0, 10.0)])
        .collect();
    let new: Vec<_> = [rect_cmd(0.0, 0.0, 60.0, 10.0), open(1)]
        .into_iter()
        .chain(boxed(3, 0.0, 100.0, 50.0, 10.0))
        .chain(boxed(2, 0.0, 120.0, 50.0, 10.0))
        .chain([close(), rect_cmd(0.0, 500.0, 50.0, 10.0)])
        .collect();
    let planned = dirty(&new, &old).expect("bounded");
    assert_same_rects(
        &planned,
        &[
            Rect::new(0.0, 0.0, 60.0, 10.0),
            Rect::new(0.0, 100.0, 50.0, 10.0),
            Rect::new(0.0, 120.0, 50.0, 10.0),
        ],
        "a loose header that changed, an inserted box and the one it moved; the loose footer is untouched",
    );
}

#[test]
fn a_change_inside_a_nested_element_dirties_only_that_element() {
    let frame = |extra_row: bool| {
        let mut commands = vec![open(1), open(2)];
        commands.extend(boxed(3, 0.0, 0.0, 100.0, 20.0));
        commands.extend(boxed(4, 0.0, 30.0, 100.0, 20.0));
        commands.extend([close(), open(5)]);
        commands.extend(boxed(6, 200.0, 0.0, 100.0, 20.0));
        if extra_row {
            commands.extend(boxed(7, 200.0, 30.0, 100.0, 20.0));
        }
        commands.extend([close(), close()]);
        commands
    };
    let planned = dirty(&frame(true), &frame(false)).expect("bounded");
    assert_same_rects(
        &planned,
        &[Rect::new(200.0, 30.0, 100.0, 20.0)],
        "a row added to the second panel",
    );
}

#[test]
fn a_box_moved_to_another_parent_dirties_where_it_was_and_where_it_is() {
    let old: Vec<_> = [open(1)]
        .into_iter()
        .chain(boxed(3, 0.0, 0.0, 10.0, 10.0))
        .chain([close(), open(2), close()])
        .collect();
    let new: Vec<_> = [open(1), close(), open(2)]
        .into_iter()
        .chain(boxed(3, 50.0, 0.0, 10.0, 10.0))
        .chain([close()])
        .collect();
    let planned = dirty(&new, &old).expect("bounded");
    assert_same_rects(
        &planned,
        &[
            Rect::new(0.0, 0.0, 10.0, 10.0),
            Rect::new(50.0, 0.0, 10.0, 10.0),
        ],
        "a reparented box",
    );
}

#[test]
fn a_box_sent_to_the_back_dirties_only_itself() {
    let a = boxed(1, 0.0, 0.0, 100.0, 100.0);
    let b = boxed(2, 400.0, 0.0, 100.0, 100.0);
    let c = boxed(3, 50.0, 50.0, 100.0, 100.0);
    let old: Vec<_> = a.iter().chain(&b).chain(&c).cloned().collect();
    let new: Vec<_> = c.iter().chain(&a).chain(&b).cloned().collect();
    let planned = dirty(&new, &old).expect("bounded");
    assert_same_rects(
        &planned,
        &[Rect::new(50.0, 50.0, 100.0, 100.0)],
        "the overlap changed order and nothing else did",
    );
}

#[test]
fn boxes_sharing_an_id_in_one_frame_still_dirty_what_moved() {
    let old: Vec<_> = boxed(1, 0.0, 0.0, 50.0, 10.0)
        .into_iter()
        .chain(boxed(1, 0.0, 50.0, 50.0, 10.0))
        .collect();
    let new: Vec<_> = boxed(1, 0.0, 0.0, 50.0, 10.0)
        .into_iter()
        .chain(boxed(2, 0.0, 50.0, 60.0, 10.0))
        .chain(boxed(1, 0.0, 100.0, 50.0, 10.0))
        .collect();
    let planned = dirty(&new, &old).expect("bounded");
    for (x, y) in [(25.0, 55.0), (55.0, 55.0), (25.0, 105.0)] {
        assert!(
            covers(&planned, x, y),
            "({x}, {y}) changed and is not dirty: {planned:?}"
        );
    }
}

#[test]
fn a_clip_that_appears_dirties_what_it_now_hides() {
    let old = vec![open(1), rect_cmd(0.0, 0.0, 400.0, 50.0), close()];
    let new = vec![
        open(1),
        clip(0.0, 0.0, 200.0, 50.0, 0.0),
        rect_cmd(0.0, 0.0, 400.0, 50.0),
        DrawCommand::PopClip,
        close(),
    ];
    let damage = union(&dirty(&new, &old).expect("bounded"));
    assert!(
        contains(damage, Rect::new(0.0, 0.0, 400.0, 50.0)),
        "{damage:?}"
    );
}

#[test]
fn content_moving_under_a_clip_is_clamped_to_it() {
    let frame = |y: f32| {
        vec![
            open(1),
            clip(0.0, 100.0, 200.0, 200.0, 0.0),
            translate(0.0, y),
            rect_cmd(0.0, 0.0, 200.0, 2000.0),
            DrawCommand::PopMatrix,
            DrawCommand::PopClip,
            close(),
        ]
    };
    let planned = dirty(&frame(100.0), &frame(130.0)).expect("bounded");
    assert_same_rects(
        &planned,
        &[Rect::new(0.0, 100.0, 200.0, 200.0)],
        "content taller than its viewport",
    );
}

#[test]
fn a_layer_under_a_clip_dirties_only_what_the_clip_shows_of_it() {
    let frame = |opacity: f32| {
        vec![
            clip(0.0, 0.0, 100.0, 100.0, 0.0),
            layer(opacity, 0.0),
            rect_cmd(50.0, 50.0, 500.0, 500.0),
            DrawCommand::PopLayer,
            DrawCommand::PopClip,
        ]
    };
    let planned = dirty(&frame(0.3), &frame(0.4)).expect("bounded");
    assert_same_rects(
        &planned,
        &[Rect::new(50.0, 50.0, 50.0, 50.0)],
        "a translucent layer spilling past its clip",
    );
}

#[test]
fn content_moving_inside_a_layer_is_clamped_by_the_clip_around_it() {
    let frame = |x: f32| {
        vec![
            clip(0.0, 0.0, 100.0, 100.0, 0.0),
            layer(0.5, 0.0),
            rect_cmd(x, 50.0, 100.0, 10.0),
            DrawCommand::PopLayer,
            DrawCommand::PopClip,
        ]
    };
    let planned = dirty(&frame(60.0), &frame(40.0)).expect("bounded");
    assert_same_rects(
        &planned,
        &[Rect::new(40.0, 50.0, 60.0, 10.0)],
        "a layer's content is masked by the clip the layer sits in",
    );
}

#[test]
fn an_unchanged_backdrop_blur_repaints_when_what_is_beneath_it_changes() {
    let frame = |beneath_x: f32| {
        vec![
            rect_cmd(beneath_x, 0.0, 20.0, 20.0),
            layer(1.0, 4.0),
            rect_cmd(0.0, 0.0, 200.0, 200.0),
            DrawCommand::PopLayer,
        ]
    };
    let damage = union(&dirty(&frame(10.0), &frame(0.0)).expect("bounded"));
    assert!(
        contains(damage, Rect::new(0.0, 0.0, 200.0, 200.0)),
        "the blur samples what moved beneath it: {damage:?}"
    );
}

#[test]
fn a_backdrop_blur_with_nothing_in_it_reaches_the_whole_surface() {
    let frame = |x: f32| {
        vec![
            rect_cmd(x, 0.0, 20.0, 20.0),
            layer(1.0, 4.0),
            DrawCommand::PopLayer,
        ]
    };
    assert!(dirty(&frame(10.0), &frame(0.0)).is_none());
    assert_eq!(dirty(&frame(0.0), &frame(0.0)).map(|d| d.len()), Some(0));
}

#[test]
fn scopes_left_open_or_closed_twice_still_dirty_what_changed() {
    let old = vec![
        DrawCommand::PopClip,
        DrawCommand::PopLayer,
        clip(0.0, 0.0, 100.0, 100.0, 0.0),
        rect_cmd(0.0, 0.0, 50.0, 50.0),
    ];
    let new = vec![
        DrawCommand::PopClip,
        DrawCommand::PopLayer,
        clip(0.0, 0.0, 100.0, 100.0, 8.0),
        rect_cmd(0.0, 0.0, 50.0, 50.0),
    ];
    let damage = union(&dirty(&new, &old).expect("bounded"));
    assert!(
        contains(damage, Rect::new(0.0, 0.0, 50.0, 50.0)),
        "a clip never closed still damages what it covers: {damage:?}"
    );
}

fn scroll_frame(offset: f32, inserted_card: bool, inserted_row: bool) -> Vec<DrawCommand> {
    let mut commands = vec![open(1), rect_cmd(0.0, 0.0, 200.0, 40.0)];
    if inserted_card {
        commands.extend(boxed(2, 0.0, 260.0, 200.0, 30.0));
    }
    commands.extend([
        clip(0.0, 50.0, 200.0, 200.0, 0.0),
        translate(0.0, 50.0 - offset),
        open(20),
        rect_cmd(0.0, 0.0, 200.0, 800.0),
    ]);
    if inserted_row {
        commands.extend(boxed(21, 0.0, 800.0, 200.0, 30.0));
    }
    commands.extend([
        close(),
        DrawCommand::PopMatrix,
        DrawCommand::PopClip,
        close(),
    ]);
    commands
}

#[test]
fn a_scroll_blit_survives_an_insertion_outside_its_clip() {
    let old = scroll_frame(50.0, false, false);
    let new = scroll_frame(60.0, true, false);
    let blit = blit(&new, &old).expect("still a blit");
    assert_eq!((blit.delta_x, blit.delta_y), (0, -10));
    assert_eq!(blit.scroll_clip, Rect::new(0.0, 50.0, 200.0, 200.0));
    assert_eq!(blit.exposed_band, Rect::new(0.0, 240.0, 200.0, 10.0));
    assert!(
        covers(&blit.extra_dirty, 100.0, 275.0),
        "the inserted card: {:?}",
        blit.extra_dirty
    );
    assert!(
        blit.extra_dirty
            .iter()
            .all(|r| !r.overlaps(Rect::new(0.0, 0.0, 200.0, 40.0))),
        "the header outside the clip neither changed nor moved: {:?}",
        blit.extra_dirty
    );
}

#[test]
fn a_scroll_blit_is_refused_when_the_scrolled_content_changes() {
    let old = scroll_frame(50.0, false, false);
    let new = scroll_frame(60.0, false, true);
    assert!(blit(&new, &old).is_none());
}

#[test]
fn a_sub_pixel_scroll_is_not_a_blit() {
    let old = scroll_frame(50.0, false, false);
    let new = scroll_frame(50.5, false, false);
    assert!(blit(&new, &old).is_none());
}

#[test]
fn a_blit_shift_is_whole_device_pixels_only_at_scales_that_keep_it_whole() {
    let blit = blit(
        &scroll_frame(60.0, false, false),
        &scroll_frame(50.0, false, false),
    )
    .expect("a ten-pixel blit");
    assert!(blit.is_whole_pixels_at(1.0));
    assert!(!blit.is_whole_pixels_at(1.25), "12.5 device pixels");
    assert!(blit.is_whole_pixels_at(2.0));
}

#[test]
fn a_blit_repaints_what_is_drawn_inside_its_clip_around_the_scrolled_content() {
    let frame = |ty: f32, thumb_y: f32| {
        vec![
            clip(0.0, 0.0, 100.0, 200.0, 0.0),
            rect_cmd(0.0, 0.0, 100.0, 30.0),
            translate(0.0, ty),
            rect_cmd(0.0, 0.0, 100.0, 400.0),
            DrawCommand::PopMatrix,
            rect_cmd(90.0, thumb_y, 10.0, 20.0),
            DrawCommand::PopClip,
        ]
    };
    let blit = blit(&frame(-60.0, 25.0), &frame(-50.0, 20.0)).expect("a blit");
    assert_eq!((blit.delta_x, blit.delta_y), (0, -10));
    assert_eq!(blit.exposed_band, Rect::new(0.0, 190.0, 100.0, 10.0));
    for (x, y, what) in [
        (50.0, 15.0, "the header under the scrolled content"),
        (95.0, 40.0, "the thumb where it is"),
        (95.0, 12.0, "where the blit moved the thumb's old pixels"),
    ] {
        assert!(
            covers(&blit.extra_dirty, x, y),
            "{what}: {:?}",
            blit.extra_dirty
        );
    }
}

#[test]
fn a_backdrop_blur_over_a_blit_repaints_all_of_itself() {
    let frame = |offset: f32| {
        vec![
            clip(0.0, 0.0, 200.0, 200.0, 0.0),
            translate(0.0, -offset),
            rect_cmd(0.0, 0.0, 200.0, 800.0),
            DrawCommand::PopMatrix,
            DrawCommand::PopClip,
            layer(1.0, 4.0),
            rect_cmd(150.0, 0.0, 100.0, 50.0),
            DrawCommand::PopLayer,
        ]
    };
    let blit = blit(&frame(20.0), &frame(10.0)).expect("a blit");
    let margin = blur_padding(blur_sigma(4.0)) as f32;
    let blurred = Rect::new(
        150.0 - margin,
        -margin,
        100.0 + margin * 2.0,
        50.0 + margin * 2.0,
    );
    assert!(
        contains(union(&blit.extra_dirty), blurred),
        "the blur samples the pixels the blit moved: {:?}",
        blit.extra_dirty
    );
}

fn nested_scroll(outer: f32, inner: f32) -> Vec<DrawCommand> {
    vec![
        clip(0.0, 0.0, 400.0, 400.0, 0.0),
        translate(0.0, -outer),
        rect_cmd(0.0, 0.0, 400.0, 80.0),
        clip(0.0, 100.0, 400.0, 200.0, 0.0),
        translate(0.0, 100.0 - inner),
        rect_cmd(0.0, 0.0, 400.0, 1000.0),
        DrawCommand::PopMatrix,
        DrawCommand::PopClip,
        DrawCommand::PopMatrix,
        DrawCommand::PopClip,
    ]
}

#[test]
fn a_nested_scroll_blits_the_inner_clip_when_only_it_scrolled() {
    let blit = blit(&nested_scroll(0.0, 20.0), &nested_scroll(0.0, 0.0)).expect("a blit");
    assert_eq!(blit.scroll_clip, Rect::new(0.0, 100.0, 400.0, 200.0));
    assert_eq!((blit.delta_x, blit.delta_y), (0, -20));
    assert!(blit.extra_dirty.is_empty(), "{:?}", blit.extra_dirty);
}

#[test]
fn a_nested_scroll_is_no_blit_when_both_scrolled() {
    let change = change(&nested_scroll(10.0, 20.0), &nested_scroll(0.0, 0.0));
    assert!(change.scroll.is_none());
    let damage = union(&change.damage.expect("bounded"));
    assert!(
        contains(damage, Rect::new(0.0, 0.0, 400.0, 300.0)),
        "{damage:?}"
    );
}

#[test]
fn an_unchanged_scroll_is_no_blit() {
    let cmds = scroll_frame(50.0, false, false);
    assert!(
        blit(&cmds, &cmds).is_none(),
        "an unchanged list scrolled by nothing"
    );
}

fn styled(x: f32, y: f32, w: f32, h: f32, style: RectStyle) -> DrawCommand {
    DrawCommand::Rect {
        rect: Rect::new(x, y, w, h),
        style: Arc::new(style),
    }
}

fn solid() -> RectStyle {
    RectStyle::default().with_fill(Color::from_rgb_u8(24, 24, 32))
}

fn ramp(start: Point, end: Point) -> RectStyle {
    let stops = [(0.0, Color::BLACK), (1.0, Color::WHITE)];
    RectStyle::default().with_fill(Paint::Gradient(Gradient::linear(start, end, &stops)))
}

const VIEWPORT: Rect = Rect {
    x: 100.0,
    y: 100.0,
    width: 200.0,
    height: 200.0,
};

// A panel background under a scrolling viewport: it reaches past the clip on every side, so what it is painted with is all that decides whether the blit survives.
fn beneath_a_scroll(background: DrawCommand, offset: f32) -> Vec<DrawCommand> {
    vec![
        open(1),
        background,
        clip(100.0, 100.0, 200.0, 200.0, 0.0),
        translate(100.0, 100.0 - offset),
        open(2),
        rect_cmd(0.0, 0.0, 200.0, 800.0),
        close(),
        DrawCommand::PopMatrix,
        DrawCommand::PopClip,
        close(),
    ]
}

fn scrolled_over(background: DrawCommand) -> ScrollBlit {
    blit(
        &beneath_a_scroll(background.clone(), 20.0),
        &beneath_a_scroll(background, 0.0),
    )
    .expect("a blit")
}

#[test]
fn a_background_that_covers_the_clip_evenly_keeps_the_blit() {
    for (background, what) in [
        (styled(50.0, 50.0, 300.0, 300.0, solid()), "a solid fill"),
        (
            styled(
                50.0,
                50.0,
                300.0,
                300.0,
                solid().with_radius(BorderRadius::all(40.0)),
            ),
            "a solid fill whose corners stay clear of the clip",
        ),
        (
            styled(
                50.0,
                50.0,
                300.0,
                300.0,
                solid().with_border(Border::uniform(Color::WHITE, 20.0)),
            ),
            "a framed fill whose frame stays clear of the clip",
        ),
        (
            styled(
                50.0,
                50.0,
                300.0,
                300.0,
                ramp(Point::new(50.0, 0.0), Point::new(350.0, 0.0)),
            ),
            "a gradient that ramps across the scroll",
        ),
    ] {
        let blit = scrolled_over(background);
        assert_eq!(
            blit.exposed_band,
            Rect::new(100.0, 280.0, 200.0, 20.0),
            "{what}"
        );
        assert!(
            blit.extra_dirty.is_empty(),
            "{what} lands looking the same wherever the blit puts it: {:?}",
            blit.extra_dirty
        );
    }
}

#[test]
fn a_background_whose_pixels_depend_on_where_they_land_repaints_the_clip_whole() {
    let radial = Gradient::radial(
        Point::new(200.0, 200.0),
        150.0,
        &[(0.0, Color::BLACK), (1.0, Color::WHITE)],
    );
    for (background, what) in [
        (
            styled(
                50.0,
                50.0,
                300.0,
                300.0,
                ramp(Point::new(0.0, 50.0), Point::new(0.0, 350.0)),
            ),
            "a gradient ramping along the scroll",
        ),
        (
            styled(
                50.0,
                50.0,
                300.0,
                300.0,
                ramp(Point::new(50.0, 50.0), Point::new(350.0, 350.0)),
            ),
            "a gradient ramping both ways",
        ),
        (
            styled(
                50.0,
                50.0,
                300.0,
                300.0,
                RectStyle::default().with_fill(Paint::Gradient(radial)),
            ),
            "a radial gradient",
        ),
        (
            DrawCommand::Image {
                data: Arc::new(ImageData::new(vec![0u8; 4 * 4 * 4], 4, 4)),
                rect: Rect::new(50.0, 50.0, 300.0, 300.0),
                raster: Raster::Smooth,
            },
            "a picture",
        ),
        (
            styled(
                50.0,
                50.0,
                300.0,
                300.0,
                solid().with_shadow(Shadow::new(0.0, 0.0, 12.0, Color::BLACK)),
            ),
            "a fill with a shadow ramping around it",
        ),
    ] {
        let blit = scrolled_over(background);
        assert!(
            contains(union(&blit.extra_dirty), VIEWPORT),
            "{what} does move: {:?}",
            blit.extra_dirty
        );
    }
}

#[test]
fn a_background_with_an_edge_inside_the_clip_repaints_where_the_edge_lands() {
    for (background, landed, what) in [
        (
            styled(50.0, 50.0, 300.0, 150.0, solid()),
            Rect::new(100.0, 180.0, 200.0, 20.0),
            "a fill that stops half way down",
        ),
        (
            styled(
                50.0,
                50.0,
                300.0,
                300.0,
                solid().with_radius(BorderRadius::all(80.0)),
            ),
            VIEWPORT,
            "a corner arc reaching in",
        ),
        (
            styled(
                50.0,
                50.0,
                300.0,
                300.0,
                solid().with_border(Border::uniform(Color::WHITE, 80.0)),
            ),
            VIEWPORT,
            "a frame reaching in",
        ),
    ] {
        let blit = scrolled_over(background);
        assert!(
            contains(union(&blit.extra_dirty), landed),
            "{what} is an edge, and an edge moves: {:?}",
            blit.extra_dirty
        );
    }
}

#[test]
fn a_notification_list_blits_over_the_panel_background_beneath_it() {
    assert_scenario("a notification list scrolling over its panel's background");
}

#[test]
fn a_pure_vertical_scroll_blits_its_exposed_band() {
    let frame = |ty: f32| {
        vec![
            clip(0.0, 0.0, 100.0, 200.0, 0.0),
            translate(0.0, ty),
            rect_cmd(0.0, 0.0, 100.0, 400.0),
            DrawCommand::PopMatrix,
            DrawCommand::PopClip,
        ]
    };
    let blit = blit(&frame(-60.0), &frame(-50.0)).unwrap();
    assert_eq!(blit.delta_y, -10);
    assert_eq!(blit.exposed_band, Rect::new(0.0, 190.0, 100.0, 10.0));
    assert!(blit.extra_dirty.is_empty(), "{:?}", blit.extra_dirty);
}
