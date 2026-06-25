//! Sprint 0 / T-1.1 baseline (F010): cost of a SINGLE signal change re-running the WHOLE tree's
//! `view()` + `flatten_view`. Today the app is one `ComponentSlot`/effect, so any tracked signal
//! re-walks the entire tree. This captures that baseline; the fine-grained-effects refactor (T-1.1)
//! should turn it into O(affected component). Keep this bench to compare before/after.

use std::hint::black_box;
use std::rc::Rc;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use geometry_core::Rect;
use reactive_core::{RwSignal, create_rw_signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, TextStyle};
use ui_tree::{Component, ComponentList, NodeVec, RenderNode, Segment, SegmentRoot};

/// One card's render structure, shared by the monolithic and segmented benches so command counts match.
fn card(row: usize, t: f32) -> RenderNode {
    let y = row as f32 * 60.0;
    let mut cells: Vec<RenderNode> = (0..8)
        .map(|cidx| {
            RenderNode::rect(
                Rect::new(cidx as f32 * 80.0, 0.0, 72.0, 48.0),
                RectStyle::default()
                    .with_fill(Color::BLACK)
                    .with_radius(BorderRadius::all(6.0)),
            )
        })
        .collect();
    if row == 0 {
        cells.push(RenderNode::rect(
            Rect::new(t, 0.0, 4.0, 48.0),
            RectStyle::default().with_fill(Color::WHITE),
        ));
    }
    cells.push(RenderNode::text(
        "label",
        Rect::new(0.0, 50.0, 200.0, 16.0),
        TextStyle::new(14.0, Color::BLACK),
    ));
    RenderNode::Clip {
        rect: Rect::new(0.0, y, 640.0, 56.0),
        radius: BorderRadius::zero(),
        children: NodeVec::collect([RenderNode::Transform {
            matrix: [1.0, 0.0, 0.0, 1.0, 0.0, y],
            children: NodeVec::collect(cells),
        }]),
    }
}

/// A representative UI tree: `rows` cards, each a Clip → Transform → group of `cols` rects + a label.
/// One cell of the first card is driven by `tick`, so a single signal write must re-run the whole
/// monolithic `view()` (every other card is recomputed even though nothing about it changed).
struct CardList {
    tick: RwSignal<i32>,
    rows: usize,
    cols: usize,
}

impl Component for CardList {
    fn view(&self) -> RenderNode {
        let t = self.tick.get() as f32;
        let _ = self.cols;
        RenderNode::group((0..self.rows).map(|r| card(r, t)))
    }
}

/// Segmented equivalent: each card is its own reactive segment. Updating the active card's signal
/// must re-run only that segment's view() (+ a cheap O(total) recompose), not the whole tree.
struct CardSegment {
    tick: RwSignal<i32>,
    row: usize,
}

impl Component for CardSegment {
    fn view(&self) -> RenderNode {
        card(self.row, self.tick.get() as f32)
    }
}

struct CardParent {
    children: Vec<Rc<Segment>>,
}

impl Component for CardParent {
    fn view(&self) -> RenderNode {
        RenderNode::group(self.children.iter().map(|s| s.boundary()))
    }
}

fn bench_single_signal_segmented(c: &mut Criterion) {
    let mut group = c.benchmark_group("tree_single_signal_segmented");
    for &rows in &[20usize, 100, 400] {
        let ticks: Vec<RwSignal<i32>> = (0..rows).map(|_| create_rw_signal(0i32)).collect();
        let children: Vec<Rc<Segment>> = (0..rows)
            .map(|r| {
                Segment::mount(CardSegment {
                    tick: ticks[r].clone(),
                    row: r,
                })
            })
            .collect();
        let root = SegmentRoot::mount(CardParent { children });
        let n = root.commands().len();
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{rows}rows_{n}cmds")),
            &rows,
            |b, _| {
                let mut t = 0i32;
                b.iter(|| {
                    t = t.wrapping_add(1);
                    ticks[0].set(t); // only the first card's segment re-runs
                    black_box(root.commands().len());
                });
            },
        );
    }
    group.finish();
}

fn bench_single_signal_reflatten(c: &mut Criterion) {
    let mut group = c.benchmark_group("tree_single_signal");
    for &rows in &[20usize, 100, 400] {
        let tick = create_rw_signal(0i32);
        let tree = ComponentList::new(CardList {
            tick: tick.clone(),
            rows,
            cols: 8,
        });
        let n = tree.commands().len(); // warm: first flatten builds the vec
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{rows}rows_{n}cmds")),
            &rows,
            |b, _| {
                let mut t = 0i32;
                b.iter(|| {
                    t = t.wrapping_add(1);
                    tick.set(t); // re-runs the whole monolithic view() + flatten_view
                    black_box(tree.commands().len());
                });
            },
        );
    }
    group.finish();
}

/// T-2.1 baseline (F014): scrollable content wrapped in Clip → Transform(offset) → items.
struct ScrollContent {
    offset: RwSignal<f32>,
    items: usize,
}

impl Component for ScrollContent {
    fn view(&self) -> RenderNode {
        let off = self.offset.get();
        let cells = (0..self.items).map(|i| {
            RenderNode::rect(
                Rect::new(0.0, i as f32 * 24.0, 300.0, 20.0),
                RectStyle::default().with_fill(Color::BLACK),
            )
        });
        RenderNode::Clip {
            rect: Rect::new(0.0, 0.0, 320.0, 600.0),
            radius: BorderRadius::zero(),
            children: NodeVec::collect([RenderNode::Transform {
                matrix: [1.0, 0.0, 0.0, 1.0, 0.0, -off],
                children: NodeVec::collect(cells),
            }]),
        }
    }
}

fn bench_scroll_tick(c: &mut Criterion) {
    let mut group = c.benchmark_group("scroll_tick");
    for &items in &[100usize, 1000] {
        // (a) Current behaviour: a scroll-offset signal change re-runs content.view() + flatten.
        let offset = create_rw_signal(0.0f32);
        let tree = ComponentList::new(ScrollContent {
            offset: offset.clone(),
            items,
        });
        let n = tree.commands().len();
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("full_reflatten", items), &items, |b, _| {
            let mut o = 0.0f32;
            b.iter(|| {
                o += 1.0;
                offset.set(o); // re-runs the whole content view() + flatten
                black_box(tree.commands().len());
            });
        });

        // (b) T-2.1 Part 1 target: only the PushMatrix changes, so a scroll tick just rewrites that one command in place — O(1) regardless of item count. Proxy on a plain command vec since ComponentList owns its cache internally.
        let mut cached: Vec<renderer_core::DrawCommand> = tree.commands().iter().cloned().collect();
        let mtx_idx = cached
            .iter()
            .position(|c| matches!(c, renderer_core::DrawCommand::PushMatrix { .. }))
            .unwrap();
        group.bench_with_input(BenchmarkId::new("matrix_only", items), &items, |b, _| {
            let mut o = 0.0f32;
            b.iter(|| {
                o += 1.0;
                cached[mtx_idx] = renderer_core::DrawCommand::PushMatrix {
                    matrix: [1.0, 0.0, 0.0, 1.0, 0.0, -o],
                };
                black_box(cached.len());
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_single_signal_reflatten,
    bench_single_signal_segmented,
    bench_scroll_tick
);
criterion_main!(benches);
