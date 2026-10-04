//! How a [`Text`](crate::Text) with a [`FontFit`] finds its size: in its measure, where the width of the box holding it is known, and handed to its view once the pass publishes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use geometry_core::Size;
use layout_core::NodeId;
use reactive_core::{RwSignal, signal};
use renderer_core::{FontFit, Span, TextStyle, TextWrap};

type Amend = Rc<dyn Fn(TextStyle) -> TextStyle>;

struct Asked {
    content: String,
    spans: Option<Vec<Span>>,
    target: Option<(f32, Option<f32>)>,
    generation: u64,
    declared: TextStyle,
}

impl Asked {
    fn same(&self, other: &Self) -> bool {
        self.content == other.content
            && self.spans == other.spans
            && self.target == other.target
            && self.generation == other.generation
            && self.declared.same_extent(&other.declared)
    }
}

pub(crate) struct Fitting {
    amend: Amend,
    fit: Rc<dyn Fn() -> FontFit>,
    inherited: Rc<dyn Fn() -> TextStyle>,
    node: Rc<Cell<Option<NodeId>>>,
    size: RwSignal<Option<f32>>,
    queued: Cell<Option<f32>>,
    answered: RefCell<Option<(Asked, Option<f32>)>>,
    containing: Cell<Option<f32>>,
    guessed: Cell<Option<Option<f32>>>,
}

impl Fitting {
    pub(crate) fn new(
        amend: Amend,
        fit: Rc<dyn Fn() -> FontFit>,
        inherited: Rc<dyn Fn() -> TextStyle>,
        node: Rc<Cell<Option<NodeId>>>,
    ) -> Self {
        Self {
            amend,
            fit,
            inherited,
            node,
            size: signal(None),
            queued: Cell::new(None),
            answered: RefCell::new(None),
            containing: Cell::new(None),
            guessed: Cell::new(None),
        }
    }

    /// The text's style at `size`, which the fit owns whatever the amendment says.
    fn at(&self, inherited: &TextStyle, size: f32) -> TextStyle {
        (self.amend)(inherited.clone().with_font_size(size))
            .with_font_size(size)
            .with_text_wrap(TextWrap::NoWrap)
    }

    fn unfitted(&self, inherited: TextStyle) -> TextStyle {
        (self.amend)(inherited).with_text_wrap(TextWrap::NoWrap)
    }

    /// What the text is measured in for a box holding it `containing_width` wide, resolving the size first. Runs inside the layout pass.
    pub(crate) fn measured_style(
        &self,
        content: &str,
        spans: Option<&[Span]>,
        containing_width: Option<f32>,
    ) -> TextStyle {
        let inherited = (self.inherited)();
        let containing_width = self.containing_width(containing_width);
        let size = self.resolve(content, spans, &inherited, containing_width);
        self.publish(size);
        match size {
            Some(size) => self.at(&inherited, size),
            None => self.unfitted(inherited),
        }
    }

    /// The width the fraction is of. A flex row sizes its items without telling them its width, as CSS keeps a percentage out of a content contribution, so those probes take the width the text last heard; when the pass then tells it a different one, it asks for another pass to size it again at what it now knows.
    fn containing_width(&self, heard: Option<f32>) -> Option<f32> {
        let Some(width) = heard else {
            let guess = self.containing.get();
            self.guessed.set(Some(guess));
            return guess;
        };
        if self
            .guessed
            .take()
            .is_some_and(|guess| guess != Some(width))
            && let Some(node) = self.node.get()
        {
            crate::context::measure_again(node);
        }
        self.containing.set(Some(width));
        Some(width)
    }

    /// What the text is drawn in: the size the last layout pass found. Reactive.
    pub(crate) fn drawn_style(&self) -> TextStyle {
        let inherited = (self.inherited)();
        match self.size.get() {
            Some(size) => self.at(&inherited, size),
            None => self.unfitted(inherited),
        }
    }

    /// What the fit reads that its text style does not, for the text's remeasure effect to follow.
    pub(crate) fn watched(&self) -> (FontFit, Size) {
        let fit = (self.fit)();
        let surface = if fit.uses_surface() {
            crate::context::use_surface_size()
        } else {
            Size::ZERO
        };
        (fit, surface)
    }

    fn resolve(
        &self,
        content: &str,
        spans: Option<&[Span]>,
        inherited: &TextStyle,
        containing_width: Option<f32>,
    ) -> Option<f32> {
        let fit = (self.fit)();
        let surface = if fit.uses_surface() {
            crate::context::surface_size()
        } else {
            Size::ZERO
        };
        let asked = Asked {
            content: content.to_owned(),
            spans: spans.map(<[Span]>::to_vec),
            target: fit.target(inherited.font_size, surface, containing_width),
            generation: renderer_core::text_metrics_generation(),
            declared: (self.amend)(inherited.clone()),
        };
        if let Some((before, size)) = self.answered.borrow().as_ref()
            && before.same(&asked)
        {
            return *size;
        }
        let size = asked.target.and_then(|(width, max_height)| {
            crate::text_metrics::fitted_font_size(content, spans, width, max_height, &|size| {
                self.at(inherited, size)
            })
        });
        *self.answered.borrow_mut() = Some((asked, size));
        size
    }

    fn publish(&self, size: Option<f32>) {
        if self.queued.get() == size {
            return;
        }
        self.queued.set(size);
        let published = self.size;
        crate::context::after_layout(move || {
            if published.is_alive() && published.peek() != size {
                published.set(size);
            }
        });
    }
}

#[cfg(test)]
#[path = "text_fit_test.rs"]
mod tests;
