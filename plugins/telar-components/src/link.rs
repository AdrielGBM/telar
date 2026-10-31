//! [`link`]: words that go somewhere — a page of the application, a place on this one, or an address outside it.

use std::rc::Rc;

use telar::{
    Accessible, AlignItems, BorderRadius, Color, Destination, IntoDestination, LayoutError,
    LayoutItem, LayoutStyle, Props, Reactive, RectStyle, ShapeStyle, StyledContainer,
    SurfaceAmendment, Text, TextWrap, amend_surface, box_item, children,
};

use crate::shared;
use crate::strings;

/// A link's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role named beside it, read when the link paints.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LinkTokens {
    /// Corner of the wash a pointer resting on the link lays behind it. `None` is the theme's `radius_item`.
    pub radius: Option<f32>,
}

impl LinkTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_item)
    }
}

/// Where a [`link`] goes, re-read whenever what it reads changes: anything a box's `to:` takes — a typed route, a [`Location`](telar::Location), an [`anchor`](telar::anchor) or an [`external`](telar::external) address — or an `Option` of one, `None` being nowhere to go right now.
#[derive(Clone)]
pub struct LinkTarget(Rc<dyn Fn() -> Option<Destination>>);

impl LinkTarget {
    /// A destination worked out from what `read` reads, so the link follows a signal.
    pub fn of<D: IntoDestination>(read: impl Fn() -> D + 'static) -> Self {
        Self(Rc::new(move || read().into_destination()))
    }

    fn read(&self) -> Option<Destination> {
        (self.0)()
    }
}

impl<D: IntoDestination + Clone + 'static> From<D> for LinkTarget {
    fn from(destination: D) -> Self {
        Self::of(move || destination.clone())
    }
}

/// Words that go somewhere when pressed, or when Enter is pressed on them: a route is pushed onto the application's history, an anchor is scrolled to, and an address outside the application is handed to the system.
///
/// Drawn in the theme's accent, made readable on the surface, and always underlined, so it is told from the words around it by more than its colour. One that leaves the application says so twice: a drawn arrow after it, and a reader hears it named an external link. Ctrl, Cmd or Shift on the press ask for a view beside this one where there is one, as on a web page.
///
/// It is one box sized to its words: for a link inside a wrapping paragraph, write the paragraph as [`Text::runs`](telar::Text::runs) with a [`TextRun::to`](telar::TextRun::to) run instead.
#[derive(Props)]
pub struct LinkProps {
    /// What the link says.
    #[props(into)]
    pub label: Reactive<String>,
    /// Where it goes.
    #[props(into)]
    pub to: LinkTarget,
    /// Marks the link as the current one of its set — the page being shown, in a navigation's links — so it is drawn in the theme's strong weight and announced as current. Off by default.
    #[props(into, default)]
    pub current: Reactive<bool>,
    /// The colour the link is drawn in. `Color::TRANSPARENT` (the default) is the theme's primary; either way it is taken just far enough toward the ink to read on the theme's surface.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Runs when the link is pressed, before it is followed.
    #[props(some, default)]
    pub on_press: Option<Rc<dyn Fn()>>,
    /// Amends the paint of the link's box — its **principal surface** — at rest and hovered. See [`telar::SurfaceStyle`] for when a theme token or [`LinkTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// Words that go somewhere: a route, an anchor or an address outside the application.
pub fn link(props: LinkProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let LinkProps {
        label,
        to,
        current,
        color,
        on_press,
        style,
    } = props;

    let ink = {
        let color = color.clone();
        Rc::new(move || {
            shared::readable_on(shared::resolve(&color, shared::accent), shared::surface())
        })
    };
    let words = {
        let (label, current, ink) = (label.clone(), current.clone(), ink.clone());
        Text::declaring(
            move || label.get(),
            LayoutStyle::new(),
            move |t| {
                let text = shared::control_text(t, 1.0)
                    .with_text_wrap(TextWrap::NoWrap)
                    .with_color(ink())
                    .with_underline(true);
                if current.get() {
                    text.with_font_weight(shared::strong_weight())
                } else {
                    text
                }
            },
        )?
    };
    let leaves = {
        let to = to.clone();
        move || matches!(to.read(), Some(Destination::External(_)))
    };
    let arrow = {
        let ink = ink.clone();
        shared::shown_while(leaves.clone(), LayoutStyle::row(), move || {
            let ink = ink.clone();
            let mark = shared::mark(|| shared::Mark::External, move |_| ink().into())?;
            let spaced = telar::Container::new(
                LayoutStyle::row()
                    .align_items(AlignItems::CENTER)
                    .padding_start(shared::gap_xs()),
                vec![mark],
            )?;
            Ok(box_item(spaced))
        })?
    };

    let paint = Rc::new(move |hovered: bool| {
        let rect =
            RectStyle::default().with_radius(BorderRadius::all(LinkTokens::current().radius()));
        let rect = if hovered {
            rect.with_fill(shared::hover_wash())
        } else {
            rect
        };
        amend_surface(rect, &style)
    });
    let (rest, hover) = (paint.clone(), paint);
    let name = move || {
        let label = label.get();
        if leaves() {
            strings::text_with(strings::EXTERNAL_LINK, &[("label", &label)])
        } else {
            label
        }
    };
    let link = StyledContainer::new(
        LayoutStyle::row().align_items(AlignItems::CENTER),
        move |_r| rest(false),
        children![words, arrow],
    )?
    .hover_style(move |_r| hover(true));
    let link = match on_press {
        Some(on_press) => link.on_press(move || on_press()),
        None => link,
    };
    let link = link
        .to(move || to.read())
        .current(move || current.get())
        .a11y_label(name);
    Ok(box_item(link))
}

#[cfg(test)]
#[path = "link_test.rs"]
mod tests;
