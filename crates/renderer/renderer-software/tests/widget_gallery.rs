//! Headless render gallery for the ui-components catalogue: builds the inline form widgets in visible
//! states and renders them to a PNG for eyeballing. Runs in CI (asserts it renders with content); pass
//! TELAR_WIDGETS_OUT=/path.png to also dump the image.

use layout_core::{AvailableSpace, LayoutStyle};
use platform_headless::HeadlessWindow;
use reactive_core::signal;
use renderer_core::TextStyle;
use renderer_core::{Color, RenderBackend};
use telar_renderer_software::{SoftwareRenderer, SoftwareRendererConfig};
use ui_components::{
    CheckboxProps, RadioProps, SliderProps, TextFieldProps, ToggleProps, checkbox, radio, slider,
    text_field, toggle,
};
use ui_components::{ModalProps, modal};
use ui_core::{
    Container, LayoutItem, Slots, Text, box_item, compute_layout, new_container, relayout_if_dirty,
    reset_layout_runtime,
};
use ui_tree::ComponentList;

#[test]
fn form_widgets_render() {
    let (w, h) = (360u32, 400u32);
    reset_layout_runtime();

    // Each widget in a clearly-visible state so the PNG shows selected/checked/filled looks.
    let cb = checkbox(CheckboxProps {
        checked: Some(signal(true)),
        label: Box::new(|| "I agree to the terms".to_string()),
        ..Default::default()
    })
    .unwrap();
    let tg = toggle(ToggleProps {
        checked: Some(signal(true)),
        label: Box::new(|| "Notifications on".to_string()),
        ..Default::default()
    })
    .unwrap();
    let choice = signal(1u32);
    let r0 = radio(RadioProps {
        selected: Some(choice.clone()),
        value: 0,
        label: Box::new(|| "Small".to_string()),
        ..Default::default()
    })
    .unwrap();
    let r1 = radio(RadioProps {
        selected: Some(choice.clone()),
        value: 1,
        label: Box::new(|| "Medium (selected)".to_string()),
        ..Default::default()
    })
    .unwrap();
    let sl = slider(SliderProps {
        value: Some(signal(0.62)),
        width: 260.0,
        ..Default::default()
    })
    .unwrap();
    let tf = text_field(TextFieldProps {
        value: Some(signal("Ada".to_string())),
        label: Box::new(|| "Name".to_string()),
        width: 260.0,
        ..Default::default()
    })
    .unwrap();
    // A SECOND field (like the demo's two): placeholder-only, its own signal. Both must render.
    let tf2 = text_field(TextFieldProps {
        value: Some(signal(String::new())),
        placeholder: Box::new(|| "Search…".to_string()),
        width: 260.0,
        ..Default::default()
    })
    .unwrap();

    let col = Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(16.0)
            .padding_all(20.0)
            .width(w as f32)
            .height(h as f32),
        vec![
            Box::new(cb),
            Box::new(tg),
            Box::new(r0),
            Box::new(r1),
            Box::new(sl),
            Box::new(tf),
            Box::new(tf2),
        ],
    )
    .unwrap();
    let root = col.layout_node();
    compute_layout(
        root,
        AvailableSpace::Definite(w as f32),
        AvailableSpace::Definite(h as f32),
    )
    .unwrap();

    let tree = ComponentList::new(col);
    let mut renderer = SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(
        w,
        h,
        SoftwareRendererConfig::default(),
    );
    renderer.begin_frame(w, h, 1.0, 0).unwrap();
    renderer
        .render_frame(&tree.commands(), Some(Color::from_rgb_u8(244, 245, 248)))
        .unwrap();
    let rgba = renderer.read_rgba().expect("pixmap after a frame");
    assert_eq!(rgba.len(), (w * h * 4) as usize);
    assert!(
        rgba.chunks_exact(4).any(|px| px[0] != 244),
        "expected widgets to draw content over the clear color"
    );
    if let Ok(out) = std::env::var("TELAR_WIDGETS_OUT") {
        image::RgbaImage::from_raw(w, h, rgba.to_vec())
            .expect("rgba len")
            .save(&out)
            .expect("write PNG");
        eprintln!("wrote {out}");
    }
}

