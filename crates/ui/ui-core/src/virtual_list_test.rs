use super::*;

#[test]
fn the_window_covers_the_screen_plus_its_overscan() {
    let (first, last) = visible_window(0.0, 100.0, 20.0, 1000, 0);
    assert_eq!((first, last), (0, 6));

    let (first, last) = visible_window(200.0, 100.0, 20.0, 1000, 2);
    assert_eq!((first, last), (8, 18));

    let (first, _) = visible_window(199.0, 100.0, 20.0, 1000, 0);
    assert_eq!(first, 9, "row 9 is still showing its last pixel");
}

#[test]
fn the_window_is_clamped_at_both_ends() {
    assert_eq!(visible_window(0.0, 100.0, 20.0, 1000, 5), (0, 11));
    assert_eq!(visible_window(19_800.0, 100.0, 20.0, 1000, 5), (985, 1000));
    let (first, last) = visible_window(100_000.0, 100.0, 20.0, 10, 0);
    assert!(first <= last && last <= 10, "got {first}..{last}");
}

#[test]
fn an_unmeasured_viewport_renders_everything_rather_than_nothing() {
    assert_eq!(visible_window(0.0, 0.0, 20.0, 40, 0), (0, 40));
    assert_eq!(visible_window(0.0, 100.0, 0.0, 40, 0), (0, 40));
    assert_eq!(
        visible_window(0.0, 100.0, 20.0, 0, 0),
        (0, 0),
        "an empty list is empty"
    );
}

mod measured {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::rc::Rc;

    use layout_core::{AvailableSpace, LayoutStyle, NodeId};
    use reactive_core::{RwSignal, on_cleanup, signal};
    use ui_tree::RenderNode;

    use super::super::*;
    use crate::canvas::Canvas;
    use crate::context::{
        compute_layout, relayout_if_dirty, reset_layout_runtime, set_surface_size, track_layout,
    };
    use crate::layout_item::box_item;
    use crate::scroll_area::LayoutScrollArea;

    const WIDTH: f32 = 200.0;
    const VIEW: f32 = 200.0;

    type Item = (usize, f32);

    struct Mounted {
        _scroll: LayoutScrollArea,
        viewport: ScrollViewport,
        rows: VirtualRows,
        items: RwSignal<Vec<Item>>,
        live: Rc<RefCell<BTreeMap<usize, NodeId>>>,
        builds: Rc<RefCell<usize>>,
    }

    fn numbered(heights: impl IntoIterator<Item = f32>) -> Vec<Item> {
        heights.into_iter().enumerate().collect()
    }

    impl Mounted {
        fn new(items: Vec<Item>, estimate: f32, overscan: f32) -> Self {
            reset_layout_runtime();
            set_surface_size(geometry_core::Size::new(WIDTH, VIEW));
            let items = signal(items);
            let live: Rc<RefCell<BTreeMap<usize, NodeId>>> = Rc::default();
            let builds = Rc::new(RefCell::new(0));
            let mut handed = None;
            let scroll = {
                let (live, builds) = (Rc::clone(&live), Rc::clone(&builds));
                LayoutScrollArea::new_with(
                    LayoutStyle::new().width(WIDTH).height(VIEW),
                    |viewport| {
                        let rows = VirtualRows::new(viewport.clone(), estimate);
                        handed = Some((viewport, rows.clone()));
                        let list = VirtualList::measured(
                            LayoutStyle::new().flex_column(),
                            rows,
                            overscan,
                            move || items.get(),
                            |item: &Item| item.0,
                            move |at, (_, height): Item| {
                                let row = Canvas::new(LayoutStyle::new().height(height), |_| {
                                    RenderNode::Empty
                                })?;
                                let node = row.layout_node();
                                live.borrow_mut().insert(at, node);
                                *builds.borrow_mut() += 1;
                                let live = Rc::clone(&live);
                                on_cleanup(move || {
                                    let mut live = live.borrow_mut();
                                    if live.get(&at) == Some(&node) {
                                        live.remove(&at);
                                    }
                                });
                                Ok(box_item(row))
                            },
                        )?;
                        Ok(box_item(list))
                    },
                )
                .unwrap()
            };
            compute_layout(
                scroll.layout_node(),
                AvailableSpace::Definite(WIDTH),
                AvailableSpace::Definite(VIEW),
            )
            .unwrap();
            let (viewport, rows) = handed.expect("the scroll built its content");
            let mounted = Self {
                _scroll: scroll,
                viewport,
                rows,
                items,
                live,
                builds,
            };
            mounted.settle();
            mounted
        }

        fn settle(&self) {
            for _ in 0..16 {
                relayout_if_dirty();
            }
        }

