//! The document real widgets render to, audited the way an accessibility review would: axe-core's WCAG A/AA and best-practice rules, run over buttons, links, a field, a checkbox, a switch and a toggle button, a named tab list, a paragraph with a link in it and the boxes an application named, gave a language or hid; with `workbench`, a tree, a toolbar, a split pane and a resizable box too. Needs a browser and the dev shell's `TELAR_AXE_CORE`, so it runs under `wasm-bindgen-test` rather than `cargo test`.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;

use renderer_dom::{CanvasTextMetrics, DomRenderer};
use telar::{
    Accessible, Children, Color, ComponentList, Container, Event, LayoutError, LayoutItem,
    LayoutStyle, Location, Reactive, RectStyle, RenderBackend, Role, Size, Slots, StyledContainer,
    Text, TextRun, TextStyle, WindowRoot, box_item, external, focus,
};
use telar_components::{
    ButtonProps, CheckboxProps, ImageProps, ImageSource, ItemProps, LinkProps, ScrubFieldProps,
    SearchFieldProps, SelectProps, TabsProps, TextFieldProps, TextareaProps, button, checkbox,
    image, item, link as text_link, scrub_field, search_field, select, tabs, text_field, textarea,
};
#[cfg(feature = "overlays")]
use telar_components::{
    Command, CommandPaletteProps, MenuProps, ModalProps, SubmenuProps, command_palette, menu,
    modal, submenu,
};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

const AXE: &str = include_str!(env!(
    "TELAR_AXE_CORE",
    "TELAR_AXE_CORE names axe-core's script; the repository's dev shell sets it, so run inside `nix develop`"
));

/// What the audit runs: the rules a WCAG 2.2 AA review checks, and the best practices beside them, shared with the audit of the exported workshop (`cargo-telar/src/workshop_audit_test.rs`).
///
/// `region` asks that a whole page sit inside landmarks, and the fixture is one fragment of a page rather than a page.
const OPTIONS: &str = include_str!("../../../crates/renderer/renderer-dom/axe.json");

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

fn link<D: telar::IntoDestination + 'static>(
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
///
/// `platform-desktop/src/accessibility_test.rs` builds the same screen from `ui-core` primitives standing in for the catalogue's button, field and checkbox, so a change here is mirrored there.
/// A scrub field and a select, each in a box the application named: the box is the only name either control has.
fn labelled_fields() -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {
    let speed = scrub_field(
        ScrubFieldProps::props().label("X").build(),
        Children::default(),
    )?;
    let speed = Container::new(LayoutStyle::new(), vec![speed])?.a11y_label(|| "Speed");
    let sizes = Children::new(|| {
        let mut slots = Slots::new();
        for label in ["Small", "Large"] {
            slots.push(
                None,
                item(
                    ItemProps::props()
                        .label(Reactive::of(move || label.to_string()))
                        .build(),
                    Children::default(),
                )?,
            );
        }
        Ok(slots)
    });
    let size = select(SelectProps::props().build(), sizes)?;
    let size = Container::new(LayoutStyle::new(), vec![size])?.a11y_label(|| "Size");
    Ok(vec![box_item(speed), box_item(size)])
}

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
                .declaring(|| telar::Declared::default().with_font_weight(700))
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
    let sections = tabs(
        TabsProps::props()
            .items(vec!["General", "Advanced"])
            .label("Sections")
            .build(),
        Children::default(),
    )?;
    let mut controls = vec![
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
        sections,
    ];
    controls.extend(labelled_fields()?);
    controls.extend(everyday()?);
    #[cfg(feature = "advanced")]
    controls.extend(data()?);
    #[cfg(feature = "workbench")]
    controls.extend([tree()?, tools()?, split()?, resizable_box()?]);
    #[cfg(feature = "overlays")]
    controls.extend(overlays()?);
    let column = Container::new(
        LayoutStyle::new().flex_column().gap(12.0).padding_all(16.0),
        controls,
    )?;
    Ok(box_item(column))
}

/// A tree with an open branch, a selected row and a badge, in a box of its own height: a tree fills the height it is given and scrolls inside it.
#[cfg(feature = "workbench")]
fn tree() -> Result<Box<dyn LayoutItem>, LayoutError> {
    use telar_components::{TreeNode, TreeViewProps, tree_view};
    let items = vec![
        TreeNode::new("inputs", "Inputs").with_children([
            TreeNode::new("button", "Button").with_badge("3"),
            TreeNode::new("checkbox", "Checkbox"),
        ]),
        TreeNode::new("about", "About"),
    ];
    let tree = tree_view(
        TreeViewProps::props()
            .items(items)
            .expanded(telar::signal(["inputs".into()].into_iter().collect()))
            .selected(telar::signal(Some("button".into())))
            .label("Components")
            .build(),
        Children::default(),
    )?;
    Ok(box_item(Container::new(
        LayoutStyle::new().flex_column().height(140.0),
        vec![tree],
    )?))
}

/// A toolbar of two icon buttons, one of them a toggle.
#[cfg(feature = "workbench")]
fn tools() -> Result<Box<dyn LayoutItem>, LayoutError> {
    use telar_components::{IconButtonProps, ToolbarProps, icon_button, toolbar};
    let children = Children::new(|| {
        let mut slots = telar::Slots::new();
        slots.push(
            None,
            icon_button(
                IconButtonProps::props()
                    .icon("lucide:bold")
                    .label("Bold")
                    .pressed(true)
                    .build(),
                Children::default(),
            )?,
        );
        slots.push(
            None,
            icon_button(
                IconButtonProps::props()
                    .icon("lucide:search")
                    .label("Find")
                    .shortcut("Mod+F")
                    .build(),
                Children::default(),
            )?,
        );
        Ok(slots)
    });
    toolbar(ToolbarProps::props().label("Formatting").build(), children)
}

