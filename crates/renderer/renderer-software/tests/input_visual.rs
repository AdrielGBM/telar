//! Visual harness for the `Input` primitive: builds a focused text field (a bordered box wrapping an
//! `Input`), lays it out, taps it to focus, and renders the flattened scene headless to `RSX_VISUAL_OUT`.
//! No-op without the env var (so it never gates CI). Run:
//!   RSX_VISUAL_OUT=/tmp/input.png cargo test -p renderer-software --test input_visual -- --nocapture

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};

use layout_core::{AlignItems, AvailableSpace, JustifyContent, LayoutStyle};
use platform_core::{Event, PointerButton, PointerSource};
use reactive_core::signal;
use renderer_core::{BorderRadius, Color, RectStyle, RenderBackend, ShapeStyle, Stroke, TextStyle};
use renderer_software::{SoftwareRenderer, SoftwareRendererConfig};
use ui_core::{
    Container, Input, LayoutItem, Overlay, StyledContainer, Text, WidgetCtx, box_item,
    box_transform, compute_layout, new_container,
};
use ui_tree::ComponentList;

struct Fake;
impl HasDisplayHandle for Fake {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Err(HandleError::Unavailable)
    }
}
impl HasWindowHandle for Fake {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        Err(HandleError::Unavailable)
    }
}

#[test]
fn input_visual_png() {
    let Ok(out) = std::env::var("RSX_VISUAL_OUT") else {
        eprintln!("set RSX_VISUAL_OUT to write a PNG; skipping");
        return;
    };

    let (w, h) = (520u32, 160u32);
    let ink = Color::from_rgb_u8(230, 232, 240);
    let mut ctx = WidgetCtx::new();

    // A text field: a bordered, padded box wrapping the bare `Input` primitive (the box supplies the look).
    let value = signal("Hello, rsx".to_string());
    let input = Input::new(
        &mut ctx,
        value,
        LayoutStyle::new().width(260.0).height(22.0),
        move || TextStyle::new(16.0, ink),
    )
    .unwrap();
    let field = StyledContainer::new(
        &mut ctx,
        LayoutStyle::new()
            .flex_column()
            .width(300.0)
            .height(46.0)
            .padding_all(12.0),
        |_r| {
            RectStyle::default()
                .with_fill(Color::from_rgb_u8(30, 33, 40))
                .with_stroke(Stroke::new(Color::from_rgb_u8(90, 130, 246), 1.5))
                .with_radius(BorderRadius::all(8.0))
        },
        vec![box_item(input)],
    )
    .unwrap();
    // Center the field in the frame with an outer container.
    let root = new_container(
        &mut ctx,
        LayoutStyle::new()
            .flex_column()
            .width(w as f32)
            .height(h as f32)
            .padding_all(40.0),
        &[field.layout_node()],
    )
    .unwrap();
    compute_layout(
        &mut ctx,
        root,
        AvailableSpace::Definite(w as f32),
        AvailableSpace::Definite(h as f32),
    )
    .unwrap();

    let mut list = ComponentList::new(RootHolder { field });
    // Tap inside the field to focus it, so the caret is drawn (at the end of the text).
    let tap = |x: f64, y: f64, pressed: bool| {
        if pressed {
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
                source: PointerSource::Mouse,
            }
        } else {
            Event::PointerReleased {
                x,
                y,
                button: PointerButton::Primary,
                source: PointerSource::Mouse,
            }
        }
    };
    list.on_event(&tap(70.0, 60.0, true));
    list.on_event(&tap(70.0, 60.0, false));

    let cmds = list.commands();
    let mut r =
        SoftwareRenderer::<Fake, Fake>::new_headless(w, h, SoftwareRendererConfig::default());
    r.begin_frame(w, h, 1.0, 0).unwrap();
    r.render_frame(cmds.as_slice(), Some(Color::from_rgb_u8(20, 22, 28)))
        .unwrap();
    let rgba = r.read_rgba().expect("pixmap exists after a frame");
    let img = image::RgbaImage::from_raw(w, h, rgba.to_vec()).expect("rgba length matches w*h*4");
    img.save(&out).expect("write PNG");
    eprintln!("wrote {out}");
}

#[test]
fn slider_visual_png() {
    let Ok(out) = std::env::var("RSX_SLIDER_OUT") else {
        eprintln!("set RSX_SLIDER_OUT to write a PNG; skipping");
        return;
    };

    let (w, h) = (360u32, 120u32);
    let mut ctx = WidgetCtx::new();

    // Track: a rounded bar; the thumb is a child offset by translate_x (as the `on_drag` demo drives it).
    let value = 0.4f32;
    let thumb = StyledContainer::new(
        &mut ctx,
        LayoutStyle::new().width(16.0).height(16.0),
        |_r| {
            RectStyle::default()
                .with_fill(Color::from_rgb_u8(90, 130, 246))
                .with_radius(BorderRadius::all(8.0))
        },
        vec![],
    )
    .unwrap()
    .with_transform(move |r| box_transform(r, 0.0, 1.0, 1.0, value * 264.0, 0.0));
    let track = StyledContainer::new(
        &mut ctx,
        LayoutStyle::new().flex_column().width(280.0).height(16.0),
        |_r| {
            RectStyle::default()
                .with_fill(Color::from_rgb_u8(45, 49, 58))
                .with_radius(BorderRadius::all(8.0))
        },
        vec![box_item(thumb)],
    )
    .unwrap();
    let root = new_container(
        &mut ctx,
        LayoutStyle::new()
            .flex_column()
            .width(w as f32)
            .height(h as f32)
            .padding_all(40.0),
        &[track.layout_node()],
    )
    .unwrap();
    compute_layout(
        &mut ctx,
        root,
        AvailableSpace::Definite(w as f32),
        AvailableSpace::Definite(h as f32),
    )
    .unwrap();

    let list = ComponentList::new(RootHolder { field: track });
    let cmds = list.commands();
    let mut r =
        SoftwareRenderer::<Fake, Fake>::new_headless(w, h, SoftwareRendererConfig::default());
    r.begin_frame(w, h, 1.0, 0).unwrap();
    r.render_frame(cmds.as_slice(), Some(Color::from_rgb_u8(20, 22, 28)))
        .unwrap();
    let rgba = r.read_rgba().expect("pixmap exists after a frame");
    let img = image::RgbaImage::from_raw(w, h, rgba.to_vec()).expect("rgba length matches w*h*4");
    img.save(&out).expect("write PNG");
    eprintln!("wrote {out}");
}