#[test]
fn modal_renders_over_a_page() {
    let (w, h) = (420u32, 300u32);
    reset_layout_runtime();
    let open = signal(false);

    let body = Text::new(
        || "Are you sure you want to continue?".to_string(),
        LayoutStyle::new().height(20.0),
        || TextStyle::new(14.0, Color::from_rgb_u8(40, 42, 52)),
    )
    .unwrap();
    let mut slots = Slots::new();
    slots.push(None, box_item(body));
    let dialog = modal(
        ModalProps {
            open: Some(open.clone()),
            title: Box::new(|| "Confirm action".to_string()),
            ..Default::default()
        },
        slots,
    )
    .unwrap();

    // A parent-less root computed against the window registers the overlay host the portal attaches to.
    let root = new_container(
        LayoutStyle::new()
            .flex_column()
            .width(w as f32)
            .height(h as f32),
        &[dialog.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(w as f32),
        AvailableSpace::Definite(h as f32),
    )
    .unwrap();
    let tree = ComponentList::new(dialog);

    open.set(true);
    relayout_if_dirty();

    let mut renderer = SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(
        w,
        h,
        SoftwareRendererConfig::default(),
    );
    renderer.begin_frame(w, h, 1.0, 0).unwrap();
    // Clear to a "page" colour so the translucent scrim + opaque dialog are visible.
    renderer
        .render_frame(&tree.commands(), Some(Color::from_rgb_u8(238, 240, 245)))
        .unwrap();
    let rgba = renderer.read_rgba().expect("pixmap after a frame");
    assert!(
        rgba.chunks_exact(4).any(|px| px[0] != 238),
        "expected the modal to draw over the page"
    );
    if let Ok(out) = std::env::var("TELAR_MODAL_OUT") {
        image::RgbaImage::from_raw(w, h, rgba.to_vec())
            .expect("rgba len")
            .save(&out)
            .expect("write PNG");
        eprintln!("wrote {out}");
    }
}

#[test]
fn select_open_renders() {
    use platform_core::{Event, PointerButton, PointerSource};
    use ui_components::{SelectProps, select};
    use ui_core::{EventResult, dispatch_overlays, track_layout};

    let (w, h) = (300u32, 240u32);
    reset_layout_runtime();
    let picked = signal(1u32);
    let sel = select(SelectProps {
        selected: Some(picked.clone()),
        options: vec!["Small", "Medium", "Large"],
        ..Default::default()
    })
    .unwrap();
    let trigger_node = sel.layout_node();
    let trigger_rect = track_layout(trigger_node).unwrap();
    // A parent-less root registers the overlay host so the dropdown portals to the viewport.
    let root = new_container(
        LayoutStyle::new()
            .flex_column()
            .padding_all(16.0)
            .width(w as f32)
            .height(h as f32),
        &[trigger_node],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(w as f32),
        AvailableSpace::Definite(h as f32),
    )
    .unwrap();

    let mut tree = ComponentList::new(sel);
    let _ = tree.commands();
    // Open the dropdown by tapping the trigger centre (route like the runner: overlays first, else tree).
    let r = trigger_rect.get();
    let (cx, cy) = ((r.x + r.width / 2.0) as f64, (r.y + r.height / 2.0) as f64);
    for ev in [
        Event::PointerPressed {
            x: cx,
            y: cy,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        },
        Event::PointerReleased {
            x: cx,
            y: cy,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        },
    ] {
        if dispatch_overlays(&ev) == EventResult::Ignored {
            tree.on_event(&ev);
        }
    }
    relayout_if_dirty();

    let mut renderer = SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(
        w,
        h,
        SoftwareRendererConfig::default(),
    );
    renderer.begin_frame(w, h, 1.0, 0).unwrap();
    renderer
        .render_frame(&tree.commands(), Some(Color::from_rgb_u8(244, 245, 248)))
        .unwrap();
    let rgba = renderer.read_rgba().expect("pixmap");
    if let Ok(out) = std::env::var("TELAR_SELECT_OUT") {
        image::RgbaImage::from_raw(w, h, rgba.to_vec())
            .expect("rgba")
            .save(&out)
            .expect("png");
        eprintln!("wrote {out}");
    }
}