        fn offset(&self) -> f32 {
            self.viewport.peek_offset().1
        }

        fn scroll_to(&self, y: f32) {
            self.viewport.scroll_to(0.0, y);
        }

        fn built(&self) -> Vec<usize> {
            self.live.borrow().keys().copied().collect()
        }

        fn laid_out(&self, at: usize) -> geometry_core::Rect {
            let node = *self
                .live
                .borrow()
                .get(&at)
                .unwrap_or_else(|| panic!("row {at} is built"));
            track_layout(node).expect("a built row is tracked").get()
        }

        fn on_screen(&self, at: usize) -> f32 {
            self.laid_out(at).y - self.offset()
        }
    }

    #[test]
    fn rows_of_mixed_heights_are_measured_and_only_those_on_screen_stay_built() {
        let heights = (0..1000).map(|at| if at % 2 == 0 { 20.0 } else { 60.0 });
        let list = Mounted::new(numbered(heights), 30.0, 0.0);

        assert_eq!(
            list.built(),
            (0..6).collect::<Vec<_>>(),
            "20 + 60 + 20 + 60 + 20 = 180, so row 5 is the last one the 200px view reaches"
        );
        assert!((0..6).all(|at| list.rows.is_measured(at)));
        assert_eq!(list.rows.offset_of(5), 180.0);
        assert_eq!(list.rows.height_of(5), 60.0);
        assert_eq!(
            list.rows.height_of(500),
            30.0,
            "an unbuilt row stands at the estimate"
        );
        for at in list.built() {
            assert_eq!(
                list.laid_out(at).y,
                list.rows.offset_of(at),
                "row {at} is laid out where the index says it is"
            );
        }

        list.scroll_to(4_000.0);
        list.settle();
        let first = list.rows.row_at(list.offset());
        let built = list.built();
        assert!(
            built.contains(&first),
            "the row at the top of the view is built"
        );
        assert!(
            built.len() <= 6,
            "a screenful is built, not {}",
            built.len()
        );
        for at in built {
            assert_eq!(list.laid_out(at).y, list.rows.offset_of(at));
        }
    }

    #[test]
    fn the_first_row_on_screen_holds_still_while_the_rows_above_it_are_measured() {
        let list = Mounted::new(numbered(std::iter::repeat_n(40.0, 1000)), 20.0, 100.0);
        assert_eq!(list.offset(), 0.0);
        assert_eq!(list.rows.height_of(0), 40.0);

        list.scroll_to(1_010.0);
        let anchor = list.rows.row_at(1_010.0);
        let shown_at = list.rows.offset_of(anchor) - 1_010.0;
        assert!(
            !list.rows.is_measured(anchor - 1),
            "the rows above it are still estimates"
        );
        list.settle();

        assert!(list.rows.is_measured(anchor - 1));
        assert!(
            list.offset() > 1_010.0,
            "the rows above came out taller than guessed, so the offset grew by the difference"
        );
        assert_eq!(
            list.on_screen(anchor),
            shown_at,
            "row {anchor} is where it was on screen before the rows above it were measured"
        );
        assert_eq!(list.rows.row_at(list.offset()), anchor);
    }

    #[test]
    fn a_row_shorter_than_its_estimate_above_the_view_pulls_the_offset_back() {
        let list = Mounted::new(numbered(std::iter::repeat_n(10.0, 1000)), 30.0, 60.0);
        list.scroll_to(3_000.0);
        let anchor = list.rows.row_at(3_000.0);
        let shown_at = list.rows.offset_of(anchor) - 3_000.0;
        list.settle();
        assert!(list.offset() < 3_000.0);
        assert_eq!(list.on_screen(anchor), shown_at);
    }

    #[test]
    fn a_row_measured_below_the_first_row_on_screen_leaves_the_offset_alone() {
        let list = Mounted::new(numbered(std::iter::repeat_n(40.0, 1000)), 20.0, 0.0);
        list.scroll_to(2_000.0);
        let anchor = list.rows.row_at(2_000.0);
        assert_eq!(list.rows.offset_of(anchor), 2_000.0);
        list.settle();
        assert_eq!(
            list.offset(),
            2_000.0,
            "nothing above row {anchor} was built, so nothing above it moved"
        );
        assert_eq!(list.on_screen(anchor), 0.0);
    }

