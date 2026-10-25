use std::cell::Cell;

use telar::testing::{advance_time, centre, moved, press, release, route, texts};
use telar::{ComponentList, Event, Size, relayout_if_dirty, set_surface_size};

use super::*;

const SURFACE: Size = Size {
    width: 800.0,
    height: 600.0,
};

struct Mounted {
    tree: ComponentList,
    queue: Toasts,
}

impl Mounted {
    fn new(placement: ToastPlacement, max_visible: u32) -> Self {
        crate::test_support::fresh_layout_runtime();
        telar::set_locale("en");
        telar::focus::clear();
        set_surface_size(SURFACE);
        let queue = Toasts::new();
        let item = toaster(
            ToasterProps::props()
                .toasts(queue)
                .placement(placement)
                .max_visible(max_visible)
                .build(),
            Children::default(),
        )
        .unwrap();
        let mut mounted = Self {
            tree: ComponentList::new(item),
            queue,
        };
        mounted.settle();
        mounted
    }

    fn settle(&mut self) {
        relayout_if_dirty();
        let _ = self.tree.commands();
        relayout_if_dirty();
    }

    fn send(&mut self, event: Event) {
        route(&mut self.tree, &event);
        self.settle();
    }

    fn click(&mut self, at: (f64, f64)) {
        self.send(press(at.0, at.1));
        self.send(release(at.0, at.1));
    }

    fn after(&mut self, elapsed: Duration) {
        advance_time(elapsed);
        self.settle();
    }

    fn drawn(&self) -> Vec<String> {
        texts(&self.tree)
    }

    fn shows(&self, text: &str) -> bool {
        self.drawn().iter().any(|drawn| drawn == text)
    }

    fn node(&self, name: &str) -> telar::AccessNode {
        ui_core::accessibility::snapshot(&self.tree.commands())
            .into_iter()
            .find(|node| node.role == Role::Button && node.name == name)
            .unwrap_or_else(|| panic!("a `{name}` button"))
    }

    fn button(&self, name: &str) -> (f64, f64) {
        centre(self.node(name).rect)
    }

    fn button_id(&self, name: &str) -> telar::focus::FocusId {
        self.node(name).id.expect("a button holds focus")
    }
}

#[test]
fn a_posted_notice_shows_its_title_and_message() {
    let mut mounted = Mounted::new(ToastPlacement::BottomEnd, 0);
    mounted
        .queue
        .push(Toast::new("Your changes are saved").titled("Saved"));
    mounted.settle();
    assert!(mounted.shows("Saved"));
    assert!(mounted.shows("Your changes are saved"));
}

#[test]
fn a_notice_puts_itself_away_when_its_time_is_up() {
    let mut mounted = Mounted::new(ToastPlacement::BottomEnd, 0);
    mounted
        .queue
        .push(Toast::new("Brief").lasting(Duration::from_secs(1)));
    mounted.queue.push(Toast::new("Stays").until_dismissed());
    mounted.settle();

    mounted.after(Duration::from_millis(500));
    assert!(mounted.shows("Brief"));

    mounted.after(Duration::from_secs(2));
    assert!(!mounted.shows("Brief"));
    assert!(mounted.shows("Stays"), "one kept until dismissed stays");
    assert_eq!(mounted.queue.len(), 1);
}

#[test]
fn the_keyboard_on_the_stack_holds_every_clock() {
    let mut mounted = Mounted::new(ToastPlacement::BottomEnd, 0);
    mounted
        .queue
        .push(Toast::new("Read me").lasting(Duration::from_secs(1)));
    mounted
        .queue
        .push(Toast::new("And me").lasting(Duration::from_secs(1)));
    mounted.settle();
    let dismiss = mounted.button_id("Dismiss");
    telar::focus::request(dismiss);
    mounted.settle();

    mounted.after(Duration::from_secs(5));
    assert!(mounted.shows("Read me") && mounted.shows("And me"));

    telar::focus::clear();
    mounted.settle();
    mounted.after(Duration::from_secs(5));
    assert!(!mounted.shows("Read me") && !mounted.shows("And me"));
}

#[test]
fn the_pointer_resting_on_the_stack_holds_every_clock() {
    let mut mounted = Mounted::new(ToastPlacement::BottomEnd, 0);
    mounted
        .queue
        .push(Toast::new("Read me").lasting(Duration::from_secs(1)));
    mounted
        .queue
        .push(Toast::new("And me").lasting(Duration::from_secs(1)));
    mounted.settle();
    mounted.after(Duration::from_millis(600));

    let over = mounted.button("Dismiss");
    mounted.send(moved(over.0, over.1));
    mounted.after(Duration::from_secs(5));
    assert!(mounted.shows("Read me") && mounted.shows("And me"));

    mounted.send(moved(10.0, 10.0));
    mounted.after(Duration::from_millis(300));
    assert!(
        mounted.shows("Read me") && mounted.shows("And me"),
        "the time the pointer rested there does not count"
    );
    mounted.after(Duration::from_millis(200));
    assert!(!mounted.shows("Read me") && !mounted.shows("And me"));
}

#[test]
fn a_notice_arriving_under_a_resting_pointer_waits_too() {
    let mut mounted = Mounted::new(ToastPlacement::BottomEnd, 0);
    mounted.queue.push(Toast::new("First").until_dismissed());
    mounted.settle();
    let over = mounted.button("Dismiss");
    mounted.send(moved(over.0, over.1));

    mounted
        .queue
        .push(Toast::new("Second").lasting(Duration::from_secs(1)));
    mounted.settle();
    mounted.after(Duration::from_secs(5));
    assert!(mounted.shows("Second"));
}

