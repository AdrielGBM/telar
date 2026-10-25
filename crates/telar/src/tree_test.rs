use super::*;

struct Blank;

impl crate::app::App for Blank {
    fn root(&self) -> Box<dyn Component> {
        ui_core::reset_layout_runtime();
        Box::new(
            ui_core::Rectangle::new(
                layout_core::LayoutStyle::new().width(10.0).height(10.0),
                || renderer_core::RectStyle::filled(renderer_core::Color::BLACK, 0.0),
            )
            .expect("a rectangle builds"),
        )
    }
}

/// A tree behind the hot-reload boundary has to feed the input registries **its own** widgets read.
///
/// A `cdylib` carries its own copy of every `thread_local` in `ui-core`, so the runner observing on the host side filled a registry nothing in the app ever looked at: `modifiers()` answered "none held" and `pointer_buttons()` answered "nothing pressed", confidently, for a whole `cargo telar dev` session. A ⇧-click was a plain click and a right-drag was a left-drag, and every gesture built on either did the wrong thing in the window while passing its own tests — which is exactly the shape of failure a state registry has, because it never says it does not know.
///
/// The boundary itself cannot be built in a unit test; the property that fixes it can: whoever dispatches an event observes it.
#[test]
fn the_hot_tree_feeds_the_registries_its_own_widgets_read() {
    ui_core::reset_keyboard();
    ui_core::reset_pointer();
    let mut tree = HotTree::mount(&Blank);

    assert!(
        !ui_core::modifiers().is_shift,
        "shift is not down before the event"
    );
    tree.on_event(&Event::ModifiersChanged {
        modifiers: platform_core::ModifiersState {
            is_shift: true,
            ..Default::default()
        },
    });
    assert!(
        ui_core::modifiers().is_shift,
        "the side that dispatched has to be the side that knows"
    );

    tree.on_event(&Event::PointerPressed {
        x: 5.0,
        y: 5.0,
        button: platform_core::PointerButton::Secondary,
        source: platform_core::PointerSource::Mouse,
    });
    assert!(
        ui_core::pointer_buttons().secondary,
        "and the same for which button is down"
    );
    drop(tree);
    ui_core::reset_keyboard();
    ui_core::reset_pointer();
}

/// A button inside a box, its focus id handed out as the tree is built.
fn panel_with_a_button(
    id: std::rc::Rc<std::cell::Cell<Option<ui_core::focus::FocusId>>>,
) -> Box<dyn Component> {
    let button = ui_core::StyledContainer::new(
        layout_core::LayoutStyle::new().width(40.0).height(20.0),
        |_| renderer_core::RectStyle::default(),
        vec![],
    )
    .expect("a box builds")
    .control(ui_core::focus::Role::Button)
    .on_press(|| {});
    id.set(button.focus_id());
    Box::new(
        ui_core::Container::new(
            layout_core::LayoutStyle::new(),
            vec![ui_core::box_item(button)],
        )
        .expect("a box builds"),
    )
}

/// What a mounted tree built is the tree's: dropping it — a remount, a window closing — leaves no control of it in the tab order, however deep it sat. A tree that only held its root left every nested control registered and every effect of the old tree running.
#[test]
fn dropping_a_mounted_tree_frees_the_controls_it_built() {
    ui_core::reset_layout_runtime();
    let id = std::rc::Rc::new(std::cell::Cell::new(None));
    let tree = LocalTree::new({
        let id = id.clone();
        move || panel_with_a_button(id)
    });
    let button = id.get().expect("the button is focusable");
    assert!(ui_core::focus::is_registered(button));

    drop(tree);
    assert!(
        !ui_core::focus::is_registered(button),
        "the button left the tab order with its tree"
    );
}

#[test]
fn releasing_a_hot_tree_frees_the_controls_it_built() {
    struct Panel(std::rc::Rc<std::cell::Cell<Option<ui_core::focus::FocusId>>>);
    impl crate::app::App for Panel {
        fn root(&self) -> Box<dyn Component> {
            ui_core::reset_layout_runtime();
            panel_with_a_button(self.0.clone())
        }
    }
    let id = std::rc::Rc::new(std::cell::Cell::new(None));
    let tree = HotTree::mount(&Panel(id.clone()));
    let button = id.get().expect("the button is focusable");
    assert!(ui_core::focus::is_registered(button));

    drop(tree);
    assert!(!ui_core::focus::is_registered(button));
}

