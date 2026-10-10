use std::cell::{Cell, RefCell};

use geometry_core::{Rect, Size};
use layout_core::{AvailableSpace, NodeId, SizeDimension};
use platform_core::{PointerButton, PointerSource};
use reactive_core::{
    OwnerId, dispose_owner, effect, owner_scope, provide_context, signal, with_context, with_owner,
};
use renderer_core::{Declared, DrawCommand, ElementId, RectStyle};
use ui_tree::{ComponentList, SegmentRoot};

use super::*;
use crate::container::Container;
use crate::context::{
    compute_layout, live_node_count, relayout_if_dirty, remove_node, reset_layout_runtime,
    set_surface_size, track_layout,
};
use crate::fixed_layer::FixedLayer;
use crate::focus::{FocusId, Role};
use crate::inherit::{context, declare};
use crate::layout_item::box_item;
use crate::reactive_list::ReactiveList;
use crate::styled_container::StyledContainer;
use crate::surface_context::Surface;
use crate::test_support::{Failure, live, stateful_leaf};

const WIDTH: f32 = 400.0;
const HEIGHT: f32 = 300.0;
const SIDEBAR: f32 = 100.0;
const HEADER: f32 = 50.0;

/// What the moved subtree's build left behind for a test to look at: how often it ran, the state it made, and the control inside it.
#[derive(Default)]
struct Probe {
    builds: Cell<u32>,
    presses: Cell<Option<RwSignal<u32>>>,
    control: Cell<Option<(FocusId, NodeId)>>,
}

impl Probe {
    fn build(self: &Rc<Self>) -> impl FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError> {
        let probe = Rc::clone(self);
        move || {
            probe.builds.set(probe.builds.get() + 1);
            let presses = signal(0u32);
            let button = StyledContainer::new(
                LayoutStyle::new().width(40.0).height(20.0),
                |_| RectStyle::default(),
                vec![],
            )?
            .control(Role::Button)
            .on_press(move || presses.update(|count| *count += 1));
            let id = button.focus_id().expect("a control has a focus id");
            probe.control.set(Some((id, button.layout_node())));
            probe.presses.set(Some(presses));
            Ok(box_item(Container::new(
                LayoutStyle::new().flex_column(),
                vec![box_item(button)],
            )?))
        }
    }

    fn presses(&self) -> RwSignal<u32> {
        self.presses.get().expect("built")
    }

    fn focus_id(&self) -> FocusId {
        self.control.get().expect("built").0
    }

    fn button(&self) -> NodeId {
        self.control.get().expect("built").1
    }
}

fn block(width: f32, height: f32) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(Container::new(
        LayoutStyle::new()
            .width(width)
            .height(height)
            .flex_shrink(0.0),
        vec![],
    )?))
}

fn fill() -> LayoutStyle {
    LayoutStyle::new().flex_column().flex_grow(1.0)
}

/// The subtree beside a sidebar when `wide`, and under a header otherwise: two arrangements rebuilt around it, as a window crossing a breakpoint rebuilds them.
fn arranged(movable: &Movable, wide: RwSignal<bool>) -> ReactiveList {
    let movable = movable.clone();
    ReactiveList::with_style(
        LayoutStyle::new().flex_column().width(WIDTH).height(HEIGHT),
        move || vec![wide.get()],
        |wide: &bool| *wide,
        move |wide| {
            let (style, items) = match wide {
                true => (
                    LayoutStyle::new().flex_row().flex_grow(1.0),
                    vec![block(SIDEBAR, 10.0)?, box_item(movable.outlet(fill())?)],
                ),
                false => (
                    LayoutStyle::new().flex_column().flex_grow(1.0),
                    vec![block(10.0, HEADER)?, box_item(movable.outlet(fill())?)],
                ),
            };
            Ok(box_item(Container::new(style, items)?))
        },
    )
    .unwrap()
}

fn lay_out(node: NodeId) {
    compute_layout(
        node,
        AvailableSpace::Definite(WIDTH),
        AvailableSpace::Definite(HEIGHT),
    )
    .unwrap();
}

