//! The document real widgets render to, audited the way an accessibility review would: axe-core's WCAG A/AA and best-practice rules, run over buttons, links, a field, a checkbox, a switch and a toggle button, a paragraph with a link in it and the boxes an application named, gave a language or hid. Needs a browser and the dev shell's `TELAR_AXE_CORE`, so it runs under `wasm-bindgen-test` rather than `cargo test`.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;

use geometry_core::Size;
use layout_core::{LayoutError, LayoutStyle};
use platform_core::{Event, Location, Role, external};
use renderer_core::{Color, RectStyle, RenderBackend, TextStyle};
use telar_renderer_dom::{CanvasTextMetrics, DomRenderer};
use ui_components::{ButtonProps, CheckboxProps, TextFieldProps, button, checkbox, text_field};
use ui_core::{
    Accessible, Children, ComponentList, Container, LayoutItem, StyledContainer, Text, TextRun,
    WindowRoot, box_item, focus,
};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const AXE: &str = include_str!(env!(
    "TELAR_AXE_CORE",
    "TELAR_AXE_CORE names axe-core's script; the repository's dev shell sets it, so run inside `nix develop`"
));

/// What the audit runs: the rules a WCAG 2.2 AA review checks, and the best practices beside them.
///
/// `region` asks that a whole page sit inside landmarks, and the fixture is one fragment of a page rather than a page.
const OPTIONS: &str = r#"{
    "runOnly": { "type": "tag", "values": ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa", "best-practice"] },
    "rules": { "region": { "enabled": false } },
    "resultTypes": ["violations"]
}"#;

const WIDTH: f32 = 640.0;
const HEIGHT: f32 = 900.0;

thread_local! {
    static PAGE: RefCell<Option<Page>> = const { RefCell::new(None) };
}

struct Page {
    host: web_sys::HtmlElement,
    tree: ComponentList,
    renderer: DomRenderer,
}

fn document() -> web_sys::Document {
    web_sys::window()
        .and_then(|window| window.document())
        .expect("a document")
}

fn ink() -> TextStyle {
    TextStyle::new(16.0, Color::BLACK)
}

fn words(content: &'static str) -> Result<Text, LayoutError> {
    Text::new(move || content.to_string(), LayoutStyle::new(), ink)
}

fn link<D: platform_core::IntoDestination + 'static>(
    content: &'static str,
    to: impl Fn() -> D + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label = words(content)?;
    let link = StyledContainer::new(
        LayoutStyle::new().padding_horizontal(4.0),
        |_| RectStyle::default(),
        vec![box_item(label)],
    )?
    .to(to);
    Ok(box_item(link))
}

/// What `box role:<role> toggled:$on label:"…"` builds in `.rsx`: a box drawing a glyph that is a control with an on/off state.
fn stateful_control(
    role: Role,
    name: &'static str,
    glyph: &'static str,
    on: bool,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let control = StyledContainer::new(
        LayoutStyle::new().flex_row().padding_all(4.0),
        |_| RectStyle::default(),
        vec![box_item(words(glyph)?)],
    )?
    .on_press(|| {})
    .control(role)
    .toggled(move || on)
    .a11y_label(move || name);
    Ok(box_item(control))
}

/// One of everything an application builds most of its screens from.
fn controls() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let heading = StyledContainer::new(
        LayoutStyle::new(),
        |_| RectStyle::default(),
        vec![box_item(words("Settings")?)],
    )?
    .role(Role::Heading(1));
    let save = button(
        ButtonProps::props()
            .label("Save")
            .on_press(std::rc::Rc::new(|| {}))
            .build(),
        Children::default(),
    )?;
    let source = link("Source code", || external("https://example.com/telar"))?;
    let about = link("About", || Location::root().segment("about"))?;
    let name = text_field(
        TextFieldProps::props()
            .label("Name")
            .placeholder("Ada Lovelace")
            .build(),
        Children::default(),
    )?;
    let subscribe = checkbox(
        CheckboxProps::props()
            .label("Send me the newsletter")
            .build(),
        Children::default(),
    )?;
    let calm = stateful_control(Role::Switch, "Reduce motion", "≈", true)?;
    let bold = stateful_control(Role::Button, "Bold", "B", false)?;
    let paragraph = Text::runs(
        vec![
            TextRun::new(|| "Read ".to_string()),
            TextRun::new(|| "the guide".to_string())
                .declaring(|| renderer_core::Declared::default().with_font_weight(700))
                .to(|| Location::root().segment("guide")),
            TextRun::new(|| " before you start.".to_string()),
        ],
        LayoutStyle::new(),
        |_| ink(),
    )?;
    let letters: Vec<Box<dyn LayoutItem>> = ["T", "e", "l", "a", "r"]
        .into_iter()
        .map(|letter| words(letter).map(|text| box_item(text.a11y_hidden())))
        .collect::<Result<_, _>>()?;
    let word = Container::new(LayoutStyle::new().flex_row(), letters)?.a11y_label(|| "Telar");
    let greeting = words("Bonjour tout le monde")?.a11y_lang(|| "fr");
    let decoration = StyledContainer::new(
        LayoutStyle::new().width(24.0).height(24.0),
        |_| RectStyle::filled(Color::from_rgb_u8(30, 90, 200), 12.0),
        vec![],
    )?
    .a11y_hidden();
    let column = Container::new(
        LayoutStyle::new().flex_column().gap(12.0).padding_all(16.0),
        vec![
            box_item(heading),
            save,
            source,
            about,
            name,
            subscribe,
            calm,
            bold,
            box_item(paragraph),
            box_item(word),
            box_item(greeting),
            box_item(decoration),
        ],
    )?;
    Ok(box_item(column))
}

