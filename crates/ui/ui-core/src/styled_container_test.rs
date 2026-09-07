use crate::context::reset_layout_runtime;
use std::cell::Cell;
use std::rc::Rc;

use layout_core::AvailableSpace;
use platform_core::{PointerButton, PointerSource};
use renderer_core::{Color, ShapeStyle};
use theme_core::{ThemeTokens, set_theme, use_theme};

use super::*;
use platform_core::ScrollDelta;

#[test]
fn box_transform_identity_is_none() {
    let r = Rect {
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
    };
    assert!(
        box_transform(r, 0.0, 1.0, 1.0, 0.0, 0.0).is_none(),
        "an identity transform is nothing to apply"
    );
}

#[test]
fn box_transform_scale_pivots_on_center() {
    let r = Rect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
    };
    // scale_around(2, 2, 50, 50) pins the centre, so e = f = 50 - 2*50 = -50.
    assert_eq!(
        box_transform(r, 0.0, 2.0, 2.0, 0.0, 0.0).unwrap(),
        [2.0, 0.0, 0.0, 2.0, -50.0, -50.0]
    );
}

#[test]
fn box_transform_translate_offsets_origin() {
    let r = Rect {
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
    };
    assert_eq!(
        box_transform(r, 0.0, 1.0, 1.0, 8.0, -4.0).unwrap(),
        [1.0, 0.0, 0.0, 1.0, 8.0, -4.0]
    );
}
use crate::container::Container;
use crate::context::{compute_layout, track_layout};