fn rect(node: NodeId) -> Rect {
    track_layout(node).expect("the node is alive").get()
}

fn click(tree: &ComponentList, at: Rect) {
    let (x, y) = (
        (at.x + at.width / 2.0) as f64,
        (at.y + at.height / 2.0) as f64,
    );
    let button = PointerButton::Primary;
    tree.dispatch(&Event::PointerPressed {
        x,
        y,
        button,
        source: PointerSource::Mouse,
    });
    tree.dispatch(&Event::PointerReleased {
        x,
        y,
        button,
        source: PointerSource::Mouse,
    });
}

/// Runs `build` under a scope of its own, handed back so the test can dispose it.
fn scoped<T>(build: impl FnOnce() -> T) -> (T, OwnerId) {
    let scope = owner_scope();
    let owner = scope.id();
    let built = build();
    drop(scope);
    (built, owner)
}

fn opened_at(commands: &[DrawCommand], node: NodeId) -> Option<usize> {
    let id = ElementId(node.into());
    commands.iter().position(
        |command| matches!(command, DrawCommand::PushElement { element } if element.id == id),
    )
}

#[test]
fn rearranging_moves_the_subtree_with_its_state_and_focus_instead_of_building_it_again() {
    reset_layout_runtime();
    focus::clear();
    let probe = Rc::new(Probe::default());
    let movable = Movable::new(probe.build()).unwrap();
    let wide = signal(true);
    let page = arranged(&movable, wide);
    let root = page.layout_node();
    let tree = ComponentList::new(page);
    lay_out(root);
    let _ = tree.commands();

    assert!(movable.is_shown());
    assert_eq!(
        (rect(probe.button()).x, rect(probe.button()).y),
        (SIDEBAR, 0.0)
    );
    click(&tree, rect(probe.button()));
    assert_eq!(
        probe.presses().get(),
        1,
        "the pointer reaches it through its outlet"
    );
    focus::request(probe.focus_id());

    wide.set(false);
    relayout_if_dirty();
    let _ = tree.commands();

    assert_eq!(
        probe.builds.get(),
        1,
        "the subtree was moved, not built again"
    );
    assert_eq!(probe.presses().get(), 1, "what it held is still there");
    assert_eq!(
        focus::current(),
        Some(probe.focus_id()),
        "and so is its focus"
    );
    assert_eq!(
        (rect(probe.button()).x, rect(probe.button()).y),
        (0.0, HEADER),
        "laid out where it stands now"
    );
    click(&tree, rect(probe.button()));
    assert_eq!(
        probe.presses().get(),
        2,
        "and pressed where it is drawn now"
    );

    wide.set(true);
    relayout_if_dirty();
    assert_eq!(probe.builds.get(), 1);
    assert_eq!(rect(probe.button()).x, SIDEBAR);
}

#[test]
fn tab_reaches_the_subtree_where_it_stands_now() {
    reset_layout_runtime();
    focus::clear();
    let probe = Rc::new(Probe::default());
    let movable = Movable::new(probe.build()).unwrap();
    let before = StyledContainer::new(
        LayoutStyle::new().width(40.0).height(20.0),
        |_| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(Role::Button);
    let before_id = before.focus_id();
    let (first, second) = (
        movable.outlet(fill()).unwrap(),
        movable.outlet(fill()).unwrap(),
    );
    let page = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(first), box_item(before), box_item(second)],
    )
    .unwrap();
    lay_out(page.layout_node());

    let order = || {
        focus::tab_order()
            .iter()
            .map(|stop| Some(stop.id))
            .collect::<Vec<_>>()
    };
    assert_eq!(order(), vec![before_id, Some(probe.focus_id())]);
}