#[test]
fn the_dismiss_button_takes_its_notice_down() {
    let mut mounted = Mounted::new(ToastPlacement::BottomEnd, 0);
    mounted.queue.push(Toast::new("Bye").until_dismissed());
    mounted.settle();
    let dismiss = mounted.button("Dismiss");
    mounted.click(dismiss);
    assert!(mounted.queue.is_empty());
    assert!(!mounted.shows("Bye"));
}

#[test]
fn an_action_runs_and_takes_its_notice_down() {
    let mut mounted = Mounted::new(ToastPlacement::BottomEnd, 0);
    let undone = Rc::new(Cell::new(false));
    let sink = undone.clone();
    mounted
        .queue
        .push(Toast::new("Deleted").with_action("Undo", move || sink.set(true)));
    mounted.settle();
    let undo = mounted.button("Undo");
    mounted.click(undo);
    assert!(undone.get());
    assert!(mounted.queue.is_empty());
}

#[test]
fn notices_past_the_limit_wait_their_turn() {
    let mut mounted = Mounted::new(ToastPlacement::BottomEnd, 2);
    let first = mounted.queue.push(Toast::new("One").until_dismissed());
    mounted.queue.push(Toast::new("Two").until_dismissed());
    mounted.queue.push(Toast::new("Three").until_dismissed());
    mounted.settle();
    assert!(mounted.shows("One") && mounted.shows("Two"));
    assert!(!mounted.shows("Three"));

    mounted.queue.dismiss(first);
    mounted.settle();
    assert!(mounted.shows("Three"));
}

#[test]
fn the_newest_notice_is_the_one_nearest_the_edge() {
    let order = |placement| {
        let mut mounted = Mounted::new(placement, 0);
        mounted.queue.push(Toast::new("Older").until_dismissed());
        mounted.queue.push(Toast::new("Newer").until_dismissed());
        mounted.settle();
        mounted
            .drawn()
            .into_iter()
            .filter(|text| text == "Older" || text == "Newer")
            .collect::<Vec<_>>()
    };
    assert_eq!(order(ToastPlacement::BottomEnd), ["Older", "Newer"]);
    assert_eq!(order(ToastPlacement::TopCenter), ["Newer", "Older"]);
}

#[test]
fn show_toast_posts_to_the_one_shared_queue() {
    crate::test_support::fresh_layout_runtime();
    let shared = toasts();
    shared.clear();
    let id = show_toast("Hello");
    assert_eq!(toasts().len(), 1);
    toasts().dismiss(id);
    assert!(shared.is_empty());
}

/// A toaster showing one notice, built after the caller installed its theme.
fn one_notice(props: ToasterProps, queue: Toasts) -> telar::testing::Scoped<ComponentList> {
    crate::test_support::fresh_layout_runtime();
    set_surface_size(SURFACE);
    let mut tree =
        telar::testing::scoped(|| ComponentList::new(toaster(props, Children::default()).unwrap()));
    queue.push(Toast::new("Saved").until_dismissed());
    for _ in 0..2 {
        relayout_if_dirty();
        let _ = tree.commands();
    }
    route(&mut tree, &moved(0.0, 0.0));
    tree
}

/// The notice's card: the last box drawn over the whole of the region a reader is told about.
fn card(tree: &ComponentList) -> (telar::Rect, RectStyle) {
    let nodes = ui_core::accessibility::snapshot(&tree.commands());
    let region = nodes
        .iter()
        .find(|node| node.role == Role::Status)
        .expect("the stack is announced");
    (
        region.rect,
        crate::test_support::painted_at(tree, region.rect),
    )
}

#[test]
fn a_notice_is_rounded_by_the_surface_role_until_the_tokens_name_a_radius() {
    let queue = Toasts::new();
    crate::test_support::assert_panel_corner_follows_its_role(
        crate::test_support::PanelRole::Surface,
        || one_notice(ToasterProps::props().toasts(queue).build(), queue),
        |tree| card(tree).1.radius,
        BorderRadius::all,
        |radius| ToastTokens {
            radius: Some(radius),
            ..ToastTokens::default()
        },
    );
}

#[test]
fn an_unset_width_is_the_tokens_and_a_named_one_wins() {
    telar::set_theme(crate::test_support::PanelTheme {
        role: crate::test_support::PanelRole::Surface,
        radius: 6.0,
        tokens: Some(ToastTokens {
            width: Some(300.0),
            ..ToastTokens::default()
        }),
    });
    let queue = Toasts::new();
    let tree = one_notice(ToasterProps::props().toasts(queue).build(), queue);
    assert_eq!(card(&tree).0.width, 300.0);
    drop(tree);

    let queue = Toasts::new();
    let tree = one_notice(
        ToasterProps::props().toasts(queue).width(240.0).build(),
        queue,
    );
    assert_eq!(card(&tree).0.width, 240.0, "the toaster's own width wins");
}

#[test]
fn a_style_amendment_restyles_every_notice() {
    let ink = Color::rgb(0.2, 0.6, 0.3);
    let queue = Toasts::new();
    let tree = one_notice(
        ToasterProps::props()
            .toasts(queue)
            .style(Rc::new(move |paint: RectStyle| paint.with_fill(ink)) as telar::SurfaceAmendment)
            .build(),
        queue,
    );
    let (_, paint) = card(&tree);
    assert_eq!(paint.fill, Some(ink.into()));
    assert!(paint.border.is_some(), "the notice's own frame is kept");
}
