use std::ops::ControlFlow;
use std::sync::Arc;

use geometry_core::Rect;

use super::*;
use crate::{Element, RectStyle, Semantics};

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

fn rect(y: f32) -> DrawCommand {
    DrawCommand::Rect {
        rect: Rect::new(0.0, y, 10.0, 10.0),
        style: Arc::new(RectStyle::default()),
    }
}

fn boxed(id: u64, y: f32) -> [DrawCommand; 3] {
    [open(id), rect(y), close()]
}

fn collect(aligner: &mut Aligner, new: &[DrawCommand], old: &[DrawCommand]) -> Vec<Step> {
    let mut out = Vec::new();
    let _ = aligner.align(new, old, |step| {
        out.push(step);
        ControlFlow::<()>::Continue(())
    });
    out
}

fn steps(new: &[DrawCommand], old: &[DrawCommand]) -> Vec<Step> {
    let out = collect(&mut Aligner::default(), new, old);
    assert_every_command_once(&out, new.len(), old.len());
    out
}

fn assert_every_command_once(steps: &[Step], new_len: usize, old_len: usize) {
    let (mut next_new, mut next_old) = (0, 0);
    for step in steps {
        let (n, o) = match *step {
            Step::Both(n, o) => (Some(n), Some(o)),
            Step::New(n) => (Some(n), None),
            Step::Old(o) => (None, Some(o)),
        };
        if let Some(n) = n {
            assert_eq!(n, next_new, "new commands are visited in order: {steps:?}");
            next_new += 1;
        }
        if let Some(o) = o {
            assert_eq!(o, next_old, "old commands are visited in order: {steps:?}");
            next_old += 1;
        }
    }
    assert_eq!(
        (next_new, next_old),
        (new_len, old_len),
        "and every one is visited: {steps:?}"
    );
}

fn unpaired_new(steps: &[Step]) -> Vec<usize> {
    steps
        .iter()
        .filter_map(|s| match s {
            Step::New(n) => Some(*n),
            _ => None,
        })
        .collect()
}

fn unpaired_old(steps: &[Step]) -> Vec<usize> {
    steps
        .iter()
        .filter_map(|s| match s {
            Step::Old(o) => Some(*o),
            _ => None,
        })
        .collect()
}

#[test]
fn boxes_where_they_were_pair_by_position() {
    let frame: Vec<_> = [open(1), rect(0.0)]
        .into_iter()
        .chain(boxed(2, 10.0))
        .chain([close()])
        .collect();
    let paired = steps(&frame, &frame);
    assert!(
        paired
            .iter()
            .enumerate()
            .all(|(i, s)| *s == Step::Both(i, i)),
        "{paired:?}"
    );
}

#[test]
fn an_inserted_box_is_the_only_thing_unpaired() {
    let old: Vec<_> = boxed(1, 0.0).into_iter().chain(boxed(3, 20.0)).collect();
    let new: Vec<_> = boxed(1, 0.0)
        .into_iter()
        .chain(boxed(2, 20.0))
        .chain(boxed(3, 40.0))
        .collect();
    let s = steps(&new, &old);
    assert_eq!(unpaired_new(&s), vec![3, 4, 5], "{s:?}");
    assert!(unpaired_old(&s).is_empty(), "{s:?}");
    assert!(
        s.contains(&Step::Both(7, 4)),
        "the box after it stays paired with itself: {s:?}"
    );
}

#[test]
fn a_box_sent_to_the_back_is_the_only_one_unpaired() {
    let old: Vec<_> = boxed(1, 0.0)
        .into_iter()
        .chain(boxed(2, 0.0))
        .chain(boxed(3, 0.0))
        .collect();
    let new: Vec<_> = boxed(3, 0.0)
        .into_iter()
        .chain(boxed(1, 0.0))
        .chain(boxed(2, 0.0))
        .collect();
    let s = steps(&new, &old);
    assert_eq!(unpaired_new(&s), vec![0, 1, 2], "{s:?}");
    assert_eq!(unpaired_old(&s), vec![6, 7, 8], "{s:?}");
}

#[test]
fn a_box_moved_under_another_parent_is_not_paired() {
    let old: Vec<_> = [open(1)]
        .into_iter()
        .chain(boxed(3, 0.0))
        .chain([close(), open(2), close()])
        .collect();
    let new: Vec<_> = [open(1), close(), open(2)]
        .into_iter()
        .chain(boxed(3, 0.0))
        .chain([close()])
        .collect();
    let s = steps(&new, &old);
    assert_eq!(unpaired_new(&s), vec![3, 4, 5], "{s:?}");
    assert_eq!(unpaired_old(&s), vec![1, 2, 3], "{s:?}");
}

#[test]
fn loose_commands_pair_by_their_common_prefix_and_suffix() {
    let old = vec![rect(0.0), rect(10.0), rect(20.0)];
    let new = vec![rect(0.0), rect(10.0), rect(15.0), rect(20.0)];
    let s = steps(&new, &old);
    assert_eq!(unpaired_new(&s), vec![2], "{s:?}");
    assert!(s.contains(&Step::Both(3, 2)), "{s:?}");
}

#[test]
fn boxes_sharing_an_id_still_visit_every_command_once() {
    let old: Vec<_> = boxed(1, 0.0).into_iter().chain(boxed(1, 10.0)).collect();
    let new: Vec<_> = boxed(1, 0.0)
        .into_iter()
        .chain(boxed(2, 10.0))
        .chain(boxed(1, 20.0))
        .collect();
    steps(&new, &old);
    steps(&old, &new);
}

#[test]
fn unbalanced_markers_still_visit_every_command_once() {
    let old = vec![close(), open(1), rect(0.0), open(2), rect(1.0)];
    let new = vec![open(1), rect(0.0), close(), close(), open(2)];
    steps(&new, &old);
    steps(&old, &new);
}

#[test]
fn an_aligner_reused_across_frames_pairs_like_a_fresh_one() {
    let a: Vec<_> = boxed(1, 0.0)
        .into_iter()
        .chain(boxed(2, 10.0))
        .chain(boxed(3, 20.0))
        .collect();
    let b: Vec<_> = boxed(3, 0.0).into_iter().chain(boxed(1, 0.0)).collect();
    let c = vec![rect(0.0), rect(5.0)];
    let mut reused = Aligner::default();
    for (new, old) in [(&a, &b), (&b, &a), (&c, &a), (&a, &c), (&b, &b), (&a, &b)] {
        assert_eq!(
            collect(&mut reused, new, old),
            collect(&mut Aligner::default(), new, old)
        );
    }
}