#[test]
fn the_newest_live_outlet_shows_the_subtree_and_the_one_before_takes_it_back() {
    reset_layout_runtime();
    let probe = Rc::new(Probe::default());
    let movable = Movable::new(probe.build()).unwrap();
    let content = movable.layout_node().unwrap();
    let first = movable.outlet(fill()).unwrap();
    let first_node = first.layout_node();
    let marker = Container::new(LayoutStyle::new().width(10.0).height(10.0), vec![]).unwrap();
    let marker_node = marker.layout_node();
    let second_shown = signal(true);
    let showing = movable.clone();
    let second = ReactiveList::with_style(
        LayoutStyle::new().flex_column(),
        move || {
            second_shown
                .get()
                .then_some(())
                .into_iter()
                .collect::<Vec<_>>()
        },
        |_: &()| (),
        move |()| Ok(box_item(showing.outlet(fill())?)),
    )
    .unwrap();
    let page = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(first), box_item(marker), box_item(second)],
    )
    .unwrap();
    lay_out(page.layout_node());
    let root = SegmentRoot::mount(page);

    let second_node = layout_reactive::parent(content).unwrap();
    assert_ne!(second_node, first_node, "shown in the newer outlet");
    let drawn = root.commands().clone();
    assert!(
        opened_at(&drawn, content).unwrap() > opened_at(&drawn, marker_node).unwrap(),
        "drawn in the newest outlet, after what stands before it"
    );
    let content_id = ElementId(content.into());
    let opened = drawn.iter().filter(
        |command| matches!(command, DrawCommand::PushElement { element } if element.id == content_id),
    );
    assert_eq!(opened.count(), 1, "and nowhere else");

    second_shown.set(false);
    relayout_if_dirty();

    assert_eq!(layout_reactive::parent(content), Some(first_node));
    assert!(
        track_layout(second_node).is_none(),
        "the newer outlet is gone"
    );
    let drawn = root.commands().clone();
    assert!(
        opened_at(&drawn, content).unwrap() < opened_at(&drawn, marker_node).unwrap(),
        "drawn in the outlet that took it back"
    );
    assert_eq!(probe.builds.get(), 1);
}

#[test]
fn an_outlet_disposed_first_leaves_the_subtree_parked_rather_than_freed() {
    reset_layout_runtime();
    focus::clear();
    let probe = Rc::new(Probe::default());
    let movable = Movable::new(probe.build()).unwrap();
    let content = movable.layout_node().unwrap();
    let ((holder, holder_node), scope) = scoped(|| {
        let outlet = movable.outlet(fill()).unwrap();
        let holder =
            Container::new(LayoutStyle::new().flex_column(), vec![box_item(outlet)]).unwrap();
        let node = holder.layout_node();
        (holder, node)
    });
    lay_out(holder_node);
    focus::request(probe.focus_id());

    // Owner before node, as a container retires a child.
    dispose_owner(scope);
    remove_node(holder_node);
    drop(holder);

    assert!(
        track_layout(content).is_some(),
        "freeing the outlet did not free what it showed"
    );
    assert!(!movable.is_shown());
    assert_eq!(
        focus::current(),
        None,
        "focus does not stay behind in a parked subtree"
    );
    assert!(
        focus::tab_order().is_empty(),
        "and Tab does not reach into one"
    );

    let shown = movable.outlet(fill()).unwrap();
    lay_out(shown.layout_node());
    assert!(movable.is_shown());
    assert_eq!(layout_reactive::parent(content), Some(shown.layout_node()));
    assert_eq!(probe.builds.get(), 1);
    assert_eq!(
        focus::tab_order()
            .iter()
            .map(|stop| stop.id)
            .collect::<Vec<_>>(),
        vec![probe.focus_id()]
    );
}

#[test]
fn a_subtree_moved_within_one_turn_keeps_its_focus() {
    reset_layout_runtime();
    focus::clear();
    let probe = Rc::new(Probe::default());
    let movable = Movable::new(probe.build()).unwrap();
    let (first, scope) = scoped(|| movable.outlet(fill()).unwrap());
    focus::request(probe.focus_id());

    let second = reactive_core::batch(|| {
        dispose_owner(scope);
        drop(first);
        movable.outlet(fill()).unwrap()
    });

    assert!(second.is_showing());
    assert_eq!(
        focus::current(),
        Some(probe.focus_id()),
        "parked only until the turn ended, so nothing was let go"
    );
}

