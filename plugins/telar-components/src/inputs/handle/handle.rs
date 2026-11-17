//! [`handle`]: a point on the canvas dragged to set a value, with the arrow keys as its keyboard path.

use std::cell::Cell;
use std::ops::RangeInclusive;
use std::rc::Rc;

use telar::{
    Accessible, Border, Color, Cursor, Handler, Insets, Key, LayoutError, LayoutItem, LayoutStyle,
    NamedKey, Props, Reactive, RectStyle, RwSignal, ShapeStyle, StyledContainer, SurfaceAmendment,
    Transaction, Transform, amend_surface, box_item, focus::Role, signal, step_factor,
    use_theme_tokens,
};

use crate::shared;
use crate::shared::edit::{Stroke, write_through};
use telar::ControlSize;

/// Where on the canvas the pointer asks for a value, as `(x, y)` → value.
pub type ToValue = Rc<dyn Fn(f32, f32) -> f32>;
/// Where on the canvas a value is drawn, as value → `(x, y)`.
pub type ToPoint = Rc<dyn Fn(f32) -> (f32, f32)>;

/// A handle's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the role named beside it, read when the handle paints.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HandleTokens {
    /// Diameter of the dot at the regular size, scaled with the handle's own. `None` is a slider's thumb less [`metrics::gap_sm`](crate::metrics::gap_sm) at that size, since a handle sits over the content it edits.
    pub size: Option<f32>,
    /// Width of the ring that parts the dot from what it sits on. `None` is the theme's `border_width_strong`.
    pub ring: Option<f32>,
}

impl HandleTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    /// The diameter of the dot of a handle of `size`.
    pub fn size(&self, size: ControlSize) -> f32 {
        self.size.map_or_else(
            || {
                crate::inputs::slider::slider::SliderTokens::current().thumb(size)
                    - shared::gap_sm(size)
            },
            |diameter| diameter * size.scale(),
        )
    }

    pub fn ring(&self) -> f32 {
        self.ring.unwrap_or_else(shared::border_width_strong)
    }
}

/// How far the dot's grab area reaches past its edge, so a small dot is still a target of [`shared::MIN_TARGET`].
fn grab_outset(size: f32) -> Insets {
    let reach = ((shared::MIN_TARGET - size) / 2.0).max(0.0);
    Insets::new(reach, reach, reach, reach)
}

/// A draggable point bound to a number, placed over its parent in that parent's coordinates; a drag keeps the grab offset so it doesn't jump, clamps to its range, and every drag or arrow-key step is its own [`Transaction`] (Escape reverts it).
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
    /// The values the handle keeps to, unbounded by default. A range whose start lies past its end is an error when the handle is built.
    #[props(default = f32::NEG_INFINITY..=f32::INFINITY)]
    pub range: RangeInclusive<f32>,
    /// One unscaled arrow-key step, one by default. A step that is not above zero is an error when the handle is built.
    #[props(default = 1.0)]
    pub step: f32,
    /// How large the dot is drawn, as [`ControlSize`] works it out from [`HandleTokens::size`]. Its grab area is at least the smallest pointer target at every size.
    #[props(default)]
    pub size: ControlSize,
    /// The dot's diameter in px. `None` (the default) is what [`size`](Self::size) works out.
    #[props(some, default)]
    pub diameter: Option<f32>,
    #[props(default = Cursor::Grab)]
    pub cursor: Cursor,
    /// Mirrors whether the last request was clamped, for a caller that shows it elsewhere too.
    #[props(some, into, default)]
    pub clamped: Option<RwSignal<bool>>,
    /// `None` (the default) falls back to the theme accent.
    #[props(some, into, default)]
    pub color: Option<Reactive<Color>>,
    /// `None` (the default) falls back to the theme's warning colour.
    #[props(some, into, default)]
    pub clamped_color: Option<Reactive<Color>>,
    /// What a reader calls the handle: the value it sets, such as "Hue" or "Start time".
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Runs as a drag takes hold of the handle, before its first value is asked for. Arrow-key steps are not drags and do not run it.
    #[props(some, into, default)]
    pub on_start: Option<Handler>,
    /// Runs once for every drag [`on_start`](Self::on_start) ran for, with whether it was kept: `true` when it was let go, `false` when it was cancelled — Escape, another pointer button, the window losing focus — or the handle went away mid-drag. An undone drag has reverted its transaction by the time this runs; a kept one has written the value it was let go at, and its transaction commits as this returns. A drag that joined a transaction already open elsewhere leaves that decision to whoever opened it, and ends kept whenever its gesture ends.
    #[props(some, default)]
    pub on_end: Option<Rc<dyn Fn(bool)>>,
    /// Amends the paint of the dot — the handle's **principal surface** — clamped or not. See [`telar::SurfaceStyle`] for when a theme token or [`HandleTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