/// What a hot tree paints is what it composed, read back on the host's side; a host that lost a frame has the tree reset, and the next frame stands alone.
#[test]
fn a_hot_tree_paints_what_it_composed_and_a_reset_frame_stands_alone() {
    let mut tree = HotTree::mount(&Blank);
    ui_core::relayout_if_dirty();
    let composed = tree.tree.list.commands().clone();
    assert!(!composed.is_empty(), "precondition: the tree draws");
    let read = |decoder: &mut renderer_core::wire::FrameDecoder, frame: LentBytes| {
        let mut commands = Vec::new();
        decoder
            .decode(unsafe { frame.as_slice() }, &mut commands, |_| None)
            .map(|()| commands)
    };

    let mut decoder = renderer_core::wire::FrameDecoder::new();
    assert_eq!(read(&mut decoder, tree.paint()), Ok(composed.clone()));
    assert_eq!(read(&mut decoder, tree.paint()), Ok(composed.clone()));

    let mut fresh = renderer_core::wire::FrameDecoder::new();
    assert!(
        read(&mut fresh, tree.paint()).is_err(),
        "precondition: a frame builds on the ones before it"
    );
    tree.reset_frames();
    assert_eq!(read(&mut fresh, tree.paint()), Ok(composed));
}

fn fill() -> layout_core::LayoutStyle {
    layout_core::LayoutStyle::new()
        .width(layout_core::SizeDimension::Percent(1.0))
        .height(layout_core::SizeDimension::Percent(1.0))
}

fn swatch() -> Box<dyn ui_core::LayoutItem> {
    Box::new(
        ui_core::Rectangle::new(fill(), || {
            renderer_core::RectStyle::filled(renderer_core::Color::BLACK, 0.0)
        })
        .expect("a rectangle builds"),
    )
}

fn resize(tree: &mut dyn UiTree) {
    tree.on_event(&Event::WindowResized {
        width: 400,
        height: 300,
    });
    ui_core::relayout_if_dirty();
}

/// The runner drops a tree before it builds the next. One that took only its owner with it left its nodes behind and its root the overlay host, so the next tree's overlay portaled onto a dead root and the next layout pass read rect signals its owner had freed — whether or not the first tree had an overlay of its own.
#[test]
fn a_remounted_tree_portals_onto_its_own_root_and_leaves_no_node_of_the_last_one() {
    ui_core::reset_layout_runtime();
    let baseline = ui_core::live_node_count();
    let mut first = LocalTree::new(|| {
        Box::new(ui_core::WindowRoot::wrapping(swatch()).expect("a root builds"))
    });
    resize(&mut first);
    drop(first);
    assert_eq!(
        ui_core::live_node_count(),
        baseline,
        "the first tree took its nodes with it"
    );

    let content = std::rc::Rc::new(std::cell::Cell::new(None));
    let mut second = LocalTree::new({
        let content = content.clone();
        move || {
            let overlay =
                ui_core::overlay::Overlay::new(fill(), vec![swatch()]).expect("an overlay builds");
            content.set(Some(overlay.content_node()));
            let page =
                ui_core::Container::new(fill(), vec![Box::new(overlay)]).expect("a page builds");
            Box::new(ui_core::WindowRoot::wrapping(Box::new(page)).expect("a root builds"))
        }
    });
    resize(&mut second);
    let content = content.get().expect("the overlay was built");
    assert_eq!(
        ui_core::track_layout(content).map(|rect| rect.peek()),
        Some(geometry_core::Rect::new(0.0, 0.0, 400.0, 300.0)),
        "the overlay fills the new tree's window"
    );

    drop(second);
    assert_eq!(
        ui_core::live_node_count(),
        baseline,
        "and so did the second"
    );
}