fn mount() {
    if PAGE.with(|page| page.borrow().is_some()) {
        return;
    }
    renderer_core::set_text_metrics(CanvasTextMetrics);
    ui_tree::set_element_capture(true);
    ui_core::set_surface_size(Size::new(WIDTH, HEIGHT));
    let mut tree = ComponentList::new(WindowRoot::new(controls().expect("the page builds")));
    tree.on_event(&Event::WindowResized {
        width: WIDTH as u32,
        height: HEIGHT as u32,
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
            host,
            tree,
            renderer,
        })
    });
}

/// Draws the page as it stands now, with whatever focus the test gave it.
fn render() -> web_sys::HtmlElement {
    mount();
    ui_core::relayout_if_dirty();
    PAGE.with(|page| {
        let mut page = page.borrow_mut();
        let page = page.as_mut().expect("mounted");
        let frame = page.tree.commands().clone();
        page.renderer
            .render_frame(&frame, None)
            .expect("the frame reconciled");
        page.host.clone()
    })
}

fn focusable(role: Role) -> focus::Exposed {
    mount();
    focus::exposed()
        .into_iter()
        .find(|exposed| exposed.role == role)
        .unwrap_or_else(|| panic!("a {role:?} is on the page"))
}

fn axe() -> JsValue {
    let window = web_sys::window().expect("a window");
    let loaded = js_sys::Reflect::get(&window, &"axe".into()).expect("a window property");
    if !loaded.is_undefined() {
        return loaded;
    }
    let script = document().create_element("script").expect("a script");
    script.set_text_content(Some(AXE));
    document()
        .head()
        .expect("a head")
        .append_child(&script)
        .expect("axe went into the page");
    js_sys::Reflect::get(&window, &"axe".into()).expect("axe defined itself")
}

/// Every violation axe finds under `host`, one paragraph each, naming the rule, the element and what is wrong with it.
async fn violations(host: &web_sys::HtmlElement) -> String {
    let axe = axe();
    let run: js_sys::Function = js_sys::Reflect::get(&axe, &"run".into())
        .expect("axe.run")
        .dyn_into()
        .expect("axe.run is a function");
    let options = js_sys::JSON::parse(OPTIONS).expect("the options are JSON");
    let promise: js_sys::Promise = run
        .call2(&axe, host, &options)
        .expect("axe ran")
        .dyn_into()
        .expect("axe.run answers with a promise");
    let results = wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .expect("axe finished");
    let summary = js_sys::Function::new_with_args(
        "results",
        r#"return results.violations.map(v => `${v.id} (${v.impact}): ${v.help}\n` + v.nodes.map(n => `  ${n.target.join(" ")}: ${n.html}\n    ${(n.failureSummary || "").replace(/\n/g, " ")}`).join("\n")).join("\n\n");"#,
    );
    summary
        .call1(&JsValue::NULL, &results)
        .expect("the summary ran")
        .as_string()
        .expect("the summary is a string")
}

async fn assert_clean(host: &web_sys::HtmlElement, what: &str) {
    let found = violations(host).await;
    assert!(found.is_empty(), "axe found violations {what}:\n\n{found}");
}

fn box_shadow_of(element: &web_sys::Element) -> String {
    web_sys::window()
        .expect("a window")
        .get_computed_style(element)
        .expect("a computed style")
        .expect("styles for an element in the document")
        .get_property_value("box-shadow")
        .expect("a box shadow")
}

fn element_of(node: ui_core::NodeId) -> web_sys::Element {
    let id = u64::from(node);
    document()
        .query_selector(&format!("[data-telar-focus=\"{id}\"]"))
        .expect("a valid selector")
        .unwrap_or_else(|| panic!("box {id} is in the document"))
}

#[wasm_bindgen_test]
async fn a_screen_of_controls_has_no_violations() {
    focus::clear();
    let host = render();
    assert_clean(&host, "with nothing focused").await;
}

#[wasm_bindgen_test]
async fn focus_on_a_field_or_a_button_adds_no_violations() {
    focus::request(focusable(Role::TextInput).id);
    let host = render();
    assert_clean(&host, "with the field focused").await;
    focus::request(focusable(Role::Button).id);
    let host = render();
    assert_clean(&host, "with the button focused").await;
    focus::clear();
}

/// The ring a keyboard user follows is Telar's own, painted as the box's shadow, and the frame around a field wears it for the line inside it.
#[wasm_bindgen_test]
fn the_keyboard_s_box_wears_telar_s_ring() {
    let button = focusable(Role::Button);
    focus::request(button.id);
    render();
    assert_ne!(box_shadow_of(&element_of(button.node)), "none");

    let field = focusable(Role::TextInput);
    focus::request_from_pointer(field.id);
    render();
    let line = element_of(field.node);
    let framed = std::iter::successors(line.parent_element(), |node| node.parent_element())
        .take(3)
        .any(|frame| box_shadow_of(&frame).contains("2px"));
    assert!(
        framed,
        "a field tapped into shows it is the one being typed into"
    );
    focus::clear();
    render();
}
