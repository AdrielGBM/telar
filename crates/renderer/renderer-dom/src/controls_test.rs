//! A box written as a control with a state — `box role:switch toggled:$on` and `box role:button toggled:$on` in `.rsx` — as the document says it and as the keyboard presses it: the role and the state attribute ARIA has for it, a Tab stop, and Space or Enter reaching the box through the platform and flipping the state the next frame writes. Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test`.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;

use geometry_core::Size;
use layout_core::{LayoutError, LayoutStyle};
use platform_core::{Event, EventHandler, Platform, Role, WindowConfig};
use platform_web::{WebPlatform, WebPlatformConfig, WebWindow};
use reactive_core::{RwSignal, signal};
use renderer_core::{Color, RectStyle, RenderBackend, TextStyle};
use telar_renderer_dom::{CanvasTextMetrics, DomRenderer};
use ui_core::{
    Accessible, ComponentList, Container, LayoutItem, StyledContainer, Text, WindowRoot, box_item,
    focus,
};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

thread_local! {
    static PAGE: RefCell<Option<Page>> = const { RefCell::new(None) };
}

struct Page {
    host: web_sys::HtmlElement,
    tree: ComponentList,
    renderer: DomRenderer,
    calm: RwSignal<bool>,
    bold: RwSignal<bool>,
}

/// Hands what the platform heard to the tree, the way the runner does.
struct Forward;

impl EventHandler<WebWindow> for Forward {
    fn on_resume(&mut self, _window: &WebWindow) -> bool {
        true
    }

    fn on_event(&mut self, event: Event, _window: &WebWindow) {
        PAGE.with(|page| {
            if let Some(page) = page.borrow_mut().as_mut() {
                page.tree.on_event(&event);
            }
        });
    }

    fn on_redraw(&mut self, _window: &WebWindow) {}
}

fn document() -> web_sys::Document {
    web_sys::window()
        .and_then(|window| window.document())
        .expect("a document")
}

/// What `box role:<role> toggled:$on label:"…" on_press:(|| $on.set(!$on.get()))` builds.
fn stateful(
    role: Role,
    name: &'static str,
    glyph: &'static str,
    on: RwSignal<bool>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::new(
        move || glyph.to_string(),
        LayoutStyle::new(),
        || TextStyle::new(16.0, Color::BLACK),
    )?;
    let control = StyledContainer::new(
        LayoutStyle::new().flex_row().padding_all(6.0),
        |_| RectStyle::default(),
        vec![box_item(text)],
    )?
    .on_press(move || on.set(!on.get()))
    .control(role)
    .toggled(move || on.get())
    .a11y_label(move || name);
    Ok(box_item(control))
}

fn mount() {
    if PAGE.with(|page| page.borrow().is_some()) {
        return;
    }
    renderer_core::set_text_metrics(CanvasTextMetrics);
    ui_tree::set_element_capture(true);
    ui_core::set_surface_size(Size::new(400.0, 200.0));
    let calm = signal(false);
    let bold = signal(true);
    let column = Container::new(
        LayoutStyle::new().flex_column().gap(8.0),
        vec![
            stateful(Role::Switch, "Reduce motion", "≈", calm).expect("the switch builds"),
            stateful(Role::Button, "Bold", "B", bold).expect("the toggle button builds"),
        ],
    )
    .expect("the column builds");
    let mut tree = ComponentList::new(WindowRoot::new(box_item(column)));
    tree.on_event(&Event::WindowResized {
        width: 400,
        height: 200,
    });
    let host: web_sys::HtmlElement = document()
        .create_element("div")
        .expect("a host element")
        .dyn_into()
        .expect("a div is an HTML element");
    document()
        .body()
        .expect("a body")
        .append_child(host.as_ref())
        .expect("the host went into the page");
    let renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    PAGE.with(|page| {
        *page.borrow_mut() = Some(Page {
            host: host.clone(),
            tree,
            renderer,
            calm,
            bold,
        })
    });
    let config = WebPlatformConfig {
        host: None,
        autofocus: false,
        owns_gestures: true,
        owns_scroll: true,
        owns_context_menu: false,
        owns_keyboard: false,
        location: None,
    };
    WebPlatform::with_host(host, config)
        .run(WindowConfig::default(), Forward)
        .expect("the platform mounted");
}

