//! [`search_field`]: a text field for a query — a magnifying glass before it, a button that clears it once there is something to clear, and Escape doing the same.

use std::cell::Cell;
use std::rc::Rc;

use telar::{
    Accessible, AlignItems, BorderRadius, JustifyContent, Key, LayoutError, LayoutItem,
    LayoutStyle, ModifiersState, NamedKey, Props, Reactive, RectStyle, RwSignal, ShapeStyle,
    StyledContainer, SurfaceAmendment, box_item, children,
    focus::{self, FocusId, Role},
    signal,
};

use crate::shared;
use crate::strings;
use crate::text_field::{Adornments, FieldKeyHandler, TextFieldProps, adorned_field};

/// What [`SearchFieldProps::on_submit`] is called with: the query as it stands.
pub type SearchHandler = Rc<dyn Fn(&str)>;

/// A field a query is typed into, in a text field's frame and sized by [`TextFieldTokens`](crate::TextFieldTokens).
///
/// Enter submits the query; Escape clears it, and a second Escape, with nothing left to clear, leaves the field. The clear button shows only while there is a query, and is no Tab stop of its own: Escape is the keyboard's way to the same thing.
///
/// A field that drives a list of results under it, the way the command palette's does, hands the list the arrows through [`on_key`](Self::on_key) and points a reader at the result its cursor is on through [`active_descendant`](Self::active_descendant) and [`controls`](Self::controls), so the keyboard never has to leave the field.
#[derive(Props)]
pub struct SearchFieldProps {
    /// `None` (the default) makes the field uncontrolled: it owns an internal `signal(String::new())`. `Some` binds the query to a caller-owned signal.
    #[props(some, into, default)]
    pub value: Option<RwSignal<String>>,
    /// The field's hint. `None` (the default) uses the localized `telar_components.search` message.
    #[props(some, into, default)]
    pub placeholder: Option<Reactive<String>>,
    /// A small caption stacked above the box, and what a reader calls the field. Empty (the default) leaves the field called by its placeholder.
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Box width in logical px. `0.0` (the default) is [`TextFieldTokens::width`](crate::TextFieldTokens::width).
    #[props(default)]
    pub width: f32,
    /// Runs with the query when Enter is pressed in the field.
    #[props(some, default)]
    pub on_submit: Option<SearchHandler>,
    /// Runs when the query is cleared, by the button or by Escape.
    #[props(some, default)]
    pub on_clear: Option<Rc<dyn Fn()>>,
    /// Hears every key pressed while the field holds the keyboard, before the field or its Escape do; answering `true` takes the key. Where a list of results under the field gets its arrows.
    #[props(some, default)]
    pub on_key: Option<FieldKeyHandler>,
    /// The result the keyboard's cursor rests on, so a reader announces it while the keyboard stays in the field. `None` (the default) points at nothing.
    #[props(some, default)]
    pub active_descendant: Option<Rc<dyn Fn() -> Option<FocusId>>>,
    /// The list of results the field drives, so a reader is told where they are. `None` (the default) names none.
    #[props(some, default)]
    pub controls: Option<Rc<dyn Fn() -> Option<FocusId>>>,
    /// Amends the paint of the framed box — the field's **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`TextFieldTokens`](crate::TextFieldTokens) is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
    /// Receives the id the line typed into holds in the tab order, once the field exists, and is emptied when the field goes. For the caller that has to take the keyboard to the field later, as `focus_id:$sig` does on an `input`.
    #[props(some, into, default)]
    pub focus_id: Option<RwSignal<Option<FocusId>>>,
}

/// A field for a query, with a magnifying glass, a clear button and Escape to clear.
pub fn search_field(props: SearchFieldProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let SearchFieldProps {
        value,
        placeholder,
        label,
        width,
        on_submit,
        on_clear,
        on_key,
        active_descendant,
        controls,
        style,
        focus_id,
    } = props;
    let value = value.unwrap_or_else(|| signal(String::new()));
    let line: Rc<Cell<Option<FocusId>>> = Rc::default();
    let clear: Rc<dyn Fn()> = Rc::new(move || {
        value.set(String::new());
        if let Some(on_clear) = &on_clear {
            on_clear();
        }
    });

    let glass = shared::mark(
        || shared::Mark::Search,
        |text| text.color.faded(shared::QUIET_ALPHA),
    )?;
    let button = {
        let (clear, line) = (clear.clone(), line.clone());
        shared::shown_while(
            move || value.with(|query| !query.is_empty()),
            LayoutStyle::row(),
            move || clear_button(clear.clone(), line.clone()),
        )?
    };

    let keys: FieldKeyHandler = {
        let clear = clear.clone();
        Rc::new(move |key: &Key, modifiers: ModifiersState| {
            if on_key.as_ref().is_some_and(|hook| hook(key, modifiers)) {
                return true;
            }
            let escape = matches!(key, Key::Named(NamedKey::Escape));
            if escape && value.with(|query| !query.is_empty()) {
                clear();
                return true;
            }
            false
        })
    };
    let mut field = TextFieldProps::props()
        .value(value)
        .placeholder(Reactive::of(move || match &placeholder {
            Some(placeholder) => placeholder.get(),
            None => strings::text(strings::SEARCH),
        }))
        .label(label)
        .width(width)
        .on_key(keys);
    if let Some(on_submit) = on_submit {
        field = field.on_submit(Rc::new(move || on_submit(&value.get())));
    }
    if let Some(read) = active_descendant {
        field = field.active_descendant(read);
    }
    if let Some(read) = controls {
        field = field.controls(read);
    }
    if let Some(style) = style {
        field = field.style(style);
    }
    if let Some(focus_id) = focus_id {
        field = field.focus_id(focus_id);
    }
    let (field, focus_id) = adorned_field(
        field.build(),
        Adornments {
            leading: Some(glass),
            trailing: Some(button),
        },
    )?;
    line.set(Some(focus_id));
    Ok(field)
}

/// The button that empties the field, handing the keyboard back to the line typed into. Announced, but no Tab stop: Escape in the field does the same.
fn clear_button(
    clear: Rc<dyn Fn()>,
    line: Rc<Cell<Option<FocusId>>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let cross = shared::mark(
        || shared::Mark::Close,
        |text| text.color.faded(shared::QUIET_ALPHA),
    )?;
    let side = || {
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .width(shared::icon_size())
            .height(shared::icon_size())
            .flex_shrink(0.0)
    };
    let paint = |hovered: bool| {
        move |_r| {
            let rect = RectStyle::default().with_radius(BorderRadius::all(shared::radius_item()));
            if hovered {
                rect.with_fill(shared::hover_wash())
            } else {
                rect
            }
        }
    };
    let button = StyledContainer::new(side(), paint(false), children![cross])?
        .styled_by(side)
        .hover_style(paint(true))
        .presented(Role::Button)
        .a11y_label(|| strings::text(strings::CLEAR_SEARCH))
        .hit_outset(Reactive::of(|| {
            telar::Insets::all(((shared::MIN_TARGET - shared::icon_size()) / 2.0).max(0.0))
        }))
        .on_press(move || {
            clear();
            if let Some(id) = line.get() {
                focus::request_from_pointer(id);
            }
        });
    Ok(box_item(button))
}

#[cfg(test)]
#[path = "search_field_test.rs"]
mod tests;
