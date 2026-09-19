//! [`scrub_field`]: a number edited by dragging its label, by the arrow keys, or by typing it.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use telar_macros::Props;
use web_time::Instant;

use layout_core::{AlignItems, LayoutError, LayoutStyle};
use platform_core::{Cursor, Key, NamedKey};
use reactive_core::{Reactive, RwSignal, Transaction, effect, signal};
use renderer_core::{BorderRadius, RectStyle, ShapeStyle};
use ui_core::focus::{self, Role};
use ui_core::{
    Children, Container, DragAxis, Input, LayoutItem, StyledContainer, Text, box_item, step_factor,
    style_follows,
};

use crate::edit::{snap, write_through};
use crate::shared;

/// Turns the value into the text the field shows and starts typed entry from.
pub type Format = Rc<dyn Fn(f32) -> String>;
/// Reads typed text back into a value; `None` rejects it and leaves the value as it was.
pub type Parse = Rc<dyn Fn(&str) -> Option<f32>>;

/// The longest gap between two clicks that still reads as a double-click.
pub const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// How far the pointer travels before a press on the label is a scrub rather than a click.
const SCRUB_THRESHOLD: f32 = 3.0;

/// A labelled number edited by dragging the label (Shift/Alt scale the step mid-drag), by the arrow keys, or by typing it after a double-click or Enter; every change is its own [`Transaction`], joining one a popover already opened on the same value.
#[derive(Props)]
pub struct ScrubFieldProps {
    /// Bound value. `None` (the default) is uncontrolled — the widget owns a signal starting at `0.0` clamped into range. Ignored when [`transaction`](Self::transaction) is given: its signal is the value.
    #[props(some, into, default)]
    pub value: Option<RwSignal<f32>>,
    /// The transaction every change goes through. `None` makes one over the value.
    #[props(some, default)]
    pub transaction: Option<Transaction<f32>>,
    /// The caption that doubles as the scrub handle.
    #[props(into, default)]
    pub label: Reactive<String>,
    #[props(default = f32::NEG_INFINITY)]
    pub min: f32,
    #[props(default = f32::INFINITY)]
    pub max: f32,
    /// One unscaled step, for the keys and for the drag. Non-positive falls back to `1.0`.
    #[props(default = 1.0)]
    pub step: f32,
    /// Pointer travel per unscaled step while scrubbing.
    #[props(default = 4.0)]
    pub pixels_per_step: f32,
    /// `None` shows the value with up to three decimals and no trailing zeros.
    #[props(some, default)]
    pub format: Option<Format>,
    /// `None` accepts any finite decimal number.
    #[props(some, default)]
    pub parse: Option<Parse>,
}

#[derive(Clone, Copy)]
struct Scrub {
    last_x: f32,
    raw: f32,
}