fn render() {
    mount();
    ui_core::relayout_if_dirty();
    PAGE.with(|page| {
        let mut page = page.borrow_mut();
        let page = page.as_mut().expect("mounted");
        let frame = page.tree.commands().clone();
        page.renderer
            .render_frame(&frame, None)
            .expect("the frame reconciled");
    });
}

fn state(read: impl Fn(&Page) -> RwSignal<bool>) -> RwSignal<bool> {
    PAGE.with(|page| read(page.borrow().as_ref().expect("mounted")))
}

/// The element of the control called `name`.
fn control(name: &str) -> web_sys::HtmlElement {
    PAGE.with(|page| {
        page.borrow()
            .as_ref()
            .expect("mounted")
            .host
            .query_selector(&format!("[data-telar-focus][aria-label=\"{name}\"]"))
            .expect("a valid selector")
            .unwrap_or_else(|| panic!("{name:?} is a control in the document"))
            .dyn_into()
            .expect("an HTML element")
    })
}

fn focus_on(name: &str) {
    let nodes = PAGE.with(|page| {
        let page = page.borrow();
        ui_core::accessibility::snapshot(&page.as_ref().expect("mounted").tree.commands())
    });
    let id = nodes
        .into_iter()
        .find(|node| node.name == name)
        .and_then(|node| node.id)
        .unwrap_or_else(|| panic!("{name:?} is a control"));
    focus::request(id);
    render();
}

/// Presses `key` on `target` and reports whether its default action was prevented.
fn press(target: &web_sys::EventTarget, key: &str) -> bool {
    let init = web_sys::KeyboardEventInit::new();
    init.set_key(key);
    init.set_bubbles(true);
    init.set_cancelable(true);
    let event = web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init)
        .expect("a keyboard event");
    target.dispatch_event(&event).expect("dispatched");
    event.default_prevented()
}

/// Waits out the platform's next turn, which is an animation frame requested when the event arrived.
async fn next_frames() {
    for _ in 0..2 {
        let promise = js_sys::Promise::new(&mut |resolve, _| {
            let _ = web_sys::window()
                .expect("a window")
                .request_animation_frame(&resolve);
        });
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
    }
}

#[wasm_bindgen_test]
fn a_switch_is_checked_and_a_toggle_button_pressed() {
    render();
    let switch = control("Reduce motion");
    assert_eq!(switch.get_attribute("role").as_deref(), Some("switch"));
    let calm = state(|page| page.calm).get().to_string();
    assert_eq!(switch.get_attribute("aria-checked"), Some(calm));
    assert_eq!(switch.get_attribute("aria-pressed"), None);
    assert_eq!(switch.get_attribute("tabindex").as_deref(), Some("0"));

    let bold = control("Bold");
    assert_eq!(bold.tag_name().to_lowercase(), "button");
    assert_eq!(
        bold.get_attribute("role"),
        None,
        "a <button> is its own role"
    );
    let pressed = state(|page| page.bold).get().to_string();
    assert_eq!(bold.get_attribute("aria-pressed"), Some(pressed));
    assert_eq!(bold.get_attribute("aria-checked"), None);
    assert_eq!(bold.get_attribute("tabindex").as_deref(), Some("0"));
}

#[wasm_bindgen_test]
async fn space_presses_the_switch_and_the_next_frame_says_so() {
    render();
    let calm = state(|page| page.calm);
    let was = calm.get();
    focus_on("Reduce motion");
    let target: web_sys::EventTarget = control("Reduce motion").into();
    assert!(
        press(&target, " "),
        "Space is the switch's, not the page's scroll"
    );
    next_frames().await;
    assert_eq!(calm.get(), !was, "the press reached the box");
    render();
    assert_eq!(
        control("Reduce motion")
            .get_attribute("aria-checked")
            .as_deref(),
        Some(if was { "false" } else { "true" })
    );
    focus::clear();
    render();
}

#[wasm_bindgen_test]
async fn enter_presses_the_toggle_button_and_the_next_frame_says_so() {
    render();
    let bold = state(|page| page.bold);
    let was = bold.get();
    focus_on("Bold");
    let target: web_sys::EventTarget = control("Bold").into();
    assert!(press(&target, "Enter"), "Enter is the button's");
    next_frames().await;
    assert_eq!(bold.get(), !was, "the press reached the box");
    render();
    assert_eq!(
        control("Bold").get_attribute("aria-pressed").as_deref(),
        Some(if was { "false" } else { "true" })
    );
    focus::clear();
    render();
}
