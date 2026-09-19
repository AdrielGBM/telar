//! [`handle`]: a point on the canvas dragged to set a value, with the arrow keys as its keyboard path.

use std::cell::Cell;
use std::rc::Rc;

use telar_macros::Props;

use layout_core::{LayoutError, LayoutStyle};
use platform_core::{Cursor, Key, NamedKey};
use reactive_core::{Reactive, RwSignal, Transaction, signal};
use renderer_core::{Border, BorderRadius, Color, RectStyle, ShapeStyle};
use theme_core::use_theme_tokens;
use ui_core::focus::Role;
use ui_core::{Children, LayoutItem, StyledContainer, box_item, step_factor};

use crate::edit::write_through;
use crate::shared;

/// Where on the canvas the pointer asks for a value, as `(x, y)` → value.
pub type ToValue = Rc<dyn Fn(f32, f32) -> f32>;
/// Where on the canvas a value is drawn, as value → `(x, y)`.
pub type ToPoint = Rc<dyn Fn(f32) -> (f32, f32)>;

/// A draggable point bound to a number, placed over its parent in that parent's coordinates; a drag keeps the grab offset so it doesn't jump, clamps to `min..=max`, and every drag or arrow-key step is its own [`Transaction`] (Escape reverts it).
#[derive(Props)]
pub struct HandleProps {
    /// Bound value. Ignored when [`transaction`](Self::transaction) is given: its signal is the value.
    #[props(some, into, default)]
    pub value: Option<RwSignal<f32>>,
    #[props(some, default)]
    pub transaction: Option<Transaction<f32>>,
    #[props(default = Rc::new(|x, _| x))]
    pub to_value: ToValue,
    #[props(default = Rc::new(|value| (value, 0.0)))]
    pub to_point: ToPoint,
    #[props(default = f32::NEG_INFINITY)]
    pub min: f32,
    #[props(default = f32::INFINITY)]
    pub max: f32,
    /// One unscaled arrow-key step. Non-positive falls back to `1.0`.
    #[props(default = 1.0)]
    pub step: f32,
    /// The handle's diameter.
    #[props(default = 12.0)]
    pub size: f32,
    #[props(default = Cursor::Grab)]
    pub cursor: Cursor,
    /// Mirrors whether the last request was clamped, for a caller that shows it elsewhere too.
    #[props(some, into, default)]
    pub clamped: Option<RwSignal<bool>>,
    /// `Color::TRANSPARENT` (the default) falls back to the theme accent.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// `Color::TRANSPARENT` (the default) falls back to the theme's warning colour.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub clamped_color: Reactive<Color>,
}

#[derive(Clone, Copy)]
struct Grab {
    lift: (f32, f32),
    offset: (f32, f32),
}

pub fn handle(props: HandleProps, _children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let HandleProps {
        value,
        transaction,
        to_value,
        to_point,
        min,
        max,
        step,
        size,
        cursor,
        clamped,
        color,
        clamped_color,
    } = props;
    let (min, max) = if max < min { (max, min) } else { (min, max) };
    let step = if step > 0.0 { step } else { 1.0 };
    let transaction = transaction.unwrap_or_else(|| {
        Transaction::new(value.unwrap_or_else(|| signal(0.0_f32.clamp(min, max))))
    });
    let value = transaction.signal();
    let clamped = clamped.unwrap_or_else(|| signal(false));
    let grab: Rc<Cell<Option<Grab>>> = Rc::new(Cell::new(None));

    let placed = {
        let to_point = to_point.clone();
        move |at: f32| {
            let (x, y) = to_point(at);
            (x - size / 2.0, y - size / 2.0)
        }
    };
    let request = move |raw: f32| {
        let bounded = raw.clamp(min, max);
        let cut = bounded != raw;
        if clamped.peek() != cut {
            clamped.set(cut);
        }
        write_through(transaction, bounded);
    };
    let release = {
        let grab = grab.clone();
        move || {
            grab.set(None);
            if clamped.peek() {
                clamped.set(false);
            }
        }
    };

    let dot = StyledContainer::new(
        LayoutStyle::new()
            .absolute()
            .inset_top(0.0)
            .inset_start(0.0)
            .width(size)
            .height(size),
        move |_| {
            let fill = match clamped.get() {
                true => shared::resolve(&clamped_color, || use_theme_tokens().warning()),
                false => shared::resolve(&color, shared::accent),
            };
            RectStyle::default()
                .with_fill(fill)
                .with_border(Border::uniform(Color::WHITE, 2.0))
                .with_radius(BorderRadius::all(size / 2.0))
        },
        vec![],
    )?
    .with_transform({
        let placed = placed.clone();
        move |_| {
            let (x, y) = placed(value.get());
            Some([1.0, 0.0, 0.0, 1.0, x, y])
        }
    })
    .control(Role::Slider)
    .valued(move || platform_core::NumericValue {
        now: value.get() as f64,
        min: min as f64,
        max: max as f64,
    })
    .on_focus({
        let release = release.clone();
        move |now| {
            if !now {
                release();
            }
        }
    })
    .cursor(cursor)
    .drag_transaction(transaction)
    .on_drag({
        let grab = grab.clone();
        move |local_x, local_y| {
            let held = grab.get().unwrap_or_else(|| {
                let at = value.peek();
                let lift = placed(at);
                let centre = to_point(at);
                let latched = Grab {
                    lift,
                    offset: (local_x + lift.0 - centre.0, local_y + lift.1 - centre.1),
                };
                grab.set(Some(latched));
                latched
            });
            let x = local_x + held.lift.0 - held.offset.0;
            let y = local_y + held.lift.1 - held.offset.1;
            request(to_value(x, y));
        }
    })
    .on_drag_end({
        let release = release.clone();
        move |_, _| release()
    })
    .on_drag_cancel(release)
    .on_focused_key(move |key: &Key| -> bool {
        let direction = match key {
            Key::Named(NamedKey::ArrowRight | NamedKey::ArrowUp) => 1.0,
            Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowDown) => -1.0,
            _ => return false,
        };
        request(value.peek() + direction * step * step_factor(ui_core::modifiers()));
        true
    });
    Ok(box_item(dot))
}

#[cfg(test)]
#[path = "handle_test.rs"]
mod tests;
