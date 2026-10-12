//! The menu: one panel of rows and submenus, and the two ways it is opened — [`menu`], a button that drops it under itself, and [`context_menu`], which opens it where a gesture asked for it.
//!
//! The panel is the same either way (see `menu_panel`): its rows are `item`, `separator`, `group` and `submenu` children, its keys and submenus behave alike, and [`MenuTokens`] draws both. What differs is only what it opens from, and so where it goes and where the keyboard returns to: under its trigger and back to it, or at a point and back to whatever held the keyboard before.

use std::rc::Rc;

#[cfg(test)]
use telar::Slots;
use telar::{
    Anchoring, Children, Color, ConsumedKeys, HasPopup, Key, LayoutError, LayoutItem, NamedKey,
    Placement, Props, Reactive, Rect, RwSignal, SurfaceAmendment, box_item, effect,
    focus::{self, FocusId, Role},
    memo, signal, track_layout,
};

use crate::dropdown;
use crate::list::ListTokens;
use crate::menu_panel::{self, Config, Keys, Level, Spec};
use crate::shared;
// What the test module below reads via `use super::*` to compute click points.
#[cfg(test)]
fn panel_width() -> f32 {
    dropdown::width(None)
}
#[cfg(test)]
use crate::shared::{panel_pad, row_height};

/// A menu's own tokens — its panels, whether a `menu` dropped them or a `context_menu` opened them — which an application's theme supplies through a `#[theme(extension)]` field, and one menu through its `tokens` prop. The button a `menu` drops from is [`MenuTriggerTokens`].
///
/// Every field left `None` follows the theme role named beside it, read when the menu is laid out or painted, so a menu that names none is drawn like the list a `select` opens, in the light theme and the dark alike. A menu is chrome, which is where an application's own look shows, so every colour is a token as well as every length.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MenuTokens {
    /// The panel's fill. `None` is the theme's `surface`.
    pub background: Option<Color>,
    /// The panel's edge and the rule between groups. `None` is the theme's `border`.
    pub border: Option<Color>,
    /// A row's label. `None` is the ink the menu inherits.
    pub label: Option<Color>,
    /// A hint, a submenu's mark, and a disabled row's label. `None` is the theme's `muted`.
    pub faint: Option<Color>,
    /// Behind the row under the pointer or the keyboard. `None` is the theme's `highlight_low`, tinted by a `menu`'s `color`.
    pub highlight: Option<Color>,
    /// Corner of a panel. `None` is the theme's `radius_surface`.
    pub radius: Option<f32>,
    /// Height of a row, at the regular control size and scaled with the control size in force. `None` is [`ListTokens::row_height`].
    pub row_height: Option<f32>,
    /// Space between a panel's edge and its rows, at the regular control size and scaled with the control size in force. `None` is [`metrics::panel_pad`](crate::metrics::panel_pad).
    pub padding: Option<f32>,
    /// The least width of a panel, at the regular control size and scaled with the control size in force: a panel widens to its widest row, as far as the room it has. `None` is 13 ems of the document's text. A `context_menu`'s `width` prop wins over it.
    pub width: Option<f32>,
    /// Space between a `menu`'s panel and its trigger, or a `context_menu`'s and the control it was asked for from the keyboard, at the regular control size and scaled with the control size in force. `None` is [`metrics::popup_offset`](crate::metrics::popup_offset).
    pub offset: Option<f32>,
}

impl MenuTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    /// These tokens, with every field they leave `None` taken from `fallback`.
    pub fn or(self, fallback: Self) -> Self {
        Self {
            background: self.background.or(fallback.background),
            border: self.border.or(fallback.border),
            label: self.label.or(fallback.label),
            faint: self.faint.or(fallback.faint),
            highlight: self.highlight.or(fallback.highlight),
            radius: self.radius.or(fallback.radius),
            row_height: self.row_height.or(fallback.row_height),
            padding: self.padding.or(fallback.padding),
            width: self.width.or(fallback.width),
            offset: self.offset.or(fallback.offset),
        }
    }

    pub fn background(&self) -> Color {
        self.background.unwrap_or_else(shared::surface)
    }

    pub fn border(&self) -> Color {
        self.border.unwrap_or_else(shared::border)
    }

    pub fn faint(&self) -> Color {
        self.faint.unwrap_or_else(shared::muted)
    }

    pub fn highlight(&self) -> Color {
        self.highlight.unwrap_or_else(shared::hover_wash)
    }

    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_surface)
    }

    pub fn row_height(&self) -> f32 {
        self.row_height.map_or_else(
            || ListTokens::current().row_height(),
            |height| (height * telar::control_scale()).round(),
        )
    }

    pub fn padding(&self) -> f32 {
        self.padding
            .map_or_else(shared::panel_pad, |pad| pad * telar::control_scale())
    }

    pub fn width(&self) -> f32 {
        dropdown::width(self.width)
    }

    pub fn offset(&self) -> f32 {
        dropdown::offset(self.offset)
    }
}