#[derive(Clone, Copy)]
struct Grab {
    lift: (f32, f32),
    offset: (f32, f32),
}

pub fn handle(props: HandleProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let HandleProps {
        value,
        transaction,
        to_value,
        to_point,
        range,
        step,
        size,
        diameter,
        cursor,
        clamped,
        color,
        clamped_color,
        label,
        on_start,
        on_end,
        style,
    } = props;
    let size = move || diameter.unwrap_or_else(|| HandleTokens::current().size(size));
    let dot_box = move || {
        LayoutStyle::new()
            .absolute()
            .inset_top(0.0)
            .inset_start(0.0)
            .width(size())
            .height(size())
    };
    let (min, max) = shared::ordered("handle", range)?;
    let step = shared::positive_step("handle", step)?;
    let transaction = transaction.unwrap_or_else(|| {
        Transaction::new(value.unwrap_or_else(|| signal(0.0_f32.clamp(min, max))))
    });
    let value = transaction.signal();
    let clamped = clamped.unwrap_or_else(|| signal(false));
    let grab: Rc<Cell<Option<Grab>>> = Rc::new(Cell::new(None));
    let stroke = Rc::new(Stroke::new(on_start, on_end));

    let placed = {
        let to_point = to_point.clone();
        move |at: f32| {
            let (x, y) = to_point(at);
            let half = size() / 2.0;
            (x - half, y - half)
        }
    };
    let request = move |raw: f32| {
        let bounded = raw.clamp(min, max);
        let cut = bounded != raw;
        clamped.set_if_changed(cut);
        write_through(transaction, bounded);
    };
    let step_by = move |steps: f32| {
        let before = value.peek();
        request(before + steps * step);
        value.peek() != before
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
        dot_box(),
        move |r| {
            let fill = match clamped.get() {
                true => shared::resolve(&clamped_color, || use_theme_tokens().warning()),
                false => shared::resolve(&color, shared::accent),
            };
            let rest = RectStyle::default()
                .with_fill(fill)
                .with_border(Border::uniform(
                    shared::thumb_on(fill),
                    HandleTokens::current().ring(),
                ))
                .with_radius(shared::pill(r));
            amend_surface(rest, &style)
        },
        vec![],
    )?
    .styled_by(dot_box)
    .hit_outset(Reactive::of(move || grab_outset(size())))
    .with_transform({
        let placed = placed.clone();
        move |_| {
            let (x, y) = placed(value.get());
            Some(Transform::translate(x, y))
        }
    })
    .control(Role::Slider)
    .a11y_label(move || label.get())
    .valued(move || {
        telar::NumericValue::new(
            f64::from(min)..=f64::from(max),
            f64::from(value.get().clamp(min, max)),
        )
        .ok()
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
        let stroke = stroke.clone();
        move |local_x, local_y| {
            let held = grab.get().unwrap_or_else(|| {
                stroke.start();
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
        let stroke = stroke.clone();
        move |_, _| {
            release();
            stroke.end(true);
        }
    })
    .on_drag_cancel(move || {
        release();
        stroke.end(false);
    })
    .stepped(move |direction| {
        step_by(match direction {
            telar::focus::Step::Increment => 1.0,
            telar::focus::Step::Decrement => -1.0,
        })
    })
    .on_focused_key(move |key: &Key| -> bool {
        let direction = match key {
            Key::Named(NamedKey::ArrowRight | NamedKey::ArrowUp) => 1.0,
            Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowDown) => -1.0,
            _ => return false,
        };
        step_by(direction * step_factor(telar::modifiers()));
        true
    });
    Ok(box_item(dot))
}

#[cfg(test)]
#[path = "handle.test.rs"]
mod tests;
