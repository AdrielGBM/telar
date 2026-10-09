//! [`split_pane`]: two panes divided by a draggable splitter that the arrow keys, Home, End and Enter also drive.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use telar::{
    Accessible, BorderRadius, Children, Clip, ClippedItem, Container, Cursor, Direction, DragAxis,
    Key, LayoutError, LayoutItem, LayoutStyle, NamedKey, NumericValue, Orientation, Props,
    Reactive, Rect, RectStyle, RwSignal, ShapeStyle, SizeDimension, StyledContainer, Transaction,
    box_item, current_direction, drag_start, focus::Role, signal, step_factor, track_layout,
};
use web_time::Instant;

use crate::edit::write_through;
use crate::shared;
use crate::strings;

const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const DRAG_THRESHOLD: f32 = 3.0;

/// How the two panes sit relative to each other.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, telar::PreviewArg)]
pub enum SplitDirection {
    /// Side by side, the first pane at the inline start; the splitter is a vertical bar.
    #[default]
    Row,
    /// Stacked, the first pane on top; the splitter is a horizontal bar.
    Column,
}

impl SplitDirection {
    fn main(self, rect: Rect) -> f32 {
        match self {
            Self::Row => rect.width,
            Self::Column => rect.height,
        }
    }

    fn along(self, x: f32, y: f32) -> f32 {
        match self {
            Self::Row => x,
            Self::Column => y,
        }
    }

    fn sized(self, style: LayoutStyle, dim: impl Into<SizeDimension>) -> LayoutStyle {
        match self {
            Self::Row => style.width(dim),
            Self::Column => style.height(dim),
        }
    }

    fn at_least(self, style: LayoutStyle, dim: impl Into<SizeDimension>) -> LayoutStyle {
        match self {
            Self::Row => style.min_width(dim),
            Self::Column => style.min_height(dim),
        }
    }

    fn at_most(self, style: LayoutStyle, dim: impl Into<SizeDimension>) -> LayoutStyle {
        match self {
            Self::Row => style.max_width(dim),
            Self::Column => style.max_height(dim),
        }
    }

    fn flow(self) -> LayoutStyle {
        match self {
            Self::Row => LayoutStyle::new().flex_row(),
            Self::Column => LayoutStyle::new().flex_column(),
        }
    }

    fn inverted(self) -> bool {
        self == Self::Row && current_direction() == Direction::Rtl
    }
}

/// Which pane [`SplitPaneProps::size`], the limits and the collapse belong to; the other pane takes the rest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, telar::PreviewArg)]
pub enum SizedPane {
    /// The pane at the inline start of a row, or at the top of a column.
    #[default]
    First,
    /// The pane at the inline end of a row, or at the bottom of a column: a panel that keeps its size while the window grows.
    Second,
}

impl SizedPane {
    /// Which way the sized pane grows as the splitter moves toward the end of the split.
    fn toward_end(self) -> f32 {
        match self {
            Self::First => 1.0,
            Self::Second => -1.0,
        }
    }
}

/// The room the sized pane may take, in px or as a fraction of the whole split.
#[derive(Clone, Copy)]
struct Limits {
    min: f32,
    max: f32,
    min_fraction: f32,
    max_fraction: f32,
    bar: f32,
}

impl Limits {
    fn range(self, total: f32) -> (f32, f32) {
        let room = if total > 0.0 {
            (total - self.bar).max(0.0)
        } else {
            f32::INFINITY
        };
        let lo = match self.min_fraction > 0.0 && total > 0.0 {
            true => self.min_fraction * total,
            false => self.min,
        };
        let hi = match (self.max_fraction > 0.0 && total > 0.0, self.max > 0.0) {
            (true, _) => self.max_fraction * total,
            (false, true) => self.max,
            (false, false) => room,
        };
        let hi = hi.min(room);
        (lo.min(hi), hi)
    }

    fn min_dimension(self) -> SizeDimension {
        match self.min_fraction > 0.0 {
            true => SizeDimension::Percent(self.min_fraction),
            false => SizeDimension::Px(self.min),
        }
    }

    fn max_dimension(self) -> Option<SizeDimension> {
        match (self.max_fraction > 0.0, self.max > 0.0) {
            (true, _) => Some(SizeDimension::Percent(self.max_fraction)),
            (false, true) => Some(SizeDimension::Px(self.max)),
            (false, false) => None,
        }
    }
}

