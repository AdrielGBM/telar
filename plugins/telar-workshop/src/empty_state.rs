//! What a panel shows while it has nothing to: what is missing, what the panel is for, and how a preview gives it something, in Rust and in `.rsx`.

use std::rc::Rc;

use telar::{
    AlignItems, Container, JustifyContent, LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle,
    Reactive, Role, SizeDimension, Text, TextAlign, box_item, children,
};
use telar_components::{ButtonProps, CodeViewProps, button, code_view};
use telar_devtools::{WORKBENCH_GRID, workbench_fill, workbench_muted};

use crate::strings;

const ABOUT_MAX_WIDTH: f32 = WORKBENCH_GRID * 60.0;
const TITLE_WEIGHT: u16 = 600;

/// A snippet showing how a preview fills the panel, captioned with the language it is written in.
#[derive(Clone, Copy)]
pub(crate) struct Example {
    pub(crate) language: &'static str,
    pub(crate) code: &'static str,
}

struct Action {
    label: &'static str,
    on_press: Rc<dyn Fn()>,
}

pub(crate) struct EmptyState {
    title: &'static str,
    about: Option<&'static str>,
    examples: &'static [Example],
    action: Option<Action>,
}

impl EmptyState {
    /// Titled with the string `title` names.
    pub(crate) fn new(title: &'static str) -> Self {
        Self {
            title,
            about: None,
            examples: &[],
            action: None,
        }
    }

    pub(crate) fn about(mut self, about: &'static str) -> Self {
        self.about = Some(about);
        self
    }

    pub(crate) fn examples(mut self, examples: &'static [Example]) -> Self {
        self.examples = examples;
        self
    }

    pub(crate) fn action(mut self, label: &'static str, on_press: Rc<dyn Fn()>) -> Self {
        self.action = Some(Action { label, on_press });
        self
    }

    /// Centred in the panel while it fits, and scrolled from the top once it does not.
    pub(crate) fn build(self) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let Self {
            title,
            about,
            examples,
            action,
        } = self;
        let mut items = children![Text::declaring(
            move || strings::text(title),
            LayoutStyle::new(),
            |text| text
                .with_font_weight(TITLE_WEIGHT)
                .with_text_align(TextAlign::Center),
        )?];
        if let Some(about) = about {
            let text = Text::declaring(
                move || strings::text(about),
                LayoutStyle::new(),
                |text| workbench_muted(text).with_text_align(TextAlign::Center),
            )?;
            // A text always stretches across its parent, so the measure it keeps to is a box of its own, which the column centres.
            let measure = Container::new(
                LayoutStyle::column().max_width(ABOUT_MAX_WIDTH),
                children![text],
            )?;
            items.push(box_item(measure));
        }
        if !examples.is_empty() {
            items.push(snippets(examples)?);
        }
        if let Some(Action { label, on_press }) = action {
            items.push(button(
                ButtonProps::props()
                    .label(Reactive::of(move || strings::text(label)))
                    .on_press(on_press)
                    .build(),
            )?);
        }
        let content = Container::new(
            LayoutStyle::column()
                .align_items(AlignItems::CENTER)
                .gap(WORKBENCH_GRID)
                .padding_all(WORKBENCH_GRID * 2.0),
            items,
        )?;
        let scroll = LayoutScrollArea::new(
            LayoutStyle::new().flex_shrink(1.0).min_height(0.0),
            box_item(content),
        )?;
        let centred = Container::new(
            workbench_fill().justify_content(JustifyContent::CENTER),
            children![scroll],
        )?
        .role(Role::Status);
        Ok(box_item(centred))
    }
}

/// Side by side as tall as each other where they fit, and else one under the other as wide as each other: the row stretches its snippets across, and each grows along the line it is on.
fn snippets(examples: &'static [Example]) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let snippets = examples
        .iter()
        .map(|example| snippet(*example))
        .collect::<Result<Vec<_>, _>>()?;
    let row = Container::new(
        LayoutStyle::row()
            .max_width(SizeDimension::Percent(1.0))
            .flex_wrap()
            .justify_content(JustifyContent::CENTER)
            .gap(WORKBENCH_GRID * 2.0)
            .padding_top(WORKBENCH_GRID),
        snippets,
    )?;
    Ok(box_item(row))
}

fn snippet(example: Example) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let caption = Text::declaring(
        move || example.language.to_string(),
        LayoutStyle::new(),
        workbench_muted,
    )?;
    let code = code_view(
        CodeViewProps::props()
            .code(example.code.to_string())
            .line_numbers(false)
            .grow(true)
            .label(example.language.to_string())
            .build(),
    )?;
    let column = Container::new(
        LayoutStyle::column()
            .flex_grow(1.0)
            .min_width(0.0)
            .gap(WORKBENCH_GRID / 2.0),
        vec![box_item(caption), code],
    )?;
    Ok(box_item(column))
}

#[cfg(test)]
#[path = "empty_state_test.rs"]
mod tests;