/// The tokens of the button a `menu` drops its panel from, which an application's theme supplies through a `#[theme(extension)]` field. The panel is [`MenuTokens`].
///
/// Every field left `None` follows the theme role named beside it, read when the trigger is laid out or painted.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MenuTriggerTokens {
    /// Corner of the trigger. `None` is the theme's `radius_control`.
    pub radius: Option<f32>,
    /// Space between the trigger's edge and its label at the regular control size, scaled with the control size in force. `None` is [`metrics::control_pad_x`](crate::metrics::control_pad_x).
    pub pad_x: Option<f32>,
    /// Width of a bordered trigger's frame. `None` is the theme's `border_width`.
    pub border_width: Option<f32>,
    /// Width of the trigger, unless it is stretched or fitted, at the regular control size and scaled with the control size in force. `None` is 13 ems of the document's text.
    pub width: Option<f32>,
}

impl MenuTriggerTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_control)
    }

    pub fn pad_x(&self) -> f32 {
        self.pad_x
            .map_or_else(shared::control_pad_x, |pad| pad * telar::control_scale())
    }

    pub fn border_width(&self) -> f32 {
        self.border_width.unwrap_or_else(shared::border_width)
    }

    pub fn width(&self) -> f32 {
        dropdown::width(self.width)
    }

    fn geometry() -> dropdown::Geometry {
        let tokens = Self::current();
        dropdown::Geometry {
            radius: tokens.radius(),
            pad_x: tokens.pad_x(),
            border_width: tokens.border_width(),
            width: tokens.width(),
        }
    }
}

