use super::*;

fn pieces(slice: ImageSlice, image: (u32, u32), dest: Rect) -> Vec<SlicePiece> {
    slice.pieces(image, dest).collect()
}

#[test]
fn nine_slice_keeps_corners_at_their_size_and_stretches_the_rest() {
    let slice = ImageSlice::new(Insets::new(2.0, 3.0, 4.0, 5.0));
    let got = pieces(slice, (12, 10), Rect::new(10.0, 20.0, 40.0, 30.0));
    assert_eq!(got.len(), 9);
    assert_eq!(
        got[0],
        SlicePiece {
            source: Rect::new(0.0, 0.0, 5.0, 2.0),
            dest: Rect::new(10.0, 20.0, 5.0, 2.0),
        },
        "top-left corner is unscaled"
    );
    assert_eq!(
        got[8],
        SlicePiece {
            source: Rect::new(9.0, 6.0, 3.0, 4.0),
            dest: Rect::new(47.0, 46.0, 3.0, 4.0),
        },
        "bottom-right corner is unscaled and flush with the far corner"
    );
    assert_eq!(
        got[1],
        SlicePiece {
            source: Rect::new(5.0, 0.0, 4.0, 2.0),
            dest: Rect::new(15.0, 20.0, 32.0, 2.0),
        },
        "top edge stretches along its length only"
    );
    assert_eq!(
        got[4],
        SlicePiece {
            source: Rect::new(5.0, 2.0, 4.0, 4.0),
            dest: Rect::new(15.0, 22.0, 32.0, 24.0),
        },
        "middle stretches both ways"
    );
}

#[test]
fn a_border_scale_sizes_the_corners_in_destination_units() {
    let slice = ImageSlice::new(Insets::all(2.0)).with_scale(3.0);
    let corner = pieces(slice, (8, 8), Rect::new(0.0, 0.0, 30.0, 30.0))[0];
    assert_eq!(corner.dest, Rect::new(0.0, 0.0, 6.0, 6.0));
    assert_eq!(corner.source, Rect::new(0.0, 0.0, 2.0, 2.0));
}

#[test]
fn a_box_smaller_than_its_borders_shrinks_them_all_by_one_factor() {
    let slice = ImageSlice::new(Insets::new(4.0, 4.0, 2.0, 2.0));
    let got = pieces(slice, (10, 10), Rect::new(0.0, 0.0, 3.0, 12.0));
    let widths: Vec<f32> = got
        .iter()
        .filter(|p| p.dest.y == 0.0)
        .map(|p| p.dest.width)
        .collect();
    assert_eq!(
        widths,
        vec![1.0, 2.0],
        "the middle column is gone, left and right keep their 1:2 ratio"
    );
    assert!(
        got.iter().all(|p| p.dest.height <= 12.0),
        "the rows shrink by the same factor, not their own"
    );
    assert_eq!(got[0].dest.height, 2.0, "4 * 0.5");
}

#[test]
fn cuts_that_overlap_the_picture_meet_in_proportion() {
    let slice = ImageSlice::new(Insets::new(0.0, 12.0, 0.0, 4.0));
    let got = pieces(slice, (8, 8), Rect::new(0.0, 0.0, 40.0, 8.0));
    let sources: Vec<(f32, f32)> = got.iter().map(|p| (p.source.x, p.source.width)).collect();
    assert_eq!(
        sources,
        vec![(0.0, 2.0), (2.0, 6.0)],
        "4:12 over 8 pixels is 2 and 6, no middle"
    );
}

#[test]
fn zero_insets_is_one_stretched_piece() {
    let got = pieces(
        ImageSlice::default(),
        (8, 4),
        Rect::new(1.0, 2.0, 30.0, 20.0),
    );
    assert_eq!(
        got,
        vec![SlicePiece {
            source: Rect::new(0.0, 0.0, 8.0, 4.0),
            dest: Rect::new(1.0, 2.0, 30.0, 20.0),
        }]
    );
}

#[test]
fn a_tile_repeats_at_the_scaled_size_and_refuses_a_useless_scale() {
    let image = ImageData::new(vec![0; 4 * 6 * 4], 6, 4);
    assert_eq!(ImageFill::tile_size(1.5, &image), Some((9.0, 6.0)));
    assert_eq!(ImageFill::tile_size(0.0, &image), None);
    assert_eq!(ImageFill::tile_size(f32::NAN, &image), None);
}

#[test]
fn scaling_a_fill_scales_what_lands_in_destination_units() {
    assert_eq!(
        ImageFill::Tile { scale: 2.0 }.scaled(1.5),
        ImageFill::Tile { scale: 3.0 }
    );
    let slice = ImageSlice::new(Insets::all(3.0));
    assert_eq!(
        ImageFill::Slice(slice).scaled(2.0),
        ImageFill::Slice(slice.with_scale(2.0)),
        "the cuts are source pixels and stay put"
    );
}