#[test]
fn overlay_visual_png() {
    let Ok(out) = std::env::var("RSX_OVERLAY_OUT") else {
        eprintln!("set RSX_OVERLAY_OUT to write a PNG; skipping");
        return;
    };

    let (w, h) = (420u32, 260u32);
    let ink = Color::from_rgb_u8(230, 232, 240);
    let mut ctx = WidgetCtx::new();

    // Page content behind: a filled panel with a label near the top.
    let label = Text::new(
        &mut ctx,
        || "Page content behind".to_string(),
        LayoutStyle::new().height(20.0),
        move || TextStyle::new(15.0, ink),
    )
    .unwrap();
    let page = StyledContainer::new(
        &mut ctx,
        LayoutStyle::new()
            .flex_column()
            .width(w as f32)
            .height(h as f32)
            .padding_all(20.0),
        |_r| RectStyle::default().with_fill(Color::from_rgb_u8(32, 36, 44)),
        vec![box_item(label)],
    )
    .unwrap();

    // A centered dialog inside an overlay: should draw on top of the page, escaping its layout.
    let dialog_label = Text::new(
        &mut ctx,
        || "Overlay on top".to_string(),
        LayoutStyle::new().height(22.0),
        move || TextStyle::new(17.0, ink),
    )
    .unwrap();
    let dialog = StyledContainer::new(
        &mut ctx,
        LayoutStyle::new()
            .flex_column()
            .width(220.0)
            .height(90.0)
            .padding_all(20.0),
        |_r| {
            RectStyle::default()
                .with_fill(Color::from_rgb_u8(60, 66, 82))
                .with_stroke(Stroke::new(Color::from_rgb_u8(90, 130, 246), 1.5))
                .with_radius(BorderRadius::all(10.0))
        },
        vec![box_item(dialog_label)],
    )
    .unwrap();

    // Portal flow (as in the app): lay out the page first so it becomes the overlay host, THEN build the
    // overlay so it attaches to that host and fills the viewport — even though it is nested here under a
    // small wrapper (proving it escapes its parent, not just when it is a viewport-sized child).
    let wrapper = Container::new(
        &mut ctx,
        LayoutStyle::new().flex_column(),
        vec![box_item(page)],
    )
    .unwrap();
    compute_layout(
        &mut ctx,
        wrapper.layout_node(),
        AvailableSpace::Definite(w as f32),
        AvailableSpace::Definite(h as f32),
    )
    .unwrap();

    // Built after layout → portals to the host (the wrapper) and fills the viewport.
    let overlay = Overlay::new(
        &mut ctx,
        LayoutStyle::new()
            .flex_column()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        vec![box_item(dialog)],
    )
    .unwrap();
    ui_core::relayout_if_dirty();

    let list = ComponentList::new(OverlayRoot { wrapper, overlay });
    let cmds = list.commands();
    let mut r =
        SoftwareRenderer::<Fake, Fake>::new_headless(w, h, SoftwareRendererConfig::default());
    r.begin_frame(w, h, 1.0, 0).unwrap();
    r.render_frame(cmds.as_slice(), Some(Color::from_rgb_u8(20, 22, 28)))
        .unwrap();
    let rgba = r.read_rgba().expect("pixmap exists after a frame");
    let img = image::RgbaImage::from_raw(w, h, rgba.to_vec()).expect("rgba length matches w*h*4");
    img.save(&out).expect("write PNG");
    eprintln!("wrote {out}");
}

// Renders the page (wrapper) plus the portaled overlay; the overlay's content is laid out via the host,
// not this tree, so it is rendered here purely by referencing its view.
struct OverlayRoot {
    wrapper: Container,
    overlay: Overlay,
}
impl ui_tree::Component for OverlayRoot {
    fn view(&self) -> ui_tree::RenderNode {
        ui_tree::RenderNode::group([self.wrapper.view(), self.overlay.view()])
    }
}

// A tiny root Component that owns the field and renders it (ComponentList needs a single root).
struct RootHolder {
    field: StyledContainer,
}
impl ui_tree::Component for RootHolder {
    fn view(&self) -> ui_tree::RenderNode {
        self.field.view()
    }
    fn on_event(&mut self, event: &Event) -> ui_tree::EventResult {
        self.field.on_event(event)
    }
}
