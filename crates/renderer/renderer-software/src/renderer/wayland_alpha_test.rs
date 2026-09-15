use geometry_core::Rect;
use smallvec::smallvec;

use super::super::pixels::PixelFormat;
use super::super::present::{FrameOp, PixelRect, PresentLog};
use super::super::test_frames::{converted, paint, pattern};
use super::{PoolResize, ShmFile, Swapchain, frame_bytes, pool_resize};

fn argb(rgba: &[u8]) -> Vec<u32> {
    converted(rgba, PixelFormat::Argb8888)
}

fn present(
    chain: &mut Swapchain<Vec<u32>>,
    log: &mut PresentLog,
    rgba: &[u8],
    (width, height): (usize, usize),
    op: FrameOp,
) -> FrameOp {
    chain.back_mut().resize(width * height, 0xDEAD_BEEF);
    log.record(op);
    let damage = chain.swap_in(log, rgba, width, height);
    log.presented();
    damage
}

#[test]
fn a_4k_pool_is_exactly_the_frame() {
    let size = frame_bytes(3840, 2160).unwrap();
    assert_eq!(size, 3840 * 2160 * 4);
    assert_eq!(ShmFile::new(size).unwrap().size(), 3840 * 2160 * 4);
}

#[test]
fn a_frame_past_the_protocols_i32_is_refused() {
    assert!(frame_bytes(46_341, 46_341).is_err());
}

#[test]
fn a_pool_grows_in_place_and_is_recreated_only_below_half() {
    assert_eq!(pool_resize(100, 101), PoolResize::Grow(101));
    assert_eq!(pool_resize(100, 100), PoolResize::Keep);
    assert_eq!(pool_resize(100, 50), PoolResize::Keep);
    assert_eq!(pool_resize(100, 49), PoolResize::Recreate(49));
}

#[test]
fn growing_and_shrinking_never_leaves_the_pool_smaller_than_the_frame() {
    let sizes = [
        (640, 480),
        (3840, 2160),
        (1920, 1080),
        (2560, 1440),
        (2000, 1500),
        (64, 64),
        (5120, 2880),
        (1, 1),
    ];
    let mut file = ShmFile::new(frame_bytes(1, 1).unwrap()).unwrap();
    for (width, height) in sizes {
        let needed = frame_bytes(width, height).unwrap();
        let change = file.fit(needed).unwrap();
        match change {
            PoolResize::Grow(size) | PoolResize::Recreate(size) => {
                assert_eq!(size, needed);
                assert_eq!(file.size(), needed, "{width}x{height} after {change:?}");
            }
            PoolResize::Keep => assert!(file.size() >= needed && file.size() / 2 <= needed),
        }

        let count = width as usize * height as usize;
        let value = 0xFF00_0000 | count as u32;
        file.pixels_mut(count).fill(value);
        let pixels = file.pixels(count);
        assert_eq!(pixels.len(), count);
        assert_eq!(pixels[count - 1], value, "{width}x{height} reads back");
    }
}

#[test]
fn a_glyph_sized_change_on_a_4k_frame_converts_and_damages_only_the_glyph() {
    const SIZE: (usize, usize) = (3840, 2160);
    let mut log = PresentLog::new();
    let mut chain = Swapchain::new(Vec::new(), Vec::new());

    let first = pattern(SIZE.0, SIZE.1, 1);
    let damage = present(&mut chain, &mut log, &first, SIZE, FrameOp::Full);
    assert_eq!(damage, FrameOp::Full);

    // Every pixel differs from the first frame, so a pixel converted outside the declared glyph would show.
    let second = pattern(SIZE.0, SIZE.1, 2);
    let glyph = Rect::new(1200.0, 700.0, 9.0, 16.0);
    let damage = present(
        &mut chain,
        &mut log,
        &second,
        SIZE,
        FrameOp::Regions(smallvec![glyph]),
    );

    assert_eq!(
        damage
            .pixel_rects(SIZE.0 as u32, SIZE.1 as u32)
            .unwrap()
            .as_slice(),
        &[PixelRect {
            x: 1200,
            y: 700,
            width: 9,
            height: 16,
        }]
    );
    let (before, after) = (argb(&first), argb(&second));
    let converted: Vec<usize> = (0..before.len())
        .filter(|&i| chain.front()[i] != before[i])
        .collect();
    assert_eq!(
        converted.len(),
        9 * 16,
        "only the glyph's pixels were converted"
    );
    for i in converted {
        let (x, y) = (i % SIZE.0, i / SIZE.0);
        assert!((1200..1209).contains(&x) && (700..716).contains(&y));
        assert_eq!(chain.front()[i], after[i]);
    }
}

