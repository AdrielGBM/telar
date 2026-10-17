//! What an input method is composing in a field: text it has not decided on yet, drawn at the caret apart from the field's own and underlined until it is committed.

use std::ops::Range;

use geometry_core::Rect;
use platform_core::Ime;
use reactive_core::{RwSignal, signal};
use renderer_core::{Paint, RectStyle, ShapeStyle};
use ui_tree::RenderNode;

/// How thick the line under a whole composition is drawn, in logical px.
const COMPOSING_THICKNESS: f32 = 1.0;
/// How thick the line under the part being converted is drawn: heavier, so the clause the candidates are for stands out.
const CONVERTING_THICKNESS: f32 = 2.0;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Preedit {
    pub text: String,
    pub cursor: Option<Range<usize>>,
}

/// What one [`Ime`] event did to a field's composition.
#[derive(Debug, PartialEq)]
pub(crate) enum Step {
    /// A composition started: a selection gives way to it, as typing would replace it.
    Began,
    Changed,
    /// The composition was emptied or abandoned without inserting anything.
    Ended,
    /// The input method decided on this text, to insert at the caret.
    Committed(String),
    Ignored,
}

/// A field's composition in progress, or `None`. Reactive, so the field redraws as the input method changes it.
#[derive(Clone, Copy)]
pub(crate) struct Composition(RwSignal<Option<Preedit>>);

impl Composition {
    pub(crate) fn new() -> Self {
        Self(signal(None))
    }

    pub(crate) fn get(&self) -> Option<Preedit> {
        self.0.get()
    }

    pub(crate) fn is_active(&self) -> bool {
        self.0.peek_with(Option::is_some)
    }

    pub(crate) fn clear(&self) {
        if self.is_active() {
            self.0.set(None);
        }
    }

    /// Types what is being composed into `value` at `caret`, as a commit would: what a field does when the keyboard leaves it mid-composition, since the input method is turned off with it and the word on screen would otherwise be lost.
    pub(crate) fn settle(&self, value: RwSignal<String>, caret: RwSignal<usize>) {
        let Some(preedit) = self.0.peek() else {
            return;
        };
        self.0.set(None);
        let kept: String = preedit.text.chars().filter(|c| !c.is_control()).collect();
        let mut text = value.peek();
        let mut at = caret.peek().min(text.len());
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        text.insert_str(at, &kept);
        value.set(text);
        caret.set(at + kept.len());
    }

    pub(crate) fn apply(&self, ime: &Ime) -> Step {
        match ime {
            Ime::Preedit { text, cursor } if !text.is_empty() => {
                let began = !self.is_active();
                self.0.set(Some(Preedit {
                    text: text.clone(),
                    cursor: cursor.clone(),
                }));
                if began { Step::Began } else { Step::Changed }
            }
            Ime::Preedit { .. } | Ime::Disabled => {
                let was = self.is_active();
                self.clear();
                if was { Step::Ended } else { Step::Ignored }
            }
            Ime::Commit(text) => {
                self.clear();
                Step::Committed(text.clone())
            }
            Ime::Enabled => Step::Ignored,
        }
    }
}

/// A field's text with a composition drawn into it at the caret.
pub(crate) struct Composed {
    pub text: String,
    /// Where the composition sits in [`text`](Self::text).
    pub span: Range<usize>,
    /// The caret the input method asked for, when it asked for one that is a point.
    pub caret: Option<usize>,
    /// The part being converted, when the input method's cursor is a range.
    pub converting: Option<Range<usize>>,
    /// Where the input method's candidates belong: its cursor, or the end of the composition when it gave none.
    pub anchor: usize,
}

impl Composed {
    /// Where byte `at` of the field's own text lands in [`text`](Self::text): text after the caret moves past the composition.
    pub(crate) fn shift(&self, at: usize) -> usize {
        if at > self.span.start {
            at + self.span.len()
        } else {
            at
        }
    }
}

/// `text` as it is drawn while `preedit` is composed at byte `at`.
pub(crate) fn compose(text: &str, at: usize, preedit: &Preedit) -> Composed {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    let mut composed = String::with_capacity(text.len() + preedit.text.len());
    composed.push_str(&text[..at]);
    composed.push_str(&preedit.text);
    composed.push_str(&text[at..]);
    let span = at..at + preedit.text.len();
    let within = |offset: usize| at + offset.min(preedit.text.len());
    let cursor = preedit
        .cursor
        .as_ref()
        .map(|cursor| within(cursor.start)..within(cursor.end));
    Composed {
        text: composed,
        caret: cursor
            .as_ref()
            .filter(|cursor| cursor.is_empty())
            .map(|cursor| cursor.start),
        converting: cursor.clone().filter(|cursor| !cursor.is_empty()),
        anchor: cursor.map_or(span.end, |cursor| cursor.start),
        span,
    }
}

/// The lines under a composition on one row: thin under all of it from `x0` to `x1`, heavy under the part being converted from `converting.0` to `converting.1`, both resting on `bottom`.
pub(crate) fn underlines(
    x0: f32,
    x1: f32,
    converting: Option<(f32, f32)>,
    bottom: f32,
    color: Paint,
) -> impl Iterator<Item = RenderNode> {
    let line = move |from: f32, to: f32, thickness: f32| {
        RenderNode::rect(
            Rect {
                x: from,
                y: bottom - thickness,
                width: (to - from).max(1.0),
                height: thickness,
            },
            RectStyle::default().with_fill(color),
        )
    };
    std::iter::once(line(x0, x1, COMPOSING_THICKNESS))
        .chain(converting.map(|(from, to)| line(from, to, CONVERTING_THICKNESS)))
}

#[cfg(test)]
#[path = "composition_test.rs"]
mod tests;