/// Two panes divided by a splitter, dragged or stepped with the keys to resize one of them, the first unless [`sized`](Self::sized) says otherwise; the other takes the rest. Children: the first pane's content, then the second's. Every drag is a [`Transaction`] on the size, so Escape puts it back.
#[derive(Props)]
pub struct SplitPaneProps {
    #[props(default)]
    pub direction: SplitDirection,
    /// The pane the size, the limits and the collapse apply to.
    #[props(default)]
    pub sized: SizedPane,
    /// Bound size of the sized pane along the split, in px. `None` (the default) is uncontrolled: the widget owns a signal starting at [`default_size`](Self::default_size). It keeps the size a collapse will restore, so a caller persisting it never stores zero.
    #[props(some, into, default)]
    pub size: Option<RwSignal<f32>>,
    /// Starting size when [`size`](Self::size) is not bound. `0.0` (the default) means 240.
    #[props(default)]
    pub default_size: f32,
    /// Smallest size of the sized pane, in px.
    #[props(default)]
    pub min: f32,
    /// Largest size of the sized pane, in px. `0.0` (the default) leaves it bounded only by the room the split has.
    #[props(default)]
    pub max: f32,
    /// Smallest size of the sized pane as a fraction of the whole split (`0.0..=1.0`); when above zero it replaces [`min`](Self::min).
    #[props(default)]
    pub min_fraction: f32,
    /// Largest size of the sized pane as a fraction of the whole split (`0.0..=1.0`); when above zero it replaces [`max`](Self::max).
    #[props(default)]
    pub max_fraction: f32,
    /// Bound collapsed state. `None` (the default) is uncontrolled. A collapsed pane is zero-sized while [`size`](Self::size) keeps its last value.
    #[props(some, into, default)]
    pub collapsed: Option<RwSignal<bool>>,
    /// Whether a double-click or Enter on the splitter collapses the sized pane. On by default.
    #[props(default = true)]
    pub collapsible: bool,
    /// One unscaled arrow-key step in px. Non-positive means 16; Shift makes it larger.
    #[props(default)]
    pub step: f32,
    /// What a reader calls the splitter. `None` (the default) uses the localized `telar_components.resize` message, which an application can override in its catalog.
    #[props(some, into, default)]
    pub label: Option<Reactive<String>>,
}

