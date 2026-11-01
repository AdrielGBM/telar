//! What a leaf change costs layout in the one-window-per-layer scene, timed rather than asserted. Run it in a release build: `cargo test --release -p telar-ui-core --lib layout_cost -- --ignored --nocapture`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use layout_core::{AlignItems, AvailableSpace, LayoutStyle, SizeDimension, TemplateTrack};
use layout_reactive::layout_passes;
use platform_core::Event;
use reactive_core::{RwSignal, signal};
use renderer_core::{Color, RectStyle, Span, TextMetrics, TextStyle};
use ui_tree::ComponentList;

use super::WindowRoot;
use crate::container::Container;
use crate::context::{compute_layout, live_node_count, relayout_if_dirty, reset_layout_runtime};
use crate::layout_item::{LayoutItem, box_item};
use crate::styled_container::StyledContainer;
use crate::text::Text;

const WARMUP: usize = 300;
const ITERATIONS: usize = 3_000;
const CHIPS_PER_BAR: usize = 20;

static MEASURES: AtomicU64 = AtomicU64::new(0);

struct CountingMetrics;

impl TextMetrics for CountingMetrics {
    fn measure(
        &self,
        text: &str,
        spans: Option<&[Span]>,
        max_width: f32,
        style: &TextStyle,
    ) -> (f32, f32) {
        MEASURES.fetch_add(1, Ordering::Relaxed);
        renderer_text::ShaperMetrics.measure(text, spans, max_width, style)
    }

    fn min_content(&self, text: &str, spans: Option<&[Span]>, style: &TextStyle) -> (f32, f32) {
        MEASURES.fetch_add(1, Ordering::Relaxed);
        renderer_text::ShaperMetrics.min_content(text, spans, style)
    }

    fn ink_bounds(&self, text: &str, max_width: f32, style: &TextStyle) -> (f32, f32) {
        renderer_text::ShaperMetrics.ink_bounds(text, max_width, style)
    }

    fn line_height(&self, font_size: f32) -> f32 {
        renderer_text::ShaperMetrics.line_height(font_size)
    }
}

fn label(content: RwSignal<String>, size: f32) -> Box<dyn LayoutItem> {
    box_item(
        Text::new(
            move || content.get(),
            LayoutStyle::new(),
            move || TextStyle::new(size, Color::WHITE),
        )
        .unwrap(),
    )
}

fn chip(content: RwSignal<String>) -> Box<dyn LayoutItem> {
    box_item(
        StyledContainer::new(
            LayoutStyle::new()
                .flex_row()
                .align_items(AlignItems::CENTER)
                .padding_horizontal(10.0)
                .padding_vertical(4.0),
            |_| RectStyle::filled(Color::rgba(1.0, 1.0, 1.0, 0.1), 8.0),
            vec![label(content, 13.0)],
        )
        .unwrap(),
    )
}

fn spacer() -> Box<dyn LayoutItem> {
    box_item(Container::new(LayoutStyle::new().flex_grow(1.0), vec![]).unwrap())
}

fn group(chips: Vec<Box<dyn LayoutItem>>) -> Box<dyn LayoutItem> {
    box_item(
        Container::new(
            LayoutStyle::new()
                .flex_row()
                .align_items(AlignItems::CENTER)
                .gap(6.0),
            chips,
        )
        .unwrap(),
    )
}

/// A shell bar: left, centre and right groups of chips split by flexible spacers, one chip per label.
fn bar(labels: &[RwSignal<String>]) -> Box<dyn LayoutItem> {
    let third = labels.len() / 3;
    let chips = |range: &[RwSignal<String>]| range.iter().map(|&l| chip(l)).collect();
    box_item(
        StyledContainer::new(
            LayoutStyle::new()
                .flex_row()
                .align_items(AlignItems::CENTER)
                .width(SizeDimension::Percent(1.0))
                .height(36.0)
                .padding_horizontal(6.0),
            |_| RectStyle::filled(Color::rgba(0.1, 0.1, 0.1, 0.9), 0.0),
            vec![
                group(chips(&labels[..third])),
                spacer(),
                group(chips(&labels[third..labels.len() - third])),
                spacer(),
                group(chips(&labels[labels.len() - third..])),
            ],
        )
        .unwrap(),
    )
}

