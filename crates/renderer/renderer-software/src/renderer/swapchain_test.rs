use std::sync::{Arc, Mutex};

use geometry_core::Rect;
use smallvec::smallvec;

use super::super::pixels::PixelFormat;
use super::super::present::{FrameOp, PixelRect, PresentLog};
use super::super::test_frames::{Compositor, MemoryWire, Screen, converted, paint, pattern};
use super::{AlphaPresenter, ShmBuffer, ShmLayout, ShmPresenter};

type Presenter = ShmPresenter<MemoryWire>;

fn presenter(compositor: Compositor, layout: ShmLayout) -> (Presenter, Arc<Mutex<Screen>>) {
    let (wire, screen) = MemoryWire::new(compositor);
    (ShmPresenter::new(wire, layout), screen)
}

fn argb(rgba: &[u8]) -> Vec<u32> {
    converted(rgba, PixelFormat::Argb8888)
}

fn front(presenter: &Presenter) -> &[u32] {
    presenter
        .chain()
        .expect("a frame was presented")
        .front()
        .pixels()
}

fn last_commit(screen: &Mutex<Screen>) -> FrameOp {
    screen
        .lock()
        .unwrap()
        .commits
        .last()
        .cloned()
        .expect("a commit")
}

fn convert(
    presenter: &mut Presenter,
    log: &mut PresentLog,
    rgba: &[u8],
    (width, height): (usize, usize),
    op: FrameOp,
) {
    log.record(op);
    presenter.present_converted(rgba, width as u32, height as u32, log);
}

fn regions(op: &FrameOp, width: usize, height: usize) -> Vec<Rect> {
    match op {
        FrameOp::NoChange => Vec::new(),
        FrameOp::Full => vec![Rect::new(0.0, 0.0, width as f32, height as f32)],
        FrameOp::Regions(regions) => regions.to_vec(),
    }
}

// Draws as the renderer does: only what changed into a buffer that was caught up, all of it into one that was not.
fn draw_in_place(
    presenter: &mut Presenter,
    log: &mut PresentLog,
    frame: &[u8],
    (width, height): (usize, usize),
    op: FrameOp,
) -> bool {
    let caught_up = presenter
        .begin(
            width as u32,
            height as u32,
            log,
            matches!(op, FrameOp::Full),
        )
        .expect("a buffer to draw into");
    let drawn = if caught_up { op } else { FrameOp::Full };
    let mut target = presenter.target().expect("the buffer begin picked");
    let data = target.data_mut();
    for rect in regions(&drawn, width, height) {
        for y in rect.y as usize..(rect.y + rect.height) as usize {
            let span =
                (y * width + rect.x as usize) * 4..(y * width + (rect.x + rect.width) as usize) * 4;
            data[span.clone()].copy_from_slice(&frame[span]);
        }
    }
    log.record(drawn);
    presenter.present(log);
    caught_up
}

