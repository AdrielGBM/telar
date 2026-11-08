use geometry_core::Rect;
use smallvec::smallvec;

use super::super::pixels::PixelFormat;
use super::super::test_frames::{converted, paint, pattern};
use super::{FrameOp, PixelRect, PresentLog, PresentPlan, SurfaceDamage, declared_damage};

#[test]
fn only_a_surface_that_keeps_its_contents_is_presented_with_partial_damage() {
    let changed = FrameOp::Regions(smallvec![Rect::new(2.0, 3.0, 4.0, 5.0)]);
    let rect = PixelRect {
        x: 2,
        y: 3,
        width: 4,
        height: 5,
    };

    assert_eq!(
        declared_damage(&changed, SurfaceDamage::Rects, 10, 10).as_deref(),
        Some(&[rect][..])
    );
    assert_eq!(
        declared_damage(&FrameOp::NoChange, SurfaceDamage::Rects, 10, 10).as_deref(),
        Some(&[][..]),
        "an unchanged frame declares nothing on a surface that kept the last one"
    );
    assert_eq!(
        declared_damage(&FrameOp::Full, SurfaceDamage::Rects, 10, 10),
        None
    );

    for op in [changed, FrameOp::NoChange, FrameOp::Full] {
        assert_eq!(
            declared_damage(&op, SurfaceDamage::Whole, 10, 10),
            None,
            "a surface that can lose its contents is presented whole: {op:?}"
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn a_surface_without_per_rect_damage_declares_all_of_a_change_or_none_of_it() {
    let changed = FrameOp::Regions(smallvec![Rect::new(2.0, 3.0, 4.0, 5.0)]);
    assert_eq!(
        declared_damage(&changed, SurfaceDamage::AllOrNothing, 10, 10),
        None
    );
    assert_eq!(
        declared_damage(&FrameOp::NoChange, SurfaceDamage::AllOrNothing, 10, 10).as_deref(),
        Some(&[][..])
    );
    assert_eq!(
        declared_damage(&FrameOp::Full, SurfaceDamage::AllOrNothing, 10, 10),
        None
    );
}

#[test]
fn a_refresh_converts_what_the_buffer_missed_and_what_changed_without_boxing_them_together() {
    const W: usize = 64;
    const H: usize = 16;
    let dots = |y: f32| -> FrameOp {
        FrameOp::Regions(
            (0..5)
                .map(|i| Rect::new(2.0 + i as f32 * 12.0, y, 4.0, 4.0))
                .collect(),
        )
    };
    let before = converted(&pattern(W, H, 1), PixelFormat::Xrgb8888);
    let mut buffer = before.clone();

    PresentPlan {
        stale: dots(2.0),
        changed: dots(10.0),
    }
    .refresh(&pattern(W, H, 2), &mut buffer, W, H, PixelFormat::Xrgb8888);

    let refreshed = buffer.iter().zip(&before).filter(|(a, b)| a != b).count();
    assert_eq!(refreshed, 10 * 4 * 4);
}

#[test]
fn an_unknown_buffer_or_one_older_than_the_log_refreshes_in_full() {
    let mut log = PresentLog::new();
    log.presented();
    log.record(FrameOp::Regions(smallvec![Rect::new(0.0, 0.0, 1.0, 1.0)]));

    assert_eq!(log.plan(0).stale, FrameOp::Full, "contents unknown");
    assert_eq!(
        log.plan(1).stale,
        FrameOp::NoChange,
        "holds the last present"
    );
    assert_eq!(
        log.plan(2).stale,
        FrameOp::Full,
        "missed the full first present"
    );
    assert_eq!(log.plan(3).stale, FrameOp::Full, "older than the log");
}

#[test]
fn frames_that_miss_a_present_carry_into_the_next() {
    let a = Rect::new(0.0, 0.0, 4.0, 4.0);
    let b = Rect::new(10.0, 10.0, 4.0, 4.0);
    let mut log = PresentLog::new();
    log.presented();

    log.record(FrameOp::Regions(smallvec![a]));
    log.record(FrameOp::NoChange);
    log.record(FrameOp::Regions(smallvec![b]));
    assert_eq!(log.plan(1).changed, FrameOp::Regions(smallvec![a, b]));

    log.presented();
    assert_eq!(log.plan(1).changed, FrameOp::NoChange);
    assert_eq!(log.plan(2).stale, FrameOp::Regions(smallvec![a, b]));
}

#[test]
fn regions_merged_past_the_inline_capacity_collapse_into_their_bounds() {
    let op = (0..9).fold(FrameOp::NoChange, |op, i| {
        op.merge(FrameOp::Regions(smallvec![Rect::new(
            i as f32 * 10.0,
            0.0,
            5.0,
            5.0
        )]))
    });
    assert_eq!(
        op,
        FrameOp::Regions(smallvec![Rect::new(0.0, 0.0, 85.0, 5.0)])
    );
}

#[test]
fn damage_is_whole_pixels_clamped_to_the_surface() {
    let op = FrameOp::Regions(smallvec![
        Rect::new(-2.0, 3.5, 6.0, 2.0),
        Rect::new(90.0, 90.0, 5.0, 5.0)
    ]);
    assert_eq!(
        op.pixel_rects(10, 10).unwrap().as_slice(),
        &[PixelRect {
            x: 0,
            y: 3,
            width: 4,
            height: 3,
        }]
    );
    assert_eq!(FrameOp::Full.pixel_rects(10, 10), None);
    assert!(FrameOp::NoChange.pixel_rects(10, 10).unwrap().is_empty());
}

// One buffer re-presented sees ages 0 then 1, as on softbuffer's X11 and Win32 backends; two presented in turn see 0, 0, then 2, as on its Wayland backend.
#[test]
fn a_refreshed_buffer_matches_the_frame_at_ages_0_1_and_2() {
    const W: usize = 40;
    const H: usize = 30;
    let a = Rect::new(3.0, 2.0, 9.0, 6.0);
    let b = Rect::new(25.0, 18.0, 10.0, 10.0);
    let c = Rect::new(0.0, 12.0, W as f32, 4.0);
    let ops = [
        FrameOp::Full,
        FrameOp::Regions(smallvec![a]),
        FrameOp::Regions(smallvec![b]),
        FrameOp::NoChange,
        FrameOp::Regions(smallvec![a, b]),
        FrameOp::Regions(smallvec![c]),
        FrameOp::Full,
        FrameOp::Regions(smallvec![b]),
        FrameOp::Regions(smallvec![a]),
    ];

    for (buffer_count, expected_ages) in [(1, [0, 1, 1, 1]), (2, [0, 0, 2, 2])] {
        let mut log = PresentLog::new();
        let mut buffers = vec![vec![0xDEAD_BEEF_u32; W * H]; buffer_count];
        let mut ages = vec![0u8; buffer_count];
        let mut seen = Vec::new();
        let mut frame = pattern(W, H, 5);

        for (step, op) in ops.iter().enumerate() {
            paint(&mut frame, W, op, step as u32 + 1);
            log.record(op.clone());
            let target = step % buffer_count;
            seen.push(ages[target]);
            log.plan(ages[target]).refresh(
                &frame,
                &mut buffers[target],
                W,
                H,
                PixelFormat::Xrgb8888,
            );
            log.presented();
            for age in ages.iter_mut().filter(|age| **age != 0) {
                *age += 1;
            }
            ages[target] = 1;

            assert!(
                buffers[target] == converted(&frame, PixelFormat::Xrgb8888),
                "{buffer_count} buffer(s), step {step}: stale pixels in the presented buffer"
            );
        }
        assert_eq!(&seen[..4], &expected_ages);
    }
}
