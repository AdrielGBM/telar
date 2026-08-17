use layout_core::LayoutError;
use renderer_core::Color;
use ui_core::{Children, LayoutItem};

use crate::dropdown;
// Re-exported for the test module below, which reads these via `use super::*` to compute click points.
#[cfg(test)]
use crate::dropdown::{PANEL_WIDTH, ROW_HEIGHT, TRIGGER_HEIGHT, panel_pad};
#[cfg(test)]
use ui_core::{Slots, track_layout};

/// A click-triggered list of action items: a labelled trigger button that opens an anchored list; picking an
/// item fires `on_select` with its index and closes. Unlike `select`, a menu holds no bound selection state —
/// its items are one-shot actions. High-level sugar built on the overlay anchor + click-through primitives;
/// lives in `ui-components`, not the kernel.
pub struct MenuProps {
    /// The trigger button's label.
    pub label: Box<dyn Fn() -> String>,
    /// Fired with the index of the chosen item when it is picked.
    pub on_select: Option<Box<dyn Fn(u32)>>,
    /// Accent colour (trigger border, hover highlight). `Color::TRANSPARENT` (the default) means "unset" and
    /// falls back to the theme accent. A closure so a theme token re-reads on every render.
    pub color: Box<dyn Fn() -> Color>,
    /// Take the width the row offers instead of the fixed trigger width — see [`crate::SelectProps::stretch`].
    pub stretch: bool,
    /// Draw the trigger as a field, with the border a `select` carries. Off by default, because a menu is a
    /// *button* that happens to open a list, and a button wears no frame until it is pressed.
    ///
    /// A prop and not a decision settled inside the component, because both readings are legitimate: a menu
    /// standing alone in a header wants no frame, and one sitting in a row of fields wants to match them.
    pub bordered: bool,
    /// Show the caret that says the trigger opens something. On by default.
    pub caret: bool,
    /// Amends the paint of the trigger — this component's **principal surface**, the thing a caller means
    /// when they point at a menu. See `shared::SurfaceStyle` for why it takes the finished style rather than
    /// naming one property, and for when a theme token is the right instrument instead.
    pub style: Option<Box<dyn Fn(renderer_core::RectStyle) -> renderer_core::RectStyle>>,
}

impl Default for MenuProps {
    fn default() -> Self {
        Self {
            label: Box::new(String::new),
            on_select: None,
            color: Box::new(|| Color::TRANSPARENT),
            stretch: false,
            bordered: false,
            caret: true,
            style: None,
        }
    }
}

// A menu carries no bound selection (`selected: None`), so its rows are one-shot actions: no index is written
// back and no row is highlighted. The reactive `label` closure is handed straight to the dropdown trigger.
pub fn menu(props: MenuProps, children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    dropdown::dropdown(dropdown::Dropdown {
        label: dropdown::TriggerLabel::Fixed(props.label),
        rows: children,
        color: props.color,
        on_pick: props.on_select,
        selected: None,
        stretch: props.stretch,
        bordered: props.bordered,
        caret: props.caret,
        style: props.style,
    })
}

/// A plain row per label, which is what the markup `menu … item label:"…"` compiles to. Enough for the tests
/// that care about the trigger and the panel rather than about what a row can be.
#[cfg(test)]
fn rows(labels: &'static [&'static str]) -> Children {
    Children::new(move || {
        let mut slots = Slots::new();
        for label in labels {
            slots.push(
                None,
                crate::list::item(
                    crate::list::ItemProps {
                        label: Box::new(move || label.to_string()),
                        ..Default::default()
                    },
                    Slots::new(),
                )?,
            );
        }
        Ok(slots)
    })
}

