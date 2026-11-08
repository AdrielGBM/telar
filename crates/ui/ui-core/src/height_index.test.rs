use super::*;

fn mixed() -> HeightIndex {
    HeightIndex::new([10.0, 30.0, 20.0, 40.0])
}

#[test]
fn offsets_are_the_heights_before_each_row() {
    let index = mixed();
    let offsets: Vec<f32> = (0..=4).map(|at| index.offset_of(at)).collect();
    assert_eq!(offsets, [0.0, 10.0, 40.0, 60.0, 100.0]);
    assert_eq!(index.total(), 100.0);
    assert_eq!(index.offset_of(99), 100.0, "past the end is the end");
    assert_eq!(index.span(1, 3), 50.0);
}

#[test]
fn a_row_owns_its_top_edge_and_not_its_bottom_one() {
    let index = mixed();
    assert_eq!(index.row_at(0.0), 0);
    assert_eq!(index.row_at(9.9), 0);
    assert_eq!(index.row_at(10.0), 1);
    assert_eq!(index.row_at(39.0), 1);
    assert_eq!(index.row_at(40.0), 2);
    assert_eq!(index.row_at(99.0), 3);
    assert_eq!(index.row_at(-5.0), 0, "above the list is its first row");
    assert_eq!(index.row_at(1_000.0), 3, "below the list is its last row");
    assert_eq!(HeightIndex::default().row_at(10.0), 0);
}

#[test]
fn setting_a_height_moves_every_row_after_it() {
    let mut index = mixed();
    assert_eq!(index.set(1, 50.0), 20.0);
    assert_eq!(index.height(1), 50.0);
    assert_eq!(index.offset_of(1), 10.0, "the rows before it stay put");
    assert_eq!(index.offset_of(2), 60.0);
    assert_eq!(index.total(), 120.0);
    assert_eq!(index.row_at(59.0), 1);
    assert_eq!(index.set(1, 50.0), 0.0, "the same height moves nothing");
    assert_eq!(index.set(9, 50.0), 0.0, "past the end changes nothing");
    assert_eq!(index.set(0, -4.0), -10.0, "a negative height is no height");
}

#[test]
fn the_window_covers_the_screen_plus_its_overscan_in_px() {
    let index = HeightIndex::new([20.0, 60.0, 20.0, 60.0, 20.0, 60.0, 20.0, 60.0, 20.0, 60.0]);
    assert_eq!(
        index.window(0.0, 100.0, 0.0),
        (0, 4),
        "rows 0 to 3 reach 160"
    );
    assert_eq!(
        index.window(100.0, 100.0, 0.0),
        (3, 6),
        "row 2 ends where the view starts, so it shows nothing"
    );
    assert_eq!(index.window(100.0, 100.0, 30.0), (1, 6));
    assert_eq!(
        index.window(390.0, 100.0, 30.0),
        (9, 10),
        "clamped at the end"
    );
}

#[test]
fn the_window_agrees_with_the_fixed_height_window_on_uniform_rows() {
    let index = HeightIndex::uniform(1000, 20.0);
    for offset in [0.0, 19.0, 200.0, 777.0, 19_800.0] {
        assert_eq!(
            index.window(offset, 100.0, 0.0),
            crate::visible_window(offset, 100.0, 20.0, 1000, 0),
            "at {offset}"
        );
    }
}

#[test]
fn an_unmeasured_viewport_or_an_empty_list_yields_every_row() {
    assert_eq!(mixed().window(0.0, 0.0, 0.0), (0, 4));
    assert_eq!(HeightIndex::default().window(0.0, 100.0, 50.0), (0, 0));
}

fn height_of(at: usize) -> f32 {
    (10 + (at * 7919) % 90) as f32
}

#[test]
fn a_hundred_thousand_rows_agree_with_a_running_sum_after_many_corrections() {
    const COUNT: usize = 100_000;
    let mut index = HeightIndex::new((0..COUNT).map(height_of));
    let mut heights: Vec<f32> = (0..COUNT).map(height_of).collect();
    for step in 0..2_000 {
        let at = (step * 104_729) % COUNT;
        let height = (step % 120) as f32;
        index.set(at, height);
        heights[at] = height;
    }
    let mut top = 0.0f64;
    for (at, &height) in heights.iter().enumerate() {
        if at % 997 == 0 {
            assert_eq!(f64::from(index.offset_of(at)), top, "row {at}'s top");
            if height > 0.0 {
                assert_eq!(index.row_at(top as f32), at, "the row at {top}");
                assert_eq!(index.row_at((top + f64::from(height) / 2.0) as f32), at);
            }
        }
        top += f64::from(height);
    }
    assert_eq!(f64::from(index.total()), top);
}

