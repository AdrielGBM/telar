//! [`window_controls`] and [`title_bar`]: the parts of a title bar an application draws itself, for a window it opens without the window system's decorations or a frame it draws around a surface.

use std::cell::Cell;
use std::rc::Rc;

use telar::{
    Accessible, AlignItems, Color, Container, JustifyContent, LayoutError, LayoutItem, LayoutStyle,
    Paint, Props, Reactive, RectStyle, ShapeStyle, StyledContainer, SurfaceAmendment,
    amend_surface, box_item, focus::Role, on_cleanup, signal,
};
use web_time::Instant;

use crate::shared::{self, DOUBLE_CLICK, Mark};
use crate::strings;

/// How far a press on a title bar travels before it moves the window rather than counting towards a double-click.
const DRAG_THRESHOLD: f32 = 4.0;

/// The buttons that minimize, maximize or restore, and close a window, for a title bar an application draws itself.
#[derive(Props)]
pub struct WindowControlsProps {
    /// Draw the button that minimizes the window. On by default; a surface with no top-level window to minimize, such as a layer-shell panel, turns it off rather than drawing a control that does nothing.
    #[props(default = true)]
    pub minimize: bool,
    /// Draw the button that maximizes the window, which is drawn and named as restoring it while the window is maximized. On by default.
    #[props(default = true)]
    pub maximize: bool,
    /// What the close button does. The default closes the window, and with it a single-window app.
    #[props(default = Rc::new(telar::window::close))]
    pub on_close: Rc<dyn Fn()>,
    /// The marks' colour. `Color::TRANSPARENT` (the default) keeps the ink the controls inherit.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub glyph: Reactive<Color>,
    /// Fill behind minimize and maximize under the pointer. `Color::TRANSPARENT` (the default) is the theme's hover wash.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub hover: Reactive<Color>,
    /// The mark of minimize and maximize under the pointer, for a hover fill the resting mark cannot be read against. `Color::TRANSPARENT` (the default) keeps the mark as it is.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub hover_glyph: Reactive<Color>,
    /// Fill behind close under the pointer, often louder than [`hover`](Self::hover) since closing is the one control that cannot be undone. `Color::TRANSPARENT` (the default) is the theme's hover wash.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub close_hover: Reactive<Color>,
    /// The close mark under the pointer. `Color::TRANSPARENT` (the default) keeps the mark as it is.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub close_hover_glyph: Reactive<Color>,
    /// The least each button measures on either side, in px. `None` (the default) is [`metrics::MIN_TARGET`](crate::metrics::MIN_TARGET); a bar of other controls passes `Reactive::of(metrics::control_height)` so the window's controls stand as tall as the rest.
    #[props(some, into, default)]
    pub min_size: Option<Reactive<f32>>,
    /// Amends the paint of each button — the controls' **principal surface** — at rest and under the pointer. See [`telar::SurfaceStyle`] for when a theme token is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// What a window-control button is painted in: its mark at rest, and its fill and mark under the pointer.
#[derive(Clone)]
struct ControlPaint {
    glyph: Reactive<Color>,
    hover: Reactive<Color>,
    hover_glyph: Reactive<Color>,
}

/// What every button of one [`window_controls`] shares: its least size and the caller's amendment.
#[derive(Clone)]
struct ControlLook {
    min_size: Option<Reactive<f32>>,
    style: Option<SurfaceAmendment>,
}

impl ControlLook {
    fn shell(&self) -> LayoutStyle {
        let side = self
            .min_size
            .as_ref()
            .map_or(shared::MIN_TARGET, Reactive::get);
        LayoutStyle::column()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .padding_horizontal(shared::gap_md())
            .padding_vertical(shared::gap_xs())
            .min_width(side)
            .min_height(side)
    }
}

/// `color`, unless it is the `Color::TRANSPARENT` that leaves it unset.
fn named(color: &Reactive<Color>) -> Option<Color> {
    let color = color.get();
    (color != Color::TRANSPARENT).then_some(color)
}

/// One window-control button under a tooltip naming it: a mark a reader hears by `name`. It holds every stroke started on it, so a title bar around it never takes a press on it for a move of the window.
fn control_button(
    shape: impl Fn() -> Mark + 'static,
    name: impl Fn() -> String + 'static,
    paint: ControlPaint,
    look: ControlLook,
    press: Rc<dyn Fn()>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let name = Reactive::of(name);
    let button = ControlButton {
        shape: Rc::new(shape),
        name: name.clone(),
        paint,
        look,
        press,
    };
    shared::hinted(move || button.build(), name, Reactive::default())
}

/// Everything one window-control button is built from, so the tooltip naming it can build it wherever and however often it places it.
#[derive(Clone)]
struct ControlButton {
    shape: Rc<dyn Fn() -> Mark>,
    name: Reactive<String>,
    paint: ControlPaint,
    look: ControlLook,
    press: Rc<dyn Fn()>,
}

impl ControlButton {
    fn build(&self) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let ControlButton {
            shape,
            name,
            paint,
            look,
            press,
        } = self.clone();
        let ControlPaint {
            glyph,
            hover,
            hover_glyph,
        } = paint;
        let hovered = signal(false);
        let mark = shared::mark(
            move || shape(),
            move |inherited| {
                let color = match hovered.get() {
                    true => named(&hover_glyph).or_else(|| named(&glyph)),
                    false => named(&glyph),
                };
                color.map_or(inherited.color, Paint::from)
            },
        )?;
        let shell = {
            let look = look.clone();
            move || look.shell()
        };
        let rest = look.style.clone();
        let over = look.style;
        let button = StyledContainer::new(
            shell(),
            move |_| amend_surface(RectStyle::default(), &rest),
            vec![mark],
        )?
        .styled_by(shell)
        .hover_style(move |_| {
            let fill = shared::resolve(&hover, shared::hover_wash);
            amend_surface(RectStyle::default().with_fill(fill), &over)
        })
        .on_hover(move |over| hovered.set(over))
        .control(Role::Button)
        .a11y_label(move || name.get())
        .on_press(move || press())
        .holds_stroke();
        Ok(box_item(button))
    }
}