/// Two panes and the splitter between them, in a box of their own size.
#[cfg(feature = "workbench")]
fn split() -> Result<Box<dyn LayoutItem>, LayoutError> {
    use telar_components::{SplitPaneProps, split_pane};
    let mut slots = telar::Slots::new();
    slots.push(None, box_item(words("Outline")?));
    slots.push(None, box_item(words("Preview")?));
    let split = split_pane(
        SplitPaneProps::props().default_size(120.0).build(),
        Children::from(slots),
    )?;
    Ok(box_item(Container::new(
        LayoutStyle::new().flex_row().height(60.0),
        vec![split],
    )?))
}

/// A box of words with handles on its end edge, its bottom edge and the corner between them.
#[cfg(feature = "workbench")]
fn resizable_box() -> Result<Box<dyn LayoutItem>, LayoutError> {
    use telar_components::{ResizableProps, resizable};
    let mut slots = telar::Slots::new();
    slots.push(None, box_item(words("Notes")?));
    resizable(
        ResizableProps::props()
            .default_size(Size::new(160.0, 48.0))
            .min(Size::new(80.0, 32.0))
            .max(Size::new(400.0, 0.0))
            .build(),
        Children::from(slots),
    )
}

fn mount() {
    if PAGE.with(|page| page.borrow().is_some()) {
        return;
    }
    telar::set_text_metrics(CanvasTextMetrics);
    ui_tree::set_element_capture(true);
    telar::set_surface_size(Size::new(WIDTH, HEIGHT));
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
    telar::relayout_if_dirty();
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

fn element_of(node: telar::NodeId) -> web_sys::Element {
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

/// The rows reach the document as tree items that say they are chosen, open and where they sit, and a focused tree adds no violations.
#[cfg(feature = "workbench")]
#[wasm_bindgen_test]
async fn a_tree_s_rows_carry_their_state_into_the_document() {
    focus::request(focusable(Role::Tree).id);
    let host = render();
    let chosen = host
        .query_selector(r#"[role="treeitem"][aria-selected="true"]"#)
        .expect("a valid selector")
        .expect("the selected row is in the document");
    assert_eq!(chosen.get_attribute("aria-level").as_deref(), Some("2"));
    assert_eq!(chosen.get_attribute("aria-posinset").as_deref(), Some("1"));
    assert_eq!(chosen.get_attribute("aria-setsize").as_deref(), Some("2"));
    assert!(
        host.query_selector(r#"[role="treeitem"][aria-expanded="true"]"#)
            .expect("a valid selector")
            .is_some(),
        "the open branch says so"
    );
    assert_clean(&host, "with the tree focused").await;
    focus::clear();
}

/// The element the box of `role` that has a cursor points a reader at with `aria-activedescendant`, which has to be inside it. Only a focused list, tree or table has one: the page holds several of each.
#[cfg(any(feature = "advanced", feature = "workbench", feature = "overlays"))]
fn pointed_at(host: &web_sys::HtmlElement, role: &str) -> web_sys::Element {
    let container = host
        .query_selector(&format!(r#"[role="{role}"][aria-activedescendant]"#))
        .expect("a valid selector")
        .unwrap_or_else(|| panic!("a {role} is in the document"));
    let id = container
        .get_attribute("aria-activedescendant")
        .unwrap_or_else(|| panic!("the focused {role} points at its cursor"));
    let item = document()
        .get_element_by_id(&id)
        .unwrap_or_else(|| panic!("`{id}` names an element"));
    assert!(
        container.contains(Some(item.as_ref())),
        "the {role} points inside itself"
    );
    item
}

/// A focused tree keeps the document's focus and points a reader at the row its cursor is on, and moving the cursor moves what it points at.
#[cfg(feature = "workbench")]
#[wasm_bindgen_test]
async fn a_focused_tree_points_a_reader_at_its_cursor_row() {
    let tree = focusable(Role::Tree);
    focus::request(tree.id);
    let host = render();
    let row = pointed_at(&host, "tree");
    assert_eq!(row.get_attribute("role").as_deref(), Some("treeitem"));
    assert_eq!(row.get_attribute("aria-selected").as_deref(), Some("true"));

    PAGE.with(|page| {
        let mut page = page.borrow_mut();
        let page = page.as_mut().expect("mounted");
        page.tree.on_event(&Event::KeyPressed {
            key: telar::Key::Named(telar::NamedKey::ArrowDown),
            modifiers: telar::ModifiersState::default(),
            unmodified: None,
        });
    });
    let host = render();
    let next = pointed_at(&host, "tree");
    assert_ne!(
        next.id(),
        row.id(),
        "the arrow moved what the tree points at"
    );
    assert!(
        row.get_attribute("id").is_none(),
        "and the row it left has no id to point at"
    );
    assert_clean(&host, "with the tree's cursor on a row").await;
    focus::clear();
}

/// A focused toolbar points a reader at the item its cursor is on, and its toggle says it is pressed.
#[cfg(feature = "workbench")]
#[wasm_bindgen_test]
async fn a_focused_toolbar_points_a_reader_at_its_cursor_item() {
    focus::request(focusable(Role::Toolbar).id);
    let host = render();
    let item = pointed_at(&host, "toolbar");
    assert_eq!(item.get_attribute("aria-label").as_deref(), Some("Bold"));
    assert_eq!(item.get_attribute("aria-pressed").as_deref(), Some("true"));
    assert_clean(&host, "with the toolbar focused").await;
    focus::clear();
}

/// The splitter is named even when the application gave it no name.
#[cfg(feature = "workbench")]
#[wasm_bindgen_test]
fn the_splitter_has_a_name() {
    focus::clear();
    let host = render();
    let separators = host
        .query_selector_all(r#"[role="separator"]"#)
        .expect("a valid selector");
    let splitter = (0..separators.length())
        .filter_map(|i| separators.item(i)?.dyn_into::<web_sys::Element>().ok())
        .find(|separator| {
            separator
                .closest(r#"[role="columnheader"]"#)
                .expect("a valid selector")
                .is_none()
        })
        .expect("the splitter is in the document");
    assert_eq!(
        splitter.get_attribute("aria-label").as_deref(),
        Some("Resize")
    );
}

/// The tabs sit in a tab list, named as the application named it.
#[wasm_bindgen_test]
fn the_tabs_sit_in_a_named_tab_list() {
    focus::clear();
    let host = render();
    let list = host
        .query_selector(r#"[role="tablist"]"#)
        .expect("a valid selector")
        .expect("the tab list is in the document");
    assert_eq!(
        list.get_attribute("aria-label").as_deref(),
        Some("Sections")
    );
    assert_eq!(
        list.query_selector_all(r#"[role="tab"]"#)
            .expect("a valid selector")
            .length(),
        2
    );
}

/// A box the application named around a scrub field or a select names the control inside, which is where a reader looks for it.
#[wasm_bindgen_test]
fn a_named_box_around_a_field_names_the_field() {
    focus::clear();
    let host = render();
    for (role, name) in [("spinbutton", "Speed"), ("combobox", "Size")] {
        let control = host
            .query_selector(&format!(r#"[role="{role}"]"#))
            .expect("a valid selector")
            .unwrap_or_else(|| panic!("the {role} is in the document"));
        assert_eq!(control.get_attribute("aria-label").as_deref(), Some(name));
    }
    let groups = host
        .query_selector_all(
            r#"[role="group"][aria-label="Speed"], [role="group"][aria-label="Size"]"#,
        )
        .expect("a valid selector");
    assert_eq!(groups.length(), 0, "the wrapper does not name itself too");
}

fn key(key: telar::NamedKey) {
    PAGE.with(|page| {
        let mut page = page.borrow_mut();
        let page = page.as_mut().expect("mounted");
        page.tree.on_event(&Event::KeyPressed {
            key: telar::Key::Named(key),
            modifiers: telar::ModifiersState::default(),
            unmodified: None,
        });
    });
}

/// A menu of more rows than the page holds and a command palette, both shut until a test opens them.
#[cfg(feature = "overlays")]
fn overlays() -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {
    let rows = Children::new(|| {
        let mut slots = Slots::new();
        for n in 1..=40 {
            let row = ItemProps::props()
                .label(Reactive::of(move || format!("Action {n}")))
                .build();
            slots.push(None, item(row, Children::default())?);
        }
        Ok(slots)
    });
    let actions = menu(MenuProps::props().label("Actions").build(), rows)?;
    let view = menu(MenuProps::props().label("View").build(), view_rows())?;
    let commands: Vec<Command> = [Command::new("zoom", "Zoom in").in_group("View")]
        .into_iter()
        .chain((1..=20).map(|n| Command::new(format!("c{n}"), format!("Command {n}"))))
        .collect();
    let palette = command_palette(
        CommandPaletteProps::props()
            .id(PALETTE)
            .commands(commands)
            .build(),
        Children::default(),
    )?;
    let terms = Children::new(|| {
        let mut slots = Slots::new();
        for n in 1..=40 {
            let clause = Text::new(
                move || format!("Clause {n} of the terms."),
                LayoutStyle::new(),
                ink,
            )?;
            slots.push(None, box_item(clause));
        }
        Ok(slots)
    });
    let notice = modal(ModalProps::props().id(NOTICE).title("Terms").build(), terms)?;
    Ok(vec![actions, view, palette, notice])
}

/// A submenu of choices, one of them on, and a row beside it.
#[cfg(feature = "overlays")]
fn view_rows() -> Children {
    Children::new(|| {
        let mut slots = Slots::new();
        let choices = Children::new(|| {
            let mut slots = Slots::new();
            for (label, on) in [("System", true), ("Light", false), ("Dark", false)] {
                let row = ItemProps::props()
                    .label(label)
                    .checked(on)
                    .radio(true)
                    .build();
                slots.push(None, item(row, Children::default())?);
            }
            Ok(slots)
        });
        let theme = SubmenuProps::props().label("Theme").hint("System").build();
        slots.push(None, submenu(theme, choices)?);
        slots.push(
            None,
            item(
                ItemProps::props()
                    .label("Rulers")
                    .shortcut("Mod+Shift+R")
                    .build(),
                Children::default(),
            )?,
        );
        slots.push(
            None,
            item(
                ItemProps::props()
                    .label("Reduce motion")
                    .switch(false)
                    .build(),
                Children::default(),
            )?,
        );
        Ok(slots)
    })
}

#[cfg(feature = "overlays")]
const PALETTE: &str = "commands";

/// A list that scrolls is one control that takes focus, drawn where it opened; focused, it points a reader at the row its cursor is on, a `menuitem` of a menu or an `option` of a list box.
#[cfg(feature = "overlays")]
async fn assert_list_points_at_its_cursor(list: Role, row_role: &str, what: &str) {
    focus::request(focusable(list).id);
    key(telar::NamedKey::ArrowDown);
    let host = render();
    let row = pointed_at(&host, list.as_str());
    assert_eq!(row.get_attribute("role").as_deref(), Some(row_role));
    assert_clean(&host, what).await;
}

#[cfg(feature = "overlays")]
#[wasm_bindgen_test]
async fn an_open_menu_too_long_for_the_page_adds_no_violations() {
    let trigger = focus::exposed()
        .into_iter()
        .find(|exposed| exposed.role == Role::Button && exposed.expanded.is_some())
        .expect("the menu's trigger is on the page");
    focus::request(trigger.id);
    key(telar::NamedKey::Enter);
    let host = render();
    let list = focusable(Role::Menu);
    assert_eq!(
        focus::current(),
        Some(list.id),
        "the menu took the keyboard"
    );
    let (named, _) = controlled(&element_of(trigger.node));
    assert!(named.is_same_node(Some(element_of(list.node).as_ref())));
    assert_clean(&host, "with a long menu open").await;
    assert_list_points_at_its_cursor(Role::Menu, "menuitem", "with the menu's list focused").await;
    key(telar::NamedKey::Escape);
    assert_eq!(
        focus::current(),
        Some(trigger.id),
        "Escape gives the keyboard back to the trigger"
    );
    focus::clear();
    render();
}

#[cfg(all(feature = "overlays", feature = "advanced"))]
fn inert_within(element: &web_sys::Element) -> bool {
    std::iter::successors(Some(element.clone()), |node| node.parent_element())
        .any(|node| node.has_attribute("inert"))
}

/// An open menu holds the keyboard, and the page behind it, the list and the table that overflow it included, is inert while it is open: out of reach of a reader as of Tab, not a scroll region left in the tree with no way to focus it. The menu's button stays, saying its menu is open; closing the menu gives the page back.
#[cfg(all(feature = "overlays", feature = "advanced"))]
#[wasm_bindgen_test]
async fn an_open_menu_makes_the_page_behind_it_inert() {
    let trigger = focus::exposed()
        .into_iter()
        .find(|exposed| exposed.role == Role::Button && exposed.expanded.is_some())
        .expect("the menu's trigger is on the page");
    focus::request(trigger.id);
    key(telar::NamedKey::Enter);
    let host = render();
    let behind = |role: &str| {
        host.query_selector(&format!(r#"[role="{role}"]"#))
            .expect("a valid selector")
            .unwrap_or_else(|| panic!("a {role} is on the page"))
    };
    for role in ["listbox", "grid", "tablist"] {
        assert!(
            inert_within(&behind(role)),
            "the {role} behind the menu is inert"
        );
    }
    assert!(!inert_within(&element_of(focusable(Role::Menu).node)));
    let button = element_of(trigger.node);
    assert!(!inert_within(&button), "the menu's button stays");
    assert_eq!(
        button.get_attribute("aria-expanded").as_deref(),
        Some("true")
    );
    assert_clean(
        &host,
        "with a menu open over a list and a table that overflow",
    )
    .await;

    key(telar::NamedKey::Escape);
    let host = render();
    assert_eq!(
        host.query_selector_all("[inert]")
            .expect("a valid selector")
            .length(),
        0,
        "closing the menu gives the page back"
    );
    focus::clear();
    render();
}

/// A row with a switch is a checkbox of the menu that says whether it is on; the switch drawn at its end is for the eye only, and the menu with it open adds no violations.
#[cfg(feature = "overlays")]
#[wasm_bindgen_test]
async fn an_open_menu_with_a_switch_row_adds_no_violations() {
    let trigger = focus::exposed()
        .into_iter()
        .filter(|exposed| exposed.role == Role::Button && exposed.expanded.is_some())
        .nth(1)
        .expect("the View menu's trigger is on the page");
    focus::request(trigger.id);
    key(telar::NamedKey::ArrowDown);
    let host = render();
    let row = host
        .query_selector(r#"[role="menuitemcheckbox"][aria-checked="false"]"#)
        .expect("a valid selector")
        .expect("the switch row says it is a checkbox that is off");
    assert_eq!(
        row.text_content().as_deref().map(str::trim),
        Some("Reduce motion")
    );
    assert!(
        row.query_selector(r#"[role="switch"]"#)
            .expect("a valid selector")
            .is_none(),
        "the drawn switch is not offered to a reader"
    );
    assert_clean(&host, "with a switch row in an open menu").await;
    key(telar::NamedKey::Escape);
    focus::clear();
    render();
}

/// A command that has keys says them as `aria-keyshortcuts` on the control that runs it, in the form UI Events names the keys, and keeps them in the row's name: the row of an open menu and an icon button.
#[cfg(feature = "overlays")]
#[wasm_bindgen_test]
async fn a_shortcut_is_written_on_the_control_it_runs() {
    telar::set_keyboard_convention(telar::KeyboardConvention::Pc);
    let host = render();
    let find = host
        .query_selector(r#"[role="toolbar"] [aria-label="Find"]"#)
        .expect("a valid selector")
        .expect("the icon button is in the document");
    assert_eq!(
        find.get_attribute("aria-keyshortcuts").as_deref(),
        Some("Control+F")
    );
    let bold = host
        .query_selector(r#"[role="toolbar"] [aria-label="Bold"]"#)
        .expect("a valid selector")
        .expect("the icon button without keys is in the document");
    assert_eq!(bold.get_attribute("aria-keyshortcuts"), None);

    let trigger = focus::exposed()
        .into_iter()
        .filter(|exposed| exposed.role == Role::Button && exposed.expanded.is_some())
        .nth(1)
        .expect("the View menu's trigger is on the page");
    focus::request(trigger.id);
    key(telar::NamedKey::ArrowDown);
    let host = render();
    let row = host
        .query_selector(r#"[role="menuitem"][aria-keyshortcuts]"#)
        .expect("a valid selector")
        .expect("the row that has keys writes them");
    assert_eq!(
        row.get_attribute("aria-keyshortcuts").as_deref(),
        Some("Control+Shift+R")
    );
    assert!(
        row.text_content()
            .is_some_and(|text| text.contains("Rulers") && text.contains("Ctrl+Shift+R")),
        "the keys are still part of what the row says for readers that skip the attribute"
    );
    assert_clean(&host, "with a shortcut on a menu row").await;
    key(telar::NamedKey::Escape);
    focus::clear();
    render();
}

/// A menu button and the row of its menu that opens a submenu each say what they open and whether it is open; the submenu is a menu of its own whose choices say which one is on, and the keyboard goes into it, back out of it and back to the button.
#[cfg(feature = "overlays")]
#[wasm_bindgen_test]
async fn an_open_submenu_says_what_opened_it_and_adds_no_violations() {
    let trigger = focus::exposed()
        .into_iter()
        .filter(|exposed| exposed.role == Role::Button && exposed.expanded.is_some())
        .nth(1)
        .expect("the View menu's trigger is on the page");
    let button = element_of(trigger.node);
    assert_eq!(
        button.get_attribute("aria-haspopup").as_deref(),
        Some("menu")
    );
    focus::request(trigger.id);
    key(telar::NamedKey::ArrowDown);
    key(telar::NamedKey::ArrowRight);
    let host = render();
    let menus = focus::exposed()
        .into_iter()
        .filter(|exposed| exposed.role == Role::Menu)
        .collect::<Vec<_>>();
    assert_eq!(menus.len(), 2, "the submenu opened beside the menu");
    assert_eq!(focus::current(), Some(menus[1].id), "and took the keyboard");
    let row = host
        .query_selector(r#"[role="menuitem"][aria-haspopup="menu"]"#)
        .expect("a valid selector")
        .expect("the submenu's row says it opens a menu");
    assert_eq!(row.get_attribute("aria-expanded").as_deref(), Some("true"));
    let choices = host
        .query_selector_all(r#"[role="menuitemradio"]"#)
        .expect("a valid selector");
    assert_eq!(choices.length(), 3);
    let on = host
        .query_selector(r#"[role="menuitemradio"][aria-checked="true"]"#)
        .expect("a valid selector")
        .expect("the choice that is on says so");
    assert_eq!(on.text_content().as_deref().map(str::trim), Some("System"));
    assert_clean(&host, "with a submenu open").await;

    key(telar::NamedKey::ArrowLeft);
    let host = render();
    assert_eq!(focus::current(), Some(menus[0].id), "back to the menu");
    let row = host
        .query_selector(r#"[role="menuitem"][aria-haspopup="menu"]"#)
        .expect("a valid selector")
        .expect("the row is still there");
    assert_eq!(row.get_attribute("aria-expanded").as_deref(), Some("false"));
    key(telar::NamedKey::Escape);
    render();
    assert_eq!(focus::current(), Some(trigger.id), "and back to the button");
    focus::clear();
    render();
}

#[cfg(feature = "overlays")]
#[wasm_bindgen_test]
async fn an_open_command_palette_adds_no_violations() {
    telar::open_overlay(PALETTE);
    let host = render();
    let field = focus::exposed()
        .into_iter()
        .find(|exposed| exposed.role == Role::TextInput && focus::current() == Some(exposed.id))
        .expect("the palette's field holds the keyboard");
    let (list, row) = controlled(&element_of(field.node));
    assert_eq!(list.get_attribute("role").as_deref(), Some("listbox"));
    assert_eq!(
        list.get_attribute("aria-label").as_deref(),
        Some("Commands")
    );
    let row = row.expect("the field points at the row its cursor is on");
    assert_eq!(row.get_attribute("role").as_deref(), Some("option"));
    assert_eq!(
        row.get_attribute("aria-selected").as_deref(),
        Some("true"),
        "the row the cursor is on is the selected option"
    );
    assert_clean(&host, "with the command palette open").await;
    assert_list_points_at_its_cursor(Role::ListBox, "option", "with the palette's list focused")
        .await;
    telar::close_overlay(PALETTE);
    focus::clear();
    render();
}

/// An open select keeps the document's focus on its combobox, which names the list it opened with `aria-controls` and points at the row its cursor is on inside that list; shut again, the keyboard is still on it.
#[wasm_bindgen_test]
async fn an_open_select_names_its_list_and_adds_no_violations() {
    let combobox = focusable(Role::ComboBox);
    focus::request(combobox.id);
    key(telar::NamedKey::Enter);
    let host = render();
    let trigger = element_of(combobox.node);
    assert_eq!(
        trigger.get_attribute("aria-expanded").as_deref(),
        Some("true")
    );
    let (list, row) = controlled(&trigger);
    assert!(
        !trigger.contains(Some(list.as_ref())),
        "the list is laid over the page, not inside the combobox"
    );
    assert_eq!(list.get_attribute("role").as_deref(), Some("listbox"));
    let row = row.expect("the combobox points at the row its cursor is on");
    assert_eq!(row.get_attribute("role").as_deref(), Some("option"));
    let row_id = row.id();
    let selected: Vec<Option<String>> = {
        let options = list
            .query_selector_all(r#"[role="option"]"#)
            .expect("a valid selector");
        (0..options.length())
            .filter_map(|i| options.item(i)?.dyn_into::<web_sys::Element>().ok())
            .map(|option| option.get_attribute("aria-selected"))
            .collect()
    };
    assert_eq!(
        selected,
        [Some("true".to_string()), Some("false".to_string())],
        "each option says whether it is the choice"
    );
    assert_ne!(
        list.get_attribute("tabindex").as_deref(),
        Some("0"),
        "the list is no Tab stop"
    );
    assert_clean(&host, "with a select open").await;

    key(telar::NamedKey::ArrowDown);
    let host = render();
    assert_ne!(
        element_of(combobox.node)
            .get_attribute("aria-activedescendant")
            .as_deref(),
        Some(row_id.as_str()),
        "the arrow moved what the combobox points at"
    );
    assert_clean(&host, "with the select's cursor moved").await;

    key(telar::NamedKey::Escape);
    let host = render();
    assert_eq!(
        focus::current(),
        Some(combobox.id),
        "Escape leaves it focused"
    );
    assert!(
        element_of(combobox.node)
            .get_attribute("aria-controls")
            .is_none(),
        "and a shut select names no list"
    );
    assert_clean(&host, "with the select shut again").await;
    focus::clear();
    render();
}

/// The element `control` names with `aria-controls`, and the row inside it that `control` points at with `aria-activedescendant`, if it points at one.
fn controlled(control: &web_sys::Element) -> (web_sys::Element, Option<web_sys::Element>) {
    let id = control
        .get_attribute("aria-controls")
        .expect("the control names what it controls");
    let list = document()
        .get_element_by_id(&id)
        .unwrap_or_else(|| panic!("`{id}` names an element"));
    let row = control.get_attribute("aria-activedescendant").map(|id| {
        let row = document()
            .get_element_by_id(&id)
            .unwrap_or_else(|| panic!("`{id}` names an element"));
        assert!(
            list.contains(Some(row.as_ref())),
            "the row pointed at is one of the list it controls"
        );
        row
    });
    (list, row)
}

#[cfg(feature = "overlays")]
const NOTICE: &str = "notice";

/// A dialog whose body holds more than it shows makes the body a Tab stop the keyboard scrolls, named after the dialog, and adds no violations.
#[cfg(feature = "overlays")]
#[wasm_bindgen_test]
async fn an_open_dialog_whose_body_scrolls_adds_no_violations() {
    telar::open_overlay(NOTICE);
    // The card is sized to the room the first layout finds, so its body only overflows from the second.
    render();
    let host = render();
    let dialog = host
        .query_selector("[role=dialog][aria-label=Terms]")
        .expect("a valid selector")
        .expect("the dialog is open");
    let body = dialog
        .query_selector("[tabindex='0'][aria-label=Terms]")
        .expect("a valid selector")
        .expect("the body is a named Tab stop");
    assert!(
        body.scroll_height() > body.client_height(),
        "and it scrolls"
    );
    assert_eq!(body.get_attribute("role").as_deref(), Some("group"));
    assert_clean(&host, "with a dialog whose body scrolls open").await;

    let stop = focus::tab_order()
        .into_iter()
        .find(|stop| stop.role == Role::ScrollArea)
        .expect("Tab stops on the body");
    focus::request(stop.id);
    key(telar::NamedKey::PageDown);
    let host = render();
    assert!(body.scroll_top() > 0, "Page Down scrolled it");
    assert_clean(&host, "with the dialog's body focused").await;
    telar::close_overlay(NOTICE);
    focus::clear();
    render();
}

/// Each edge of a resizable box reaches the document as a named separator with its axis and its range, and an edge with no largest size writes none.
#[cfg(feature = "workbench")]
#[wasm_bindgen_test]
async fn a_resizable_box_s_edges_carry_their_axis_and_range() {
    focus::clear();
    let host = render();
    let edge = |name: &str| {
        host.query_selector(&format!(r#"[role="separator"][aria-label="{name}"]"#))
            .expect("a valid selector")
            .unwrap_or_else(|| panic!("the {name} edge is in the document"))
    };
    let width = edge("Resize width");
    assert_eq!(
        width.get_attribute("aria-orientation").as_deref(),
        Some("vertical")
    );
    assert_eq!(width.get_attribute("aria-valuenow").as_deref(), Some("160"));
    assert_eq!(width.get_attribute("aria-valuemin").as_deref(), Some("80"));
    assert_eq!(width.get_attribute("aria-valuemax").as_deref(), Some("400"));
    let height = edge("Resize height");
    assert_eq!(
        height.get_attribute("aria-orientation").as_deref(),
        Some("horizontal")
    );
    assert_eq!(height.get_attribute("aria-valuenow").as_deref(), Some("48"));
    assert_eq!(height.get_attribute("aria-valuemax"), None);
    assert_clean(&host, "with a resizable box").await;
}

/// A picture worked out rather than loaded: a band of colour, which is all a reader is told about through its name.
fn swatch_picture() -> std::sync::Arc<telar::ImageData> {
    let pixels = (0..16 * 8)
        .flat_map(|i: u32| [(i * 2) as u8, 90, 160, 255])
        .collect();
    std::sync::Arc::new(telar::ImageData::new(pixels, 16, 8))
}

/// A link inside the application and one out of it, a picture and one that only decorates, a text area with its count, and a search field holding a query.
fn everyday() -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {
    let links = Container::new(
        LayoutStyle::new().flex_row().gap(16.0),
        vec![
            text_link(
                LinkProps::props()
                    .label("Changelog")
                    .to(Location::root().segment("changelog"))
                    .current(true)
                    .build(),
                Children::default(),
            )?,
            text_link(
                LinkProps::props()
                    .label("Sponsor")
                    .to(external("https://example.com/sponsor"))
                    .build(),
                Children::default(),
            )?,
        ],
    )?;
    let pictures = Container::new(
        LayoutStyle::new().flex_row().gap(16.0),
        vec![
            image(
                ImageProps::props()
                    .source(ImageSource::Ready(swatch_picture()))
                    .alt("A blue gradient")
                    .width(48.0)
                    .build(),
                Children::default(),
            )?,
            image(
                ImageProps::props()
                    .source(ImageSource::Ready(swatch_picture()))
                    .alt("")
                    .width(24.0)
                    .build(),
                Children::default(),
            )?,
            image(
                ImageProps::props()
                    .source(ImageSource::Failed)
                    .alt("Team photo")
                    .width(96.0)
                    .height(32.0)
                    .build(),
                Children::default(),
            )?,
        ],
    )?;
    let notes = textarea(
        TextareaProps::props()
            .label("Notes")
            .rows(2)
            .counter(true)
            .max_length(80)
            .build(),
        Children::default(),
    )?;
    let search = search_field(
        SearchFieldProps::props()
            .value(telar::signal("buttons".to_string()))
            .build(),
        Children::default(),
    )?;
    Ok(vec![box_item(links), box_item(pictures), notes, search])
}

/// A list to choose several from and a sortable table with resizable columns, each with more rows than the box it fills shows, as a real one has: both build only the rows on screen and say where each sits among them all. And a table with no rows, which says so.
#[cfg(feature = "advanced")]
fn data() -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {
    use telar_components::{
        CellAlign, Column, DataTableProps, ListRow, SelectionMode, TableRow, VirtualListProps,
        data_table, virtual_list,
    };
    let rows: Vec<ListRow> = (0..40)
        .map(|i| ListRow::new(format!("c{i}"), format!("Contact {i}")).disabled(i == 3))
        .collect();
    let list = virtual_list(
        VirtualListProps::props()
            .items(rows)
            .selection_mode(SelectionMode::Multiple)
            .selected_rows(telar::signal(std::collections::BTreeSet::from([
                "c1".into()
            ])))
            .label("Contacts")
            .build(),
        Children::default(),
    )?;
    let files: Vec<TableRow> = (0..40)
        .map(|i| {
            TableRow::new(
                format!("f{i}"),
                [format!("file_{i}.rs"), format!("{}", i * 3)],
            )
        })
        .collect();
    let table = data_table(
        DataTableProps::props()
            .columns(vec![
                Column::new("name", "Name").sortable(true).resizable(true),
                Column::new("size", "Size")
                    .fixed(80.0)
                    .align(CellAlign::End)
                    .sortable(true)
                    .resizable(true),
            ])
            .rows(files)
            .selected(telar::signal(Some("f2".into())))
            .label("Files")
            .build(),
        Children::default(),
    )?;
    let downloads = data_table(
        DataTableProps::props()
            .columns(vec![Column::new("name", "Name")])
            .label("Downloads")
            .build(),
        Children::default(),
    )?;
    Ok(vec![
        box_item(Container::new(
            LayoutStyle::new().flex_column().height(176.0),
            vec![list],
        )?),
        box_item(Container::new(
            LayoutStyle::new().flex_column().height(200.0),
            vec![table],
        )?),
        box_item(Container::new(
            LayoutStyle::new().flex_column().height(96.0),
            vec![downloads],
        )?),
    ])
}

/// A picture reaches the document named by its words, and one that only decorates is hidden from a reader.
#[wasm_bindgen_test]
fn a_picture_is_named_and_a_decoration_is_hidden() {
    focus::clear();
    let host = render();
    assert!(
        host.query_selector(r#"[role="img"][aria-label="A blue gradient"]"#)
            .expect("a valid selector")
            .is_some(),
        "the picture is an image named by its words"
    );
    let hidden = host
        .query_selector_all(r#"[role="img"]:not([aria-label])"#)
        .expect("a valid selector");
    for i in 0..hidden.length() {
        let picture: web_sys::Element = hidden
            .item(i)
            .expect("a node")
            .dyn_into()
            .expect("an element");
        let hidden_above = std::iter::successors(Some(picture), |node| node.parent_element())
            .any(|node| node.get_attribute("aria-hidden").as_deref() == Some("true"));
        assert!(hidden_above, "an unnamed picture is hidden from a reader");
    }
}

/// A link out of the application says so in its name, and is written as a link a page cannot reach back from.
#[wasm_bindgen_test]
fn a_link_out_of_the_application_says_where_it_goes() {
    focus::clear();
    let host = render();
    let out = host
        .query_selector(r#"a[aria-label="Sponsor (external link)"]"#)
        .expect("a valid selector")
        .expect("the external link is in the document");
    assert_eq!(
        out.get_attribute("href").as_deref(),
        Some("https://example.com/sponsor")
    );
    assert_eq!(out.get_attribute("rel").as_deref(), Some("noopener"));
    let current = host
        .query_selector(r#"a[aria-current]"#)
        .expect("a valid selector")
        .expect("the current link says so");
    assert_eq!(
        current.get_attribute("aria-current").as_deref(),
        Some("page")
    );
}

/// The text area and the search field are named fields, and the search field's clear button is named and no Tab stop.
#[wasm_bindgen_test]
async fn a_text_area_and_a_search_field_are_named_and_add_no_violations() {
    focus::clear();
    let host = render();
    assert!(
        host.query_selector(r#"[role="textbox"][aria-label="Notes"]"#)
            .expect("a valid selector")
            .is_some()
    );
    let clear = host
        .query_selector(r#"button[aria-label="Clear search"]"#)
        .expect("a valid selector")
        .expect("a query brings a clear button");
    assert_eq!(clear.get_attribute("tabindex").as_deref(), Some("-1"));
    focus::request(
        focus::exposed()
            .into_iter()
            .find(|exposed| exposed.role == Role::MultilineTextInput)
            .expect("a text area")
            .id,
    );
    let host = render();
    assert_clean(&host, "with the text area focused").await;
    focus::clear();
}

/// A focused list points a reader at the option its cursor is on, inside itself.
#[cfg(feature = "advanced")]
#[wasm_bindgen_test]
async fn a_focused_virtual_list_points_a_reader_at_its_cursor_option() {
    mount();
    let list = focus::exposed()
        .into_iter()
        .filter(|exposed| exposed.role == Role::ListBox)
        .find(|exposed| {
            render();
            element_of(exposed.node)
                .get_attribute("aria-label")
                .as_deref()
                == Some("Contacts")
        })
        .expect("the contacts list is on the page");
    focus::request(list.id);
    key(telar::NamedKey::ArrowDown);
    let host = render();
    let contacts = element_of(list.node);
    assert_eq!(
        contacts.get_attribute("aria-multiselectable").as_deref(),
        Some("true"),
        "the list takes several rows"
    );
    let chosen = contacts
        .query_selector(r#"[role="option"][aria-posinset="2"]"#)
        .expect("a valid selector")
        .expect("the chosen option is built");
    assert_eq!(
        chosen.get_attribute("aria-selected").as_deref(),
        Some("true")
    );
    let id = contacts
        .get_attribute("aria-activedescendant")
        .expect("the focused list points at its cursor");
    let option = document()
        .get_element_by_id(&id)
        .unwrap_or_else(|| panic!("`{id}` names an element"));
    assert!(contacts.contains(Some(option.as_ref())));
    assert_eq!(option.get_attribute("role").as_deref(), Some("option"));
    assert_eq!(
        option.get_attribute("aria-selected").as_deref(),
        Some("false")
    );
    assert_eq!(option.get_attribute("aria-posinset").as_deref(), Some("3"));
    assert_eq!(option.get_attribute("aria-setsize").as_deref(), Some("40"));
    assert_eq!(
        option.get_attribute("aria-level"),
        None,
        "an option has no level"
    );
    assert_clean(&host, "with the list focused").await;
    focus::clear();
}

/// A focused table points a reader at the cell its cursor is on, the header of the column it is sorted by says which way, and the chosen row says it is chosen.
#[cfg(feature = "advanced")]
#[wasm_bindgen_test]
async fn a_focused_data_table_points_a_reader_at_its_cursor_cell() {
    focus::request(focusable(Role::Grid).id);
    let host = render();
    let cell = pointed_at(&host, "grid");
    assert_eq!(cell.get_attribute("role").as_deref(), Some("gridcell"));
    let row = cell
        .closest(r#"[role="row"]"#)
        .expect("a valid selector")
        .expect("a cell sits in a row");
    assert_eq!(row.get_attribute("aria-selected").as_deref(), Some("true"));
    assert_eq!(row.get_attribute("aria-rowindex").as_deref(), Some("4"));
    assert_eq!(
        row.get_attribute("aria-level"),
        None,
        "a grid's row has no level"
    );
    let grid = row
        .closest(r#"[role="grid"]"#)
        .expect("a valid selector")
        .expect("a row sits in a grid");
    assert_eq!(grid.get_attribute("aria-rowcount").as_deref(), Some("41"));
    assert_eq!(
        grid.get_attribute("aria-multiselectable"),
        None,
        "a table of one choice does not say it takes several"
    );
    let header = grid
        .query_selector(r#"[role="row"]"#)
        .expect("a valid selector")
        .expect("the header is the first row");
    assert_eq!(header.get_attribute("aria-rowindex").as_deref(), Some("1"));

    for _ in 0..3 {
        key(telar::NamedKey::ArrowUp);
    }
    key(telar::NamedKey::Enter);
    let host = render();
    let header = pointed_at(&host, "grid");
    assert_eq!(
        header.get_attribute("role").as_deref(),
        Some("columnheader")
    );
    assert_eq!(
        header.get_attribute("aria-sort").as_deref(),
        Some("ascending")
    );
    assert_eq!(
        host.query_selector_all("[aria-sort]")
            .expect("a valid selector")
            .length(),
        1,
        "only the column the rows are sorted by says so"
    );
    assert_clean(&host, "with the table focused and sorted").await;
    focus::clear();
}

/// Each resizable column's header holds a named separator that reads the column's width and its narrowest, and the header is named by its column alone.
#[cfg(feature = "advanced")]
#[wasm_bindgen_test]
async fn a_data_table_s_resizable_columns_hold_named_separators() {
    focus::clear();
    let host = render();
    let separator = host
        .query_selector(
            r#"[role="columnheader"] [role="separator"][aria-label="Resize Size column"]"#,
        )
        .expect("a valid selector")
        .expect("the size column's header holds its separator");
    assert_eq!(
        separator.get_attribute("aria-orientation").as_deref(),
        Some("vertical")
    );
    assert_eq!(
        separator.get_attribute("aria-valuenow").as_deref(),
        Some("80")
    );
    assert_eq!(
        separator.get_attribute("aria-valuemin").as_deref(),
        Some("48")
    );
    assert_eq!(separator.get_attribute("aria-valuemax"), None);
    assert_eq!(separator.get_attribute("tabindex").as_deref(), Some("0"));
    let header = separator
        .closest(r#"[role="columnheader"]"#)
        .expect("a valid selector")
        .expect("inside its header");
    assert_eq!(header.get_attribute("aria-label").as_deref(), Some("Size"));
    focus::request(
        focus::exposed()
            .into_iter()
            .find(|exposed| exposed.role == Role::Splitter)
            .expect("a separator to focus")
            .id,
    );
    let host = render();
    assert_clean(&host, "with a column's separator focused").await;
    focus::clear();
}

/// A table with no rows keeps its grid and its header, and says so in a status beside the grid rather than inside it.
#[cfg(feature = "advanced")]
#[wasm_bindgen_test]
async fn an_empty_data_table_says_so_in_a_status_beside_its_grid() {
    focus::clear();
    let host = render();
    let grid = host
        .query_selector(r#"[role="grid"][aria-label="Downloads"]"#)
        .expect("a valid selector")
        .expect("the empty table's grid is in the document");
    assert_eq!(grid.get_attribute("aria-rowcount").as_deref(), Some("1"));
    let statuses = host
        .query_selector_all(r#"[role="status"]"#)
        .expect("a valid selector");
    let status = (0..statuses.length())
        .filter_map(|i| statuses.item(i)?.dyn_into::<web_sys::Element>().ok())
        .find(|status| {
            status
                .text_content()
                .unwrap_or_default()
                .contains("Nothing to show")
        })
        .expect("the empty table says so in a status");
    assert!(
        status
            .closest(r#"[role="grid"]"#)
            .expect("a valid selector")
            .is_none(),
        "the message is no part of the grid"
    );
    assert_clean(&host, "with an empty table").await;
}