#[test]
fn both_buffers_hold_the_frames_their_ages_claim_across_alternating_updates() {
    const SIZE: (usize, usize) = (48, 32);
    let left = Rect::new(2.0, 3.0, 10.0, 7.0);
    let right = Rect::new(30.0, 20.0, 12.0, 9.0);
    let middle = Rect::new(8.0, 5.0, 30.0, 20.0);
    let ops = [
        FrameOp::Full,
        FrameOp::Regions(smallvec![left]),
        FrameOp::Regions(smallvec![right]),
        FrameOp::NoChange,
        FrameOp::Regions(smallvec![left, right]),
        FrameOp::Regions(smallvec![middle]),
        FrameOp::Full,
        FrameOp::Regions(smallvec![right]),
        FrameOp::NoChange,
        FrameOp::Regions(smallvec![left]),
    ];

    let mut log = PresentLog::new();
    let mut chain = Swapchain::new(Vec::new(), Vec::new());
    let mut frame = pattern(SIZE.0, SIZE.1, 7);
    let mut previous: Option<Vec<u32>> = None;

    for (step, op) in ops.into_iter().enumerate() {
        paint(&mut frame, SIZE.0, &op, step as u32 + 1);
        present(&mut chain, &mut log, &frame, SIZE, op);

        let reference = argb(&frame);
        assert_eq!(chain.front.age, 1, "step {step}");
        assert!(
            chain.front() == &reference,
            "step {step}: the front shows the frame"
        );
        match &previous {
            None => assert_eq!(chain.back.age, 0, "the back has never been filled"),
            Some(previous) => {
                assert_eq!(chain.back.age, 2, "step {step}");
                assert!(
                    chain.back() == previous,
                    "step {step}: the back holds the frame before"
                );
            }
        }
        previous = Some(reference);
    }
}

#[test]
fn changes_that_missed_a_present_are_converted_and_damaged_with_the_next() {
    const SIZE: (usize, usize) = (40, 30);
    let earlier = FrameOp::Regions(smallvec![Rect::new(10.0, 10.0, 5.0, 5.0)]);
    let first = FrameOp::Regions(smallvec![Rect::new(2.0, 2.0, 6.0, 5.0)]);
    let second = FrameOp::Regions(smallvec![Rect::new(20.0, 15.0, 8.0, 9.0)]);
    let mut log = PresentLog::new();
    let mut chain = Swapchain::new(Vec::new(), Vec::new());
    let mut frame = pattern(SIZE.0, SIZE.1, 1);
    present(&mut chain, &mut log, &frame, SIZE, FrameOp::Full);
    paint(&mut frame, SIZE.0, &earlier, 2);
    present(&mut chain, &mut log, &frame, SIZE, earlier);

    paint(&mut frame, SIZE.0, &first, 3);
    log.record(first.clone());
    paint(&mut frame, SIZE.0, &second, 4);
    let damage = present(&mut chain, &mut log, &frame, SIZE, second.clone());

    assert_eq!(damage, first.merge(second));
    assert!(chain.front() == &argb(&frame));
}