#[test]
fn dropping_the_last_handle_frees_the_subtree_and_empties_its_outlet() {
    reset_layout_runtime();
    focus::clear();
    let baseline = live_node_count();
    let probe = Rc::new(Probe::default());
    let movable = Movable::new(probe.build()).unwrap();
    let content = movable.layout_node().unwrap();
    let outlet = movable.outlet(fill()).unwrap();
    let holder = Container::new(LayoutStyle::new().flex_column(), vec![box_item(outlet)]).unwrap();
    let holder_node = holder.layout_node();
    lay_out(holder_node);
    let root = SegmentRoot::mount(holder);
    assert!(opened_at(&root.commands(), content).is_some());

    drop(movable);

    assert!(
        track_layout(content).is_none(),
        "the subtree's nodes are freed"
    );
    assert_eq!(
        layout_reactive::children(holder_node).len(),
        1,
        "the outlet stays"
    );
    assert!(
        opened_at(&root.commands(), content).is_none(),
        "and draws nothing"
    );
    assert!(
        focus::tab_order().is_empty(),
        "the subtree's controls are gone from the tab order"
    );
    assert_eq!(
        live_node_count(),
        baseline + 2,
        "only the holder and the outlet are left"
    );
}

#[test]
fn the_scope_it_was_made_in_frees_it_even_while_a_handle_and_an_outlet_remain() {
    reset_layout_runtime();
    let probe = Rc::new(Probe::default());
    let (movable, scope) = scoped(|| Movable::new(probe.build()).unwrap());
    let content = movable.layout_node().unwrap();
    let outlet = movable.outlet(fill()).unwrap();
    let presses = probe.presses();

    dispose_owner(scope);

    assert!(track_layout(content).is_none());
    assert!(
        !presses.is_alive(),
        "what the build made went with its scope"
    );
    assert!(!outlet.is_showing());
    assert_eq!(movable.layout_node(), None);
    drop(outlet);
    drop(movable);
}

#[test]
fn disposing_both_ends_in_one_wave_leaves_nothing_behind_whichever_goes_first() {
    for outlet_first in [true, false] {
        reset_layout_runtime();
        focus::clear();
        // Once first, so what the surface's worlds create on first use is not counted against the wave.
        one_wave(outlet_first);
        let baseline = live();
        one_wave(outlet_first);
        assert_eq!(live(), baseline, "outlet disposed first: {outlet_first}");
    }
}

fn one_wave(outlet_first: bool) {
    let probe = Rc::new(Probe::default());
    let ((holder, movable), scope) = scoped(|| {
        // A wave runs the cleanups of the scope made first first.
        let early = scoped(|| ()).1;
        let movable = Movable::new(probe.build()).unwrap();
        let outlet_scope = if outlet_first { early } else { scoped(|| ()).1 };
        let outlet = with_owner(Some(outlet_scope), || movable.outlet(fill()).unwrap());
        let holder =
            Container::new(LayoutStyle::new().flex_column(), vec![box_item(outlet)]).unwrap();
        (holder, movable)
    });
    let holder_node = holder.layout_node();
    lay_out(holder_node);
    focus::request(probe.focus_id());

    dispose_owner(scope);
    remove_node(holder_node);
    drop(holder);
    drop(movable);
}

#[test]
fn a_build_that_fails_leaves_nothing_behind() {
    for fail in [Failure::Err, Failure::Panic] {
        reset_layout_runtime();
        dispose_owner(scoped(|| ()).1);
        let baseline = live();
        let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Movable::new(|| stateful_leaf(fail)).map(drop)
        }));
        assert!(!matches!(built, Ok(Ok(()))));
        assert_eq!(live(), baseline);
    }
}