#[test]
fn a_hundred_thousand_rows_answer_in_logarithmic_time() {
    const COUNT: usize = 100_000;
    let started = std::time::Instant::now();
    let mut index = HeightIndex::new((0..COUNT).map(height_of));
    let total = index.total();
    let mut checksum = 0usize;
    for step in 0..COUNT {
        let offset = (step as f32 * 37.0) % total;
        let (first, last) = index.window(offset, 600.0, 200.0);
        checksum = checksum.wrapping_add(first + last + index.row_at(offset));
        index.set(step, height_of(step) + 1.0);
    }
    let elapsed = started.elapsed();
    assert!(checksum > 0);
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "100k windows, lookups and corrections took {elapsed:?}; a linear scan per call would take minutes"
    );
}

fn running_offsets(heights: &[f32]) -> Vec<f64> {
    let mut top = 0.0f64;
    let mut offsets = Vec::with_capacity(heights.len() + 1);
    for &height in heights {
        offsets.push(top);
        top += f64::from(height);
    }
    offsets.push(top);
    offsets
}

fn assert_matches(index: &HeightIndex, heights: &[f32]) {
    assert_eq!(index.len(), heights.len());
    let offsets = running_offsets(heights);
    for at in (0..=heights.len()).step_by(1.max(heights.len() / 200)) {
        assert_eq!(
            f64::from(index.offset_of(at)),
            offsets[at],
            "row {at}'s top"
        );
    }
    assert_eq!(f64::from(index.total()), offsets[heights.len()]);
    for (at, &height) in heights
        .iter()
        .enumerate()
        .step_by(1.max(heights.len() / 200))
    {
        assert_eq!(index.height(at), height);
        if height > 0.0 {
            assert_eq!(index.row_at(offsets[at] as f32), at);
        }
    }
}

#[test]
fn splicing_rows_in_and_out_agrees_with_a_running_sum() {
    let mut heights: Vec<f32> = (0..500).map(height_of).collect();
    let mut index = HeightIndex::new(heights.iter().copied());
    let edits: [(usize, usize, &[f32]); 9] = [
        (0, 0, &[7.0]),
        (250, 1, &[]),
        (499, 0, &[11.0, 13.0]),
        (10, 3, &[5.0, 6.0, 7.0]),
        (100, 0, &[]),
        (0, 4, &[]),
        (300, 50, &[9.0]),
        (1_000, 2, &[3.0]),
        (200, 2, &[-4.0, 8.0]),
    ];
    for (at, remove, insert) in edits {
        index.splice(at, remove, insert.iter().copied());
        let at = at.min(heights.len());
        let end = (at + remove).min(heights.len());
        heights.splice(at..end, insert.iter().map(|h| h.max(0.0)));
        assert_matches(&index, &heights);
    }
    index.splice(0, heights.len(), []);
    assert!(index.is_empty());
    assert_eq!(index.total(), 0.0);
    index.splice(0, 0, [4.0, 6.0]);
    assert_eq!(index.offset_of(1), 4.0);
    assert_eq!(index.row_at(5.0), 1);
}

#[test]
fn replacing_as_many_rows_as_it_removes_updates_them_in_place() {
    let mut heights: Vec<f32> = (0..5_000).map(height_of).collect();
    let mut index = HeightIndex::new(heights.iter().copied());
    index.splice(100, 3, [1.0, 2.0, 3.0]);
    heights.splice(100..103, [1.0, 2.0, 3.0]);
    assert_matches(&index, &heights);
    index.splice(4_990, 5, [9.0; 5]);
    heights.splice(4_990..4_995, [9.0; 5]);
    assert_matches(&index, &heights);
}

#[test]
fn a_thousand_single_row_edits_in_a_hundred_thousand_rows_are_quick() {
    const COUNT: usize = 100_000;
    let mut heights: Vec<f32> = (0..COUNT).map(height_of).collect();
    let mut index = HeightIndex::new(heights.iter().copied());
    let started = std::time::Instant::now();
    for step in 0..1_000usize {
        let at = (step * 104_729) % heights.len();
        if step % 2 == 0 {
            index.splice(at, 0, [25.0]);
            heights.insert(at, 25.0);
        } else {
            index.splice(at, 1, []);
            heights.remove(at);
        }
    }
    let elapsed = started.elapsed();
    assert_matches(&index, &heights);
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "a thousand edits took {elapsed:?}"
    );
}
