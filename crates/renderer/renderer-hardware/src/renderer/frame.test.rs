use geometry_core::Rect;
use renderer_core::dirty::FrameDiff;
use renderer_core::dirty_scenarios::{self, Plan, Scenario};
use renderer_core::{Color, DrawCommand, FontMetrics};

use super::{Conditions, FramePlan, Prime, Reuse, plan_frame};

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

/// A transparent frame is always single-sample, so gating the scroll blit on the sample count — as it was before the single-sample path learned to move its pixels by texture copy — would silently stop every transparent frame from scrolling.
///
/// `plan_frame` cannot catch that: it is handed a `Reuse` already built, so every test around it hand-writes one. This pins the decision itself.
#[test]
fn a_single_sample_target_scrolls_though_it_cannot_be_primed() {
    let transparent = Conditions {
        msaa_samples: 1,
        holds_retained: false,
        clear_color: None,
        clear_kept: true,
        frame_has_backdrop_blur: false,
        damage_enabled: true,
        scroll_enabled: true,
    };
    let reuse = Reuse::decide(transparent);
    assert!(
        !reuse.prime,
        "the single-sample target is the previous frame, so nothing draws it back in"
    );
    assert!(
        reuse.scroll,
        "and it still moves its scrolled pixels rather than repainting them"
    );
    assert!(reuse.damage_transparent);
    assert!(
        !reuse.damage_with_clear,
        "there is no clear colour to repaint inside the dirty rect"
    );

    let multisample = Conditions {
        msaa_samples: 4,
        holds_retained: true,
        clear_color: Some(Color::BLACK),
        ..transparent
    };
    let reuse = Reuse::decide(multisample);
    assert!(
        reuse.prime,
        "the multisample target keeps the previous frame elsewhere and needs it drawn in"
    );
    assert!(reuse.scroll);
    assert!(reuse.damage_with_clear);
    assert!(
        !reuse.damage_transparent,
        "a frame that clears is not the transparent path"
    );

    let no_blit = Conditions {
        scroll_enabled: false,
        ..transparent
    };
    assert!(
        !Reuse::decide(no_blit).scroll,
        "TELAR_HW_SCROLL_BLIT=0 still turns it off"
    );
}

/// A backend that reports it cannot move a scrolled region's pixels repaints them where they now are, however else the frame is planned.
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

/// A transparent frame renders into the very texture its previous frame is still in, so it cannot be primed by a quad — but its scrolled pixels still move, by texture copy, and only the exposed band is repainted.
#[test]
fn a_transparent_target_shifts_even_though_it_cannot_be_primed() {
    let scenario = ten_pixel_scroll();
    let Plan::Scroll {
        clip,
        delta,
        exposed,
        extra,
    } = &scenario.plan
    else {
        unreachable!("found by its plan")
    };
    let change = FrameDiff::default().compare(&scenario.new, &scenario.old, visual_rect);
    let transparent = Reuse {
        prime: false,
        scroll: true,
        damage_with_clear: false,
        damage_transparent: true,
    };

    let plan = plan_frame(&change, transparent, 1.0, logical(&scenario, 1.0));
    assert_eq!(
        plan.prime,
        Prime::Scrolled {
            clip: *clip,
            delta: (delta.0 as f32, delta.1 as f32),
        },
        "the scrolled pixels move rather than being repainted"
    );
    assert_eq!(
        plan.dirty_scissor,
        Some(extra.iter().copied().fold(*exposed, Rect::union)),
        "and the repaint is confined to what the move exposed"
    );
    assert!(
        !plan.damage,
        "with no clear colour of its own to repaint inside that rect"
    );
}