fn card(index: usize) -> Box<dyn LayoutItem> {
    let text = |s: String, size| label(signal(s), size);
    box_item(
        StyledContainer::new(
            LayoutStyle::new().flex_column().padding_all(12.0).gap(6.0),
            |_| RectStyle::filled(Color::rgba(0.2, 0.2, 0.2, 0.9), 12.0),
            vec![
                text(format!("Card {index}"), 15.0),
                text("Now playing — Some Artist".into(), 13.0),
                text("A longer line that wraps inside the card when the card is narrow enough to force it onto a second line".into(), 12.0),
            ],
        )
        .unwrap(),
    )
}

fn grid() -> Box<dyn LayoutItem> {
    let cell = |i: usize| {
        box_item(
            StyledContainer::new(
                LayoutStyle::new().flex_column().padding_all(10.0).gap(4.0),
                |_| RectStyle::filled(Color::rgba(0.2, 0.2, 0.2, 0.9), 10.0),
                vec![
                    label(signal(format!("Widget {i}")), 12.0),
                    label(signal(format!("{}%", i * 11)), 20.0),
                ],
            )
            .unwrap(),
        )
    };
    box_item(
        Container::new(
            LayoutStyle::new()
                .display_grid()
                .grid_template_columns((0..3).map(|_| TemplateTrack::fr(1.0)).collect())
                .gap(8.0)
                .width(420.0),
            (0..9).map(cell).collect(),
        )
        .unwrap(),
    )
}

struct Scene {
    tree: ComponentList,
    root: layout_core::NodeId,
    clock: RwSignal<String>,
    network: RwSignal<String>,
}

/// One fullscreen root holding everything a wlr layer shows: two bars of chips, a column of cards and a grid of widgets.
fn layer_scene() -> Scene {
    let labels = |prefix: &str| -> Vec<RwSignal<String>> {
        (0..CHIPS_PER_BAR)
            .map(|i| signal(format!("{prefix} {i}")))
            .collect()
    };
    let top = labels("ws");
    let bottom = labels("app");
    let clock = top[CHIPS_PER_BAR - 1];
    clock.set("12:34".into());
    let network = top[CHIPS_PER_BAR - 2];
    network.set("Wi-Fi".into());
    let cards = box_item(
        Container::new(
            LayoutStyle::new().flex_column().gap(10.0).width(360.0),
            (0..6).map(card).collect(),
        )
        .unwrap(),
    );
    let middle = box_item(
        Container::new(
            LayoutStyle::new()
                .flex_row()
                .flex_grow(1.0)
                .justify_content(layout_core::JustifyContent::SPACE_BETWEEN)
                .padding_all(12.0),
            vec![cards, grid()],
        )
        .unwrap(),
    );
    let content = Container::new(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0)),
        vec![bar(&top), middle, bar(&bottom)],
    )
    .unwrap();
    let root = content.layout_node();
    let mut tree = ComponentList::new(WindowRoot::new(box_item(content)));
    tree.on_event(&Event::WindowResized {
        width: 1920,
        height: 1080,
    });
    let _ = tree.commands();
    Scene {
        tree,
        root,
        clock,
        network,
    }
}

struct Sample {
    micros: Vec<f64>,
    passes: u64,
    measures: u64,
}

/// Times `relayout` alone, after `change` has dirtied what it dirties, and repaints between iterations so rect subscribers are live as in a frame.
fn sample(
    tree: &ComponentList,
    mut change: impl FnMut(usize),
    mut relayout: impl FnMut(usize),
) -> Sample {
    let mut run = |iterations: usize| {
        let mut sample = Sample {
            micros: Vec::with_capacity(iterations),
            passes: 0,
            measures: 0,
        };
        for i in 0..iterations {
            change(i);
            let passes = layout_passes();
            let measures = MEASURES.load(Ordering::Relaxed);
            let start = Instant::now();
            relayout(i);
            sample.micros.push(start.elapsed().as_secs_f64() * 1e6);
            sample.passes += layout_passes() - passes;
            sample.measures += MEASURES.load(Ordering::Relaxed) - measures;
            let _ = tree.commands();
        }
        sample
    };
    run(WARMUP);
    run(ITERATIONS)
}

