//! Pairs the commands of two frames by the boxes that drew them, so a box that appears or goes shifts nothing drawn after it.

use std::ops::{ControlFlow, Range};

use rustc_hash::FxHashMap;

use crate::{DrawCommand, ElementId};

/// One move through both frames, in order: a command of each taken as the same command, or one only the new or only the old frame has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Both(usize, usize),
    New(usize),
    Old(usize),
}

struct Span {
    id: ElementId,
    start: usize,
    // One past the `PopElement`, or the end of the list for a box never closed.
    end: usize,
    closed: bool,
    parent: Option<usize>,
    // The first span after this one's subtree, which is its next sibling when it has one.
    next: usize,
}

fn parse(cmds: &[DrawCommand], spans: &mut Vec<Span>, open: &mut Vec<usize>) {
    spans.clear();
    open.clear();
    for (index, cmd) in cmds.iter().enumerate() {
        match cmd {
            DrawCommand::PushElement { element } => {
                spans.push(Span {
                    id: element.id,
                    start: index,
                    end: cmds.len(),
                    closed: false,
                    parent: open.last().copied(),
                    next: 0,
                });
                open.push(spans.len() - 1);
            }
            DrawCommand::PopElement => {
                if let Some(span) = open.pop() {
                    let next = spans.len();
                    let span = &mut spans[span];
                    span.end = index + 1;
                    span.closed = true;
                    span.next = next;
                }
            }
            _ => {}
        }
    }
    let next = spans.len();
    for span in open.drain(..) {
        spans[span].next = next;
    }
}

fn first_child(parent: Option<usize>) -> usize {
    parent.map_or(0, |p| p + 1)
}

struct Tree<'a> {
    cmds: &'a [DrawCommand],
    spans: &'a [Span],
}

impl Tree<'_> {
    fn children_end(&self, parent: Option<usize>) -> usize {
        parent.map_or(self.spans.len(), |p| self.spans[p].next)
    }

    fn content(&self, span: Option<usize>) -> Range<usize> {
        match span {
            None => 0..self.cmds.len(),
            Some(s) => {
                let span = &self.spans[s];
                span.start + 1..span.end - usize::from(span.closed)
            }
        }
    }

    fn close_index(&self, span: Option<usize>) -> Option<usize> {
        let span = &self.spans[span?];
        span.closed.then(|| span.end - 1)
    }

    fn loose_commands(&self, range: Range<usize>, out: &mut Vec<usize>) {
        out.clear();
        let mut index = range.start;
        while index < range.end {
            if matches!(self.cmds[index], DrawCommand::PushElement { .. }) {
                let span = self
                    .spans
                    .binary_search_by_key(&index, |span| span.start)
                    .expect("every PushElement opens a span");
                index = self.spans[span].end;
            } else {
                out.push(index);
                index += 1;
            }
        }
    }
}

struct Level {
    new: Option<usize>,
    old: Option<usize>,
    child: usize,
    new_at: usize,
    old_at: usize,
}

/// Reuses its buffers across frames instead of reallocating them each call.
#[derive(Default)]
pub(crate) struct Aligner {
    new_spans: Vec<Span>,
    old_spans: Vec<Span>,
    open: Vec<usize>,
    matching: Matching,
    levels: Vec<Level>,
    gap: Gap,
}

impl Aligner {
    /// Hands `emit` every command of both frames once, in order, pairing a box with the old box of the same id in the same order among same-parent siblings, and otherwise pairing by position when lengths match or by common prefix/suffix when they don't.
    pub(crate) fn align<B>(
        &mut self,
        new: &[DrawCommand],
        old: &[DrawCommand],
        mut emit: impl FnMut(Step) -> ControlFlow<B>,
    ) -> ControlFlow<B> {
        if markers_coincide(new, old) {
            return (0..new.len()).try_for_each(|index| emit(Step::Both(index, index)));
        }

        let Self {
            new_spans,
            old_spans,
            open,
            matching,
            levels,
            gap,
        } = self;
        parse(new, new_spans, open);
        parse(old, old_spans, open);
        let new = Tree {
            cmds: new,
            spans: &new_spans[..],
        };
        let old = Tree {
            cmds: old,
            spans: &old_spans[..],
        };
        matching.pair(&new, &old);
        let partner = &matching.partner;

        levels.clear();
        levels.push(Level {
            new: None,
            old: None,
            child: 0,
            new_at: 0,
            old_at: 0,
        });
        while let Some(level) = levels.last_mut() {
            let children_end = new.children_end(level.new);
            let mut child = level.child;
            while child < children_end && partner[child].is_none() {
                child = new.spans[child].next;
            }

            if let Some(old_child) = (child < children_end).then(|| partner[child]).flatten() {
                let (new_span, old_span) = (&new.spans[child], &old.spans[old_child]);
                gap.walk(
                    &new,
                    &old,
                    level.new_at..new_span.start,
                    level.old_at..old_span.start,
                    &mut emit,
                )?;
                emit(Step::Both(new_span.start, old_span.start))?;
                level.child = new_span.next;
                level.new_at = new_span.end;
                level.old_at = old_span.end;
                levels.push(Level {
                    new: Some(child),
                    old: Some(old_child),
                    child: first_child(Some(child)),
                    new_at: new_span.start + 1,
                    old_at: old_span.start + 1,
                });
            } else {
                gap.walk(
                    &new,
                    &old,
                    level.new_at..new.content(level.new).end,
                    level.old_at..old.content(level.old).end,
                    &mut emit,
                )?;
                match (new.close_index(level.new), old.close_index(level.old)) {
                    (Some(n), Some(o)) => emit(Step::Both(n, o))?,
                    (Some(n), None) => emit(Step::New(n))?,
                    (None, Some(o)) => emit(Step::Old(o))?,
                    (None, None) => {}
                }
                levels.pop();
            }
        }
        ControlFlow::Continue(())
    }
}

