use super::*;
use taffy::{AvailableSpace, Dimension, FlexDirection, LengthPercentage, Rect, Size, TaffyTree};

/// A column of rows at fractional heights with fractional padding, each holding two leaves at fractional widths: every box lands between pixels somewhere, which is what rounding has to get right.
struct Rows {
    root: NodeId,
    rows: Vec<NodeId>,
    leaves: Vec<NodeId>,
}

fn row_style(height: f32) -> Style {
    Style {
        flex_direction: FlexDirection::Row,
        size: Size {
            width: Dimension::auto(),
            height: Dimension::length(height),
        },
        padding: Rect {
            left: LengthPercentage::length(1.3),
            right: LengthPercentage::length(0.4),
            top: LengthPercentage::length(0.7),
            bottom: LengthPercentage::length(0.2),
        },
        ..Style::default()
    }
}

fn leaf_style(width: f32) -> Style {
    Style {
        size: Size {
            width: Dimension::length(width),
            height: Dimension::length(5.3),
        },
        ..Style::default()
    }
}

fn column_style() -> Style {
    Style {
        flex_direction: FlexDirection::Column,
        size: Size {
            width: Dimension::length(203.7),
            height: Dimension::auto(),
        },
        ..Style::default()
    }
}

/// The same rows in both trees, so each pass can be checked value for value against taffy's own rounding.
fn build(ours: &mut LayoutTree, theirs: &mut TaffyTree<()>) -> (Rows, Rows) {
    let mut built = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for index in 0..12 {
        let width = 20.0 + index as f32 * 3.37;
        let ours_leaves = [
            ours.new_leaf(leaf_style(width)),
            ours.new_leaf(leaf_style(7.9)),
        ];
        let theirs_leaves = [
            theirs.new_leaf(leaf_style(width)).unwrap(),
            theirs.new_leaf(leaf_style(7.9)).unwrap(),
        ];
        let height = 10.0 + index as f32 * 0.45;
        built
            .0
            .push(ours.new_with_children(row_style(height), &ours_leaves));
        built.1.push(
            theirs
                .new_with_children(row_style(height), &theirs_leaves)
                .unwrap(),
        );
        built.2.extend(ours_leaves);
        built.3.extend(theirs_leaves);
    }
    let ours_root = ours.new_with_children(column_style(), &built.0);
    let theirs_root = theirs.new_with_children(column_style(), &built.1).unwrap();
    (
        Rows {
            root: ours_root,
            rows: built.0,
            leaves: built.2,
        },
        Rows {
            root: theirs_root,
            rows: built.1,
            leaves: built.3,
        },
    )
}

const SPACE: Size<AvailableSpace> = Size {
    width: AvailableSpace::Definite(400.0),
    height: AvailableSpace::MaxContent,
};

fn compute(ours: &mut LayoutTree, theirs: &mut TaffyTree<()>, (o, t): &(Rows, Rows)) {
    ours.compute(o.root, SPACE, |inputs, _, style| {
        taffy::compute_leaf_layout(inputs, style, |_, _| 0.0, |_, _| Size::ZERO)
    });
    theirs.compute_layout(t.root, SPACE).unwrap();
}

fn assert_same(ours: &LayoutTree, theirs: &TaffyTree<()>, (o, t): &(Rows, Rows)) {
    let pairs = std::iter::once((o.root, t.root))
        .chain(o.rows.iter().copied().zip(t.rows.iter().copied()))
        .chain(o.leaves.iter().copied().zip(t.leaves.iter().copied()));
    for (mine, reference) in pairs {
        assert_eq!(ours.layout(mine), theirs.layout(reference).unwrap());
        assert_eq!(
            ours.unrounded_layout(mine),
            theirs.unrounded_layout(reference)
        );
    }
}

/// Rounding only what moved has to come out exactly where rounding everything does, pass after pass: the drawn boxes are the contract, and how much of the tree was looked at to get them is not.
#[test]
fn rounding_what_moved_matches_rounding_everything() {
    let mut ours = LayoutTree::new();
    let mut theirs = TaffyTree::new();
    let built = build(&mut ours, &mut theirs);
    compute(&mut ours, &mut theirs, &built);
    assert_same(&ours, &theirs, &built);

    for (step, (mine, reference)) in built.0.rows.iter().zip(&built.1.rows).enumerate() {
        let height = 9.0 + step as f32 * 0.61;
        ours.set_style(*mine, row_style(height));
        theirs.set_style(*reference, row_style(height)).unwrap();
        compute(&mut ours, &mut theirs, &built);
        assert_same(&ours, &theirs, &built);
    }

    let (mine, reference) = (built.0.leaves[5], built.1.leaves[5]);
    ours.set_style(mine, leaf_style(61.25));
    theirs.set_style(reference, leaf_style(61.25)).unwrap();
    compute(&mut ours, &mut theirs, &built);
    assert_same(&ours, &theirs, &built);
}

/// A leaf that grows within its row pushes the leaf after it along and moves no other box, so only those two, their row and the column holding it are stamped as moved.
#[test]
fn a_change_inside_one_row_moves_only_that_row() {
    let mut ours = LayoutTree::new();
    let mut theirs = TaffyTree::new();
    let built = build(&mut ours, &mut theirs);
    compute(&mut ours, &mut theirs, &built);
    let before = ours.rounding_passes();

    ours.set_style(built.0.leaves[6], leaf_style(31.0));
    compute(&mut ours, &mut theirs, &built);

    let moved: Vec<NodeId> = std::iter::once(built.0.root)
        .chain(built.0.rows.iter().copied())
        .chain(built.0.leaves.iter().copied())
        .filter(|&node| ours.moved_in(node) > before)
        .collect();
    assert_eq!(
        moved,
        vec![
            built.0.root,
            built.0.rows[3],
            built.0.leaves[6],
            built.0.leaves[7]
        ],
        "the column, the row the leaf sits in, the leaf, and the leaf it pushed along"
    );
}

/// A child added at the origin with no size of its own rounds to where it already reads, and still has to be reached: nothing has ever placed it.
#[test]
fn a_child_added_without_a_size_is_still_reached() {
    let mut ours = LayoutTree::new();
    let mut theirs = TaffyTree::new();
    let built = build(&mut ours, &mut theirs);
    compute(&mut ours, &mut theirs, &built);
    let before = ours.rounding_passes();

    let empty = ours.new_leaf(Style {
        size: Size {
            width: Dimension::length(0.0),
            height: Dimension::length(0.0),
        },
        ..Style::default()
    });
    ours.add_child(built.0.rows[0], empty);
    compute(&mut ours, &mut theirs, &built);

    assert!(ours.moved_in(empty) > before);
    assert!(ours.moved_in(built.0.rows[0]) > before);
}