/// The shape of a trigger is the caller's call, not the component's.
///
/// The default splits them by what they are — a menu is a button, a select is a field — but a menu dropped
/// into a row of inputs wants to match them, and there has to be a way to say so that is not editing the
/// catalogue. Guards the props rather than the pixels: what matters is that they *reach*
/// the trigger's paint at all.
#[cfg(test)]
#[test]
fn a_menu_can_be_asked_for_a_field_and_for_no_caret() {
    use renderer_core::DrawCommand;
    use ui_core::{ComponentList, LayoutItem};

    let strokes = |bordered: bool, caret: bool| {
        crate::test_support::fresh_layout_runtime();
        let item = menu(
            MenuProps {
                label: Box::new(|| "File".to_string()),
                bordered,
                caret,
                ..Default::default()
            },
            rows(&["New"]),
        )
        .unwrap();
        // Laid out first: the caret is drawn by a `Canvas`, which has nothing to draw into until it has a rect.
        let root = ui_core::new_container(
            layout_core::LayoutStyle::new().width(300.0).height(80.0),
            &[item.layout_node()],
        )
        .unwrap();
        ui_core::compute_layout(
            root,
            layout_core::AvailableSpace::Definite(300.0),
            layout_core::AvailableSpace::Definite(80.0),
        )
        .unwrap();
        let tree = ComponentList::new(item);
        let cmds = tree.commands().to_vec();
        let bordered_boxes = cmds
            .iter()
            .filter(|c| matches!(c, DrawCommand::Rect { style, .. } if style.stroke.is_some()))
            .count();
        let paths = cmds
            .iter()
            .filter(|c| matches!(c, DrawCommand::Path { .. }))
            .count();
        (bordered_boxes, paths)
    };

    let (plain_border, plain_caret) = strokes(false, true);
    assert_eq!(
        plain_border, 0,
        "a menu is a button, so no frame by default"
    );
    assert_eq!(plain_caret, 1, "and it says it opens something");

    let (asked_border, _) = strokes(true, true);
    assert_eq!(asked_border, 1, "a caller that wants the field gets it");

    let (_, no_caret) = strokes(false, false);
    assert_eq!(no_caret, 0, "and one that wants no caret gets none");
}

