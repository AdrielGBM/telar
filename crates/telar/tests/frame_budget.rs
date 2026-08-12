//! What a steady frame costs in allocations.
//!
//! Nothing forces you to look at this until something drives the loop at a sustained 60 Hz, and by then a
//! few hundred bytes of per-frame garbage is a hitch you cannot find by reading. The guard is not a number
//! — numbers rot across compilers and allocators — but a shape: **two identical frames must cost the same**.
//! A cost that climbs is the signature of the bug worth catching, an allocation made per frame where the
//! last one should have been reused.
//!
//! Its own test binary because a counting global allocator is process-wide.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use telar::{
    Color, Component, LayoutStyle, LocalTree, RectStyle, Rectangle, SizeDimension, Text, TextStyle,
    UiTree, new_container, reset_layout_runtime, signal,
};

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static COUNTING: AtomicUsize = AtomicUsize::new(0);

struct Counting;

// Counts bytes rather than calls: a Vec that doubles once is one call and a real cost, while a handful of small boxes are several calls and almost none. Bytes is what turns into a hitch.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) == 1 {
            ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

fn measure(mut body: impl FnMut()) -> usize {
    ALLOCATED.store(0, Ordering::Relaxed);
    COUNTING.store(1, Ordering::Relaxed);
    body();
    COUNTING.store(0, Ordering::Relaxed);
    ALLOCATED.load(Ordering::Relaxed)
}

/// A game's chrome: a few static panels around a counter that changes every frame. Deliberately not big —
/// the question is whether a frame's cost is *stable*, and a small tree makes a per-frame allocation stand
/// out instead of hiding inside a large one.
fn chrome() -> (Box<dyn Component>, telar::RwSignal<i32>) {
    reset_layout_runtime();
    let ticks = signal(0);

    let counter = Text::new(
        {
            let ticks = ticks.clone();
            move || format!("{}", ticks.get())
        },
        LayoutStyle::new(),
        || TextStyle::new(14.0, Color::rgb(0.9, 0.9, 0.9)),
    )
    .expect("text builds");

    let mut children: Vec<Box<dyn telar::LayoutItem>> = vec![Box::new(counter)];
    for _ in 0..8 {
        children.push(Box::new(
            Rectangle::new(LayoutStyle::new().width(40.0).height(12.0), || {
                RectStyle::filled(Color::rgb(0.2, 0.2, 0.25), 2.0)
            })
            .expect("rectangle builds"),
        ));
    }

    let nodes: Vec<telar::NodeId> = children.iter().map(|c| c.layout_node()).collect();
    let root = new_container(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0)),
        &nodes,
    )
    .expect("container builds");
    let _ = root;

    struct Chrome {
        children: Vec<Box<dyn telar::LayoutItem>>,
    }

    impl Component for Chrome {
        fn view(&self) -> telar::RenderNode {
            telar::RenderNode::Group {
                children: telar::NodeVec::collect(self.children.iter().map(|c| c.view())),
            }
        }

        fn on_event(&mut self, event: &telar::Event) -> telar::EventResult {
            for child in &mut self.children {
                if child.on_event(event) == telar::EventResult::Handled {
                    return telar::EventResult::Handled;
                }
            }
            telar::EventResult::Ignored
        }

        fn debug_name(&self) -> &'static str {
            "Chrome"
        }
    }

    (Box::new(Chrome { children }), ticks)
}

/// An unchanged tree must cost nothing to ask again — that is the whole point of the dirty gate, and the
/// case a viewport hits constantly: its texture changes, its commands do not.
#[test]
fn asking_an_unchanged_tree_for_its_frame_allocates_nothing() {
    let (root, _ticks) = chrome();
    let tree = LocalTree::new(root);
    let _ = tree.frame();

    let cost = measure(|| {
        let _ = tree.frame();
    });
    assert_eq!(
        cost, 0,
        "a clean tree recomposed instead of serving its cache, at {cost} bytes a frame"
    );
}

/// Two values, alternated, and only ever these two. Same digit count so the text box never changes width,
/// and both strings are in every cache after the second frame — so nothing measured later can be the first
/// of anything.
///
/// Counting up instead minted a new string every frame. The caches therefore never stopped growing, and
/// which frame paid for a resize — or for the first three-digit box, the one the text gets wider on — came
/// out of the platform's font metrics. On macOS both landed on the frame being measured: 3 KB over the
/// steady cost, which the test could only read as a leak.
const EVEN: i32 = 2424;
const ODD: i32 = 4242;

fn advance(tree: &LocalTree, ticks: &telar::RwSignal<i32>, value: i32) {
    ticks.set(value);
    telar::relayout_if_dirty();
    let _ = tree.frame();
}

/// The real guard: a frame whose content changed costs the same every time. Compared against a later frame
/// rather than a constant, so it survives a compiler that lays the tree out differently and still fails on
/// a per-frame allocation nobody meant to keep.
#[test]
fn a_changing_frame_costs_the_same_every_time() {
    let (root, ticks) = chrome();
    let tree = LocalTree::new(root);

    // Every pair leaves the tree on ODD, so both measured frames are the same transition, ODD -> EVEN.
    for _ in 0..16 {
        advance(&tree, &ticks, EVEN);
        advance(&tree, &ticks, ODD);
    }

    let early = measure(|| advance(&tree, &ticks, EVEN));

    for _ in 0..50 {
        advance(&tree, &ticks, EVEN);
        advance(&tree, &ticks, ODD);
    }

    let late = measure(|| advance(&tree, &ticks, EVEN));

    // Printed rather than asserted against: the number is the point of the file, and `--nocapture` is how you read it. Only its stability is a pass/fail matter.
    eprintln!("cost per changed frame: {early} bytes");
    assert_eq!(
        early, late,
        "frame cost moved from {early} to {late} bytes across a hundred frames, so something is \
         allocating per frame that should have been reused"
    );
}
