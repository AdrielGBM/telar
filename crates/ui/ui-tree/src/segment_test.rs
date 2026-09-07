use geometry_core::Rect;
use reactive_core::{RwSignal, signal};
use renderer_core::{Color, RectStyle, ShapeStyle};

use super::*;

fn rect(x: f32) -> RenderNode {
    RenderNode::rect(
        Rect::new(x, 0.0, 10.0, 10.0),
        RectStyle::default().with_fill(Color::BLACK),
    )
}

struct Leaf {
    x: RwSignal<f32>,
}
impl Component for Leaf {
    fn view(&self) -> RenderNode {
        RenderNode::group([rect(self.x.get()), rect(self.x.get() + 5.0)])
    }
}

struct Parent {
    children: Vec<Rc<Segment>>,
}
impl Component for Parent {
    fn view(&self) -> RenderNode {
        RenderNode::group(self.children.iter().map(|s| s.boundary()))
    }
}

struct Nested;
impl Component for Nested {
    fn view(&self) -> RenderNode {
        RenderNode::group([
            rect(0.0),
            RenderNode::group([rect(1.0), RenderNode::Empty, RenderNode::group([rect(2.0)])]),
            rect(3.0),
        ])
    }
}

#[test]
fn flatten_nested_groups_and_empties() {
    let root = SegmentRoot::mount(Nested);
    assert_eq!(root.commands().len(), 4);
}

#[test]
fn composes_children_in_order() {
    let a = signal(0.0f32);
    let b = signal(100.0f32);
    let (sa, sb) = (a, b);
    let children = vec![
        Segment::mount(Leaf { x: sa }),
        Segment::mount(Leaf { x: sb }),
    ];
    let root = SegmentRoot::mount(Parent { children });
    assert_eq!(root.commands().len(), 4);
}

fn cmd_x(c: &DrawCommand) -> f32 {
    match c {
        DrawCommand::Rect { rect, .. } => rect.x,
        _ => -1.0,
    }
}

struct WithOverlay;
impl Component for WithOverlay {
    fn view(&self) -> RenderNode {
        RenderNode::group([rect(1.0), RenderNode::overlay([rect(2.0)]), rect(3.0)])
    }
}

#[test]
fn overlay_hoists_to_end() {
    let root = SegmentRoot::mount(WithOverlay);
    let cmds = root.commands();
    let xs: Vec<f32> = cmds.iter().map(cmd_x).collect();
    assert_eq!(xs, vec![1.0, 3.0, 2.0]);
}

struct OverlayParent {
    child: Rc<Segment>,
}
impl Component for OverlayParent {
    fn view(&self) -> RenderNode {
        RenderNode::group([rect(1.0), RenderNode::overlay([self.child.boundary()])])
    }
}

#[test]
fn overlay_hoists_child_segment() {
    let child = Segment::mount(Leaf { x: signal(9.0) }); // emits rect(9), rect(14)
    let root = SegmentRoot::mount(OverlayParent { child });
    let cmds = root.commands();
    let xs: Vec<f32> = cmds.iter().map(cmd_x).collect();
    assert_eq!(xs, vec![1.0, 9.0, 14.0]);
}

#[test]
fn child_change_updates_output_without_parent_rerun() {
    let a = signal(0.0f32);
    let sa = a;
    let children = vec![Segment::mount(Leaf { x: sa })];
    let root = SegmentRoot::mount(Parent { children });
    let g0 = root.generation();
    let first_x = match &root.commands()[0] {
        DrawCommand::Rect { rect, .. } => rect.x,
        _ => unreachable!(),
    };
    assert_eq!(first_x, 0.0);

    a.set(42.0);
    assert_ne!(root.generation(), g0, "child change must bump generation");
    let new_x = match &root.commands()[0] {
        DrawCommand::Rect { rect, .. } => rect.x,
        _ => unreachable!(),
    };
    assert_eq!(new_x, 42.0, "composed output reflects the child update");
}

struct MemoLeaf {
    double: reactive_core::Memo<i32>,
}
impl Component for MemoLeaf {
    fn view(&self) -> RenderNode {
        rect(self.double.get() as f32)
    }
}

#[test]
fn signal_dependent_segment_updates_with_runner_batching() {
    use reactive_core::{begin_batch, end_batch};
    let a = signal(0.0f32);
    let sa = a;
    let root = SegmentRoot::mount(Leaf { x: sa });
    assert_eq!(animated_rect_x(&root), 0.0);
    begin_batch();
    a.set(42.0);
    end_batch();
    begin_batch();
    let mid = animated_rect_x(&root);
    end_batch();
    assert_eq!(
        mid, 42.0,
        "signal-reading segment must reflect the batched set"
    );
}

// Regression probe for the frozen memo: replicates the runner's exact batching.
#[test]
fn memo_dependent_segment_updates_with_runner_batching() {
    use reactive_core::{begin_batch, end_batch, memo};
    let count = signal(0i32);
    let count_mv = count;
    let double = memo(move || count_mv.get() * 2);
    let root = SegmentRoot::mount(MemoLeaf { double: double });
    assert_eq!(animated_rect_x(&root), 0.0);

    begin_batch();
    count.set(3);
    end_batch();
    begin_batch();
    let mid = animated_rect_x(&root);
    end_batch();
    assert_eq!(
        mid, 6.0,
        "memo-reading segment must reflect the flushed memo"
    );
}

