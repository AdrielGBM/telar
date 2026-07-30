//! End-to-end headless check for the `overlay` modal fix: builds a red background covered by a blue
//! overlay scrim, renders the real widget tree, and confirms (1) the overlay draws on top (compose still
//! hoists it), and (2) a tap at the center reaches the scrim and is blocked from the background behind it
//! (priority pointer routing). Runs in CI without an env var; pass `TELAR_VISUAL_OUT` to also dump a PNG.

use layout_core::{AvailableSpace, LayoutStyle};
use platform_core::{Event, PointerButton, PointerSource};
use platform_headless::HeadlessWindow;
use reactive_core::{RwSignal, signal};
use renderer_core::{Color, RectStyle, RenderBackend, ShapeStyle};
use renderer_software::{SoftwareRenderer, SoftwareRendererConfig};
use ui_core::{LayoutItem, Overlay, StyledContainer, compute_layout, reset_layout_runtime};
use ui_tree::ComponentList;

fn filled(rgb: (u8, u8, u8), flag: RwSignal<bool>) -> StyledContainer {
    let fill = Color::from_rgb_u8(rgb.0, rgb.1, rgb.2);
    StyledContainer::new(
        LayoutStyle::new().width(200.0).height(200.0),
        move |_r| RectStyle::default().with_fill(fill),
        vec![],
    )
    .unwrap()
    .on_press(move || flag.set(true))
}

fn press(x: f64, y: f64) -> Event {
    Event::PointerPressed {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}
fn release(x: f64, y: f64) -> Event {
    Event::PointerReleased {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

#[test]
fn overlay_draws_on_top_and_captures_the_tap() {
    let (w, h) = (200u32, 200u32);
    reset_layout_runtime();

    let bg_clicked = signal(false);
    let overlay_clicked = signal(false);

    let background = filled((200, 60, 60), bg_clicked.clone()); // red
    let scrim = filled((60, 60, 200), overlay_clicked.clone()); // blue
    let overlay = Overlay::new(LayoutStyle::new(), vec![Box::new(scrim)]).unwrap();

    let root = StyledContainer::new(
        LayoutStyle::new().flex_column().width(200.0).height(200.0),
        |_r| RectStyle::default(),
        vec![Box::new(background), Box::new(overlay)],
    )
    .unwrap();
    let root_node = root.layout_node();
    compute_layout(
        root_node,
        AvailableSpace::Definite(w as f32),
        AvailableSpace::Definite(h as f32),
    )
    .unwrap();

    let mut tree = ComponentList::new(root);

    // Render the composed tree headless.
    let mut renderer = SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(
        w,
        h,
        SoftwareRendererConfig::default(),
    );
    renderer.begin_frame(w, h, 1.0, 0).unwrap();
    renderer
        .render_frame(&tree.commands(), Some(Color::BLACK))
        .unwrap();
    let rgba = renderer.read_rgba().expect("pixmap exists after a frame");
    if let Ok(out) = std::env::var("TELAR_VISUAL_OUT") {
        image::RgbaImage::from_raw(w, h, rgba.to_vec())
            .expect("rgba length matches w*h*4")
            .save(&out)
            .expect("write PNG");
        eprintln!("wrote {out}");
    }

    // The center pixel must be the blue scrim, not the red background: the overlay hoisted above it.
    let center = (((h / 2) * w + w / 2) * 4) as usize;
    let (r, _g, b) = (rgba[center], rgba[center + 1], rgba[center + 2]);
    assert!(
        b > 150 && r < 120,
        "overlay scrim must be drawn on top of the background (got r={r} b={b})"
    );

    // A tap at the center: the overlay must receive it and block it from the background it covers.
    // Mirror the runner: consult the overlay registry first, walk the tree only if nothing consumed it.
    let mut route = |event: &Event| {
        if ui_tree::dispatch_overlays(event) == ui_tree::EventResult::Ignored {
            tree.on_event(event);
        }
    };
    route(&press(100.0, 100.0));
    route(&release(100.0, 100.0));
    assert!(
        overlay_clicked.get(),
        "the tap must reach the overlay scrim"
    );
    assert!(
        !bg_clicked.get(),
        "the overlay must block the tap from the background behind it"
    );
}