/// A caller can restyle the surface without editing the catalogue.
///
/// The pressure this relieves is real: an app wanted its menu trigger squared off, and the only lever the
/// catalogue offered was the theme's radius — which moves every rounded thing in the application. Editing
/// `dropdown.rs` to hold one app's opinion is how a shared component stops being shared.
///
/// The amendment must *compose*, not replace: the component still decides that a bordered trigger wears a
/// stroke, and the caller only says what they came to say.
#[cfg(test)]
#[test]
fn a_caller_can_amend_the_paint_the_trigger_worked_out_for_itself() {
    use renderer_core::{BorderRadius, DrawCommand, ShapeStyle};
    use ui_core::{ComponentList, LayoutItem};

    crate::test_support::fresh_layout_runtime();
    let item = menu(
        MenuProps {
            label: Box::new(|| "File".to_string()),
            bordered: true,
            style: Some(Box::new(|s| {
                s.with_radius(BorderRadius::all(0.0))
                    .with_fill(Color::rgba(1.0, 0.0, 0.0, 1.0))
            })),
            ..Default::default()
        },
        rows(&["New"]),
    )
    .unwrap();
    let root = ui_core::new_container(
        layout_core::LayoutStyle::new().width(300.0).height(80.0),
        &[item.layout_node()],
    )
    .unwrap();
    ui_core::compute_layout(
        root,
        layout_core::AvailableSpace::Definite(300.0),
        layout_core::AvailableSpace::Definite(80.0),
    )
    .unwrap();
    let tree = ComponentList::new(item);
    let trigger = tree
        .commands()
        .iter()
        .find_map(|c| match c {
            DrawCommand::Rect { style, .. } if style.stroke.is_some() => Some(style.clone()),
            _ => None,
        })
        .expect("a bordered trigger is painted");

    assert_eq!(
        trigger.radius,
        BorderRadius::all(0.0),
        "the caller's radius reaches the paint"
    );
    assert_eq!(
        trigger.fill,
        Some(renderer_core::Paint::Solid(Color::rgba(1.0, 0.0, 0.0, 1.0))),
        "and so does their fill"
    );
    assert!(
        trigger.stroke.is_some(),
        "while the component keeps the border it decided a field wears"
    );
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use layout_core::{AvailableSpace, LayoutStyle};
    use platform_core::Event;
    use ui_core::{
        ComponentList, EventResult, compute_layout, dispatch_overlays, relayout_if_dirty,
    };

    use super::*;
    use crate::harness::{press, release, route};

    // Mirror the runner: consult the overlay registry first, then walk the tree only if no overlay
    // consumed the event (the anchored panel routes through the registry, the trigger through the tree).

    /// A compact trigger does not squeeze the panel. `stretch` means "be at least as wide as the control I sit
    /// under", and taking that width outright turned a `File` button into a 40px sheet with one character per
    /// line — every item wrapped down its own column.
    #[test]
    fn a_narrow_filled_trigger_still_opens_a_readable_panel() {
        crate::test_support::fresh_layout_runtime();
        let item = menu(
            MenuProps {
                label: Box::new(|| "File".to_string()),
                stretch: true,
                ..Default::default()
            },
            rows(&["New", "Open…", "Save", "Import STEP…"]),
        )
        .unwrap();
        // A row 44px wide: what a compact menu button gets in a header.
        let root_node = item.layout_node();
        let row = ui_core::new_container(
            LayoutStyle::new().flex_row().width(44.0).height(400.0),
            &[root_node],
        )
        .unwrap();
        let mut tree = ComponentList::new(item);
        compute_layout(
            row,
            AvailableSpace::Definite(44.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let _ = tree.commands();

        route(&mut tree, &press(20.0, 18.0));
        route(&mut tree, &release(20.0, 18.0));
        relayout_if_dirty();
        let _ = tree.commands();

        // The first item's row: it belongs to the panel, so its width is the panel's minus its padding.
        let widths: Vec<f32> = tree
            .commands()
            .iter()
            .filter_map(|c| match c {
                renderer_core::DrawCommand::Rect { rect, .. } => Some(rect.width),
                _ => None,
            })
            .collect();
        assert!(
            widths
                .iter()
                .any(|w| *w >= PANEL_WIDTH - panel_pad() * 2.0 - 0.5),
            "the open panel should be at least {PANEL_WIDTH}px wide, got {widths:?}"
        );
    }

    // Construction: a menu builds headless, lays out (the trigger takes its fixed size), and renders.
    #[test]
    fn builds_and_lays_out() {
        crate::test_support::fresh_layout_runtime();
        let item = menu(
            MenuProps {
                label: Box::new(|| "Actions".to_string()),
                ..Default::default()
            },
            rows(&["Rename", "Duplicate", "Delete"]),
        )
        .unwrap();
        let root_node = item.layout_node();
        let root_rect = track_layout(root_node).unwrap();
        compute_layout(
            root_node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        assert!(
            root_rect.get().height >= TRIGGER_HEIGHT - 0.5,
            "closed menu is at least the trigger tall: {:?}",
            root_rect.get()
        );
        let tree = ComponentList::new(item);
        let _ = tree.commands();
    }

    fn key(named: platform_core::NamedKey) -> Event {
        Event::KeyPressed {
            key: platform_core::Key::Named(named),
            modifiers: platform_core::ModifiersState::default(),
        }
    }

    /// A menu was reachable by mouse and by nothing else: the trigger took no focus, so Tab passed it by, and
    /// the panel answered to no key at all. Radix gives arrows, Home/End and Escape away for free; here every
    /// one of them was absent, which is the difference between a control a keyboard user can operate and one
    /// they cannot.
    #[test]
    fn a_menu_can_be_opened_and_driven_from_the_keyboard() {
        use platform_core::NamedKey;

        crate::test_support::fresh_layout_runtime();
        ui_core::focus::clear();
        let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
        let sink = seen.clone();
        let item = menu(
            MenuProps {
                label: Box::new(|| "Actions".to_string()),
                on_select: Some(Box::new(move |i| sink.set(Some(i)))),
                ..Default::default()
            },
            rows(&["Rename", "Duplicate", "Delete"]),
        )
        .unwrap();
        let root_node = item.layout_node();
        compute_layout(
            root_node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut tree = ComponentList::new(item);

        // Tab reaches the trigger at all, which is what makes the rest addressable.
        ui_core::focus::focus_next();
        assert!(
            ui_core::focus::current().is_some(),
            "the trigger joined the tab order"
        );

        route(&mut tree, &key(NamedKey::ArrowDown));
        relayout_if_dirty();
        // Down from the trigger opens, Down again steps to the second row, Enter commits it.
        route(&mut tree, &key(NamedKey::ArrowDown));
        route(&mut tree, &key(NamedKey::Enter));
        assert_eq!(seen.get(), Some(1), "the highlighted row is what commits");
    }

    /// Escape closes it even though the trigger holds focus. `dispatch_overlays` only dismisses when nothing
    /// is focused — right for a field, which blurs itself first, and wrong here, where the focused thing *is*
    /// the control the menu belongs to.
    #[test]
    fn escape_closes_a_menu_whose_trigger_holds_focus() {
        use platform_core::NamedKey;

        crate::test_support::fresh_layout_runtime();
        ui_core::focus::clear();
        let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
        let sink = seen.clone();
        let item = menu(
            MenuProps {
                label: Box::new(|| "Actions".to_string()),
                on_select: Some(Box::new(move |i| sink.set(Some(i)))),
                ..Default::default()
            },
            rows(&["Rename", "Duplicate"]),
        )
        .unwrap();
        let root_node = item.layout_node();
        compute_layout(
            root_node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut tree = ComponentList::new(item);

        ui_core::focus::focus_next();
        route(&mut tree, &key(NamedKey::ArrowDown));
        relayout_if_dirty();
        route(&mut tree, &key(NamedKey::Escape));
        relayout_if_dirty();

        // Shut: Enter commits nothing, because there is no list to commit from.
        route(&mut tree, &key(NamedKey::Enter));
        assert_eq!(seen.get(), None, "Escape shut it before Enter could pick");
    }

    // Picking an item fires on_select with its index and closes the menu.
    #[test]
    fn selecting_an_item_fires_on_select_and_closes() {
        crate::test_support::fresh_layout_runtime();
        let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
        let sink = seen.clone();
        let item = menu(
            MenuProps {
                label: Box::new(|| "Actions".to_string()),
                on_select: Some(Box::new(move |i| sink.set(Some(i)))),
                ..Default::default()
            },
            rows(&["Rename", "Duplicate", "Delete"]),
        )
        .unwrap();
        // The widget's own root is the parent-less layout host, laid out at the origin: the trigger sits at
        // (0,0) and the panel anchors directly below it, so click points are computable from the constants.
        let root_node = item.layout_node();
        compute_layout(
            root_node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut tree = ComponentList::new(item);
        let _ = tree.commands();

        // A tap on the trigger (through the tree) opens the panel.
        let tx = (PANEL_WIDTH / 2.0) as f64;
        let ty = (TRIGGER_HEIGHT / 2.0) as f64;
        route(&mut tree, &press(tx, ty));
        route(&mut tree, &release(tx, ty));
        relayout_if_dirty();

        // Item index 1 ("Duplicate") sits at the trigger's bottom + panel padding + one row.
        let ox = (PANEL_WIDTH / 2.0) as f64;
        let oy = (TRIGGER_HEIGHT + panel_pad() + ROW_HEIGHT + ROW_HEIGHT / 2.0) as f64;
        route(&mut tree, &press(ox, oy));
        route(&mut tree, &release(ox, oy));

        assert_eq!(
            seen.get(),
            Some(1),
            "picking the second item fires on_select(1)"
        );
        assert_eq!(
            dispatch_overlays(&press(ox, oy)),
            EventResult::Ignored,
            "the menu closes after a pick"
        );
    }
    /// What a menu could not say before, said end to end: a disabled row, a separator, and a heading are all
    /// *in* the list and none of them is a place the keyboard stops.
    ///
    /// The three used to be inexpressible for the same reason — the rows were `Vec<&str>`, and a string
    /// carries no state — and they are testable together now for the same reason: each row builds itself
    /// inside the menu, so it can register what it is rather than being told.
    #[test]
    fn the_keyboard_steps_over_what_it_cannot_commit() {
        use platform_core::NamedKey;

        crate::test_support::fresh_layout_runtime();
        ui_core::focus::clear();
        let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
        let sink = seen.clone();
        // Rename · [disabled] Duplicate · ─── · "Danger" · Delete.
        let structured = Children::new(|| {
            let mut slots = Slots::new();
            let row = |label: &'static str, disabled: bool| {
                crate::list::item(
                    crate::list::ItemProps {
                        label: Box::new(move || label.to_string()),
                        disabled: Box::new(move || disabled),
                        ..Default::default()
                    },
                    Slots::new(),
                )
            };
            slots.push(None, row("Rename", false)?);
            slots.push(None, row("Duplicate", true)?);
            slots.push(None, crate::list::separator()?);
            slots.push(
                None,
                crate::list::group(crate::list::GroupProps {
                    label: Box::new(|| "Danger".to_string()),
                })?,
            );
            slots.push(None, row("Delete", false)?);
            Ok(slots)
        });
        let item = menu(
            MenuProps {
                label: Box::new(|| "Actions".to_string()),
                on_select: Some(Box::new(move |i| sink.set(Some(i)))),
                ..Default::default()
            },
            structured,
        )
        .unwrap();
        compute_layout(
            item.layout_node(),
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut tree = ComponentList::new(item);

        ui_core::focus::focus_next();
        route(&mut tree, &key(NamedKey::ArrowDown));
        relayout_if_dirty();
        // One step down from "Rename" is "Delete" at index 4: the disabled row, the rule and the heading are
        // all passed over rather than stopped on.
        route(&mut tree, &key(NamedKey::ArrowDown));
        route(&mut tree, &key(NamedKey::Enter));
        assert_eq!(
            seen.get(),
            Some(4),
            "three unreachable rows sit between the two that are not"
        );
    }

    /// A disabled row does not commit when it is clicked either, which is the half a keyboard test cannot see.
    #[test]
    fn a_disabled_row_does_not_commit_on_a_tap() {
        crate::test_support::fresh_layout_runtime();
        let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
        let sink = seen.clone();
        let structured = Children::new(|| {
            let mut slots = Slots::new();
            for (label, disabled) in [("Rename", false), ("Duplicate", true)] {
                slots.push(
                    None,
                    crate::list::item(
                        crate::list::ItemProps {
                            label: Box::new(move || label.to_string()),
                            disabled: Box::new(move || disabled),
                            ..Default::default()
                        },
                        Slots::new(),
                    )?,
                );
            }
            Ok(slots)
        });
        let item = menu(
            MenuProps {
                label: Box::new(|| "Actions".to_string()),
                on_select: Some(Box::new(move |i| sink.set(Some(i)))),
                ..Default::default()
            },
            structured,
        )
        .unwrap();
        compute_layout(
            item.layout_node(),
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut tree = ComponentList::new(item);
        let _ = tree.commands();

        let tx = (PANEL_WIDTH / 2.0) as f64;
        let ty = (TRIGGER_HEIGHT / 2.0) as f64;
        route(&mut tree, &press(tx, ty));
        route(&mut tree, &release(tx, ty));
        relayout_if_dirty();

        // The second row, which is the disabled one.
        let oy = (TRIGGER_HEIGHT + panel_pad() + ROW_HEIGHT + ROW_HEIGHT / 2.0) as f64;
        route(&mut tree, &press(tx, oy));
        route(&mut tree, &release(tx, oy));
        assert_eq!(seen.get(), None, "a disabled row commits nothing");
    }

    fn char_key(c: char) -> Event {
        Event::KeyPressed {
            key: platform_core::Key::Char(c),
            modifiers: platform_core::ModifiersState::default(),
        }
    }

    /// Rename · Duplicate · Delete · [disabled] Deploy. Three rows share a first letter and the fourth is out
    /// of reach, which is the whole of what type-ahead has to tell apart.
    fn typeahead_menu(sink: Rc<Cell<Option<u32>>>) -> Box<dyn LayoutItem> {
        let structured = Children::new(|| {
            let mut slots = Slots::new();
            for (label, disabled) in [
                ("Rename", false),
                ("Duplicate", false),
                ("Delete", false),
                ("Deploy", true),
            ] {
                slots.push(
                    None,
                    crate::list::item(
                        crate::list::ItemProps {
                            label: Box::new(move || label.to_string()),
                            disabled: Box::new(move || disabled),
                            ..Default::default()
                        },
                        Slots::new(),
                    )?,
                );
            }
            Ok(slots)
        });
        let item = menu(
            MenuProps {
                label: Box::new(|| "Actions".to_string()),
                on_select: Some(Box::new(move |i| sink.set(Some(i)))),
                ..Default::default()
            },
            structured,
        )
        .unwrap();
        compute_layout(
            item.layout_node(),
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        item
    }

    /// Opens a menu from the keyboard, types `typed`, commits, and reports the index that committed.
    fn typed_pick(typed: &str) -> Option<u32> {
        use platform_core::NamedKey;

        crate::test_support::fresh_layout_runtime();
        ui_core::focus::clear();
        ui_core::reset_keyboard();
        let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
        let mut tree = ComponentList::new(typeahead_menu(seen.clone()));

        ui_core::focus::focus_next();
        route(&mut tree, &key(NamedKey::ArrowDown));
        // The rows are built on this flush, so nothing can be searched until it has run.
        relayout_if_dirty();
        for c in typed.chars() {
            route(&mut tree, &char_key(c));
        }
        route(&mut tree, &key(NamedKey::Enter));
        seen.get()
    }

    /// The plain case, and the one a list of more than a screenful is unusable without: a letter takes the
    /// cursor to the row that starts with it instead of making the user arrow there.
    #[test]
    fn a_typed_letter_moves_the_cursor_to_the_row_it_names() {
        assert_eq!(typed_pick("d"), Some(1), "`d` lands on Duplicate");
    }

    /// Refining holds still. `de` after `d` narrows towards a row rather than asking for the next one, which
    /// is why a multi-character query does not skip where the cursor already is.
    #[test]
    fn a_longer_query_narrows_instead_of_advancing() {
        assert_eq!(typed_pick("de"), Some(2), "`de` narrows past Duplicate");
    }

    /// A repeated letter cycles. `ddd` is not a query — no label could match it — it is "the next row starting
    /// with d", and it is the only way to reach the second of two rows sharing a first letter. The third press
    /// wraps back around, stepping over the disabled `Deploy` on the way: a row the keyboard may not stop on
    /// is not a search result either.
    #[test]
    fn a_repeated_letter_cycles_through_the_rows_that_share_it() {
        assert_eq!(
            typed_pick("dd"),
            Some(2),
            "the second `d` advances to Delete"
        );
        assert_eq!(
            typed_pick("ddd"),
            Some(1),
            "the third wraps past the disabled Deploy, back to Duplicate"
        );
    }

    /// A chord is a command, not a query. Without this, an application-level `Ctrl+S` reaching an open menu
    /// would silently walk its cursor to the first row starting with `s`.
    #[test]
    fn a_modified_character_is_not_a_search() {
        use platform_core::NamedKey;

        crate::test_support::fresh_layout_runtime();
        ui_core::focus::clear();
        ui_core::reset_keyboard();
        let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
        let mut tree = ComponentList::new(typeahead_menu(seen.clone()));

        ui_core::focus::focus_next();
        route(&mut tree, &key(NamedKey::ArrowDown));
        relayout_if_dirty();
        ui_core::observe_keyboard(&Event::ModifiersChanged {
            modifiers: platform_core::ModifiersState {
                is_ctrl: true,
                ..Default::default()
            },
        });
        route(&mut tree, &char_key('d'));
        route(&mut tree, &key(NamedKey::Enter));
        ui_core::reset_keyboard();

        assert_eq!(
            seen.get(),
            Some(0),
            "the cursor stayed on Rename, where opening put it"
        );
    }
}