struct ThemedButton {
    theme: RwSignal<f32>,
    sel: RwSignal<i32>,
}
impl Component for ThemedButton {
    fn view(&self) -> RenderNode {
        let c = self.theme.get(); // subscribe to theme
        self.sel.get(); // subscribe to sel
        RenderNode::rect(
            Rect::new(0.0, 0.0, 10.0, 10.0),
            RectStyle::default().with_fill(Color::rgba(c, c, c, 1.0)),
        )
    }
    fn on_event(&mut self, _event: &platform_core::Event) -> crate::component::EventResult {
        self.sel.update(|n| *n += 1); // a handler write, like is_hovered/selected
        crate::component::EventResult::Handled
    }
}

fn first_rect_r(root: &SegmentRoot) -> f32 {
    match &root.commands()[0] {
        DrawCommand::Rect { style, .. } => style.fill.unwrap().solid_color().r,
        _ => unreachable!(),
    }
}

// A segment must keep its reactive subscriptions across event dispatch, and the invariant guaranteeing it is that dispatch is batched: a signal written by a handler then flushes only after the widget's borrow is released. Unbatched, the write flushes mid-borrow, the segment's effect cannot borrow the widget to re-render, and the subscription is dropped.
#[test]
fn dispatch_must_be_batched_or_segment_drops_subscriptions() {
    use reactive_core::{batch, signal};

    {
        let theme = signal(0.2f32);
        let sel = signal(0i32);
        let widget = Rc::new(RefCell::new(ThemedButton {
            theme: theme,
            sel: sel,
        }));
        let render = {
            let w = Rc::clone(&widget);
            move || w.try_borrow().ok().map(|c| c.view())
        };
        let root = SegmentRoot::from_segment(Segment::mount_fn_named("Component", render));
        assert!(
            (first_rect_r(&root) - 0.2).abs() < 1e-6,
            "{}",
            first_rect_r(&root)
        );

        widget
            .borrow_mut()
            .on_event(&platform_core::Event::CursorLeft); // UNBATCHED write mid-borrow
        theme.set(0.9);
        assert!(
            (first_rect_r(&root) - 0.2).abs() < 1e-6,
            "unbatched dispatch must drop the theme subscription (frozen at old value)"
        );
    }

    {
        let theme = signal(0.2f32);
        let sel = signal(0i32);
        let widget = Rc::new(RefCell::new(ThemedButton {
            theme: theme,
            sel: sel,
        }));
        let render = {
            let w = Rc::clone(&widget);
            move || w.try_borrow().ok().map(|c| c.view())
        };
        let root = SegmentRoot::from_segment(Segment::mount_fn_named("Component", render));
        assert!(
            (first_rect_r(&root) - 0.2).abs() < 1e-6,
            "{}",
            first_rect_r(&root)
        );

        batch(|| {
            widget
                .borrow_mut()
                .on_event(&platform_core::Event::CursorLeft)
        });
        theme.set(0.9);
        assert!(
            (first_rect_r(&root) - 0.9).abs() < 1e-6,
            "batched dispatch must preserve the theme subscription (tracks new value)"
        );
    }
}

struct AnimatedLeaf {
    x: motion_core::Animated<f32>,
}
impl Component for AnimatedLeaf {
    fn view(&self) -> RenderNode {
        rect(self.x.get())
    }
}

fn animated_rect_x(root: &SegmentRoot) -> f32 {
    match &root.commands()[0] {
        DrawCommand::Rect { rect, .. } => rect.x,
        _ => unreachable!(),
    }
}

// A segment reading `Animated::get()` must see the ticker's interpolated value in the same `commands()` call once `tick` has run, mirroring the runner. A fixed base `Instant` advanced by explicit `Duration`s drives the tween deterministically, with no sleeps.
#[test]
fn animated_get_reflects_tick_in_commands_and_settles() {
    use std::time::Duration;
    use web_time::Instant;

    // The registry is thread-local, so other tests on a reused libtest thread must not leak active animations into this one.
    motion_core::reset();
    motion_core::set_scale(1.0);

    let anim = motion_core::Animated::new(
        0.0f32,
        motion_core::tween(Duration::from_millis(100), motion_core::Easing::Linear),
    );
    let root = SegmentRoot::mount(AnimatedLeaf { x: anim });

    assert_eq!(animated_rect_x(&root), 0.0);
    let g0 = root.generation();

    anim.retarget(10.0);
    assert!(
        motion_core::has_active(),
        "retarget must register an active animation"
    );

    let base = Instant::now();
    motion_core::tick(base);
    assert_eq!(
        root.generation(),
        g0,
        "the t0-establishing tick must not recompose"
    );
    assert_eq!(animated_rect_x(&root), 0.0);

    // `generation()` only bumps inside the lazy recompose, so read the value first and capture the generation right after: capturing it beforehand would show the stale pre-tick generation and make the assert vacuous.
    motion_core::tick(base + Duration::from_millis(50));
    let mid_x = animated_rect_x(&root);
    let g1 = root.generation();
    assert!(
        (mid_x - 5.0).abs() < 1e-3,
        "expected the midpoint of the tween, got {mid_x}"
    );
    assert_ne!(g1, g0, "an in-flight tick must bump the compose generation");

    motion_core::tick(base + Duration::from_millis(100));
    let end_x = animated_rect_x(&root);
    let g2 = root.generation();
    assert_eq!(end_x, 10.0);
    assert_ne!(g2, g1, "the settling tick must still bump the generation");
    assert!(
        !motion_core::has_active(),
        "a settled tween must deregister"
    );

    motion_core::tick(base + Duration::from_millis(200));
    assert_eq!(animated_rect_x(&root), 10.0);
    assert_eq!(
        root.generation(),
        g2,
        "a tick with no active animations must not bump the generation"
    );
}