#[test]
fn what_an_owner_provides_is_read_where_the_subtree_was_made_wherever_it_is_shown() {
    #[derive(Clone, Copy, PartialEq, Debug)]
    struct Place(&'static str);

    reset_layout_runtime();
    let read = Rc::new(Cell::new(None));
    let (movable, _) = scoped(|| {
        provide_context(Place("made"));
        let read = Rc::clone(&read);
        Movable::new(move || {
            Ok(box_item(
                StyledContainer::new(
                    LayoutStyle::new().width(40.0).height(20.0),
                    |_| RectStyle::default(),
                    vec![],
                )?
                .on_press(move || read.set(with_context(|place: &Place| *place))),
            ))
        })
        .unwrap()
    });
    let (outlet, _) = scoped(|| {
        provide_context(Place("shown"));
        movable.outlet(fill()).unwrap()
    });
    let holder = Container::new(LayoutStyle::new().flex_column(), vec![box_item(outlet)]).unwrap();
    let node = holder.layout_node();
    let tree = ComponentList::new(holder);
    lay_out(node);

    click(&tree, rect(movable.layout_node().unwrap()));

    assert_eq!(read.get(), Some(Place("made")));
}

#[test]
fn what_the_layout_tree_cascades_follows_the_outlet_and_is_resolved_again_on_a_move() {
    reset_layout_runtime();
    let movable = Movable::new(|| block(10.0, 10.0)).unwrap();
    let content = movable.layout_node().unwrap();
    let large = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(movable.outlet(fill()).unwrap())],
    )
    .unwrap();
    declare(
        large.layout_node(),
        Declared::default().with_font_size(22.0),
    );
    let (small, small_scope) = scoped(|| {
        Container::new(
            LayoutStyle::new().flex_column(),
            vec![box_item(movable.outlet(fill()).unwrap())],
        )
        .unwrap()
    });
    declare(
        small.layout_node(),
        Declared::default().with_font_size(11.0),
    );
    let seen = Rc::new(Cell::new(0.0));
    let seeing = Rc::clone(&seen);
    let _reading = effect(move || seeing.set(context(content).text.font_size));
    assert_eq!(seen.get(), 11.0);

    dispose_owner(small_scope);

    assert_eq!(
        seen.get(),
        22.0,
        "a reader re-resolved what it inherits once the move took it back, though no declaration changed"
    );
}

#[test]
fn an_outlet_on_another_surface_is_refused() {
    let home = Surface::new();
    let away = Surface::new();
    let movable = {
        let _home = home.enter();
        Movable::new(|| block(10.0, 10.0)).unwrap()
    };
    let _away = away.enter();
    assert!(movable.outlet(fill()).is_err());
}

#[test]
fn an_outlet_released_on_another_surfaces_turn_moves_the_subtree_in_its_own_tree() {
    let home = Surface::new();
    let away = Surface::new();
    let (movable, first, second, second_scope) = {
        let _home = home.enter();
        let movable = Movable::new(|| block(10.0, 10.0)).unwrap();
        let first = movable.outlet(fill()).unwrap();
        let (second, scope) = scoped(|| movable.outlet(fill()).unwrap());
        (movable, first, second, scope)
    };
    let content = {
        let _home = home.enter();
        movable.layout_node().unwrap()
    };

    {
        let _away = away.enter();
        dispose_owner(second_scope);
        assert_eq!(
            live_node_count(),
            0,
            "the other surface's tree is untouched"
        );
    }

    let _home = home.enter();
    assert!(first.is_showing());
    drop(second);
    assert_eq!(layout_reactive::parent(content), Some(first.layout_node()));
}

#[test]
fn a_handler_inside_that_drops_the_last_handle_finishes_before_the_subtree_is_freed() {
    reset_layout_runtime();
    let kept: Rc<RefCell<Option<Movable>>> = Rc::default();
    let after_drop = Rc::new(Cell::new(None));
    let (keeping, seeing) = (Rc::clone(&kept), Rc::clone(&after_drop));
    let movable = Movable::new(move || {
        let presses = signal(0u32);
        Ok(box_item(
            StyledContainer::new(
                LayoutStyle::new().width(40.0).height(20.0),
                |_| RectStyle::default(),
                vec![],
            )?
            .on_press(move || {
                drop(keeping.borrow_mut().take());
                presses.update(|count| *count += 1);
                seeing.set(Some(presses.get()));
            }),
        ))
    })
    .unwrap();
    let content = movable.layout_node().unwrap();
    let outlet = movable.outlet(fill()).unwrap();
    *kept.borrow_mut() = Some(movable);
    let holder = Container::new(LayoutStyle::new().flex_column(), vec![box_item(outlet)]).unwrap();
    let node = holder.layout_node();
    let tree = ComponentList::new(holder);
    lay_out(node);

    click(&tree, rect(content));

    assert_eq!(
        after_drop.get(),
        Some(1),
        "its state outlived the drop until it returned"
    );
    assert!(track_layout(content).is_none(), "and was freed once it had");
}