#[test]
fn a_glyph_sized_change_on_a_4k_frame_converts_and_damages_only_the_glyph() {
    const SIZE: (usize, usize) = (3840, 2160);
    let (mut presenter, screen) = presenter(Compositor::Holds, ShmLayout::Argb);
    let mut log = PresentLog::new();

    let first = pattern(SIZE.0, SIZE.1, 1);
    convert(&mut presenter, &mut log, &first, SIZE, FrameOp::Full);
    assert_eq!(last_commit(&screen), FrameOp::Full);

    // Every pixel differs from the first frame, so a pixel converted outside the declared glyph would show.
    let second = pattern(SIZE.0, SIZE.1, 2);
    let glyph = Rect::new(1200.0, 700.0, 9.0, 16.0);
    convert(
        &mut presenter,
        &mut log,
        &second,
        SIZE,
        FrameOp::Regions(smallvec![glyph]),
    );

    assert_eq!(
        last_commit(&screen)
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
    let shown = front(&presenter);
    let converted: Vec<usize> = (0..before.len())
        .filter(|&i| shown[i] != before[i])
        .collect();
    assert_eq!(
        converted.len(),
        9 * 16,
        "only the glyph's pixels were converted"
    );
    for i in converted {
        let (x, y) = (i % SIZE.0, i / SIZE.0);
        assert!((1200..1209).contains(&x) && (700..716).contains(&y));
        assert_eq!(shown[i], after[i]);
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

    let (mut presenter, screen) = presenter(Compositor::Holds, ShmLayout::Argb);
    let mut log = PresentLog::new();
    let mut frame = pattern(SIZE.0, SIZE.1, 7);
    let mut previous: Option<Vec<u32>> = None;

    for (step, op) in ops.into_iter().enumerate() {
        paint(&mut frame, SIZE.0, &op, step as u32 + 1);
        convert(&mut presenter, &mut log, &frame, SIZE, op);

        let reference = argb(&frame);
        let chain = presenter.chain().unwrap();
        assert_eq!(chain.front.age, 1, "step {step}");
        assert!(
            chain.front().pixels() == reference,
            "step {step}: the front shows the frame"
        );
        assert!(
            screen.lock().unwrap().pixels == reference,
            "step {step}: what was damaged brings the compositor up to the frame"
        );
        match &previous {
            None => assert!(
                chain.back.is_none(),
                "no second buffer before one is needed"
            ),
            Some(previous) => {
                let back = chain.back.as_ref().unwrap();
                assert_eq!(back.age, 2, "step {step}");
                assert!(
                    back.buffer.pixels() == previous,
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
    let (mut presenter, screen) = presenter(Compositor::Holds, ShmLayout::Argb);
    let mut log = PresentLog::new();
    let mut frame = pattern(SIZE.0, SIZE.1, 1);
    convert(&mut presenter, &mut log, &frame, SIZE, FrameOp::Full);
    paint(&mut frame, SIZE.0, &earlier, 2);
    convert(&mut presenter, &mut log, &frame, SIZE, earlier);

    paint(&mut frame, SIZE.0, &first, 3);
    log.record(first.clone());
    paint(&mut frame, SIZE.0, &second, 4);
    convert(&mut presenter, &mut log, &frame, SIZE, second.clone());

    assert_eq!(last_commit(&screen), first.merge(second));
    assert!(front(&presenter) == argb(&frame));
}

#[test]
fn more_regions_than_fit_inline_collapse_into_their_bounds_through_a_present() {
    const SIZE: (usize, usize) = (120, 40);
    let dots = |y: f32| -> Vec<FrameOp> {
        (0..9)
            .map(|i| FrameOp::Regions(smallvec![Rect::new(4.0 + i as f32 * 12.0, y, 3.0, 3.0)]))
            .collect()
    };
    let (mut presenter, screen) = presenter(Compositor::Holds, ShmLayout::Argb);
    let mut log = PresentLog::new();
    let mut frame = pattern(SIZE.0, SIZE.1, 1);
    convert(&mut presenter, &mut log, &frame, SIZE, FrameOp::Full);
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
        convert(&mut presenter, &mut log, &frame, SIZE, last.clone());

        assert_eq!(
            last_commit(&screen),
            FrameOp::Regions(smallvec![Rect::new(4.0, y, 99.0, 3.0)]),
            "round {round}"
        );
        assert!(front(&presenter) == argb(&frame), "round {round}");
        let back = presenter.chain().unwrap().back.as_ref().unwrap();
        assert!(back.buffer.pixels() == previous, "round {round}");
        previous = argb(&frame);
    }
}

#[test]
fn a_first_frame_or_a_resize_presents_everything() {
    for compositor in [Compositor::Holds, Compositor::Copies] {
        let (mut presenter, screen) = presenter(compositor, ShmLayout::Argb);
        let mut log = PresentLog::new();
        let region = FrameOp::Regions(smallvec![Rect::new(1.0, 1.0, 2.0, 2.0)]);
        // Clears the log's initial full change, so only the swapchain's own state can force a full present.
        log.presented();

        let small = pattern(16, 16, 3);
        convert(&mut presenter, &mut log, &small, (16, 16), region.clone());
        assert_eq!(
            last_commit(&screen),
            FrameOp::Full,
            "{compositor:?}: nothing is on screen yet"
        );
        assert!(front(&presenter) == argb(&small));

        let large = pattern(24, 20, 4);
        convert(&mut presenter, &mut log, &large, (24, 20), region.clone());
        assert_eq!(
            last_commit(&screen),
            FrameOp::Full,
            "{compositor:?}: what is on screen is another size"
        );
        assert!(front(&presenter) == argb(&large));

        let mut next = large.clone();
        paint(&mut next, 24, &region, 9);
        convert(&mut presenter, &mut log, &next, (24, 20), region.clone());
        assert_eq!(
            last_commit(&screen),
            region,
            "{compositor:?}: once the front matches, only the change"
        );
        assert!(front(&presenter) == argb(&next));
        assert!(screen.lock().unwrap().pixels == argb(&next));
    }
}

// The log is left unreset across both resizes, so only the swapchain's own sizes can keep the old back from being trusted.
#[test]
fn a_back_buffer_left_at_the_size_the_surface_returns_to_is_not_trusted() {
    const S1: (usize, usize) = (16, 12);
    const S2: (usize, usize) = (20, 14);
    let region = FrameOp::Regions(smallvec![Rect::new(3.0, 3.0, 4.0, 4.0)]);
    let (mut presenter, screen) = presenter(Compositor::Holds, ShmLayout::Argb);
    let mut log = PresentLog::new();

    let mut first = pattern(S1.0, S1.1, 1);
    convert(&mut presenter, &mut log, &first, S1, FrameOp::Full);
    paint(&mut first, S1.0, &region, 2);
    convert(&mut presenter, &mut log, &first, S1, region.clone());
    convert(
        &mut presenter,
        &mut log,
        &pattern(S2.0, S2.1, 3),
        S2,
        region.clone(),
    );
    let back = presenter.chain().unwrap().back.as_ref().unwrap();
    assert_eq!((back.size, back.age), (S1, 2));

    let mut returned = pattern(S1.0, S1.1, 4);
    convert(&mut presenter, &mut log, &returned, S1, region.clone());
    assert_eq!(
        last_commit(&screen),
        FrameOp::Full,
        "what is on screen is the other size"
    );
    assert!(front(&presenter) == argb(&returned));

    paint(&mut returned, S1.0, &region, 5);
    convert(&mut presenter, &mut log, &returned, S1, region.clone());
    assert_eq!(last_commit(&screen), region);
    assert!(front(&presenter) == argb(&returned));
    assert_eq!(presenter.chain().unwrap().back.as_ref().unwrap().age, 2);
}

#[test]
fn a_compositor_that_releases_at_once_is_presented_from_one_buffer() {
    const SIZE: (usize, usize) = (40, 24);
    let ops = [
        FrameOp::Full,
        FrameOp::Regions(smallvec![Rect::new(1.0, 1.0, 5.0, 5.0)]),
        FrameOp::Regions(smallvec![Rect::new(20.0, 10.0, 9.0, 7.0)]),
        FrameOp::NoChange,
        FrameOp::Regions(smallvec![Rect::new(0.0, 20.0, 40.0, 4.0)]),
    ];
    for layout in [ShmLayout::Argb, ShmLayout::Rgba] {
        let (mut presenter, screen) = presenter(Compositor::Copies, layout);
        let mut log = PresentLog::new();
        let mut frame = pattern(SIZE.0, SIZE.1, 5);
        for round in 0..60u32 {
            let op = ops[round as usize % ops.len()].clone();
            paint(&mut frame, SIZE.0, &op, round + 1);
            match layout {
                ShmLayout::Argb => convert(&mut presenter, &mut log, &frame, SIZE, op),
                ShmLayout::Rgba => {
                    draw_in_place(&mut presenter, &mut log, &frame, SIZE, op);
                }
            }
            let screen = screen.lock().unwrap();
            let shows = match layout {
                ShmLayout::Argb => screen.pixels == argb(&frame),
                ShmLayout::Rgba => screen.bytes() == frame,
            };
            assert!(
                shows,
                "{layout:?}, round {round}: the compositor shows the frame"
            );
            assert_eq!(
                screen.created, 1,
                "{layout:?}: every frame refills the one buffer"
            );
        }
    }
}

#[test]
fn drawing_in_place_keeps_every_committed_frame_exact_across_idle_releases() {
    const SIZE: (usize, usize) = (64, 40);
    let ops = [
        FrameOp::Regions(smallvec![Rect::new(2.0, 2.0, 10.0, 8.0)]),
        FrameOp::Regions(smallvec![Rect::new(40.0, 20.0, 20.0, 15.0)]),
        FrameOp::NoChange,
        FrameOp::Regions(smallvec![
            Rect::new(5.0, 30.0, 50.0, 6.0),
            Rect::new(60.0, 0.0, 4.0, 40.0)
        ]),
        FrameOp::Regions(smallvec![Rect::new(0.0, 0.0, 64.0, 3.0)]),
    ];
    for compositor in [Compositor::Holds, Compositor::Copies] {
        let (mut presenter, screen) = presenter(compositor, ShmLayout::Rgba);
        let mut log = PresentLog::new();
        let mut frame = pattern(SIZE.0, SIZE.1, 11);
        assert!(
            !draw_in_place(&mut presenter, &mut log, &frame, SIZE, FrameOp::Full),
            "nothing is on screen yet"
        );

        for round in 0..40u32 {
            let op = ops[round as usize % ops.len()].clone();
            paint(&mut frame, SIZE.0, &op, round + 20);
            let caught_up = draw_in_place(&mut presenter, &mut log, &frame, SIZE, op);
            assert!(
                caught_up,
                "{compositor:?}, round {round}: only the change is drawn"
            );
            assert_eq!(
                presenter.presented(),
                Some(frame.as_slice()),
                "{compositor:?}, round {round}: the front is the frame"
            );
            assert!(
                screen.lock().unwrap().bytes() == frame,
                "{compositor:?}, round {round}: the compositor shows the frame"
            );
            if round % 7 == 6 {
                presenter.release_idle();
                assert_eq!(
                    screen.lock().unwrap().alive,
                    1,
                    "{compositor:?}: idle keeps only the front"
                );
                assert_eq!(presenter.presented(), Some(frame.as_slice()));
            }
        }
        let screen = screen.lock().unwrap();
        match compositor {
            Compositor::Copies => assert_eq!(screen.created, 1),
            Compositor::Holds | Compositor::Lags => assert!(
                screen.created > 2,
                "a held front brings a second buffer back after each idle release"
            ),
        }
    }
}

#[test]
fn a_held_front_brings_in_a_second_buffer_that_going_idle_drops() {
    const SIZE: (usize, usize) = (32, 20);
    let change = FrameOp::Regions(smallvec![Rect::new(4.0, 4.0, 6.0, 6.0)]);
    let (mut presenter, screen) = presenter(Compositor::Holds, ShmLayout::Rgba);
    let mut log = PresentLog::new();
    let mut frame = pattern(SIZE.0, SIZE.1, 3);
    draw_in_place(&mut presenter, &mut log, &frame, SIZE, FrameOp::Full);
    assert_eq!(
        screen.lock().unwrap().alive,
        1,
        "the first frame needs one buffer"
    );

    paint(&mut frame, SIZE.0, &change, 4);
    draw_in_place(&mut presenter, &mut log, &frame, SIZE, change.clone());
    assert_eq!(
        screen.lock().unwrap().alive,
        2,
        "the compositor holds the first"
    );

    presenter.release_idle();
    assert_eq!(screen.lock().unwrap().alive, 1);
    assert_eq!(presenter.chain().unwrap().buffers(), 1);

    paint(&mut frame, SIZE.0, &change, 5);
    assert!(
        draw_in_place(&mut presenter, &mut log, &frame, SIZE, change.clone()),
        "a new second buffer is caught up from the front, so only the change is drawn"
    );
    let screen = screen.lock().unwrap();
    assert_eq!((screen.alive, screen.created), (2, 3));
    assert!(screen.bytes() == frame);
    assert_eq!(
        screen.commits.last(),
        Some(&change),
        "and only the change is damaged"
    );
}

#[test]
fn a_present_without_a_begin_commits_nothing() {
    let (mut presenter, screen) = presenter(Compositor::Copies, ShmLayout::Rgba);
    let mut log = PresentLog::new();
    log.record(FrameOp::Full);
    presenter.present(&mut log);
    assert!(presenter.target().is_none());
    assert!(presenter.presented().is_none());
    assert!(screen.lock().unwrap().commits.is_empty());
}

#[test]
fn a_back_that_ages_through_many_in_place_presents_is_refreshed_whole() {
    const SIZE: (usize, usize) = (24, 16);
    let change = FrameOp::Regions(smallvec![Rect::new(1.0, 1.0, 3.0, 3.0)]);
    let (mut presenter, screen) = presenter(Compositor::Holds, ShmLayout::Rgba);
    let mut log = PresentLog::new();
    let mut frame = pattern(SIZE.0, SIZE.1, 1);
    draw_in_place(&mut presenter, &mut log, &frame, SIZE, FrameOp::Full);
    paint(&mut frame, SIZE.0, &change, 2);
    draw_in_place(&mut presenter, &mut log, &frame, SIZE, change.clone());

    // Waiting releases what the compositor held, so the front is refilled in place hundreds of times while the back only ages.
    for round in 0..300u32 {
        let spot = FrameOp::Regions(smallvec![Rect::new((round % 20) as f32, 10.0, 2.0, 2.0)]);
        paint(&mut frame, SIZE.0, &spot, round + 3);
        let mut wire_released = presenter.chain().unwrap().front().released();
        if !wire_released {
            use super::Wire;
            wire_released = presenter.wire.wait();
        }
        assert!(wire_released);
        draw_in_place(&mut presenter, &mut log, &frame, SIZE, spot);
    }
    let back_age = presenter.chain().unwrap().back.as_ref().unwrap().age;
    assert_eq!(back_age, u8::MAX, "the age saturates rather than wrapping");

    // Held again, so the next frame goes to the stale back, which has to be refreshed whole.
    let spot = FrameOp::Regions(smallvec![Rect::new(8.0, 2.0, 2.0, 2.0)]);
    paint(&mut frame, SIZE.0, &spot, 999);
    assert!(draw_in_place(&mut presenter, &mut log, &frame, SIZE, spot));
    assert!(screen.lock().unwrap().bytes() == frame);
    assert_eq!(presenter.presented(), Some(frame.as_slice()));
}

#[test]
fn a_whole_redraw_copies_nothing_into_the_buffer_it_overwrites() {
    const SIZE: (usize, usize) = (32, 20);
    let change = FrameOp::Regions(smallvec![Rect::new(2.0, 2.0, 5.0, 5.0)]);
    for layout in [ShmLayout::Rgba, ShmLayout::Argb] {
        let (mut presenter, screen) = presenter(Compositor::Holds, layout);
        let mut log = PresentLog::new();
        let mut frame = pattern(SIZE.0, SIZE.1, 1);
        let show = |presenter: &mut Presenter, log: &mut PresentLog, frame: &[u8], op| match layout
        {
            ShmLayout::Rgba => {
                draw_in_place(presenter, log, frame, SIZE, op);
            }
            ShmLayout::Argb => convert(presenter, log, frame, SIZE, op),
        };
        show(&mut presenter, &mut log, &frame, FrameOp::Full);
        paint(&mut frame, SIZE.0, &change, 2);
        show(&mut presenter, &mut log, &frame, change.clone());
        let caught_up = presenter.chain().unwrap().copied;
        assert_eq!(
            caught_up,
            SIZE.0 * SIZE.1,
            "{layout:?}: a new back is caught up whole"
        );

        assert!(presenter.release_idle());
        frame = pattern(SIZE.0, SIZE.1, 3);
        show(&mut presenter, &mut log, &frame, FrameOp::Full);
        assert_eq!(
            presenter.chain().unwrap().copied,
            caught_up,
            "{layout:?}: nothing is copied into a buffer drawn over whole"
        );
        let screen = screen.lock().unwrap();
        match layout {
            ShmLayout::Rgba => assert!(screen.bytes() == frame),
            ShmLayout::Argb => assert!(screen.pixels == argb(&frame)),
        }
    }
}

#[test]
fn a_back_the_compositor_still_holds_at_idle_is_freed_once_it_is_released() {
    const SIZE: (usize, usize) = (24, 16);
    let change = FrameOp::Regions(smallvec![Rect::new(1.0, 1.0, 4.0, 4.0)]);
    let (mut presenter, screen) = presenter(Compositor::Lags, ShmLayout::Rgba);
    let mut log = PresentLog::new();
    let mut frame = pattern(SIZE.0, SIZE.1, 1);
    draw_in_place(&mut presenter, &mut log, &frame, SIZE, FrameOp::Full);
    paint(&mut frame, SIZE.0, &change, 2);
    draw_in_place(&mut presenter, &mut log, &frame, SIZE, change);

    assert!(
        !presenter.release_idle(),
        "the replaced buffer is not released yet"
    );
    assert_eq!(screen.lock().unwrap().alive, 2);

    screen.lock().unwrap().release_lagging();
    assert!(presenter.release_idle());
    assert_eq!(screen.lock().unwrap().alive, 1);
    assert_eq!(presenter.presented(), Some(frame.as_slice()));
}

#[test]
fn a_second_buffer_that_cannot_be_made_waits_for_the_front_instead() {
    const SIZE: (usize, usize) = (24, 16);
    let change = FrameOp::Regions(smallvec![Rect::new(3.0, 3.0, 6.0, 6.0)]);
    let (mut presenter, screen) = presenter(Compositor::Holds, ShmLayout::Rgba);
    let mut log = PresentLog::new();
    let mut frame = pattern(SIZE.0, SIZE.1, 1);
    draw_in_place(&mut presenter, &mut log, &frame, SIZE, FrameOp::Full);

    screen.lock().unwrap().refuse_buffers = true;
    paint(&mut frame, SIZE.0, &change, 2);
    assert!(
        draw_in_place(&mut presenter, &mut log, &frame, SIZE, change.clone()),
        "the front, once released, is refilled with only the change"
    );
    let screen = screen.lock().unwrap();
    assert_eq!((screen.created, screen.alive), (1, 1));
    assert!(screen.bytes() == frame);
    assert_eq!(screen.commits.last(), Some(&change));
}

#[test]
fn a_frame_abandoned_part_way_leaves_its_buffer_untrusted() {
    const SIZE: (usize, usize) = (24, 16);
    let change = FrameOp::Regions(smallvec![Rect::new(2.0, 2.0, 5.0, 5.0)]);
    for compositor in [Compositor::Copies, Compositor::Holds] {
        let (mut presenter, screen) = presenter(compositor, ShmLayout::Rgba);
        let mut log = PresentLog::new();
        let mut frame = pattern(SIZE.0, SIZE.1, 1);
        draw_in_place(&mut presenter, &mut log, &frame, SIZE, FrameOp::Full);
        paint(&mut frame, SIZE.0, &change, 2);
        draw_in_place(&mut presenter, &mut log, &frame, SIZE, change.clone());

        presenter
            .begin(SIZE.0 as u32, SIZE.1 as u32, &log, false)
            .expect("a buffer to draw into");
        presenter.target().unwrap().data_mut().fill(0x5A);
        presenter.abandon();
        assert!(presenter.target().is_none());

        paint(&mut frame, SIZE.0, &change, 3);
        draw_in_place(&mut presenter, &mut log, &frame, SIZE, change.clone());
        assert_eq!(
            presenter.presented(),
            Some(frame.as_slice()),
            "{compositor:?}: nothing of the abandoned frame shows"
        );
        assert!(screen.lock().unwrap().bytes() == frame, "{compositor:?}");
    }
}