#[test]
fn more_regions_than_fit_inline_collapse_into_their_bounds_through_a_present() {
    const SIZE: (usize, usize) = (120, 40);
    let dots = |y: f32| -> Vec<FrameOp> {
        (0..9)
            .map(|i| FrameOp::Regions(smallvec![Rect::new(4.0 + i as f32 * 12.0, y, 3.0, 3.0)]))
            .collect()
    };
    let mut log = PresentLog::new();
    let mut chain = Swapchain::new(Vec::new(), Vec::new());
    let mut frame = pattern(SIZE.0, SIZE.1, 1);
    present(&mut chain, &mut log, &frame, SIZE, FrameOp::Full);
    let mut previous = argb(&frame);

    // The second round's back missed the first round's present, so what it copies was collapsed too.
    for (round, y) in [4.0, 20.0].into_iter().enumerate() {
        let ops = dots(y);
        for (i, op) in ops.iter().enumerate() {
            paint(&mut frame, SIZE.0, op, (round * 10 + i) as u32 + 2);
        }
        let (last, rest) = ops.split_last().unwrap();
        for op in rest {
            log.record(op.clone());
        }
        let damage = present(&mut chain, &mut log, &frame, SIZE, last.clone());

        assert_eq!(
            damage,
            FrameOp::Regions(smallvec![Rect::new(4.0, y, 99.0, 3.0)]),
            "round {round}"
        );
        assert!(chain.front() == &argb(&frame), "round {round}");
        assert!(chain.back() == &previous, "round {round}");
        previous = argb(&frame);
    }
}

#[test]
fn a_first_frame_or_a_resize_presents_everything() {
    let mut log = PresentLog::new();
    let mut chain = Swapchain::new(Vec::new(), Vec::new());
    let region = FrameOp::Regions(smallvec![Rect::new(1.0, 1.0, 2.0, 2.0)]);
    // Clears the log's initial full change, so only the swapchain's own state can force a full present.
    log.presented();

    let small = pattern(16, 16, 3);
    let damage = present(&mut chain, &mut log, &small, (16, 16), region.clone());
    assert_eq!(damage, FrameOp::Full, "nothing is on screen yet");
    assert!(chain.front() == &argb(&small));

    let large = pattern(24, 20, 4);
    let damage = present(&mut chain, &mut log, &large, (24, 20), region.clone());
    assert_eq!(damage, FrameOp::Full, "what is on screen is another size");
    assert!(chain.front() == &argb(&large));

    let mut next = large.clone();
    paint(&mut next, 24, &region, 9);
    let damage = present(&mut chain, &mut log, &next, (24, 20), region.clone());
    assert_eq!(damage, region, "once the front matches, only the change");
    assert!(chain.front() == &argb(&next));
}

// The log is left unreset across both resizes, so only the swapchain's own sizes can keep the old back from being trusted.
#[test]
fn a_back_buffer_left_at_the_size_the_surface_returns_to_is_not_trusted() {
    const S1: (usize, usize) = (16, 12);
    const S2: (usize, usize) = (20, 14);
    let region = FrameOp::Regions(smallvec![Rect::new(3.0, 3.0, 4.0, 4.0)]);
    let mut log = PresentLog::new();
    let mut chain = Swapchain::new(Vec::new(), Vec::new());

    let mut first = pattern(S1.0, S1.1, 1);
    present(&mut chain, &mut log, &first, S1, FrameOp::Full);
    paint(&mut first, S1.0, &region, 2);
    present(&mut chain, &mut log, &first, S1, region.clone());
    present(
        &mut chain,
        &mut log,
        &pattern(S2.0, S2.1, 3),
        S2,
        region.clone(),
    );
    assert_eq!((chain.back.size, chain.back.age), (S1, 2));

    let mut returned = pattern(S1.0, S1.1, 4);
    let damage = present(&mut chain, &mut log, &returned, S1, region.clone());
    assert_eq!(damage, FrameOp::Full, "what is on screen is the other size");
    assert!(chain.front() == &argb(&returned));

    paint(&mut returned, S1.0, &region, 5);
    let damage = present(&mut chain, &mut log, &returned, S1, region.clone());
    assert_eq!(damage, region);
    assert!(chain.front() == &argb(&returned));
    assert_eq!(chain.back.age, 2);
}
