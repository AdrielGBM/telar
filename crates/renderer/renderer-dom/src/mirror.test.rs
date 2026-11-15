//! A frame drawn as pixels, as the hidden document assistive technology reads in its place: what a snapshot says each node is, said on the element standing for it, and what a reader does to an element reaching the control through the platform. Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test`.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;

use geometry_core::{Rect, Size};
use layout_core::LayoutStyle;
use platform_core::{
    AccessNode, ActionSupport, Event, EventHandler, HeadingLevel, NumericValue, Platform, Role,
    Toggle, Value, WindowConfig,
};
use platform_web::{PageNames, WebPlatform, WebPlatformConfig, WebWindow};
use reactive_core::{RwSignal, signal};
use renderer_core::RectStyle;
use telar_renderer_dom::{AccessMirror, CanvasTextMetrics};
use ui_core::{Accessible, ComponentList, Container, StyledContainer, WindowRoot, box_item, focus};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

fn document() -> web_sys::Document {
    web_sys::window()
        .and_then(|window| window.document())
        .expect("a document")
}

/// A host holding the element a frame is drawn on, as a canvas page's host holds its canvas.
fn host_with_drawing() -> (web_sys::HtmlElement, web_sys::Element) {
    let host: web_sys::HtmlElement = document()
        .create_element("div")
        .expect("a host")
        .dyn_into()
        .expect("a div is an HTML element");
    let _ = host.set_attribute("tabindex", "-1");
    let drawing = document().create_element("canvas").expect("a canvas");
    host.append_child(&drawing).expect("the canvas went in");
    document()
        .body()
        .expect("a body")
        .append_child(&host)
        .expect("the host went into the page");
    (host, drawing)
}

fn node(role: Role, name: &str, parent: Option<usize>) -> AccessNode {
    AccessNode {
        id: None,
        parent,
        role,
        name: name.to_string(),
        rect: Rect::new(10.0, 10.0, 100.0, 24.0),
        focused: false,
        enabled: true,
        toggled: None,
        value: None,
        description: None,
        invalid: false,
        required: false,
        readonly: false,
        lang: None,
        url: None,
        current: None,
        orientation: None,
        expanded: None,
        position: None,
        active_descendant: None,
        controls: None,
        has_popup: None,
        keyboard_shortcut: None,
        sort: None,
        multiselectable: false,
        modal: false,
        selection: None,
        line_starts: Vec::new(),
        supports: ActionSupport::default(),
    }
}

fn control(id: u64, role: Role, name: &str, parent: Option<usize>) -> AccessNode {
    AccessNode {
        id: Some(id),
        ..node(role, name, parent)
    }
}

/// A dialog asking for settings: a heading, a checked box, a slider, a field that fails its checks, and a button that cannot be pressed yet.
fn settings() -> Vec<AccessNode> {
    vec![
        AccessNode {
            modal: true,
            ..node(Role::Dialog, "Settings", None)
        },
        node(Role::Heading(HeadingLevel::Two), "Account", Some(0)),
        AccessNode {
            toggled: Some(Toggle::On),
            ..control(1, Role::CheckBox, "Remember me", Some(0))
        },
        AccessNode {
            value: Some(Value::Number {
                reading: NumericValue::new(0.0..=100.0, 30.0).expect("a reading within its range"),
                text: Some("30 percent".into()),
            }),
            ..control(2, Role::Slider, "Volume", Some(0))
        },
        AccessNode {
            value: Some(Value::Text("ada@".into())),
            invalid: true,
            required: true,
            description: Some("Enter a whole address".to_string()),
            ..control(3, Role::TextInput, "Email", Some(0))
        },
        AccessNode {
            enabled: false,
            ..control(4, Role::Button, "Save", Some(0))
        },
        node(Role::Label, "Changes apply at once", None),
    ]
}

fn named(host: &web_sys::Element, name: &str) -> web_sys::HtmlElement {
    host.query_selector(&format!("[aria-label=\"{name}\"]"))
        .expect("a valid selector")
        .unwrap_or_else(|| panic!("{name:?} has an element"))
        .dyn_into()
        .expect("an HTML element")
}

