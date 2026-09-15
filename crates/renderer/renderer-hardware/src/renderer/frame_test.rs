use geometry_core::Rect;
use renderer_core::dirty::FrameDiff;
use renderer_core::dirty_scenarios::{self, Plan, Scenario};
use renderer_core::{DrawCommand, FontMetrics};

use super::{FramePlan, Prime, Reuse, plan_frame};

fn visual_rect(cmd: &DrawCommand, matrix: [f32; 6]) -> Option<Rect> {
    renderer_core::culling::command_visual_rect(cmd, matrix, &FontMetrics::default())
}

const PRIMED: Reuse = Reuse {
    prime: true,
    scroll: true,
    damage_with_clear: true,
    damage_transparent: false,
};

fn logical(scenario: &Scenario, scale: f32) -> (f32, f32) {
    (
        scenario.size.0 as f32 / scale,
        scenario.size.1 as f32 / scale,
    )
}

#[test]
fn a_primed_frame_scissors_each_shared_scenario_and_shifts_only_a_scroll() {
    let mut diff = FrameDiff::default();
    for scenario in dirty_scenarios::all() {
        let change = diff.compare(&scenario.new, &scenario.old, visual_rect);
        let expected = match &scenario.plan {
            Plan::Damage(rects) => FramePlan {
                prime: Prime::InPlace,
                dirty_scissor: rects.iter().copied().reduce(Rect::union),
                damage: true,
            },
            Plan::Scroll {
                clip,
                delta,
                exposed,
                extra,
            } => FramePlan {
                prime: Prime::Scrolled {
                    clip: *clip,
                    delta: (delta.0 as f32, delta.1 as f32),
                },
                dirty_scissor: Some(extra.iter().copied().fold(*exposed, Rect::union)),
                damage: true,
            },
        };
        assert_eq!(
            plan_frame(&change, PRIMED, 1.0, logical(&scenario, 1.0)),
            expected,
            "{}",
            scenario.name
        );
    }
}

fn ten_pixel_scroll() -> Scenario {
    dirty_scenarios::all()
        .into_iter()
        .find(|s| {
            matches!(
                s.plan,
                Plan::Scroll {
                    delta: (0, -10),
                    ..
                }
            )
        })
        .expect("a ten-pixel scroll")
}

#[test]
fn a_scroll_off_the_device_pixel_grid_is_repainted_in_place() {
    let scenario = ten_pixel_scroll();
    let Plan::Scroll { clip, .. } = scenario.plan else {
        unreachable!("found by its plan")
    };
    let change = FrameDiff::default().compare(&scenario.new, &scenario.old, visual_rect);

    let fractional = plan_frame(&change, PRIMED, 1.25, logical(&scenario, 1.25));
    assert_eq!(fractional.prime, Prime::InPlace, "12.5 device pixels");
    let scissor = fractional.dirty_scissor.expect("still damage-tracked");
    assert!(
        scissor.x <= clip.x
            && scissor.y <= clip.y
            && scissor.x + scissor.width >= clip.x + clip.width
            && scissor.y + scissor.height >= clip.y + clip.height,
        "the scrolled content is repainted where it now is: {scissor:?}"
    );

    let doubled = plan_frame(&change, PRIMED, 2.0, logical(&scenario, 2.0));
    assert!(
        matches!(doubled.prime, Prime::Scrolled { .. }),
        "20 device pixels: {doubled:?}"
    );
}

#[test]
fn a_target_that_cannot_be_primed_never_shifts() {
    let scenario = ten_pixel_scroll();
    let change = FrameDiff::default().compare(&scenario.new, &scenario.old, visual_rect);
    let transparent = Reuse {
        prime: false,
        scroll: false,
        damage_with_clear: false,
        damage_transparent: true,
    };
    let plan = plan_frame(&change, transparent, 1.0, logical(&scenario, 1.0));
    assert_eq!(plan.prime, Prime::None);
    assert!(!plan.damage);
    assert!(plan.dirty_scissor.is_some());
}