/// Flips `leaf` between two strings and times the relayout each flip costs, the way the runner's frame asks for it.
fn leaf_change(tree: &ComponentList, leaf: RwSignal<String>, values: [&str; 2]) -> Sample {
    sample(
        tree,
        |i| leaf.set(values[i % 2].to_string()),
        |_| relayout_if_dirty(),
    )
}

fn report(name: &str, mut sample: Sample) -> f64 {
    sample.micros.sort_by(f64::total_cmp);
    let at = |q: f64| sample.micros[((sample.micros.len() - 1) as f64 * q).round() as usize];
    let iterations = sample.micros.len() as f64;
    println!(
        "{name:<42} median {:>6.1} µs  p10 {:>6.1}  p90 {:>6.1}  p99 {:>6.1}  min {:>6.1}  max {:>7.1}  passes/iter {:.2}  text measures/iter {:.1}",
        at(0.5),
        at(0.1),
        at(0.9),
        at(0.99),
        at(0.0),
        at(1.0),
        sample.passes as f64 / iterations,
        sample.measures as f64 / iterations,
    );
    at(0.5)
}

/// One root, relying on taffy's cache, against the 0.5 ms budget, beside what the same bar costs as a root of its own.
#[test]
#[ignore = "a timing, not a check: run it in release with --ignored --nocapture"]
fn layout_cost_of_a_leaf_change_in_one_fullscreen_root() {
    renderer_core::set_text_metrics(CountingMetrics);
    reset_layout_runtime();

    let scene = layer_scene();
    println!(
        "scene: one 1920x1080 WindowRoot, {} layout nodes, 2 bars x {CHIPS_PER_BAR} chips, 6 cards, 3x3 grid; {ITERATIONS} iterations after {WARMUP} warm-up",
        live_node_count()
    );
    report(
        "one root, clean frame",
        sample(&scene.tree, |_| {}, |_| relayout_if_dirty()),
    );
    let tick = report(
        "one root, clock tick (same width)",
        leaf_change(&scene.tree, scene.clock, ["12:34", "12:43"]),
    );
    let grow = report(
        "one root, chip width change",
        leaf_change(
            &scene.tree,
            scene.network,
            ["Wi-Fi", "Wi-Fi · HomeNetwork 5G"],
        ),
    );
    report(
        "one root, window width 1919<->1920",
        sample(
            &scene.tree,
            |_| {},
            |i| {
                let width = if i % 2 == 0 { 1919.0 } else { 1920.0 };
                compute_layout(
                    scene.root,
                    AvailableSpace::Definite(width),
                    AvailableSpace::Definite(1080.0),
                )
                .unwrap();
            },
        ),
    );
    drop(scene);

    reset_layout_runtime();
    let labels: Vec<RwSignal<String>> = (0..CHIPS_PER_BAR)
        .map(|i| signal(format!("ws {i}")))
        .collect();
    let clock = labels[CHIPS_PER_BAR - 1];
    clock.set("12:34".into());
    let network = labels[CHIPS_PER_BAR - 2];
    network.set("Wi-Fi".into());
    let bar_root = bar(&labels);
    compute_layout(
        bar_root.layout_node(),
        AvailableSpace::Definite(1920.0),
        AvailableSpace::Definite(36.0),
    )
    .unwrap();
    let tree = ComponentList::new(bar_root);
    let _ = tree.commands();
    println!("bar alone: {} layout nodes", live_node_count());
    report(
        "bar as its own root, clock tick",
        leaf_change(&tree, clock, ["12:34", "12:43"]),
    );
    report(
        "bar as its own root, chip width change",
        leaf_change(&tree, network, ["Wi-Fi", "Wi-Fi · HomeNetwork 5G"]),
    );

    println!(
        "verdict: worst leaf-change median {:.3} ms against the 0.5 ms budget",
        tick.max(grow) / 1000.0
    );
}