#[wasm_bindgen_test]
fn each_node_of_a_snapshot_says_its_role_name_value_and_state_on_its_element() {
    let (host, drawing) = host_with_drawing();
    let mut mirror = AccessMirror::before(&drawing).expect("a mirror beside the canvas");
    mirror.show(&settings());

    let dialog = named(&host, "Settings");
    assert_eq!(dialog.get_attribute("role").as_deref(), Some("dialog"));
    assert_eq!(dialog.get_attribute("aria-modal").as_deref(), Some("true"));

    let heading = dialog
        .query_selector("h2")
        .expect("a valid selector")
        .expect("the heading is inside the dialog");
    assert_eq!(
        heading.get_attribute("aria-label").as_deref(),
        Some("Account")
    );

    let remember = named(&host, "Remember me");
    assert!(
        dialog.contains(Some(remember.as_ref())),
        "the box is in the dialog"
    );
    assert_eq!(remember.get_attribute("role").as_deref(), Some("checkbox"));
    assert_eq!(
        remember.get_attribute("aria-checked").as_deref(),
        Some("true")
    );

    let volume = named(&host, "Volume");
    assert_eq!(volume.get_attribute("role").as_deref(), Some("slider"));
    assert_eq!(volume.get_attribute("aria-valuenow").as_deref(), Some("30"));
    assert_eq!(volume.get_attribute("aria-valuemin").as_deref(), Some("0"));
    assert_eq!(
        volume.get_attribute("aria-valuemax").as_deref(),
        Some("100")
    );
    assert_eq!(
        volume.get_attribute("aria-valuetext").as_deref(),
        Some("30 percent")
    );

    let email = named(&host, "Email");
    assert_eq!(email.get_attribute("role").as_deref(), Some("textbox"));
    assert_eq!(
        email.text_content().as_deref(),
        Some("ada@"),
        "a field holds its value as its text"
    );
    assert_eq!(email.get_attribute("aria-invalid").as_deref(), Some("true"));
    assert_eq!(
        email.get_attribute("aria-required").as_deref(),
        Some("true")
    );
    assert_eq!(
        email.get_attribute("aria-description").as_deref(),
        Some("Enter a whole address")
    );

    let save = named(&host, "Save");
    assert_eq!(save.tag_name().to_lowercase(), "button");
    assert_eq!(save.get_attribute("aria-disabled").as_deref(), Some("true"));

    for control in [&remember, &volume, &email, &save] {
        assert_eq!(
            control.get_attribute("tabindex").as_deref(),
            Some("-1"),
            "no element is a Tab stop: the app walks its own order"
        );
    }
    let text = host.text_content().expect("the host holds text");
    assert!(
        text.contains("Changes apply at once"),
        "static text is the element's own: {text}"
    );

    let mut unticked = settings();
    unticked[2].toggled = Some(Toggle::Off);
    mirror.show(&unticked);
    let again = named(&host, "Remember me");
    assert!(
        again.is_same_node(Some(remember.as_ref())),
        "a control keeps its element"
    );
    assert_eq!(
        again.get_attribute("aria-checked").as_deref(),
        Some("false")
    );

    drop(mirror);
    assert!(
        host.query_selector("[aria-label]")
            .expect("a valid selector")
            .is_none(),
        "the mirror leaves with its owner"
    );
    host.remove();
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

struct App {
    host: web_sys::HtmlElement,
    tree: ComponentList,
    mirror: AccessMirror,
    pressed: RwSignal<u32>,
}

/// Hands what the platform heard to the tree, the way the runner does: a box event or a reader's action to the control it names, anything else down the tree.
struct Forward;

impl EventHandler<WebWindow> for Forward {
    fn on_resume(&mut self, _window: &WebWindow) -> bool {
        true
    }

    fn on_event(&mut self, event: Event, _window: &WebWindow) {
        APP.with(|app| {
            if let Some(app) = app.borrow_mut().as_mut() {
                let tree = &mut app.tree;
                if ui_core::follow_box_event(&event, |_| {}).is_none() {
                    tree.on_event(&event);
                }
            }
        });
    }

    fn on_redraw(&mut self, _window: &WebWindow) {}
}

fn mount() {
    if APP.with(|app| app.borrow().is_some()) {
        return;
    }
    renderer_core::set_text_metrics(CanvasTextMetrics);
    ui_core::set_surface_size(Size::new(300.0, 120.0));
    let pressed = signal(0u32);
    let button = StyledContainer::new(
        LayoutStyle::new().width(80.0).height(32.0),
        |_| RectStyle::default(),
        vec![],
    )
    .expect("the button builds")
    .on_press(move || pressed.set(pressed.get() + 1))
    .control(Role::Button)
    .a11y_label(|| "Publish");
    let column = Container::new(LayoutStyle::new().flex_column(), vec![box_item(button)])
        .expect("the column builds");
    let mut tree = ComponentList::new(WindowRoot::new(box_item(column)));
    tree.on_event(&Event::WindowResized {
        width: 300,
        height: 120,
    });
    let (host, drawing) = host_with_drawing();
    let mirror = AccessMirror::before(&drawing).expect("a mirror beside the canvas");
    APP.with(|app| {
        *app.borrow_mut() = Some(App {
            host: host.clone(),
            tree,
            mirror,
            pressed,
        })
    });
    let config = WebPlatformConfig {
        host: None,
        autofocus: false,
        owns_gestures: true,
        owns_scroll: false,
        owns_context_menu: true,
        owns_keyboard: true,
        location: None,
        page: PageNames::default(),
    };
    WebPlatform::with_host(host, config)
        .run(WindowConfig::default(), Forward)
        .expect("the platform mounted");
}

/// Draws the tree and describes what it drew, as the canvas runner does after each frame.
fn describe() {
    mount();
    ui_core::relayout_if_dirty();
    APP.with(|app| {
        let mut app = app.borrow_mut();
        let app = app.as_mut().expect("mounted");
        let nodes = ui_core::accessibility::snapshot(&app.tree.commands());
        app.mirror.show(&nodes);
    });
}

fn host() -> web_sys::HtmlElement {
    APP.with(|app| app.borrow().as_ref().expect("mounted").host.clone())
}

fn publish_id() -> u64 {
    APP.with(|app| {
        let app = app.borrow();
        ui_core::accessibility::snapshot(&app.as_ref().expect("mounted").tree.commands())
            .into_iter()
            .find(|node| node.name == "Publish")
            .and_then(|node| node.id)
            .expect("Publish is a control")
    })
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
async fn a_click_on_a_controls_element_activates_the_control() {
    describe();
    let pressed = APP.with(|app| app.borrow().as_ref().expect("mounted").pressed);
    let was = pressed.get();
    named(&host(), "Publish").click();
    next_frames().await;
    assert_eq!(
        pressed.get(),
        was + 1,
        "the reader's click pressed the button once"
    );
    assert_eq!(
        focus::current(),
        Some(publish_id()),
        "and gave it focus, as a tap does"
    );
    focus::clear();
    describe();
}

#[wasm_bindgen_test]
async fn focus_landing_on_an_element_moves_the_apps_focus_and_the_apps_moves_the_documents() {
    describe();
    focus::clear();
    named(&host(), "Publish").focus().expect("focus");
    next_frames().await;
    assert_eq!(
        focus::current(),
        Some(publish_id()),
        "the app follows the reader"
    );

    host().focus().expect("focus");
    describe();
    assert_eq!(
        document().active_element().as_ref(),
        Some(named(&host(), "Publish").as_ref()),
        "the document follows the app while the keyboard is in the surface"
    );

    focus::clear();
    describe();
    assert_eq!(
        document().active_element().as_ref(),
        Some(host().as_ref()),
        "focus no control holds stays with the surface"
    );
}