pub fn scrub_field(
    props: ScrubFieldProps,
    _children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ScrubFieldProps {
        value,
        transaction,
        label,
        min,
        max,
        step,
        pixels_per_step,
        format,
        parse,
    } = props;
    let (min, max) = if max < min { (max, min) } else { (min, max) };
    let step = if step > 0.0 { step } else { 1.0 };
    let pixels_per_step = pixels_per_step.max(f32::EPSILON);
    let transaction = transaction.unwrap_or_else(|| {
        Transaction::new(value.unwrap_or_else(|| signal(0.0_f32.clamp(min, max))))
    });
    let value = transaction.signal();
    let format = format.unwrap_or_else(|| Rc::new(plain));
    let parse = parse.unwrap_or_else(|| Rc::new(decimal));

    let draft = signal(String::new());
    let scrub: Rc<Cell<Option<Scrub>>> = Rc::new(Cell::new(None));

    let shown = Text::declaring(
        {
            let format = format.clone();
            move || format(value.get())
        },
        LayoutStyle::new(),
        |t| shared::control_text(t, 1.0),
    )?;
    let input = Input::declaring(draft, LayoutStyle::new(), |t| shared::control_text(t, 1.0))?
        .select_on_focus();
    let input_id = input.focus_id();
    let editing = signal(false);
    style_follows(input.layout_node(), move || match editing.get() {
        true => LayoutStyle::new().min_width(48.0),
        false => LayoutStyle::new().display_none(),
    });
    style_follows(shown.layout_node(), move || match editing.get() {
        true => LayoutStyle::new().display_none(),
        false => LayoutStyle::new(),
    });

    let commit_draft = {
        let parse = parse.clone();
        Rc::new(move || {
            if !editing.peek() {
                return;
            }
            editing.set(false);
            if let Some(typed) = parse(&draft.peek()) {
                write_through(transaction, typed.clamp(min, max));
            }
        })
    };
    let start_editing = {
        let format = format.clone();
        Rc::new(move || {
            draft.set(format(value.peek()));
            editing.set(true);
            focus::request(input_id);
        })
    };

    let input = input
        .on_submit({
            let commit_draft = commit_draft.clone();
            move || {
                commit_draft();
                focus::release(input_id);
            }
        })
        .on_cancel(move || editing.set(false));
    {
        let commit_draft = commit_draft.clone();
        let had = Cell::new(false);
        effect(move || {
            let has = focus::is_focused(input_id);
            if had.replace(has) && !has {
                commit_draft();
            }
        });
    }

    let caption = Text::declaring(
        move || label.get(),
        LayoutStyle::new(),
        |t| shared::quiet(t, 1.0),
    )?;

    let last_click: Rc<Cell<Option<Instant>>> = Rc::new(Cell::new(None));
    let handle = StyledContainer::new(
        handle_box(),
        |_| {
            RectStyle::default()
                .with_fill(shared::surface_alt())
                .with_radius(BorderRadius::all(shared::radius()))
        },
        vec![box_item(caption), box_item(shown)],
    )?
    .styled_by(handle_box)
    .control(Role::SpinButton)
    .valued(move || platform_core::NumericValue {
        now: value.get() as f64,
        min: min as f64,
        max: max as f64,
    })
    .cursor(Cursor::EwResize)
    .drag_threshold(SCRUB_THRESHOLD)
    .drag_axis(DragAxis::Horizontal)
    .drag_transaction(transaction)
    .on_drag({
        let scrub = scrub.clone();
        move |x, _| {
            if editing.peek() {
                editing.set(false);
            }
            let factor = step_factor(ui_core::modifiers());
            let held = scrub.get().unwrap_or(Scrub {
                last_x: x,
                raw: value.peek(),
            });
            let raw =
                (held.raw + (x - held.last_x) / pixels_per_step * step * factor).clamp(min, max);
            scrub.set(Some(Scrub { last_x: x, raw }));
            write_through(transaction, snap(raw, step * factor.min(1.0), min, max));
        }
    })
    .on_drag_end({
        let scrub = scrub.clone();
        move |_, _| scrub.set(None)
    })
    .on_drag_cancel({
        let scrub = scrub.clone();
        move || scrub.set(None)
    })
    .on_press({
        let start_editing = start_editing.clone();
        move || {
            let now = Instant::now();
            let previous = last_click.replace(Some(now));
            if previous.is_some_and(|then| now.duration_since(then) <= DOUBLE_CLICK) {
                last_click.set(None);
                start_editing();
            }
        }
    })
    .on_focused_key(move |key: &Key| -> bool {
        if editing.peek() {
            return false;
        }
        let direction = match key {
            Key::Named(NamedKey::ArrowRight | NamedKey::ArrowUp) => 1.0,
            Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowDown) => -1.0,
            Key::Named(NamedKey::Enter) => {
                start_editing();
                return true;
            }
            _ => return false,
        };
        let factor = step_factor(ui_core::modifiers());
        let next = snap(
            value.peek() + direction * step * factor,
            step * factor.min(1.0),
            min,
            max,
        );
        write_through(transaction, next);
        true
    });

    let row =
        Container::new(row_box(), vec![box_item(handle), box_item(input)])?.styled_by(row_box);
    Ok(box_item(row))
}

fn row_box() -> LayoutStyle {
    LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .gap(shared::spacing())
}

fn handle_box() -> LayoutStyle {
    LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .gap(shared::spacing())
        .padding_horizontal(shared::spacing())
        .padding_vertical(shared::spacing() / 2.0)
}

fn plain(value: f32) -> String {
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    match text {
        "-0" => "0".to_string(),
        other => other.to_string(),
    }
}

fn decimal(text: &str) -> Option<f32> {
    text.trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
}

#[cfg(test)]
#[path = "scrub_field_test.rs"]
mod tests;