    #[test]
    fn reveal_brings_an_unbuilt_row_into_view_and_follows_it_once_measured() {
        let heights = (0..2000).map(|at| if at == 700 { 150.0 } else { 20.0 });
        let list = Mounted::new(numbered(heights), 20.0, 40.0);
        assert!(!list.rows.is_measured(700));

        list.rows.reveal(700, 0.0);
        list.settle();

        assert!(list.rows.is_measured(700));
        let row = list.laid_out(700);
        let top = row.y - list.offset();
        assert!(
            top >= 0.0 && top + row.height <= VIEW,
            "row 700 is wholly in view: it sits at {top}..{} in a {VIEW}px view",
            top + row.height
        );
        assert_eq!(
            top + row.height,
            VIEW,
            "brought in from above, it stops at the bottom edge, not 130px past it"
        );

        let before = list.offset();
        list.rows.reveal(700, 0.0);
        list.settle();
        assert_eq!(
            list.offset(),
            before,
            "a row already in view stays where it is"
        );

        list.rows.reveal(3, 0.0);
        list.settle();
        assert_eq!(
            list.on_screen(3),
            0.0,
            "brought in from below, it stops at the top edge"
        );
    }

    #[test]
    fn measurements_follow_their_keys_when_the_rows_change() {
        let heights = (0..100).map(|at| 10.0 + at as f32);
        let list = Mounted::new(numbered(heights), 30.0, 0.0);
        assert!(list.rows.is_measured(1));
        assert_eq!(list.rows.height_of(1), 11.0);

        list.items.update(|items| {
            items.remove(0);
        });
        list.settle();
        assert_eq!(list.rows.len(), 99);
        assert_eq!(
            list.rows.height_of(0),
            11.0,
            "row 1 moved up and kept its measurement"
        );
        assert_eq!(list.laid_out(0).y, 0.0);
        assert_eq!(list.laid_out(0).height, 11.0);
    }

    #[test]
    fn a_hundred_thousand_rows_build_a_screenful() {
        let heights = (0..100_000).map(|at| (16 + at % 5 * 8) as f32);
        let list = Mounted::new(numbered(heights), 24.0, 50.0);
        let first = *list.builds.borrow();
        assert!(first < 40, "{first} rows built for a 200px view");

        list.scroll_to(1_200_000.0);
        list.settle();
        let built = list.built();
        assert!(built.len() < 40, "{} rows built", built.len());
        let anchor = list.rows.row_at(list.offset());
        assert!(built.contains(&anchor));
        assert!(
            anchor > 40_000,
            "the view is deep in the list, at row {anchor}"
        );
    }
}

mod fixed {
    use std::cell::RefCell;
    use std::rc::Rc;

    use layout_core::{AvailableSpace, LayoutStyle};
    use reactive_core::on_cleanup;
    use ui_tree::RenderNode;

    use super::super::*;
    use crate::canvas::Canvas;
    use crate::context::{compute_layout, relayout_if_dirty, reset_layout_runtime, track_layout};
    use crate::layout_item::box_item;
    use crate::scroll_area::LayoutScrollArea;

    #[test]
    fn a_fixed_height_list_builds_the_visible_window_between_two_spacers() {
        reset_layout_runtime();
        let live: Rc<RefCell<Vec<usize>>> = Rc::default();
        let mut handed = None;
        let mut content = None;
        let scroll = {
            let live = Rc::clone(&live);
            LayoutScrollArea::new_with(LayoutStyle::new().width(200.0).height(100.0), |viewport| {
                handed = Some(viewport.clone());
                let list = VirtualList::new(
                    LayoutStyle::new().flex_column(),
                    viewport,
                    20.0,
                    2,
                    || (0..1000).collect::<Vec<usize>>(),
                    |item: &usize| *item,
                    move |at, _| {
                        live.borrow_mut().push(at);
                        let live = Rc::clone(&live);
                        on_cleanup(move || live.borrow_mut().retain(|&built| built != at));
                        Ok(box_item(Canvas::new(
                            LayoutStyle::new().height(20.0),
                            |_| RenderNode::Empty,
                        )?))
                    },
                )?;
                content = Some(list.layout_node());
                Ok(box_item(list))
            })
            .unwrap()
        };
        compute_layout(
            scroll.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        let viewport = handed.expect("the scroll built its content");
        let sorted = || {
            let mut built = live.borrow().clone();
            built.sort_unstable();
            built
        };
        assert_eq!(sorted(), (0..8).collect::<Vec<_>>());

        viewport.scroll_to(0.0, 200.0);
        relayout_if_dirty();
        assert_eq!(
            sorted(),
            (8..18).collect::<Vec<_>>(),
            "visible_window(200, 100, 20, 1000, 2)"
        );
        let content = track_layout(content.expect("the list was built"))
            .unwrap()
            .get();
        assert_eq!(
            content.height, 20_000.0,
            "the spacers keep the content every row tall"
        );
    }
}