/// A button that opens a menu under itself: a labelled trigger, and the panel of rows it drops. Picking a row fires `on_select` with its index and the row's own `on_press`, and closes the menu. Unlike `select`, a menu holds no bound selection: its rows are one-shot actions, or toggles that carry their own state.
#[derive(Props)]
pub struct MenuProps {
    /// The trigger button's label.
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Fired with the index of the row picked on the first panel, among its rows. A submenu's rows report to its own `on_select`.
    #[props(some, default)]
    pub on_select: Option<Rc<dyn Fn(u32)>>,
    /// The colour a bordered trigger is framed in and the rows' washes are tinted with. `Color::TRANSPARENT` (the default) frames it in the theme accent and leaves the theme's washes as they are. A closure so a theme token re-reads on every render.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Take the width the row offers instead of the fixed trigger width — see [`crate::SelectProps::stretch`]. The panel opens at least that wide.
    #[props(default)]
    pub stretch: bool,
    /// Draw the trigger as a field, with the border a `select` carries. Off by default, because a menu is a *button* that happens to open a list, and a button wears no frame until it is pressed.
    ///
    /// A prop and not a decision settled inside the component, because both readings are legitimate: a menu standing alone in a header wants no frame, and one sitting in a row of fields wants to match them.
    #[props(default)]
    pub bordered: bool,
    /// Show the caret that says the trigger opens something. On by default. Without it the label stands centred in the trigger, as a button's does.
    #[props(default = true)]
    pub caret: bool,
    /// Size the trigger to its label instead of the fixed width a column of fields shares. Off by default; the panel it opens keeps its own width.
    #[props(default)]
    pub fit: bool,
    /// This menu's own panel tokens, over the theme's: each field it names wins, and each it leaves `None` is the theme's.
    #[props(default)]
    pub tokens: MenuTokens,
    /// Amends the paint of the trigger — this component's **principal surface**, the thing a caller means when they point at a menu — at rest, hovered and held open. See [`telar::SurfaceStyle`] for when a theme token or [`MenuTriggerTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A button that opens a menu under itself.
pub fn menu(props: MenuProps, children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let MenuProps {
        label,
        on_select,
        color,
        stretch,
        bordered,
        caret,
        fit,
        tokens,
        style,
    } = props;
    let open = signal(false);
    // Opening with the down arrow lands on the first row, as a native menu button does; opening with the mouse highlights nothing.
    let seed: Rc<std::cell::Cell<bool>> = Rc::default();
    // The keys of the panel open now: the trigger answers them, since it lives for the widget's whole life and the panel is rebuilt on every open.
    let held: Rc<std::cell::RefCell<Option<Keys>>> = Rc::default();
    let open_list: RwSignal<Option<FocusId>> = signal(None);
    // Key events are broadcast, so without this a bare Enter would open every menu on the page.
    let trigger_focused = signal(false);
    let toggle: Rc<dyn Fn()> = Rc::new(move || open.set(!open.peek()));
    let face = dropdown::Face {
        label,
        color: color.clone(),
        bordered,
        caret,
        sized_by_root: stretch || fit,
        geometry: MenuTriggerTokens::geometry,
        style,
    };
    let on_key = {
        let (held, seed, toggle) = (held.clone(), seed.clone(), toggle.clone());
        move |key: &Key| -> bool {
            if open.peek() {
                let keys = held.borrow().clone();
                return keys.is_some_and(|keys| keys(key));
            }
            match key {
                Key::Named(NamedKey::ArrowDown | NamedKey::Enter | NamedKey::Space)
                    if trigger_focused.peek() =>
                {
                    seed.set(matches!(key, Key::Named(NamedKey::ArrowDown)));
                    toggle();
                    true
                }
                _ => false,
            }
        }
    };
    let trigger = dropdown::trigger(face, open, toggle)?
        // A control, which is what makes every key above addressable. It answers its own keys, so the default Enter-activates-the-press is stood down.
        .control(Role::Button)
        .has_popup(HasPopup::Menu)
        .expanded(move || open.get())
        .on_focus(move |now| trigger_focused.set(now))
        .on_key(on_key)
        .consumes_keys(move || {
            if open.get() {
                ConsumedKeys::ARROWS | ConsumedKeys::EDGES | ConsumedKeys::ACTIVATION
            } else {
                ConsumedKeys::ARROW_DOWN | ConsumedKeys::ACTIVATION
            }
        })
        .controls(move || open_list.get());
    let trigger_node = trigger.layout_node();
    // `track_layout` returns the signal now and layout fills it in, so the trigger's rect is known by the time the panel is built.
    let trigger_rect = track_layout(trigger_node).expect("trigger node is registered");

    let holder = {
        let held = held.clone();
        dropdown::while_open(open, move || {
            let close: Rc<dyn Fn()> = Rc::new(move || open.set(false));
            let config = Rc::new(Config {
                tokens,
                width: None,
                within: Rect::new(0.0, 0.0, f32::MAX, f32::MAX),
                style: None,
                color: color.clone(),
                close: close.clone(),
            });
            let look = tokens.or(MenuTokens::current());
            let offset = memo(move || tokens.or(MenuTokens::current()).offset());
            let anchoring = dropdown::below(trigger_node, trigger_rect, offset);
            let built = menu_panel::panel(
                Spec {
                    rows: children.clone(),
                    on_select: on_select.clone(),
                    anchoring,
                    width: dropdown::panel_width(
                        look.width(),
                        stretch,
                        trigger_node,
                        &trigger_rect,
                    ),
                    level: Level::Root,
                    seed: seed.take(),
                },
                config,
            )?;
            held.replace(Some(built.keys.clone()));
            open_list.set(Some(built.list));
            let overlay = menu_panel::overlay(built, close, None)?.owned_by(trigger_node);
            Ok(box_item(overlay))
        })?
    };
    effect(move || {
        if !open.get() {
            held.replace(None);
            if open_list.peek().is_some() {
                open_list.set(None);
            }
        }
    });
    dropdown::dismissable(open, trigger_node);
    dropdown::root(trigger, holder, stretch)
}

#[derive(Props)]
/// A menu opened where it was asked for, with no trigger: its rows, where it goes, and what is told when it is done with.
pub struct ContextMenuProps {
    /// Where it was asked for, in surface coordinates: the pointer, for a secondary press.
    #[props(default = (0.0, 0.0))]
    pub at: (f32, f32),
    /// The box it opens under instead of a point, for a menu asked for from the keyboard — the context-menu key, or Shift+F10 (see [`is_context_menu_key`]) — which is the box of the control the keyboard is on ([`focused_rect`]). It opens with the cursor on its first row, as a menu opened from the keyboard does.
    #[props(some, default)]
    pub around: Option<Rect>,
    /// Fired with the index of the row picked on the first panel, among its rows. A submenu's rows report to its own `on_select`.
    #[props(some, default)]
    pub on_select: Option<Rc<dyn Fn(u32)>>,
    /// Run when the menu is done with: a row was picked, Escape was pressed, or the hand went elsewhere. The caller owns whether the menu exists, so this is how it says the answer is in.
    #[props(default = Rc::new(|| {}))]
    pub on_close: Rc<dyn Fn()>,
    /// The least width of each panel in logical px; a panel widens to its widest row. `0.0` (the default) is [`MenuTokens::width`].
    #[props(default)]
    pub width: f32,
    /// The box the panel is kept inside. The window, usually; a pane for a menu that belongs to one.
    #[props(default = Rect::new(0.0, 0.0, f32::MAX, f32::MAX))]
    pub within: Rect,
    /// This menu's own tokens, over the theme's: each field it names wins, and each it leaves `None` is the theme's.
    #[props(default)]
    pub tokens: MenuTokens,
    /// Amends the paint of every panel — this component's **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`MenuTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A menu opened where it was asked for: at the pointer, or under the control the keyboard is on.
///
/// Not [`menu`], which is a labelled button that drops the panel under itself. This one has no trigger at all, because the thing it is about is what was under the pointer; the caller mounts it while it is wanted and takes it out on `on_close`. Its rows are its children — `item`, `separator`, `group`, `submenu` — exactly as a `menu`'s are, and the keyboard goes back to whatever held it when it goes.
pub fn context_menu(
    props: ContextMenuProps,
    children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ContextMenuProps {
        at,
        around,
        on_select,
        on_close,
        width,
        within,
        tokens,
        style,
    } = props;
    let config = Rc::new(Config {
        tokens,
        width: (width > 0.0).then_some(width),
        within,
        style,
        color: Reactive::of(|| Color::TRANSPARENT),
        close: on_close.clone(),
    });
    let offset = memo(move || tokens.or(MenuTokens::current()).offset());
    let anchoring: menu_panel::Anchored = Rc::new(move || {
        let inside = menu_panel::kept_inside(within);
        match around {
            Some(control) => Anchoring::new(control, Placement::Below, inside).gap(offset.get()),
            None => Anchoring::new(Rect::new(at.0, at.1, 0.0, 0.0), Placement::Below, inside),
        }
    });
    let built = menu_panel::panel(
        Spec {
            rows: children,
            on_select,
            anchoring,
            width: config.panel_width(),
            level: Level::Root,
            seed: around.is_some(),
        },
        config,
    )?;
    let keys = built.keys.clone();
    Ok(box_item(menu_panel::overlay(built, on_close, Some(keys))?))
}

/// Whether `key` asks for the context menu of whatever the keyboard is on: the context-menu key, or Shift+F10 on a keyboard without one.
pub fn is_context_menu_key(key: &Key) -> bool {
    match key {
        Key::Named(NamedKey::ContextMenu) => true,
        Key::Named(NamedKey::F10) => telar::modifiers().is_shift,
        _ => false,
    }
}

/// Where the control the keyboard is on is drawn, in surface coordinates: what a menu asked for from the keyboard opens under.
pub fn focused_rect() -> Option<Rect> {
    let node = focus::node_of(focus::current()?)?;
    let laid = track_layout(node)?;
    Some(telar::anchor_rect(node, &laid))
}

#[cfg(test)]
#[path = "menu_test.rs"]
mod tests;