pub fn split_pane(
    props: SplitPaneProps,
    children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let SplitPaneProps {
        direction,
        sized,
        size,
        default_size,
        min,
        max,
        min_fraction,
        max_fraction,
        collapsed,
        collapsible,
        step,
        label,
    } = props;
    let default_size = if default_size > 0.0 {
        default_size
    } else {
        240.0
    };
    let step = if step > 0.0 { step } else { 16.0 };
    let size = size.unwrap_or_else(|| signal(default_size));
    let collapsed = collapsed.unwrap_or_else(|| signal(false));
    let transaction = Transaction::new(size);
    let limits = Limits {
        min: min.max(0.0),
        max,
        min_fraction,
        max_fraction,
        bar: bar_thickness(),
    };

    let mut slots = children.build()?.take_default().into_iter();
    let first_items: Vec<_> = slots.by_ref().take(1).collect();
    let second_items: Vec<_> = slots.collect();

    let fixed = move || {
        let style = LayoutStyle::new().flex_column().flex_shrink(0.0);
        if collapsed.get() {
            return style.shown(false);
        }
        let style = direction.at_least(direction.sized(style, size.get()), limits.min_dimension());
        match limits.max_dimension() {
            Some(max) => direction.at_most(style, max),
            None => style,
        }
    };
    let pane = |items: Vec<Box<dyn LayoutItem>>, is_sized: bool| {
        let pane = if is_sized {
            Container::new(fixed(), items)?.styled_by(fixed)
        } else {
            Container::new(
                direction.at_least(
                    LayoutStyle::new()
                        .flex_column()
                        .flex_grow(1.0)
                        .flex_shrink(1.0),
                    0.0,
                ),
                items,
            )?
        };
        let rect = track_layout(pane.layout_node()).expect("a pane is registered");
        Ok::<_, LayoutError>((ClippedItem::following(box_item(pane), Clip::both), rect))
    };
    let (first, first_rect) = pane(first_items, sized == SizedPane::First)?;
    let (second, second_rect) = pane(second_items, sized == SizedPane::Second)?;
    let (sized_rect, other_rect) = match sized {
        SizedPane::First => (first_rect, second_rect),
        SizedPane::Second => (second_rect, first_rect),
    };
    let applied = move || match collapsed.get() {
        true => 0.0,
        false => direction.main(sized_rect.get()),
    };
    let total = move || applied() + direction.main(other_rect.get()) + limits.bar;
    let toward_end = sized.toward_end();
    let sign = move || {
        let reading = if direction.inverted() { -1.0 } else { 1.0 };
        reading * toward_end
    };

    let dragging = signal(false);
    let last_click: Rc<Cell<Option<Instant>>> = Rc::new(Cell::new(None));

    let toggle_collapse = move || {
        if collapsible {
            collapsed.set(!collapsed.peek());
        }
    };
    let move_to = move |raw: f32| {
        let (lo, hi) = limits.range(total());
        collapsed.set(false);
        write_through(transaction, raw.clamp(lo, hi));
    };

    let bar = StyledContainer::new(
        bar_box(direction, limits.bar),
        move |_| {
            let fill = match dragging.get() {
                true => shared::accent(),
                false => shared::border(),
            };
            RectStyle::default()
                .with_fill(fill)
                .with_radius(BorderRadius::all(limits.bar / 2.0))
        },
        vec![],
    )?
    .styled_by(move || bar_box(direction, limits.bar))
    .control(Role::Splitter)
    .oriented(match direction {
        SplitDirection::Row => Orientation::Vertical,
        SplitDirection::Column => Orientation::Horizontal,
    })
    .valued(move || {
        let (lo, hi) = limits.range(total());
        NumericValue {
            now: applied() as f64,
            min: lo as f64,
            max: hi as f64,
        }
    })
    .cursor(match direction {
        SplitDirection::Row => Cursor::ColResize,
        SplitDirection::Column => Cursor::RowResize,
    })
    .drag_threshold(DRAG_THRESHOLD)
    .drag_axis(match direction {
        SplitDirection::Row => DragAxis::Horizontal,
        SplitDirection::Column => DragAxis::Vertical,
    })
    .drag_transaction(transaction)
    .on_drag(move |x, y| {
        dragging.set(true);
        let local = direction.along(x, y);
        let grabbed = drag_start().map_or(local, |start| direction.along(start.at.0, start.at.1));
        move_to(applied() + (local - grabbed) * sign());
    })
    .a11y_label(move || match &label {
        Some(label) => label.get(),
        None => strings::text(strings::RESIZE),
    })
    .on_drag_end(move |_, _| dragging.set(false))
    .on_drag_cancel(move || dragging.set(false))
    .on_press({
        let last_click = last_click.clone();
        move || {
            let now = Instant::now();
            let previous = last_click.replace(Some(now));
            if previous.is_some_and(|then| now.duration_since(then) <= DOUBLE_CLICK) {
                last_click.set(None);
                toggle_collapse();
            }
        }
    })
    .on_focused_key(move |key: &Key| -> bool {
        let Key::Named(named) = key else {
            return false;
        };
        let grows = match (direction, named) {
            (SplitDirection::Row, NamedKey::ArrowRight) => sign(),
            (SplitDirection::Row, NamedKey::ArrowLeft) => -sign(),
            (SplitDirection::Column, NamedKey::ArrowDown) => toward_end,
            (SplitDirection::Column, NamedKey::ArrowUp) => -toward_end,
            _ => 0.0,
        };
        if grows != 0.0 {
            if collapsed.peek() {
                collapsed.set(false);
            } else {
                move_to(size.peek() + grows * step * step_factor(telar::modifiers()));
            }
            return true;
        }
        match named {
            NamedKey::Home => move_to(f32::NEG_INFINITY),
            NamedKey::End => move_to(f32::INFINITY),
            NamedKey::Enter if collapsible => toggle_collapse(),
            _ => return false,
        }
        true
    });

    let root = Container::new(
        direction.flow().flex_grow(1.0),
        vec![box_item(first), box_item(bar), box_item(second)],
    )?;
    Ok(box_item(root))
}

fn bar_thickness() -> f32 {
    (shared::spacing() * 0.75).max(4.0)
}

fn bar_box(direction: SplitDirection, thickness: f32) -> LayoutStyle {
    direction.sized(LayoutStyle::new().flex_shrink(0.0), thickness)
}

#[cfg(test)]
#[path = "split_pane_test.rs"]
mod tests;