#[test]
fn a_handle_outliving_its_surface_touches_no_other_tree_when_it_drops() {
    let home = Surface::new();
    let away = Surface::new();
    let (movable, outlet) = {
        let _home = home.enter();
        let movable = Movable::new(|| block(10.0, 10.0)).unwrap();
        let outlet = movable.outlet(fill()).unwrap();
        (movable, outlet)
    };
    drop(home);

    let _away = away.enter();
    let namesakes = (0..4)
        .map(|_| layout_reactive::new_container(LayoutStyle::new(), &[]).unwrap())
        .collect::<Vec<_>>();
    drop(outlet);
    drop(movable);

    assert!(
        namesakes.iter().all(|&node| track_layout(node).is_some()),
        "the nodes another surface minted under the same ids are left alone"
    );
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Place {
    Hidden,
    Visible,
    Nowhere,
}

/// The subtree's one outlet inside a hidden box, inside a shown one, or nowhere at all, as `place` says: the arrangement is rebuilt around it whenever that changes.
fn placed(movable: &Movable, place: RwSignal<Place>) -> ReactiveList {
    let movable = movable.clone();
    ReactiveList::with_style(
        LayoutStyle::new().flex_column().width(WIDTH).height(HEIGHT),
        move || vec![place.get()],
        |place: &Place| *place,
        move |place| match place {
            Place::Nowhere => block(10.0, 10.0),
            Place::Hidden | Place::Visible => Ok(box_item(Container::new(
                fill().shown(place == Place::Visible),
                vec![box_item(movable.outlet(fill())?)],
            )?)),
        },
    )
    .unwrap()
}

/// Whether the box `node` names is drawn, read through the elements the frame opens.
fn drawn(tree: &SegmentRoot, node: NodeId) -> bool {
    opened_at(&tree.commands(), node).is_some()
}

#[test]
fn a_fixed_layer_moved_out_of_a_hidden_outlet_is_drawn_and_counted_where_it_stands_now() {
    reset_layout_runtime();
    set_surface_size(Size::new(WIDTH, HEIGHT));
    let bar_node = Rc::new(Cell::new(None));
    let noting = Rc::clone(&bar_node);
    let movable = Movable::new(move || {
        let bar = Container::new(
            LayoutStyle::new()
                .width(SizeDimension::Percent(1.0))
                .height(HEADER),
            vec![],
        )?;
        noting.set(Some(bar.layout_node()));
        let layer = FixedLayer::new(LayoutStyle::new().flex_column(), vec![box_item(bar)])?;
        Ok(box_item(Container::new(
            LayoutStyle::new().flex_column(),
            vec![box_item(layer)],
        )?))
    })
    .unwrap();
    let bar = bar_node.get().expect("built");
    let place = signal(Place::Hidden);
    let list = placed(&movable, place);
    let node = list.layout_node();
    let tree = SegmentRoot::mount(list);
    lay_out(node);
    relayout_if_dirty();
    let covered = Rc::new(Cell::new((0.0, 0.0)));
    let seeing = Rc::clone(&covered);
    let _margin = effect(move || seeing.set(crate::arrival_margin::covered_by_layers()));
    assert!(!drawn(&tree, bar), "hidden with the outlet it stands in");
    assert_eq!(covered.get(), (0.0, 0.0), "and covers no edge");

    place.set(Place::Visible);
    lay_out(node);
    relayout_if_dirty();

    assert!(drawn(&tree, bar), "drawn where it stands now");
    assert_eq!(
        covered.get(),
        (HEADER, 0.0),
        "and counted over the top edge, though nothing above it was hidden or shown"
    );
}

#[test]
fn an_overlay_opened_inside_is_hidden_while_its_subtree_is_and_back_when_it_is_shown() {
    use crate::overlay::Overlay;

    reset_layout_runtime();
    let panel_node = Rc::new(Cell::new(None));
    let noting = Rc::clone(&panel_node);
    let movable = Movable::new(move || {
        let panel = StyledContainer::new(
            LayoutStyle::new().width(50.0).height(20.0),
            |_| RectStyle::default(),
            vec![],
        )?;
        noting.set(Some(panel.layout_node()));
        let overlay = Overlay::new(LayoutStyle::new(), vec![box_item(panel)])?;
        Ok(box_item(Container::new(
            LayoutStyle::new().flex_column(),
            vec![box_item(overlay)],
        )?))
    })
    .unwrap();
    let panel = panel_node.get().expect("built");
    let place = signal(Place::Visible);
    let list = placed(&movable, place);
    let node = list.layout_node();
    let tree = SegmentRoot::mount(list);
    lay_out(node);
    let hidden = Rc::new(Cell::new(None));
    let seeing = Rc::clone(&hidden);
    let _reading = effect(move || seeing.set(Some(crate::input_region::tracks_hidden(panel))));
    assert!(drawn(&tree, panel));

    place.set(Place::Hidden);
    lay_out(node);
    assert!(!drawn(&tree, panel), "hidden with a hidden outlet");

    place.set(Place::Nowhere);
    lay_out(node);
    assert!(!movable.is_shown());
    assert_eq!(hidden.get(), Some(true), "parked is hidden, not closed");
    assert!(
        !crate::input_region::receives_input(panel),
        "and takes no input"
    );

    place.set(Place::Visible);
    lay_out(node);
    assert_eq!(hidden.get(), Some(false));
    assert!(drawn(&tree, panel), "drawn again once it is shown");
}

#[test]
fn a_dialog_inside_a_parked_subtree_cannot_be_dismissed_until_it_is_shown_again() {
    use crate::dismiss::{DismissRegistration, dismiss_depth, dismiss_top};
    use crate::overlay::Overlay;

    reset_layout_runtime();
    let overlay_node = Rc::new(Cell::new(None));
    let noting = Rc::clone(&overlay_node);
    let movable = Movable::new(move || {
        let overlay = Overlay::new(LayoutStyle::new(), vec![])?;
        noting.set(Some(overlay.layout_node()));
        Ok(box_item(Container::new(
            LayoutStyle::new().flex_column(),
            vec![box_item(overlay)],
        )?))
    })
    .unwrap();
    let dialog = overlay_node.get().expect("built");
    let place = signal(Place::Visible);
    let list = placed(&movable, place);
    let node = list.layout_node();
    let _tree = SegmentRoot::mount(list);
    lay_out(node);
    let closed = Rc::new(Cell::new(0));
    let counting = Rc::clone(&closed);
    let _registration =
        DismissRegistration::within(dialog, Rc::new(move || counting.set(counting.get() + 1)));
    let depth = Rc::new(Cell::new(usize::MAX));
    let reading = Rc::clone(&depth);
    let _reading = effect(move || reading.set(crate::dismiss::use_dismiss_depth()));
    assert_eq!(depth.get(), 1);

    place.set(Place::Nowhere);
    lay_out(node);
    assert!(!movable.is_shown());
    assert_eq!(
        dismiss_depth(),
        0,
        "parked, it is not on the stack that counts"
    );
    assert_eq!(depth.get(), 0, "and a reader of the depth hears it go");
    assert!(!dismiss_top(), "Escape finds nothing to close");
    assert_eq!(closed.get(), 0);

    place.set(Place::Visible);
    lay_out(node);
    assert_eq!(depth.get(), 1, "shown again, it counts again");
    assert!(dismiss_top());
    assert_eq!(closed.get(), 1);
}