/// A window's minimize, maximize and close buttons, in that order, as one group a reader hears as the window's.
///
/// Maximize follows the window: while it is maximized the button draws two offset squares and is named Restore, and pressing it restores the window. Minimize and maximize ask the platform through [`telar::window`]; on a surface without a top-level window they do nothing, so a backend that cannot honour them turns them off.
pub fn window_controls(props: WindowControlsProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let WindowControlsProps {
        minimize,
        maximize,
        on_close,
        glyph,
        hover,
        hover_glyph,
        close_hover,
        close_hover_glyph,
        min_size,
        style,
    } = props;
    let look = ControlLook { min_size, style };
    let quiet = || ControlPaint {
        glyph: glyph.clone(),
        hover: hover.clone(),
        hover_glyph: hover_glyph.clone(),
    };

    let mut buttons = Vec::new();
    if minimize {
        buttons.push(control_button(
            || Mark::Minimize,
            || strings::text(strings::MINIMIZE),
            quiet(),
            look.clone(),
            Rc::new(telar::window::minimize),
        )?);
    }
    if maximize {
        buttons.push(control_button(
            || match telar::window::is_maximized() {
                true => Mark::Restore,
                false => Mark::Maximize,
            },
            || {
                strings::text(match telar::window::is_maximized() {
                    true => strings::RESTORE,
                    false => strings::MAXIMIZE,
                })
            },
            quiet(),
            look.clone(),
            Rc::new(telar::window::toggle_maximize),
        )?);
    }
    buttons.push(control_button(
        || Mark::Close,
        || strings::text(strings::CLOSE),
        ControlPaint {
            glyph: glyph.clone(),
            hover: close_hover,
            hover_glyph: close_hover_glyph,
        },
        look,
        on_close,
    )?);

    let group = Container::new(LayoutStyle::row().align_items(AlignItems::CENTER), buttons)?
        .role(Role::Group)
        .a11y_label(|| strings::text(strings::WINDOW));
    Ok(box_item(group))
}

/// What [`title_bar`] makes a bar do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TitleBarOptions {
    /// A drag from the bar's empty space moves the window. On by default.
    pub drag: bool,
    /// A double-click on the bar's empty space maximizes the window, or restores it. On by default; a window that offers no maximize turns it off.
    pub maximize: bool,
    /// The bar stands in for the title bar the window system draws: the window has no decorations while the bar is shown, and has them again once it is gone. Off by default, for a bar in a window opened without decorations, or a frame drawn inside a surface that is not a window of its own.
    pub replace_decorations: bool,
}

impl Default for TitleBarOptions {
    fn default() -> Self {
        Self {
            drag: true,
            maximize: true,
            replace_decorations: false,
        }
    }
}

/// Makes `bar` the window's title bar for as long as it is shown, as `options` say: a drag from its empty space moves the window, a double-click there maximizes or restores it, and the window system's own decorations can be taken off meanwhile.
///
/// A press on a control inside the bar stays the control's, and so does a stroke started on one that holds it, as [`window_controls`] does, so only the bar's empty space moves the window. A bar without a role is made a `Role::Group`, since a box that answers a press would otherwise be announced as a button; give it a `Role::Banner` first for an application's top bar.
pub fn title_bar(bar: StyledContainer, options: TitleBarOptions) -> StyledContainer {
    let bar = match bar.declared_role() {
        Some(_) => bar,
        None => bar.role(Role::Group),
    };
    let TitleBarOptions {
        drag,
        maximize,
        replace_decorations,
    } = options;
    if replace_decorations {
        telar::window::set_decorations(false);
        on_cleanup(|| telar::window::set_decorations(true));
    }
    let bar = match drag {
        true => bar
            .drag_threshold(DRAG_THRESHOLD)
            .on_drag(|_, _| telar::window::drag()),
        false => bar,
    };
    if !maximize {
        return bar;
    }
    let last_press = Rc::new(Cell::new(None::<Instant>));
    bar.on_press(move || {
        let now = Instant::now();
        let previous = last_press.replace(Some(now));
        if previous.is_some_and(|then| now.duration_since(then) <= DOUBLE_CLICK) {
            last_press.set(None);
            telar::window::toggle_maximize();
        }
    })
}

#[cfg(test)]
#[path = "title_bar_test.rs"]
mod tests;