// Every box sits where it sat, so position already pairs every command with its own.
fn markers_coincide(new: &[DrawCommand], old: &[DrawCommand]) -> bool {
    new.len() == old.len()
        && new.iter().zip(old).all(|pair| match pair {
            (DrawCommand::PushElement { element: a }, DrawCommand::PushElement { element: b }) => {
                a.id == b.id
            }
            (DrawCommand::PopElement, DrawCommand::PopElement) => true,
            (DrawCommand::PushElement { .. } | DrawCommand::PopElement, _)
            | (_, DrawCommand::PushElement { .. } | DrawCommand::PopElement) => false,
            _ => true,
        })
}

#[derive(Default)]
struct Matching {
    by_id: FxHashMap<ElementId, usize>,
    partner: Vec<Option<usize>>,
    candidates: Vec<(usize, usize)>,
    tails: Vec<usize>,
    previous: Vec<Option<usize>>,
    kept: Vec<(usize, usize)>,
}

impl Matching {
    // For each new box, the old box it continues, visiting parents before their children so a box can only continue one under its own parent's partner.
    fn pair(&mut self, new: &Tree, old: &Tree) {
        let Self {
            by_id,
            partner,
            candidates,
            tails,
            previous,
            kept,
        } = self;
        by_id.clear();
        for (index, span) in old.spans.iter().enumerate() {
            by_id.entry(span.id).or_insert(index);
        }

        partner.clear();
        partner.resize(new.spans.len(), None);
        for parent in std::iter::once(None).chain((0..new.spans.len()).map(Some)) {
            let old_parent = match parent {
                None => None,
                Some(p) => match partner[p] {
                    Some(o) => Some(o),
                    None => continue,
                },
            };
            candidates.clear();
            let mut child = first_child(parent);
            while child < new.children_end(parent) {
                if let Some(&o) = by_id.get(&new.spans[child].id)
                    && old.spans[o].parent == old_parent
                {
                    candidates.push((child, o));
                }
                child = new.spans[child].next;
            }
            keep_longest_increasing(candidates, tails, previous, kept);
            for &(child, o) in candidates.iter() {
                partner[child] = Some(o);
            }
        }
    }
}

// A reordered sibling that kept its id still changed its place in the painting order, so only an order-preserving subset may pair; the longest keeps the most boxes paired.
fn keep_longest_increasing(
    pairs: &mut Vec<(usize, usize)>,
    tails: &mut Vec<usize>,
    previous: &mut Vec<Option<usize>>,
    kept: &mut Vec<(usize, usize)>,
) {
    if pairs.windows(2).all(|w| w[0].1 < w[1].1) {
        return;
    }
    tails.clear();
    previous.clear();
    for (index, &(_, old)) in pairs.iter().enumerate() {
        let at = tails.partition_point(|&tail| pairs[tail].1 < old);
        previous.push(at.checked_sub(1).map(|a| tails[a]));
        if at == tails.len() {
            tails.push(index);
        } else {
            tails[at] = index;
        }
    }
    kept.clear();
    let mut cursor = tails.last().copied();
    while let Some(index) = cursor {
        kept.push(pairs[index]);
        cursor = previous[index];
    }
    kept.reverse();
    std::mem::swap(pairs, kept);
}

#[derive(Default)]
struct Gap {
    new: Vec<usize>,
    old: Vec<usize>,
    pairs: Vec<(usize, usize)>,
}

impl Gap {
    fn walk<B>(
        &mut self,
        new: &Tree,
        old: &Tree,
        new_range: Range<usize>,
        old_range: Range<usize>,
        emit: &mut impl FnMut(Step) -> ControlFlow<B>,
    ) -> ControlFlow<B> {
        new.loose_commands(new_range.clone(), &mut self.new);
        old.loose_commands(old_range.clone(), &mut self.old);
        self.pair_loose(new.cmds, old.cmds);

        let (mut i, mut j) = (new_range.start, old_range.start);
        for &(n, o) in &self.pairs {
            (i..n).try_for_each(|index| emit(Step::New(index)))?;
            (j..o).try_for_each(|index| emit(Step::Old(index)))?;
            emit(Step::Both(n, o))?;
            (i, j) = (n + 1, o + 1);
        }
        (i..new_range.end).try_for_each(|index| emit(Step::New(index)))?;
        (j..old_range.end).try_for_each(|index| emit(Step::Old(index)))
    }

    fn pair_loose(&mut self, new: &[DrawCommand], old: &[DrawCommand]) {
        let (n, o) = (&self.new, &self.old);
        self.pairs.clear();
        if n.len() == o.len() {
            self.pairs.extend(n.iter().copied().zip(o.iter().copied()));
            return;
        }
        let prefix = n
            .iter()
            .zip(o)
            .take_while(|&(&a, &b)| new[a] == old[b])
            .count();
        let suffix = n[prefix..]
            .iter()
            .rev()
            .zip(o[prefix..].iter().rev())
            .take_while(|&(&a, &b)| new[a] == old[b])
            .count();
        self.pairs
            .extend(n[..prefix].iter().copied().zip(o[..prefix].iter().copied()));
        self.pairs.extend(
            n[n.len() - suffix..]
                .iter()
                .copied()
                .zip(o[o.len() - suffix..].iter().copied()),
        );
    }
}

#[cfg(test)]
#[path = "align_test.rs"]
mod tests;