/// The three halves a control is made of, asserted together because that is the whole reason they are one call: Tab reaches it, Enter fires it, and it says what it is. Nine catalogue components had none of the three while compiling and looking correct, which is what a split API buys you.
#[test]
fn a_control_joins_the_tab_order_answers_enter_and_says_what_it_is() {
    use std::cell::Cell;

    reset_layout_runtime();
    focus::clear();
    let fired: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let sink = fired.clone();
    let mut card = StyledContainer::new(
        LayoutStyle::new().width(80.0).height(30.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(focus::Role::CheckBox)
    .on_press(move || sink.set(sink.get() + 1));
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(80.0),
        AvailableSpace::Definite(30.0),
    )
    .unwrap();

    focus::focus_next();
    assert!(focus::current().is_some(), "Tab reaches it");

    let key = |named| Event::KeyPressed {
        key: Key::Named(named),
        modifiers: platform_core::ModifiersState::default(),
    };
    assert_eq!(card.on_event(&key(NamedKey::Enter)), EventResult::Handled);
    assert_eq!(card.on_event(&key(NamedKey::Space)), EventResult::Handled);
    assert_eq!(fired.get(), 2, "Enter and Space each fire the press");

    let exposed = focus::exposed();
    assert_eq!(exposed.len(), 1);
    assert_eq!(exposed[0].role, focus::Role::CheckBox);
    assert!(exposed[0].enabled, "a control joins the tab order enabled");
}

/// And what it must *not* do. A scrim, a click-away backdrop and a drag surface all take presses, and none of them is a place the keyboard should stop — so a press on its own still buys nothing.
#[test]
fn a_press_handler_alone_is_not_a_control() {
    reset_layout_runtime();
    focus::clear();
    let mut card = StyledContainer::new(
        LayoutStyle::new().width(80.0).height(30.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_press(|| {});
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(80.0),
        AvailableSpace::Definite(30.0),
    )
    .unwrap();

    focus::focus_next();
    assert!(focus::exposed().is_empty(), "it is not a tab stop");
    assert_eq!(
        card.on_event(&Event::KeyPressed {
            key: Key::Named(NamedKey::Enter),
            modifiers: platform_core::ModifiersState::default(),
        }),
        EventResult::Ignored,
        "and Enter is left for whoever else wanted it"
    );
}

fn press(x: f64, y: f64, source: PointerSource) -> Event {
    Event::PointerPressed {
        x,
        y,
        button: PointerButton::Primary,
        source,
    }
}
fn release(x: f64, y: f64, source: PointerSource) -> Event {
    Event::PointerReleased {
        x,
        y,
        button: PointerButton::Primary,
        source,
    }
}

/// **A strip is draggable even where the thing under the pointer is pressable.** A row of tabs that can be clicked *and* dragged into another order is the ordinary shape of a reorderable list, and the press was standing the parent's drag down the instant a child took it — so the threshold was never reached and the strip could only be dragged by its gaps.
///
/// What the child takes is the tap. The stroke is still the box's, and the child's own tap is cancelled by its slop once the stroke has committed to being a drag.
#[test]
fn a_child_that_takes_the_press_does_not_take_the_drag_with_it() {
    reset_layout_runtime();
    let dragged: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let sink = dragged.clone();
    let tab = StyledContainer::new(
        LayoutStyle::new().width(100.0).height(30.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_press(|| {});
    let mut strip = StyledContainer::new(
        LayoutStyle::new().flex_row().width(100.0).height(30.0),
        |_r| RectStyle::default(),
        vec![Box::new(tab)],
    )
    .unwrap()
    .drag_threshold(4.0)
    .on_drag(move |_x, _y| sink.set(sink.get() + 1));
    compute_layout(
        strip.layout_node(),
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(30.0),
    )
    .unwrap();

    strip.on_event(&press(10.0, 15.0, PointerSource::Mouse));
    strip.on_event(&Event::PointerMoved {
        x: 60.0,
        y: 15.0,
        source: PointerSource::Mouse,
    });

    assert!(
        dragged.get() > 0,
        "la pulsación del hijo se llevó el arrastre del padre"
    );
}

/// **And a box that holds the stroke stops it without pretending to drag.** The rule needs a way to be said by something that is not draggable at all — a button in a band that moves the window, a panel over a backdrop that dismisses — and saying it with an `on_drag` that does nothing is a lie about what the widget is.
#[test]
fn a_box_that_holds_the_stroke_keeps_it_from_whatever_contains_it() {
    reset_layout_runtime();
    let moved: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let sink = moved.clone();
    let control = StyledContainer::new(
        LayoutStyle::new().width(60.0).height(30.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_press(|| {})
    .holds_stroke();
    let mut band = StyledContainer::new(
        LayoutStyle::new().flex_row().width(200.0).height(30.0),
        |_r| RectStyle::default(),
        vec![Box::new(control)],
    )
    .unwrap()
    .on_drag(move |_x, _y| sink.set(sink.get() + 1));
    compute_layout(
        band.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(30.0),
    )
    .unwrap();

    band.on_event(&press(30.0, 15.0, PointerSource::Mouse));
    band.on_event(&Event::PointerMoved {
        x: 80.0,
        y: 15.0,
        source: PointerSource::Mouse,
    });
    assert_eq!(moved.get(), 0, "la banda se llevó el trazo del control");

    band.on_event(&release(80.0, 15.0, PointerSource::Mouse));
    band.on_event(&press(150.0, 15.0, PointerSource::Mouse));
    assert!(moved.get() > 0, "y donde no hay control sigue siendo suya");
}

/// **And the innermost drag owns the stroke.** The other half of the rule above: a tab that reorders sits in a band that moves the window, so a press that armed both ran two gestures at once — the tab went nowhere because the window went with it.
#[test]
fn a_drag_inside_another_one_is_the_only_one_that_runs() {
    reset_layout_runtime();
    let (inner, outer) = (Rc::new(Cell::new(0u32)), Rc::new(Cell::new(0u32)));
    let (near, far) = (inner.clone(), outer.clone());
    let tab = StyledContainer::new(
        LayoutStyle::new().width(60.0).height(30.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .drag_threshold(4.0)
    .on_drag(move |_x, _y| near.set(near.get() + 1));
    let mut band = StyledContainer::new(
        LayoutStyle::new().flex_row().width(200.0).height(30.0),
        |_r| RectStyle::default(),
        vec![Box::new(tab)],
    )
    .unwrap()
    .drag_threshold(4.0)
    .on_drag(move |_x, _y| far.set(far.get() + 1));
    compute_layout(
        band.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(30.0),
    )
    .unwrap();

    // On the tab: the tab reorders and the band stays put.
    band.on_event(&press(30.0, 15.0, PointerSource::Mouse));
    band.on_event(&Event::PointerMoved {
        x: 80.0,
        y: 15.0,
        source: PointerSource::Mouse,
    });
    assert!(inner.get() > 0, "la pestaña no se arrastró");
    assert_eq!(outer.get(), 0, "y la banda se fue con ella");

    // Past the tabs, where the band is the only thing there: the band is what moves.
    band.on_event(&release(80.0, 15.0, PointerSource::Mouse));
    band.on_event(&press(150.0, 15.0, PointerSource::Mouse));
    band.on_event(&Event::PointerMoved {
        x: 190.0,
        y: 15.0,
        source: PointerSource::Mouse,
    });
    assert!(outer.get() > 0, "la banda ya no se puede arrastrar");
}

#[test]
fn on_hover_fires_on_enter_and_leave() {
    let seen: Rc<Cell<Option<bool>>> = Rc::new(Cell::new(None));
    let sink = seen.clone();
    reset_layout_runtime();
    let inner = Container::new(LayoutStyle::new().width(100.0).height(100.0), vec![]).unwrap();
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(100.0).height(100.0),
        |_r| RectStyle::default(),
        vec![Box::new(inner)],
    )
    .unwrap()
    .on_hover(move |h| sink.set(Some(h)));
    let node = card.layout_node();
    compute_layout(
        node,
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    card.on_event(&Event::PointerMoved {
        x: 50.0,
        y: 50.0,
        source: PointerSource::Mouse,
    });
    assert_eq!(seen.get(), Some(true), "entering fires on_hover(true)");
    card.on_event(&Event::CursorLeft);
    assert_eq!(seen.get(), Some(false), "leaving fires on_hover(false)");
}

/// The wheel is targeted by where it happened, not by a hover the box had to have seen first — so the very first wheel over a box lands, and one over a sibling never does.
#[test]
fn on_scroll_targets_by_position_and_normalises_lines() {
    let seen: Rc<Cell<(f32, f32)>> = Rc::new(Cell::new((0.0, 0.0)));
    let sink = seen.clone();
    reset_layout_runtime();
    let inner = Container::new(LayoutStyle::new().width(100.0).height(100.0), vec![]).unwrap();
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(100.0).height(100.0),
        |_r| RectStyle::default(),
        vec![Box::new(inner)],
    )
    .unwrap()
    .on_scroll(move |dx, dy| sink.set((dx, dy)));
    let node = card.layout_node();
    compute_layout(
        node,
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    assert_eq!(
        card.on_event(&Event::Scrolled {
            delta: ScrollDelta::Pixels { x: 0.0, y: -30.0 },
            x: 300.0,
            y: 300.0,
        }),
        EventResult::Ignored,
        "a wheel event outside the box is ignored"
    );
    assert_eq!(seen.get(), (0.0, 0.0));

    assert_eq!(
        card.on_event(&Event::Scrolled {
            delta: ScrollDelta::Pixels { x: 0.0, y: -30.0 },
            x: 50.0,
            y: 50.0,
        }),
        EventResult::Handled,
        "a wheel over the box is ours, with no move having preceded it"
    );
    assert_eq!(seen.get(), (0.0, -30.0));

    card.on_event(&Event::Scrolled {
        delta: ScrollDelta::Lines { x: 0.0, y: 3.0 },
        x: 50.0,
        y: 50.0,
    });
    assert_eq!(seen.get(), (0.0, 60.0));
}

#[test]
fn on_key_fires_on_key_press() {
    let count = Rc::new(Cell::new(0u32));
    let sink = count.clone();
    reset_layout_runtime();
    let inner = Container::new(LayoutStyle::new().width(10.0).height(10.0), vec![]).unwrap();
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column(),
        |_r| RectStyle::default(),
        vec![Box::new(inner)],
    )
    .unwrap()
    .on_key(move |_k| sink.set(sink.get() + 1));
    card.on_event(&Event::KeyPressed {
        key: Key::Char('a'),
        modifiers: platform_core::ModifiersState::default(),
    });
    assert_eq!(count.get(), 1, "a key press fires on_key");
}

/// The bug this closes: an app-level shortcut table sharing letters with what the user types. `3` selects a mode until a field has the caret, and `⌘S` saves either way because no editor here wants it.
#[test]
fn a_global_key_handler_stands_aside_while_a_field_has_the_caret() {
    let count = Rc::new(Cell::new(0u32));
    let sink = count.clone();
    reset_layout_runtime();
    focus::clear();
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column(),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_key(move |_k| sink.set(sink.get() + 1));
    let press = |key, modifiers| Event::KeyPressed { key, modifiers };
    let plain = platform_core::ModifiersState::default();
    let meta = platform_core::ModifiersState {
        is_meta: true,
        ..Default::default()
    };

    card.on_event(&press(Key::Char('3'), plain));
    assert_eq!(count.get(), 1, "with nothing focused the shortcut fires");

    let field = focus::next_id();
    focus::register_as(field, focus::FocusKind::TextEntry);
    focus::request(field);
    card.on_event(&press(Key::Char('3'), plain));
    assert_eq!(count.get(), 1, "typing into a field is not a shortcut");
    card.on_event(&press(Key::Char('s'), meta));
    assert_eq!(count.get(), 2, "a chord is a command, not text");
    card.on_event(&press(Key::Named(NamedKey::F5), plain));
    assert_eq!(count.get(), 3, "no editor takes F5");

    focus::unregister(field);
    card.on_event(&press(Key::Char('3'), plain));
    assert_eq!(count.get(), 4, "the caret left, the shortcut is back");
}

/// A focusable that is not a text entry — a button, a tab — leaves the shortcut table alone.
#[test]
fn a_focused_button_does_not_swallow_shortcuts() {
    let count = Rc::new(Cell::new(0u32));
    let sink = count.clone();
    reset_layout_runtime();
    focus::clear();
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column(),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_key(move |_k| sink.set(sink.get() + 1));
    let button = focus::next_id();
    focus::register_as(button, focus::FocusKind::Widget);
    focus::request(button);
    card.on_event(&Event::KeyPressed {
        key: Key::Char('3'),
        modifiers: platform_core::ModifiersState::default(),
    });
    assert_eq!(count.get(), 1);
    focus::unregister(button);
}

/// A box drags from the primary button and no other, until it says otherwise — a slider must not slide on a right-click. A surface with more than one thing to drag opts the others in, and tells them apart through the button registry rather than through a wider callback.
#[test]
fn a_drag_starts_only_from_the_buttons_the_box_asked_for() {
    let seen = Rc::new(Cell::new(0u32));
    let sink = seen.clone();
    reset_layout_runtime();
    let mut plain = StyledContainer::new(
        LayoutStyle::new().width(100.0).height(100.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_drag(move |_x, _y| sink.set(sink.get() + 1));
    compute_layout(
        plain.layout_node(),
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    let press = |button: PointerButton| Event::PointerPressed {
        x: 50.0,
        y: 50.0,
        button,
        source: PointerSource::Mouse,
    };
    plain.on_event(&press(PointerButton::Secondary));
    assert_eq!(seen.get(), 0, "a secondary press is not this box's drag");
    plain.on_event(&press(PointerButton::Primary));
    assert_eq!(seen.get(), 1, "the primary one always is");

    let count = Rc::new(Cell::new(0u32));
    let sink = count.clone();
    reset_layout_runtime();
    let mut viewport = StyledContainer::new(
        LayoutStyle::new().width(100.0).height(100.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_drag(move |_x, _y| sink.set(sink.get() + 1))
    .drag_button(PointerButton::Secondary);
    compute_layout(
        viewport.layout_node(),
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    assert_eq!(
        viewport.on_event(&press(PointerButton::Secondary)),
        EventResult::Handled
    );
    assert_eq!(count.get(), 1, "the box asked for this button");
    assert_eq!(
        viewport.on_event(&Event::PointerReleased {
            x: 60.0,
            y: 60.0,
            button: PointerButton::Secondary,
            source: PointerSource::Mouse,
        }),
        EventResult::Handled
    );
}

/// The registry that tells the buttons apart, since the drag callback reports where the pointer is and not what pressed it.
#[test]
fn the_button_registry_holds_what_is_down() {
    crate::reset_pointer();
    assert!(
        !crate::pointer_buttons().any(),
        "no button is down to begin with"
    );
    crate::observe_pointer(&Event::PointerPressed {
        x: 0.0,
        y: 0.0,
        button: PointerButton::Secondary,
        source: PointerSource::Mouse,
    });
    assert!(
        crate::pointer_buttons().secondary,
        "the secondary button is the one that went down"
    );
    assert!(!crate::pointer_buttons().primary, "and the primary did not");
    // Losing focus is where a reconstructed state goes wrong: the release never comes.
    crate::observe_pointer(&Event::FocusChanged { is_focused: false });
    assert!(
        !crate::pointer_buttons().any(),
        "the release empties the registry"
    );
}

#[test]
fn on_pointer_move_reports_the_position_local_to_the_box() {
    let seen: Rc<Cell<Option<(f32, f32)>>> = Rc::new(Cell::new(None));
    let sink = seen.clone();
    reset_layout_runtime();
    // A 20px spacer above the box, so its origin is not the window's and a raw position would show.
    let spacer = Container::new(LayoutStyle::new().width(100.0).height(20.0), vec![]).unwrap();
    let card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(100.0).height(100.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_pointer_move(move |x, y| sink.set(Some((x, y))));
    let mut root = Container::new(
        LayoutStyle::new().flex_column().width(100.0).height(120.0),
        vec![Box::new(spacer), Box::new(card)],
    )
    .unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(120.0),
    )
    .unwrap();

    root.on_event(&Event::PointerMoved {
        x: 30.0,
        y: 50.0,
        source: PointerSource::Mouse,
    });
    assert_eq!(
        seen.get(),
        Some((30.0, 30.0)),
        "the box starts 20px down, so the y arrives 20 less — as on_drag reports it"
    );

    seen.set(None);
    root.on_event(&Event::PointerMoved {
        x: 300.0,
        y: 300.0,
        source: PointerSource::Mouse,
    });
    assert_eq!(seen.get(), None, "a move outside the box is not its move");
}

#[derive(Clone)]
struct TestTheme(Color);
impl ThemeTokens for TestTheme {
    fn primary(&self) -> Color {
        self.0
    }
    fn on_primary(&self) -> Color {
        Color::WHITE
    }
}

// Regression: setting the global THEME from a press must not re-enter the themed ancestor's render segment while it is on the dispatch stack, mid `borrow_mut`.
#[test]
fn a_theme_button_click_that_repaints_the_tree_does_not_panic() {
    set_theme(TestTheme(Color::RED));

    reset_layout_runtime();
    let btn = StyledContainer::new(
        LayoutStyle::new().width(50.0).height(30.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_press(move || set_theme(TestTheme(Color::GREEN)));
    let btn_node = btn.layout_node();
    let inner = Container::new(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        vec![Box::new(btn)],
    )
    .unwrap();
    let card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        |_r| RectStyle::default().with_fill(use_theme::<TestTheme>().0),
        vec![Box::new(inner)],
    )
    .unwrap();
    let card_node = card.layout_node();
    compute_layout(
        card_node,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    let br = track_layout(btn_node).unwrap().get();

    let mut tree = crate::ComponentList::new(card);
    let _ = tree.commands();

    reactive_core::begin_batch();
    let handled = tree.on_event(&Event::PointerPressed {
        x: (br.x + br.width / 2.0) as f64,
        y: (br.y + br.height / 2.0) as f64,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    });
    if handled == EventResult::Handled {
        reactive_core::end_batch();
        reactive_core::begin_batch();
    }
    let _ = tree.commands();
    reactive_core::end_batch();
}

#[test]
fn on_press_fires_on_tap_not_press() {
    let flag = Rc::new(Cell::new(false));
    let f = flag.clone();
    reset_layout_runtime();
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_press(move || f.set(true));
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    assert_eq!(
        card.on_event(&press(100.0, 50.0, PointerSource::Mouse)),
        EventResult::Handled
    );
    assert!(!flag.get(), "press alone must not fire on_press");
    assert_eq!(
        card.on_event(&release(100.0, 50.0, PointerSource::Mouse)),
        EventResult::Handled
    );
    assert!(flag.get(), "release inside the box fires on_press");
}

// Fired on the next pointer event, since there is no dedicated timer, and it suppresses the tap.
#[test]
fn on_long_press_fires_after_threshold_not_on_quick_release() {
    let long_flag = Rc::new(Cell::new(false));
    let tap_flag = Rc::new(Cell::new(false));
    let lf = long_flag.clone();
    let tf = tap_flag.clone();
    reset_layout_runtime();
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_press(move || tf.set(true))
    .on_long_press(move || lf.set(true));
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    card.on_event(&press(100.0, 50.0, PointerSource::Mouse));
    card.on_event(&release(100.0, 50.0, PointerSource::Mouse));
    assert!(tap_flag.get(), "a quick release still fires on_press");
    assert!(
        !long_flag.get(),
        "a quick release must not fire on_long_press"
    );

    tap_flag.set(false);
    card.on_event(&press(100.0, 50.0, PointerSource::Mouse));
    std::thread::sleep(std::time::Duration::from_millis(550));
    card.on_event(&release(100.0, 50.0, PointerSource::Mouse));
    assert!(
        long_flag.get(),
        "a release after the threshold fires on_long_press"
    );
    assert!(!tap_flag.get(), "a long press must not also fire on_press");
}

fn press_with(x: f64, y: f64, button: PointerButton) -> Event {
    Event::PointerPressed {
        x,
        y,
        button,
        source: PointerSource::Mouse,
    }
}
fn release_with(x: f64, y: f64, button: PointerButton) -> Event {
    Event::PointerReleased {
        x,
        y,
        button,
        source: PointerSource::Mouse,
    }
}

fn laid_out_box() -> StyledContainer {
    reset_layout_runtime();
    StyledContainer::new(
        LayoutStyle::new().width(100.0).height(100.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
}

fn settle(card: &mut StyledContainer) {
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
}

#[test]
fn on_alt_press_reports_which_non_primary_button_tapped() {
    let seen: Rc<Cell<Option<PointerButton>>> = Rc::new(Cell::new(None));
    let sink = seen.clone();
    let mut card = laid_out_box().on_alt_press(move |b| sink.set(Some(b)));
    settle(&mut card);

    card.on_event(&press_with(50.0, 50.0, PointerButton::Secondary));
    card.on_event(&release_with(50.0, 50.0, PointerButton::Secondary));
    assert_eq!(seen.take(), Some(PointerButton::Secondary));

    card.on_event(&press_with(50.0, 50.0, PointerButton::Auxiliary));
    card.on_event(&release_with(50.0, 50.0, PointerButton::Auxiliary));
    assert_eq!(seen.take(), Some(PointerButton::Auxiliary));
}

#[test]
fn a_box_wanting_only_alt_presses_leaves_the_primary_one_alone() {
    let alt = Rc::new(Cell::new(false));
    let sink = alt.clone();
    let mut card = laid_out_box().on_alt_press(move |_| sink.set(true));
    settle(&mut card);

    assert_eq!(
        card.on_event(&press_with(50.0, 50.0, PointerButton::Primary)),
        EventResult::Ignored,
        "a primary press must still fall through to whatever is behind the box"
    );
    card.on_event(&release_with(50.0, 50.0, PointerButton::Primary));
    assert!(!alt.get(), "the primary button is not an alt press");
}

// `None` must leave the box transparent to a right-click, not report the press handled and swallow the context menu behind it.
#[test]
fn maybe_on_alt_press_of_none_lets_a_secondary_press_fall_through() {
    let mut card = laid_out_box().maybe_on_alt_press(None::<fn(PointerButton)>);
    settle(&mut card);

    assert_eq!(
        card.on_event(&press_with(50.0, 50.0, PointerButton::Secondary)),
        EventResult::Ignored
    );
}

#[test]
fn a_document_backend_does_not_cost_a_box_its_press() {
    let tapped = Rc::new(Cell::new(false));
    let sink = tapped.clone();
    let mut card = laid_out_box().on_press(move || sink.set(true));
    settle(&mut card);
    let previous = ui_tree::set_element_capture(true);
    let _ = card.view();
    card.on_event(&press_with(50.0, 50.0, PointerButton::Primary));
    card.on_event(&release_with(50.0, 50.0, PointerButton::Primary));
    ui_tree::set_element_capture(previous);
    assert!(
        tapped.get(),
        "a box that becomes an element is still the box that was pressed"
    );
}

#[test]
fn a_plain_pressable_box_still_ignores_non_primary_buttons() {
    let tapped = Rc::new(Cell::new(false));
    let sink = tapped.clone();
    let mut card = laid_out_box().on_press(move || sink.set(true));
    settle(&mut card);

    assert_eq!(
        card.on_event(&press_with(50.0, 50.0, PointerButton::Secondary)),
        EventResult::Ignored,
        "right-click keeps passing through a box that never asked for it"
    );
    card.on_event(&release_with(50.0, 50.0, PointerButton::Secondary));
    assert!(!tapped.get(), "on_press is a primary-button gesture");
}

#[test]
fn releasing_a_different_button_than_armed_completes_nothing() {
    let seen: Rc<Cell<Option<PointerButton>>> = Rc::new(Cell::new(None));
    let sink = seen.clone();
    let tapped = Rc::new(Cell::new(false));
    let tap_sink = tapped.clone();
    let mut card = laid_out_box()
        .on_press(move || tap_sink.set(true))
        .on_alt_press(move |b| sink.set(Some(b)));
    settle(&mut card);

    card.on_event(&press_with(50.0, 50.0, PointerButton::Secondary));
    card.on_event(&release_with(50.0, 50.0, PointerButton::Primary));
    assert_eq!(
        seen.take(),
        None,
        "the right button armed it, the left cannot complete it"
    );
    assert!(
        !tapped.get(),
        "releasing a button that was never armed completes nothing"
    );
}

#[test]
fn dragging_off_the_box_cancels_an_alt_press() {
    let seen: Rc<Cell<Option<PointerButton>>> = Rc::new(Cell::new(None));
    let sink = seen.clone();
    let mut card = laid_out_box().on_alt_press(move |b| sink.set(Some(b)));
    settle(&mut card);

    card.on_event(&press_with(50.0, 50.0, PointerButton::Secondary));
    card.on_event(&Event::PointerMoved {
        x: 95.0,
        y: 95.0,
        source: PointerSource::Mouse,
    });
    card.on_event(&release_with(95.0, 95.0, PointerButton::Secondary));
    assert_eq!(
        seen.take(),
        None,
        "travel past the tap slop cancels an alt press just as it cancels a tap"
    );
}

#[test]
fn inner_button_press_wins_over_box() {
    let card_flag = Rc::new(Cell::new(false));
    let btn_flag = Rc::new(Cell::new(false));
    let cf = card_flag.clone();
    let bf = btn_flag.clone();
    reset_layout_runtime();
    let btn = StyledContainer::new(
        LayoutStyle::new().width(50.0).height(30.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_press(move || bf.set(true));
    let btn_node = btn.layout_node();
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        |_r| RectStyle::default(),
        vec![Box::new(btn)],
    )
    .unwrap()
    .on_press(move || cf.set(true));
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    let br = track_layout(btn_node).unwrap().get();
    let (cx, cy) = (
        (br.x + br.width / 2.0) as f64,
        (br.y + br.height / 2.0) as f64,
    );
    card.on_event(&press(cx, cy, PointerSource::Mouse));
    card.on_event(&release(cx, cy, PointerSource::Mouse));
    assert!(btn_flag.get(), "the inner button should fire");
    assert!(
        !card_flag.get(),
        "the box on_press must not fire when a child handled the press"
    );
}

/// A ring is not another state but a different question — where the keyboard is going — so it composes with whichever state won instead of replacing it. A hovered box that lost its ring would hide that answer exactly when the user reached for the mouse.
#[test]
fn a_focus_ring_is_drawn_over_the_state_that_won_not_instead_of_it() {
    reset_layout_runtime();
    let hover_fill = Color::rgba(0.9, 0.9, 0.9, 1.0);
    let ring = Border::uniform(Color::rgba(0.0, 0.4, 1.0, 1.0), 2.0);
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        |_r| RectStyle::default().with_fill(Color::rgba(0.1, 0.1, 0.1, 1.0)),
        vec![],
    )
    .unwrap()
    .hover_style(move |_r| RectStyle::default().with_fill(hover_fill))
    .focus_style(move |_r| RectStyle {
        border: Some(ring),
        ..RectStyle::default()
    });
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    let id = card.focusable.id.expect("a ring makes the box focusable");
    focus::request(id);
    card.on_event(&Event::PointerMoved {
        x: 100.0,
        y: 50.0,
        source: PointerSource::Mouse,
    });

    let painted = rect_style(&card.view()).expect("the box paints a rect");
    assert_eq!(
        painted.fill,
        Some(renderer_core::Paint::Solid(hover_fill)),
        "the hover fill survives the ring"
    );
    assert_eq!(painted.border, Some(ring), "and the ring is drawn over it");
    focus::release(id);
}

/// `:focus-visible`, which CSS spent years arriving at: a ring on every click is noise, and the ring drawn anyway is why so many stylesheets turned outlines off altogether and took the keyboard's only cue with them. Focus taken by a tap shows none; focus taken any other way does.
#[test]
fn a_tap_takes_focus_without_drawing_a_ring() {
    reset_layout_runtime();
    let ring = Border::uniform(Color::rgba(0.0, 0.4, 1.0, 1.0), 2.0);
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .focus_style(move |_r| RectStyle {
        border: Some(ring),
        ..RectStyle::default()
    });
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    let id = card.focusable.id.expect("a ring makes the box focusable");

    card.on_event(&press(100.0, 50.0, PointerSource::Mouse));
    assert!(focus::is_focused(id), "the tap did take focus");
    assert_eq!(
        rect_style(&card.view()).and_then(|s| s.border),
        None,
        "but drew no ring for it"
    );

    focus::request(id);
    assert_eq!(rect_style(&card.view()).and_then(|s| s.border), Some(ring));
    focus::release(id);
}

/// The bug this exists to make unwritable, taken from a real port: a control the application had already disabled still lit up with the accent under the pointer and still showed a hand cursor, because the author remembered to guard the callback and the tint but not the hover and the cursor. Three places to remember is three places to get wrong, so the box reads one flag and closes all of them.
#[test]
fn a_disabled_box_neither_lights_up_nor_fires() {
    reset_layout_runtime();
    let presses = Rc::new(Cell::new(0u32));
    let sink = presses.clone();
    let enabled = signal(false);
    let flag = enabled;
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        |_r| RectStyle::default().with_fill(Color::rgba(0.1, 0.1, 0.1, 1.0)),
        vec![],
    )
    .unwrap()
    .hover_style(|_r| RectStyle::default().with_fill(Color::rgba(0.9, 0.9, 0.9, 1.0)))
    .on_press(move || sink.set(sink.get() + 1))
    .disabled(move || !flag.get());
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    let base = fill_color(&card.view());
    card.on_event(&Event::PointerMoved {
        x: 100.0,
        y: 50.0,
        source: PointerSource::Mouse,
    });
    assert_eq!(
        fill_color(&card.view()),
        base,
        "a disabled box does not take the hover paint"
    );
    card.on_event(&press(100.0, 50.0, PointerSource::Mouse));
    card.on_event(&release(100.0, 50.0, PointerSource::Mouse));
    assert_eq!(presses.get(), 0, "and its press callback never fires");

    enabled.set(true);
    card.on_event(&Event::PointerMoved {
        x: 100.0,
        y: 50.0,
        source: PointerSource::Mouse,
    });
    assert_ne!(fill_color(&card.view()), base, "now it hovers");
    card.on_event(&press(100.0, 50.0, PointerSource::Mouse));
    card.on_event(&release(100.0, 50.0, PointerSource::Mouse));
    assert_eq!(presses.get(), 1);
}

/// The shape a `surface_local!` world was supposed to be unable to survive: a style closure reading the very rect the layout pass that runs it is about to write. `styled_by` makes it reachable from any widget, since `style()` is an arbitrary closure the author wrote.
///
/// It settles instead of panicking, and the reason is worth pinning: `compute_layout` collects the `(signal, rect)` updates *while* holding the layout-runtime borrow and applies them only after releasing it, so the flush that re-runs this closure never re-enters a live borrow. The remaining failure mode of this shape is a re-layout cycle, which has its own named assert.
#[test]
fn a_style_effect_that_reads_the_rect_its_own_layout_pass_just_wrote_settles_instead_of_panicking()
{
    reset_layout_runtime();
    let card = StyledContainer::new(
        LayoutStyle::new().width(200.0).height(100.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap();
    let node = card.layout_node();
    let seen = track_layout(node).expect("the container registers a rect signal");
    let settled = seen;
    let runs = Rc::new(Cell::new(0u32));
    let counted = runs.clone();
    let card = card.styled_by(move || {
        counted.set(counted.get() + 1);
        let width = seen.get().width;
        LayoutStyle::new()
            .width(200.0)
            .height((width * 0.5).max(1.0))
    });

    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    assert!(runs.get() >= 1, "the style closure ran");
    assert_eq!(
        settled.peek().height,
        100.0,
        "and the rect it derives itself from came to rest instead of running away"
    );
}

/// The other half of the port's bug, and the one that used to be a wall rather than an oversight: `cursor:` compiles from a literal and never passed through the signal path, so `cursor:$enabled` was not expressible at all. It does not need to be — the box suppresses the shape while disabled, so the attribute stays a literal and the framework answers the question.
#[test]
fn a_disabled_box_does_not_claim_the_pointer_shape() {
    use platform_core::take_window_commands;

    reset_layout_runtime();
    let enabled = signal(false);
    let flag = enabled;
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .cursor(Cursor::Pointer)
    .disabled(move || !flag.get());
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    let over = Event::PointerMoved {
        x: 100.0,
        y: 50.0,
        source: PointerSource::Mouse,
    };
    let _ = take_window_commands();
    card.on_event(&over);
    assert!(
        take_window_commands().is_empty(),
        "a disabled box asks for no cursor at all"
    );

    enabled.set(true);
    card.on_event(&over);
    assert!(
        take_window_commands()
            .iter()
            .any(|c| matches!(c, WindowCommand::SetCursor(Cursor::Pointer))),
        "and asks for it again once it can be used"
    );

    enabled.set(false);
    card.on_event(&over);
    assert!(
        take_window_commands()
            .iter()
            .any(|c| matches!(c, WindowCommand::SetCursor(Cursor::Default))),
        "the shape is given back when the box stops accepting the pointer"
    );
}

/// Disabling a box while the pointer is inside it has to take back what it was already showing — nothing else will, since the pointer never leaves and the box stops accepting the moves that would settle it.
#[test]
fn disabling_a_hovered_box_takes_the_hover_back() {
    reset_layout_runtime();
    let enabled = signal(true);
    let flag = enabled;
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        |_r| RectStyle::default().with_fill(Color::rgba(0.1, 0.1, 0.1, 1.0)),
        vec![],
    )
    .unwrap()
    .hover_style(|_r| RectStyle::default().with_fill(Color::rgba(0.9, 0.9, 0.9, 1.0)))
    .disabled(move || !flag.get());
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    let base = fill_color(&card.view());
    let moved = Event::PointerMoved {
        x: 100.0,
        y: 50.0,
        source: PointerSource::Mouse,
    };
    card.on_event(&moved);
    assert_ne!(fill_color(&card.view()), base, "hovered while enabled");

    enabled.set(false);
    card.on_event(&moved);
    assert_eq!(
        fill_color(&card.view()),
        base,
        "the highlight goes with the ability to act on it"
    );
}

/// The disabled paint wins over the pressed one, which already won over hover — so a box cannot be shown mid-press and unusable at the same time.
#[test]
fn the_disabled_paint_wins_over_every_other_state() {
    reset_layout_runtime();
    let off = Color::rgba(0.5, 0.5, 0.5, 1.0);
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        |_r| RectStyle::default().with_fill(Color::rgba(0.1, 0.1, 0.1, 1.0)),
        vec![],
    )
    .unwrap()
    .hover_style(|_r| RectStyle::default().with_fill(Color::rgba(0.9, 0.9, 0.9, 1.0)))
    .active_style(|_r| RectStyle::default().with_fill(Color::rgba(0.7, 0.7, 0.7, 1.0)))
    .disabled_style(move |_r| RectStyle::default().with_fill(off))
    .disabled(|| true);
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    card.on_event(&Event::PointerMoved {
        x: 100.0,
        y: 50.0,
        source: PointerSource::Mouse,
    });
    card.on_event(&press(100.0, 50.0, PointerSource::Mouse));
    assert_eq!(fill_color(&card.view()), off);
}

/// `disabled` on a region means the region, as an HTML `fieldset` does: a wrapper with no handlers of its own still has to stop the pointer reaching what is inside it, or a disabled panel is disabled only in the places nobody put a control.
#[test]
fn a_disabled_wrapper_shields_its_children() {
    reset_layout_runtime();
    let presses = Rc::new(Cell::new(0u32));
    let sink = presses.clone();
    let inner = StyledContainer::new(
        LayoutStyle::new().width(200.0).height(100.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_press(move || sink.set(sink.get() + 1));
    let mut wrapper = StyledContainer::new(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        |_r| RectStyle::default(),
        vec![Box::new(inner)],
    )
    .unwrap()
    .disabled(|| true);
    compute_layout(
        wrapper.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    wrapper.on_event(&press(100.0, 50.0, PointerSource::Mouse));
    wrapper.on_event(&release(100.0, 50.0, PointerSource::Mouse));
    assert_eq!(presses.get(), 0);
}

#[test]
fn hover_style_swaps_on_mouse_move() {
    reset_layout_runtime();
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        |_r| RectStyle::default().with_fill(Color::rgba(0.1, 0.1, 0.1, 1.0)),
        vec![],
    )
    .unwrap()
    .hover_style(|_r| RectStyle::default().with_fill(Color::rgba(0.9, 0.9, 0.9, 1.0)));
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    let normal = fill_color(&card.view());
    card.on_event(&Event::PointerMoved {
        x: 100.0,
        y: 50.0,
        source: PointerSource::Mouse,
    });
    let hovered = fill_color(&card.view());
    assert_ne!(normal, hovered, "hover should swap the fill");

    card.on_event(&Event::PointerMoved {
        x: 9999.0,
        y: 9999.0,
        source: PointerSource::Mouse,
    });
    assert_eq!(
        fill_color(&card.view()),
        normal,
        "leaving the box restores the base fill"
    );
}

// Touch has no "pointer left", so a tap must leave no stuck hover style.
#[test]
fn touch_move_does_not_set_hover() {
    reset_layout_runtime();
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        |_r| RectStyle::default().with_fill(Color::rgba(0.1, 0.1, 0.1, 1.0)),
        vec![],
    )
    .unwrap()
    .hover_style(|_r| RectStyle::default().with_fill(Color::rgba(0.9, 0.9, 0.9, 1.0)));
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    let normal = fill_color(&card.view());
    card.on_event(&Event::PointerMoved {
        x: 100.0,
        y: 50.0,
        source: PointerSource::Touch { id: 1 },
    });
    assert_eq!(
        fill_color(&card.view()),
        normal,
        "a touch move must not trigger hover"
    );
}

#[test]
fn active_style_swaps_on_press_and_clears_on_release() {
    reset_layout_runtime();
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        |_r| RectStyle::default().with_fill(Color::rgba(0.1, 0.1, 0.1, 1.0)),
        vec![],
    )
    .unwrap()
    .active_style(|_r| RectStyle::default().with_fill(Color::rgba(0.5, 0.5, 0.5, 1.0)));
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    let normal = fill_color(&card.view());
    card.on_event(&press(100.0, 50.0, PointerSource::Mouse));
    assert_ne!(
        normal,
        fill_color(&card.view()),
        "press swaps to the active fill"
    );
    card.on_event(&release(100.0, 50.0, PointerSource::Mouse));
    assert_eq!(
        fill_color(&card.view()),
        normal,
        "release restores the base fill"
    );
}

#[test]
fn active_style_takes_precedence_over_hover() {
    reset_layout_runtime();
    let hover = Color::rgba(0.9, 0.9, 0.9, 1.0);
    let active = Color::rgba(0.4, 0.4, 0.4, 1.0);
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        |_r| RectStyle::default().with_fill(Color::rgba(0.1, 0.1, 0.1, 1.0)),
        vec![],
    )
    .unwrap()
    .hover_style(move |_r| RectStyle::default().with_fill(hover))
    .active_style(move |_r| RectStyle::default().with_fill(active));
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    card.on_event(&Event::PointerMoved {
        x: 100.0,
        y: 50.0,
        source: PointerSource::Mouse,
    });
    assert_eq!(
        fill_color(&card.view()),
        hover,
        "hovering shows the hover fill"
    );
    card.on_event(&press(100.0, 50.0, PointerSource::Mouse));
    assert_eq!(
        fill_color(&card.view()),
        active,
        "pressing while hovered shows the active fill (precedence)"
    );
    card.on_event(&release(100.0, 50.0, PointerSource::Mouse));
    assert_eq!(
        fill_color(&card.view()),
        hover,
        "releasing inside falls back to the hover fill"
    );
}

#[test]
fn active_style_clears_when_press_drags_off() {
    reset_layout_runtime();
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        |_r| RectStyle::default().with_fill(Color::rgba(0.1, 0.1, 0.1, 1.0)),
        vec![],
    )
    .unwrap()
    .active_style(|_r| RectStyle::default().with_fill(Color::rgba(0.5, 0.5, 0.5, 1.0)));
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    let normal = fill_color(&card.view());
    card.on_event(&press(100.0, 50.0, PointerSource::Mouse));
    assert_ne!(normal, fill_color(&card.view()), "press activates");
    card.on_event(&Event::PointerMoved {
        x: 9999.0,
        y: 9999.0,
        source: PointerSource::Mouse,
    });
    assert_eq!(
        fill_color(&card.view()),
        normal,
        "dragging off the box clears the pressed state"
    );
}

fn rect_style(view: &RenderNode) -> Option<RectStyle> {
    let RenderNode::Group { children, .. } = view else {
        return None;
    };
    match children.first() {
        Some(RenderNode::Primitive(renderer_core::DrawCommand::Rect { style, .. })) => {
            Some(**style)
        }
        _ => None,
    }
}

/// The elements a view emits, outermost first, as `(id, css)`.
fn elements(view: &RenderNode) -> Vec<(u64, String)> {
    let mut out = Vec::new();
    collect_elements(view, &mut out);
    out
}

fn collect_elements(node: &RenderNode, out: &mut Vec<(u64, String)>) {
    match node {
        RenderNode::Element { element, children } => {
            out.push((element.id.0, element.layout.to_string()));
            for child in children.iter() {
                collect_elements(child, out);
            }
        }
        RenderNode::Group { children }
        | RenderNode::Transform { children, .. }
        | RenderNode::Clip { children, .. }
        | RenderNode::Layer { children, .. }
        | RenderNode::Overlay { children } => {
            for child in children.iter() {
                collect_elements(child, out);
            }
        }
        _ => {}
    }
}

/// A document backend is handed what the box *asked layout for*, not the rect layout produced — that is what lets the browser lay it out itself. Checked here, with no browser in sight, which is the whole reason capture is a flag and not a build.
#[test]
fn a_captured_box_carries_the_css_it_asked_for() {
    reset_layout_runtime();
    let card = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .width(300.0)
            .padding_all(24.0)
            .gap(8.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap();

    let was = ui_tree::set_element_capture(true);
    let captured = elements(&card.view());
    ui_tree::set_element_capture(was);
    assert!(!ui_tree::element_capture(), "the flag is restored");

    assert_eq!(captured.len(), 1, "one box, one element: {captured:?}");
    let css = &captured[0].1;
    for declaration in [
        "display:flex",
        "flex-direction:row",
        "width:300px",
        "padding:24px",
        "gap:8px",
    ] {
        assert!(
            css.contains(declaration),
            "{declaration} missing from {css}"
        );
    }
}

#[test]
fn a_box_emits_no_element_while_capture_is_off() {
    reset_layout_runtime();
    let card = StyledContainer::new(LayoutStyle::new(), |_r| RectStyle::default(), vec![]).unwrap();
    assert!(
        elements(&card.view()).is_empty(),
        "a desktop build pays for none of this"
    );
}

#[test]
fn nested_boxes_nest_their_elements() {
    reset_layout_runtime();
    let inner = StyledContainer::new(
        LayoutStyle::new().height(10.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap();
    let inner_node = inner.layout_node();
    let outer = StyledContainer::new(
        LayoutStyle::new().flex_column(),
        |_r| RectStyle::default(),
        vec![Box::new(inner)],
    )
    .unwrap();
    let outer_node = outer.layout_node();

    let was = ui_tree::set_element_capture(true);
    // The child is behind a segment boundary, so the parent's own view holds one element and names it.
    let captured = elements(&outer.view());
    ui_tree::set_element_capture(was);

    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].0, u64::from(outer_node));
    assert_ne!(u64::from(inner_node), u64::from(outer_node));
}

fn fill_color(view: &RenderNode) -> Color {
    let group = match view {
        RenderNode::Group { children, .. } => children,
        _ => panic!("expected Group"),
    };
    if let RenderNode::Primitive(renderer_core::DrawCommand::Rect { style, .. }) = &group[0] {
        if let Some(renderer_core::Paint::Solid(c)) = style.fill {
            return c;
        }
    }
    panic!("expected a solid-fill background rect");
}

#[test]
fn scroll_drag_does_not_press_box() {
    let flag = Rc::new(Cell::new(false));
    let f = flag.clone();
    reset_layout_runtime();
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(200.0).height(200.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_press(move || f.set(true));
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let touch = PointerSource::Touch { id: 1 };
    card.on_event(&press(50.0, 20.0, touch.clone()));
    card.on_event(&Event::PointerMoved {
        x: 50.0,
        y: 120.0, // > TAP_SLOP away
        source: touch.clone(),
    });
    card.on_event(&release(50.0, 120.0, touch));
    assert!(!flag.get(), "a scroll drag over the box must not press it");
}

#[test]
fn on_drag_reports_press_then_moves_until_release() {
    use std::cell::RefCell;
    let seen: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    reset_layout_runtime();
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(200.0).height(200.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_drag(move |x, y| sink.borrow_mut().push((x, y)));
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let moved = |x: f64, y: f64| Event::PointerMoved {
        x,
        y,
        source: PointerSource::Mouse,
    };
    card.on_event(&press(40.0, 40.0, PointerSource::Mouse));
    card.on_event(&moved(80.0, 90.0));
    card.on_event(&moved(400.0, 400.0)); // outside the box: drag still tracks
    card.on_event(&release(400.0, 400.0, PointerSource::Mouse));
    card.on_event(&moved(10.0, 10.0)); // after release: no longer dragging

    assert_eq!(
        *seen.borrow(),
        vec![(40.0, 40.0), (80.0, 90.0), (400.0, 400.0)],
        "drag reports the press point then each move until release"
    );
}

/// A click and a drag on the same button stop overlapping once a threshold is set: below it the stroke is only a press, above it only a drag. A viewport is the case — a click picks what is under it and a drag orbits — and without this a one-pixel wobble did both.
#[test]
fn a_threshold_splits_a_click_from_a_drag_on_the_same_button() {
    use std::cell::Cell;
    use std::cell::RefCell;

    let build = || {
        let clicks: Rc<Cell<u32>> = Rc::new(Cell::new(0));
        let drags: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
        reset_layout_runtime();
        let (c, d) = (clicks.clone(), drags.clone());
        let card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(200.0).height(200.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .drag_threshold(4.0)
        .on_press(move || c.set(c.get() + 1))
        .on_drag(move |x, y| d.borrow_mut().push((x, y)));
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();
        (card, clicks, drags)
    };
    let moved = |x: f64, y: f64| Event::PointerMoved {
        x,
        y,
        source: PointerSource::Mouse,
    };

    let (mut card, clicks, drags) = build();
    card.on_event(&press(40.0, 40.0, PointerSource::Mouse));
    card.on_event(&moved(41.0, 40.0));
    card.on_event(&release(41.0, 40.0, PointerSource::Mouse));
    assert_eq!(clicks.get(), 1, "the click survives the wobble");
    assert!(drags.borrow().is_empty(), "and nothing was dragged");

    let (mut card, clicks, drags) = build();
    card.on_event(&press(40.0, 40.0, PointerSource::Mouse));
    card.on_event(&moved(90.0, 40.0));
    card.on_event(&release(90.0, 40.0, PointerSource::Mouse));
    assert_eq!(clicks.get(), 0, "a drag is not also a click");
    assert_eq!(*drags.borrow(), vec![(90.0, 40.0)]);
}

// Regression: a release outside the widget must still end the drag. The parent's release path position-filters presses, so it must broadcast to the dragging child anyway.
#[test]
fn drag_released_outside_bounds_ends_via_parent_dispatch() {
    use std::cell::RefCell;
    let seen: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    reset_layout_runtime();
    let child = StyledContainer::new(
        LayoutStyle::new().width(100.0).height(100.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_drag(move |x, y| sink.borrow_mut().push((x, y)));
    let mut parent = Container::new(
        LayoutStyle::new().flex_column().width(300.0).height(300.0),
        vec![Box::new(child)],
    )
    .unwrap();
    compute_layout(
        parent.layout_node(),
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(300.0),
    )
    .unwrap();

    let moved = |x: f64, y: f64| Event::PointerMoved {
        x,
        y,
        source: PointerSource::Mouse,
    };
    parent.on_event(&press(50.0, 50.0, PointerSource::Mouse));
    parent.on_event(&moved(250.0, 250.0));
    parent.on_event(&release(250.0, 250.0, PointerSource::Mouse));
    parent.on_event(&moved(60.0, 60.0));
    assert_eq!(
        *seen.borrow(),
        vec![(50.0, 50.0), (250.0, 250.0)],
        "drag ended on the outside release; the post-release move must not fire"
    );
}

#[test]
fn on_drag_end_fires_once_with_the_release_position() {
    use std::cell::RefCell;
    let ends: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = ends.clone();
    reset_layout_runtime();
    let mut card = StyledContainer::new(
        LayoutStyle::new().width(100.0).height(100.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_drag_end(move |x, y| sink.borrow_mut().push((x, y)));
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    let moved = |x: f64, y: f64| Event::PointerMoved {
        x,
        y,
        source: PointerSource::Mouse,
    };
    card.on_event(&moved(10.0, 10.0));
    assert!(ends.borrow().is_empty(), "a move with no drag ends nothing");

    card.on_event(&press(20.0, 20.0, PointerSource::Mouse));
    card.on_event(&moved(70.0, 30.0));
    assert!(ends.borrow().is_empty(), "still dragging");
    card.on_event(&release(90.0, 40.0, PointerSource::Mouse));
    assert_eq!(
        *ends.borrow(),
        vec![(90.0, 40.0)],
        "the release position, not the last move — a drag can end past it"
    );

    card.on_event(&release(95.0, 45.0, PointerSource::Mouse));
    assert_eq!(ends.borrow().len(), 1);
}

/// The pointer reaching the edge of the window is not the end of the gesture, and treating it as one is what makes an orbit stop dead against the border of a viewport that fills its window. The drag was armed by a press this widget took; it ends when that press is released, or when the window loses the focus that would have carried the release ([`losing_window_focus_ends_a_live_drag`]).
#[test]
fn a_drag_survives_the_cursor_leaving_the_window() {
    use std::cell::RefCell;
    let moves: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
    let ends: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
    let move_sink = moves.clone();
    let end_sink = ends.clone();
    reset_layout_runtime();
    let mut card = StyledContainer::new(
        LayoutStyle::new().width(100.0).height(100.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_drag(move |x, y| move_sink.borrow_mut().push((x, y)))
    .on_drag_end(move |x, y| end_sink.borrow_mut().push((x, y)));
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    card.on_event(&press(20.0, 20.0, PointerSource::Mouse));
    card.on_event(&Event::PointerMoved {
        x: 60.0,
        y: 25.0,
        source: PointerSource::Mouse,
    });
    card.on_event(&Event::CursorLeft);
    assert!(
        ends.borrow().is_empty(),
        "leaving the window does not finish the drag"
    );

    // Past the border the coordinates go negative, which is what a local drag reports outside the bounds.
    card.on_event(&Event::PointerMoved {
        x: -15.0,
        y: 25.0,
        source: PointerSource::Mouse,
    });
    assert_eq!(
        moves.borrow().last().copied(),
        Some((-15.0, 25.0)),
        "the drag is still reporting after the pointer left"
    );

    card.on_event(&release(-15.0, 25.0, PointerSource::Mouse));
    assert_eq!(
        *ends.borrow(),
        vec![(-15.0, 25.0)],
        "the release is what ends it, wherever it lands"
    );
}

/// The other half of [`a_drag_survives_the_cursor_leaving_the_window`], and the reason the two have to land together: a window that loses focus never sends the release for what was held, and Alt-Tab with a button down never crosses the border. Before `CursorLeft` stopped ending drags this was latent — it only looked safe because leaving was aggressive enough to usually coincide.
#[test]
fn losing_window_focus_ends_a_live_drag() {
    use std::cell::RefCell;
    let ends: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = ends.clone();
    reset_layout_runtime();
    let mut card = StyledContainer::new(
        LayoutStyle::new().width(100.0).height(100.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_drag_end(move |x, y| sink.borrow_mut().push((x, y)));
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    card.on_event(&press(20.0, 20.0, PointerSource::Mouse));
    card.on_event(&Event::PointerMoved {
        x: 60.0,
        y: 25.0,
        source: PointerSource::Mouse,
    });
    card.on_event(&Event::FocusChanged { is_focused: false });
    assert_eq!(
        *ends.borrow(),
        vec![(60.0, 25.0)],
        "the last position the drag reached, since the loss carries none of its own"
    );

    card.on_event(&Event::FocusChanged { is_focused: true });
    card.on_event(&Event::PointerMoved {
        x: 70.0,
        y: 30.0,
        source: PointerSource::Mouse,
    });
    assert_eq!(ends.borrow().len(), 1);
}

#[test]
fn on_drag_end_works_without_an_on_drag() {
    use std::cell::RefCell;
    let ends: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = ends.clone();
    reset_layout_runtime();
    let mut card = StyledContainer::new(
        LayoutStyle::new().width(100.0).height(100.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_drag_end(move |x, y| sink.borrow_mut().push((x, y)));
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    card.on_event(&press(10.0, 10.0, PointerSource::Mouse));
    card.on_event(&release(80.0, 10.0, PointerSource::Mouse));
    assert_eq!(*ends.borrow(), vec![(80.0, 10.0)]);
}

#[test]
fn on_focus_fires_on_gain_and_loss() {
    use std::cell::RefCell;
    let seen: Rc<RefCell<Vec<bool>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    reset_layout_runtime();
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(100.0).height(100.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_focus(move |f| sink.borrow_mut().push(f));
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    card.on_event(&press(50.0, 50.0, PointerSource::Mouse)); // tap focuses → on_focus(true)
    crate::focus::clear(); // → on_focus(false)
    assert_eq!(
        *seen.borrow(),
        vec![true, false],
        "on_focus fires true on gain then false on loss"
    );
}

#[test]
fn maybe_on_focus_of_none_does_not_join_the_tab_order() {
    reset_layout_runtime();
    focus::clear();
    let card = StyledContainer::new(
        LayoutStyle::new().width(80.0).height(30.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .maybe_on_focus(None::<fn(bool)>);
    assert!(card.focusable.id.is_none(), "no handler, no focus id");

    focus::focus_next();
    assert!(focus::exposed().is_empty(), "and it is not a tab stop");
}

#[test]
fn maybe_on_focus_of_some_fires_like_on_focus() {
    use std::cell::RefCell;
    let seen: Rc<RefCell<Vec<bool>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    reset_layout_runtime();
    let mut card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(100.0).height(100.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .maybe_on_focus(Some(move |f| sink.borrow_mut().push(f)));
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    card.on_event(&press(50.0, 50.0, PointerSource::Mouse));
    crate::focus::clear();
    assert_eq!(*seen.borrow(), vec![true, false]);
}

#[test]
fn pressable_publishes_rect_to_interactive_registry_and_withdraws_on_drop() {
    use crate::interactive_rects;
    reset_layout_runtime();
    let baseline = interactive_rects().len();
    let card = StyledContainer::new(
        LayoutStyle::new().width(120.0).height(40.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_press(|| {});
    let node = card.layout_node();
    assert_eq!(
        interactive_rects().len(),
        baseline,
        "an unlaid-out pressable contributes no rect"
    );
    compute_layout(
        node,
        AvailableSpace::Definite(120.0),
        AvailableSpace::Definite(40.0),
    )
    .unwrap();
    let rects = interactive_rects();
    assert_eq!(rects.len(), baseline + 1);
    assert!(
        rects.iter().any(|r| r.width == 120.0 && r.height == 40.0),
        "a laid-out pressable reports its rect"
    );
    drop(card);
    assert_eq!(
        interactive_rects().len(),
        baseline,
        "dropping the pressable withdraws its rect"
    );
}

/// The span an effect belonging to a widget wants, and the two failures either side of it: dropped on the floor it runs once and stops, parked somewhere longer-lived it keeps firing at a node that is gone.
///
/// The widget used to hold the handle, which is why `keeping` existed. The owner holds it now, so the span is the scope the widget was *built* in rather than the widget value's own Rust lifetime — which is the same span for every widget a view produces, and unlike the handle it does not need a field.
#[test]
fn an_effect_lives_exactly_as_long_as_the_scope_that_built_it() {
    crate::reset_layout_runtime();
    reactive_core::reset_runtime();
    let source = signal(0i32);
    let seen = std::rc::Rc::new(std::cell::Cell::new(0i32));

    let sink = seen.clone();
    let scope = reactive_core::owner_scope();
    let owner = scope.id();
    let _boxed =
        StyledContainer::new(LayoutStyle::new(), |_r| RectStyle::default(), vec![]).unwrap();
    effect(move || sink.set(source.get()));
    drop(scope);

    source.set(7);
    assert_eq!(seen.get(), 7, "the effect runs while the scope is alive");

    reactive_core::dispose_owner(owner);
    source.set(9);
    assert_eq!(seen.get(), 7, "and stops when the scope is disposed");
}
