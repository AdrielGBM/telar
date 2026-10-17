use geometry_core::Rect;

use super::*;

fn node(id: Option<u64>, role: Role, name: &str) -> AccessNode {
    AccessNode {
        id,
        parent: None,
        role,
        name: name.to_string(),
        rect: Rect::new(0.0, 0.0, 10.0, 10.0),
        focused: false,
        enabled: true,
        toggled: None,
        value: None,
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
    }
}

/// The shape a screen reader is handed: one window, every control and every piece of text under it, and each control addressable so focus and activation can come back.
#[test]
fn the_window_is_the_root_and_everything_hangs_from_it() {
    let nodes = vec![
        node(Some(7), Role::Button, "Save"),
        node(None, Role::Label, "Unsaved changes"),
    ];
    let update = tree_update(&nodes, "Editor", None);

    assert_eq!(update.nodes.len(), 3, "two nodes plus the window");
    let (root_id, root) = update.nodes.last().unwrap();
    assert_eq!(*root_id, ROOT);
    assert_eq!(root.role(), AkRole::Window);
    assert_eq!(root.children().len(), 2);
    assert_eq!(update.nodes[0].1.role(), AkRole::Button);
    assert_eq!(update.nodes[1].1.role(), AkRole::Label);
}

/// A control keeps its identity across frames, so a reader is not told the button appeared anew every time anything else on screen moved. The label beside it has no identity to keep and does not need one.
#[test]
fn a_control_keeps_the_same_node_id_between_updates() {
    let first = tree_update(&[node(Some(7), Role::Button, "Save")], "Editor", None);
    let with_extra = tree_update(
        &[
            node(None, Role::Label, "Heading"),
            node(Some(7), Role::Button, "Save"),
        ],
        "Editor",
        None,
    );
    let button = |u: &TreeUpdate| {
        u.nodes
            .iter()
            .find(|(_, n)| n.role() == AkRole::Button)
            .unwrap()
            .0
    };
    assert_eq!(button(&first), button(&with_extra));
}

/// The focused control is what the reader is told about first, and the window stands in when nothing is.
#[test]
fn focus_points_at_the_focused_control_or_at_the_window() {
    assert_eq!(tree_update(&[], "Editor", None).focus, ROOT);

    let mut focused = node(Some(7), Role::Button, "Save");
    focused.focused = true;
    let update = tree_update(&[focused], "Editor", None);
    assert_ne!(update.focus, ROOT);
    assert_eq!(update.focus, update.nodes[0].0);
}

/// A request coming back names a node; only a control has a focus id behind it, and one that has gone from the tree since the reader last looked names nothing.
#[test]
fn a_request_maps_back_to_the_control_that_claimed_it() {
    let nodes = vec![node(Some(7), Role::Button, "Save")];
    let update = tree_update(&nodes, "Editor", None);
    let target = update.nodes[0].0;

    let request = ActionRequest {
        action: Action::Click,
        target_tree: TreeId::ROOT,
        target_node: target,
        data: None,
    };
    assert_eq!(requested_focus_id(&request, &nodes), Some((7, true)));

    let stale = ActionRequest {
        action: Action::Focus,
        target_tree: TreeId::ROOT,
        target_node: NodeId(target.0 + 1),
        data: None,
    };
    assert_eq!(requested_focus_id(&stale, &nodes), None);
    assert_eq!(
        requested_focus_id(
            &ActionRequest {
                action: Action::Focus,
                target_tree: TreeId::ROOT,
                target_node: ROOT,
                data: None
            },
            &nodes
        ),
        None,
        "the window itself is not a control"
    );
}

/// A quotation in another language is read in that language's voice, and a node the application said nothing about inherits the window's.
#[test]
fn a_node_in_another_language_says_which() {
    let mut quote = node(None, Role::Label, "Bonjour");
    quote.lang = Some("fr".to_string());
    let update = tree_update(&[quote, node(None, Role::Label, "Hello")], "Editor", None);
    assert_eq!(update.nodes[0].1.language(), Some("fr"));
    assert_eq!(update.nodes[1].1.language(), None);
}

/// The root carries the surface's own language, and a node saying nothing about its own inherits it —
/// the nearest one wins, so a quotation still overrides it for its own subtree.
#[test]
fn the_root_language_is_the_surfaces_own_and_a_node_can_still_override_it() {
    let mut quote = node(None, Role::Label, "Bonjour");
    quote.lang = Some("fr".to_string());
    let update = tree_update(
        &[quote, node(None, Role::Label, "Hello")],
        "Editor",
        Some("es"),
    );
    let (root_id, root) = update.nodes.last().unwrap();
    assert_eq!(*root_id, ROOT);
    assert_eq!(root.language(), Some("es"));
    assert_eq!(update.nodes[0].1.language(), Some("fr"));
    assert_eq!(update.nodes[1].1.language(), None);
}

/// A named picture is an image to the reader, which is what AccessKit calls it.
#[test]
fn a_named_drawing_is_an_image() {
    let update = tree_update(&[node(None, Role::Drawing, "Company logo")], "Editor", None);
    assert_eq!(update.nodes[0].1.role(), AkRole::Image);
    assert_eq!(update.nodes[0].1.label(), Some("Company logo"));
}

#[test]
fn a_link_is_announced_with_where_it_goes() {
    let link = AccessNode {
        url: Some("https://example.com".to_string()),
        ..node(Some(3), Role::Link, "Example")
    };
    let update = tree_update(&[link], "Links", None);
    let ak = &update.nodes[0].1;
    assert_eq!(ak.role(), AkRole::Link);
    assert_eq!(ak.url(), Some("https://example.com"));
}

/// The current page, the current location on it, or plainly the current one: AccessKit's `aria_current` carries each, and a link not marked carries none.
#[test]
fn a_current_link_says_what_it_is_the_current_one_of() {
    let published = |current: Option<CurrentKind>| {
        let link = AccessNode {
            url: Some("/#web".to_string()),
            current,
            ..node(Some(3), Role::Link, "Web")
        };
        tree_update(&[link], "Links", None).nodes[0]
            .1
            .aria_current()
    };
    assert_eq!(published(Some(CurrentKind::Page)), Some(AriaCurrent::Page));
    assert_eq!(
        published(Some(CurrentKind::Location)),
        Some(AriaCurrent::Location)
    );
    assert_eq!(published(Some(CurrentKind::Item)), Some(AriaCurrent::True));
    assert_eq!(published(None), None);
}

/// One flag, carried in the property AccessKit reads for the role: a switch's and a toggle button's `toggled`, a tab's `selected`, a disclosure's `expanded`.
#[test]
fn a_state_lands_where_its_role_is_read_from() {
    let toggled = |role: Role, on: bool| AccessNode {
        toggled: Some(on),
        ..node(Some(1), role, "Control")
    };
    let published = |node: AccessNode| tree_update(&[node], "States", None).nodes[0].1.clone();

    let switch = published(toggled(Role::Switch, true));
    assert_eq!(switch.role(), AkRole::Switch);
    assert_eq!(switch.toggled(), Some(Toggled::True));

    let pressed = published(toggled(Role::Button, false));
    assert_eq!(pressed.role(), AkRole::Button);
    assert_eq!(
        pressed.toggled(),
        Some(Toggled::False),
        "a button with a state is a toggle button"
    );

    let tab = published(toggled(Role::Tab, true));
    assert_eq!(tab.is_selected(), Some(true));
    assert_eq!(tab.toggled(), None, "a tab is selected, not checked");

    let disclosure = published(toggled(Role::Disclosure, false));
    assert_eq!(disclosure.is_expanded(), Some(false));
    assert_eq!(disclosure.toggled(), None);

    let ticked = published(toggled(Role::MenuItemCheckBox, true));
    assert_eq!(ticked.role(), AkRole::MenuItemCheckBox);
    assert_eq!(ticked.toggled(), Some(Toggled::True));
    let choice = published(toggled(Role::MenuItemRadio, false));
    assert_eq!(choice.role(), AkRole::MenuItemRadio);
    assert_eq!(choice.toggled(), Some(Toggled::False));

    let slider = published(toggled(Role::Slider, true));
    assert_eq!(
        slider.toggled(),
        None,
        "a slider has no on/off state to say"
    );
    assert_eq!(slider.is_selected(), None);
}

/// A tree row is selected and expanded at once, and says how deep it is and where among its siblings, counted from zero as AccessKit counts.
#[test]
fn a_tree_row_says_it_is_chosen_open_and_where_it_sits() {
    let row = AccessNode {
        toggled: Some(true),
        expanded: Some(true),
        position: Some(platform_core::SetPosition {
            level: 2,
            position: 3,
            size: 5,
        }),
        ..node(Some(4), Role::TreeItem, "Buttons")
    };
    let update = tree_update(&[row], "Explorer", None);
    let ak = &update.nodes[0].1;
    assert_eq!(ak.role(), AkRole::TreeItem);
    assert_eq!(ak.is_selected(), Some(true));
    assert_eq!(ak.is_expanded(), Some(true));
    assert_eq!(ak.level(), Some(1));
    assert_eq!(ak.position_in_set(), Some(2));

    let leaf = AccessNode {
        toggled: Some(false),
        ..node(Some(5), Role::TreeItem, "Primary")
    };
    let update = tree_update(&[leaf], "Explorer", None);
    let ak = &update.nodes[0].1;
    assert_eq!(ak.is_expanded(), None, "a leaf is neither open nor shut");
    assert_eq!(ak.is_selected(), Some(false));
}

/// A menu button and a select's combobox publish what they open beside whether it is open; a control that opens nothing publishes no popup.
#[test]
fn a_control_publishes_what_it_opens() {
    let opening = |id: u64, role: Role, popup: HasPopup| AccessNode {
        has_popup: Some(popup),
        expanded: Some(false),
        ..node(Some(id), role, "Opens")
    };
    let nodes = [
        opening(1, Role::Button, HasPopup::Menu),
        opening(2, Role::ComboBox, HasPopup::ListBox),
        node(Some(3), Role::Button, "Save"),
    ];
    let update = tree_update(&nodes, "Editor", None);
    assert_eq!(update.nodes[0].1.has_popup(), Some(AkHasPopup::Menu));
    assert_eq!(update.nodes[0].1.is_expanded(), Some(false));
    assert_eq!(update.nodes[1].1.has_popup(), Some(AkHasPopup::Listbox));
    assert_eq!(update.nodes[2].1.has_popup(), None);
}

/// A tree keeps focus while its cursor walks the rows, so the row the cursor is on is named to the reader as the tree's active descendant; one that names a row the update does not carry names nothing.
#[test]
fn a_focused_tree_names_the_row_its_cursor_is_on() {
    let tree = AccessNode {
        focused: true,
        active_descendant: Some(5),
        ..node(Some(3), Role::Tree, "Files")
    };
    let row = node(Some(5), Role::TreeItem, "docs");
    let update = tree_update(&[tree.clone(), row], "Explorer", None);
    assert_eq!(update.nodes[0].1.active_descendant(), Some(control_id(5)));
    assert_eq!(update.focus, control_id(3), "focus stays on the tree");

    let update = tree_update(&[tree], "Explorer", None);
    assert_eq!(update.nodes[0].1.active_descendant(), None);
}

/// An open combobox names the list it opened as what it controls, and its cursor rests on a row of that list; one naming a list the update does not carry names nothing.
#[test]
fn an_open_combobox_names_the_list_it_controls() {
    let combobox = AccessNode {
        focused: true,
        controls: Some(7),
        active_descendant: Some(8),
        ..node(Some(3), Role::ComboBox, "Size")
    };
    let list = node(Some(7), Role::ListBox, "Size");
    let row = AccessNode {
        toggled: Some(true),
        ..node(Some(8), Role::Option, "Large")
    };
    let update = tree_update(&[combobox.clone(), list, row], "Form", None);
    assert_eq!(update.nodes[0].1.controls(), [control_id(7)]);
    assert_eq!(update.nodes[0].1.active_descendant(), Some(control_id(8)));
    assert_eq!(update.nodes[1].1.role(), AkRole::ListBox);
    assert_eq!(update.nodes[2].1.role(), AkRole::ListBoxOption);
    assert_eq!(
        update.nodes[2].1.is_selected(),
        Some(true),
        "the chosen option is selected"
    );

    let update = tree_update(&[combobox], "Form", None);
    assert!(update.nodes[0].1.controls().is_empty());
}

mod from_a_screen {
    use std::cell::Cell;
    use std::rc::Rc;

    use accesskit::{Action, AriaCurrent, Node, Role as AkRole, Toggled};
    use geometry_core::Size;
    use layout_core::{LayoutError, LayoutStyle};
    use platform_core::{Event, Key, Location, ModifiersState, NamedKey, anchor, external};
    use reactive_core::signal;
    use renderer_core::{Color, RectStyle, TextStyle};
    use ui_core::{
        Accessible, ComponentList, Container, FixedLayer, Input, LayoutItem, PageAnchor,
        ScrollPage, ScrollViewport, StyledContainer, Text, TextRun, WindowRoot, box_item, focus,
        use_anchor_at,
    };

    use super::*;

    const WIDTH: u32 = 640;
    const HEIGHT: u32 = 900;

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

    /// What `box role:<role> toggled:$on label:"…" on_press:(…)` builds in `.rsx`: a box drawing a glyph that is a control with an on/off state, flipped by its own press.
    fn stateful_control(
        role: platform_core::Role,
        name: &'static str,
        glyph: &'static str,
        on: bool,
    ) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let state = Rc::new(Cell::new(on));
        let read = state.clone();
        let control = StyledContainer::new(
            LayoutStyle::new().flex_row(),
            |_| RectStyle::default(),
            vec![box_item(words(glyph)?)],
        )?
        .on_press(move || state.set(!state.get()))
        .control(role)
        .toggled(move || read.get())
        .a11y_label(move || name);
        Ok(box_item(control))
    }

    /// What the catalogue's `button` is to a reader: a box around its words that is a button and presses.
    fn button(label: &'static str) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let control = StyledContainer::new(
            LayoutStyle::new().padding_all(8.0),
            |_| RectStyle::default(),
            vec![box_item(words(label)?)],
        )?
        .control(platform_core::Role::Button)
        .on_press(|| {});
        Ok(box_item(control))
    }

    /// What the catalogue's `text_field` is to a reader: a caption over a framed `Input` that carries the caption as its name.
    fn text_field(
        caption: &'static str,
        placeholder: &'static str,
    ) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let input = Input::new(signal(String::new()), LayoutStyle::new(), ink)?
            .placeholder(placeholder)
            .a11y_label(move || caption);
        let line = input.focus_id();
        let frame = StyledContainer::new(
            LayoutStyle::new().padding_all(8.0),
            |_| RectStyle::default(),
            vec![box_item(input)],
        )?
        .frames_focus_of(line);
        let column = Container::new(
            LayoutStyle::new().flex_column().gap(4.0),
            vec![box_item(words(caption)?), box_item(frame)],
        )?;
        Ok(box_item(column))
    }

    /// What the catalogue's `checkbox` is to a reader: a row of a box and its words that is one checkbox, flipped by its own press.
    fn checkbox(label: &'static str) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let checked = Rc::new(Cell::new(false));
        let read = checked.clone();
        let mark = StyledContainer::new(
            LayoutStyle::new().width(18.0).height(18.0),
            |_| RectStyle::default(),
            vec![],
        )?;
        let row = StyledContainer::new(
            LayoutStyle::new().flex_row().gap(10.0),
            |_| RectStyle::default(),
            vec![box_item(mark), box_item(words(label)?)],
        )?
        .control(platform_core::Role::CheckBox)
        .toggled(move || read.get())
        .on_press(move || checked.set(!checked.get()));
        Ok(box_item(row))
    }

    /// The same screen the document's axe audit runs over (`plugins/telar-components/src/audit_test.rs`), with `ui-core` primitives standing in for the catalogue's button, field and checkbox, so the two targets are held to one fixture.
    fn screen() -> Result<Box<dyn LayoutItem>, LayoutError> {
        let heading = StyledContainer::new(
            LayoutStyle::new(),
            |_| RectStyle::default(),
            vec![box_item(words("Settings")?)],
        )?
        .role(platform_core::Role::Heading(1));
        let save = button("Save")?;
        let source = link("Source code", || external("https://example.com/telar"))?;
        let about = link("About", || Location::root().segment("about"))?;
        let name = text_field("Name", "Ada Lovelace")?;
        let subscribe = checkbox("Send me the newsletter")?;
        let calm = stateful_control(platform_core::Role::Switch, "Reduce motion", "≈", true)?;
        let bold = stateful_control(platform_core::Role::Button, "Bold", "B", false)?;
        let paragraph = Text::runs(
            vec![
                TextRun::new(|| "Read ".to_string()),
                TextRun::new(|| "the guide".to_string()).to(|| Location::root().segment("guide")),
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
        let hidden =
            Container::new(LayoutStyle::new(), vec![box_item(words("Decoration")?)])?.a11y_hidden();
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
                box_item(hidden),
            ],
        )?;
        Ok(box_item(column))
    }

    fn mount() -> ComponentList {
        renderer_core::set_default_text_metrics(renderer_text::ShaperMetrics);
        ui_core::reset_layout_runtime();
        focus::clear();
        ui_core::set_surface_size(Size::new(WIDTH as f32, HEIGHT as f32));
        let mut tree = ComponentList::new(WindowRoot::new(screen().expect("the screen builds")));
        tree.on_event(&Event::WindowResized {
            width: WIDTH,
            height: HEIGHT,
        });
        tree
    }

    fn published(tree: &ComponentList) -> TreeUpdate {
        ui_core::relayout_if_dirty();
        let nodes = ui_core::accessibility::snapshot(&tree.commands());
        tree_update(&nodes, "Settings", Some("en"))
    }

    fn named<'a>(update: &'a TreeUpdate, name: &str) -> &'a Node {
        update
            .nodes
            .iter()
            .map(|(_, node)| node)
            .find(|node| node.label() == Some(name))
            .unwrap_or_else(|| panic!("{name:?} is in the tree: {:#?}", labels(update)))
    }

    /// The control called `name`, where a caption beside it may carry the same words.
    fn control<'a>(update: &'a TreeUpdate, name: &str) -> &'a Node {
        update
            .nodes
            .iter()
            .map(|(_, node)| node)
            .find(|node| node.label() == Some(name) && focusable(node))
            .unwrap_or_else(|| {
                panic!(
                    "a control called {name:?} is in the tree: {:#?}",
                    labels(update)
                )
            })
    }

    fn labels(update: &TreeUpdate) -> Vec<(AkRole, String)> {
        update
            .nodes
            .iter()
            .map(|(_, node)| (node.role(), node.label().unwrap_or_default().to_string()))
            .collect()
    }

    fn focusable(node: &Node) -> bool {
        node.supports_action(Action::Focus) && node.supports_action(Action::Click)
    }

    #[test]
    fn each_control_is_its_role_and_its_name_and_can_be_focused() {
        let tree = mount();
        let update = published(&tree);
        for (name, role) in [
            ("Save", AkRole::Button),
            ("Source code", AkRole::Link),
            ("About", AkRole::Link),
            ("Name", AkRole::TextInput),
            ("Send me the newsletter", AkRole::CheckBox),
            ("Reduce motion", AkRole::Switch),
            ("Bold", AkRole::Button),
        ] {
            assert_eq!(control(&update, name).role(), role, "{name}");
        }
        assert_eq!(
            control(&update, "Send me the newsletter").toggled(),
            Some(Toggled::False)
        );
    }

    fn key(named: NamedKey) -> Event {
        Event::KeyPressed {
            key: Key::Named(named),
            modifiers: ModifiersState::default(),
            unmodified: None,
        }
    }

    fn focus_on(tree: &ComponentList, name: &str) {
        let node = ui_core::accessibility::snapshot(&tree.commands())
            .into_iter()
            .find(|node| node.name == name && node.id.is_some())
            .unwrap_or_else(|| panic!("{name:?} is a control"));
        focus::request(node.id.expect("a control has an id"));
    }

    /// A switch and a toggle button written as a box with a role: each says its state, and Space or Enter on it presses it the way a tap does, which the next tree reports.
    #[test]
    fn a_box_with_a_role_and_a_state_is_pressed_from_the_keyboard() {
        let mut tree = mount();
        let update = published(&tree);
        assert_eq!(
            control(&update, "Reduce motion").toggled(),
            Some(Toggled::True)
        );
        assert_eq!(control(&update, "Bold").toggled(), Some(Toggled::False));

        focus_on(&tree, "Reduce motion");
        tree.on_event(&key(NamedKey::Space));
        assert_eq!(
            control(&published(&tree), "Reduce motion").toggled(),
            Some(Toggled::False)
        );

        focus_on(&tree, "Bold");
        tree.on_event(&key(NamedKey::Enter));
        assert_eq!(
            control(&published(&tree), "Bold").toggled(),
            Some(Toggled::True),
            "a toggle button is pressed by Enter and stays pressed"
        );
        focus::clear();
    }

    /// What `box to:anchor("…") current:(…)` builds in `.rsx` for a bar over the page: a link current while its section is the place under the bar.
    fn section_link(name: &'static str, section: &'static str) -> StyledContainer {
        StyledContainer::new(
            LayoutStyle::new().padding_horizontal(4.0),
            |_| RectStyle::default(),
            vec![box_item(words(name).expect("a link's words"))],
        )
        .expect("a link builds")
        .to(move || anchor(section))
        .current(move || use_anchor_at(BAR).as_deref() == Some(section))
    }

    const BAR: f32 = 48.0;

    /// A bar fixed over a page of two sections, each taller than the window, with a link to each; `viewport` is the page's scroll.
    fn sections() -> (ComponentList, ScrollViewport) {
        renderer_core::set_default_text_metrics(renderer_text::ShaperMetrics);
        ui_core::reset_layout_runtime();
        focus::clear();
        ui_core::set_surface_size(Size::new(WIDTH as f32, HEIGHT as f32));
        let bar = Container::new(
            LayoutStyle::new().flex_row().gap(8.0).height(BAR),
            vec![
                box_item(section_link("Intro", "intro")),
                box_item(section_link("Usage", "usage")),
            ],
        )
        .expect("the bar builds");
        let layer = FixedLayer::new(LayoutStyle::new().flex_column(), vec![box_item(bar)])
            .expect("a layer");
        let section = |name: &'static str| {
            box_item(
                Container::new(LayoutStyle::new().height(HEIGHT as f32 * 1.5), vec![])
                    .expect("a section builds")
                    .page_anchor(move || name),
            )
        };
        let content = Container::column(vec![box_item(layer), section("intro"), section("usage")])
            .expect("the content builds");
        let mut page = ScrollPage::new(Box::new(content)).expect("the page builds");
        page.relayout(WIDTH as f32, HEIGHT as f32);
        let viewport = page.viewport();
        let mut tree = ComponentList::new(page);
        tree.on_event(&Event::WindowResized {
            width: WIDTH,
            height: HEIGHT,
        });
        (tree, viewport)
    }

    /// The link to the section under the bar is the current location, and only that one; scrolling the next section under the bar moves the mark to its link.
    #[test]
    fn the_link_to_the_section_under_a_bar_is_the_current_location() {
        let (tree, viewport) = sections();
        let update = published(&tree);
        assert_eq!(
            control(&update, "Intro").aria_current(),
            Some(AriaCurrent::Location)
        );
        assert_eq!(control(&update, "Usage").aria_current(), None);

        viewport.scroll_to(0.0, HEIGHT as f32 * 1.5 - BAR);
        let update = published(&tree);
        assert_eq!(control(&update, "Intro").aria_current(), None);
        assert_eq!(
            control(&update, "Usage").aria_current(),
            Some(AriaCurrent::Location)
        );
        assert_eq!(control(&update, "Usage").url(), Some("/#usage"));
    }

    #[test]
    fn a_link_is_announced_with_where_it_goes_and_a_link_run_is_one_too() {
        let tree = mount();
        let update = published(&tree);
        assert_eq!(
            control(&update, "Source code").url(),
            Some("https://example.com/telar")
        );
        assert_eq!(control(&update, "About").url(), Some("/about"));
        let run = named(&update, "the guide");
        assert_eq!(run.role(), AkRole::Link);
        assert_eq!(run.url(), Some("/guide"));
        assert!(
            !focusable(run),
            "a link run is announced, but Tab reaches it only in a document"
        );
    }

    #[test]
    fn a_name_a_language_and_a_hidden_box_reach_the_reader_as_written() {
        let tree = mount();
        let update = published(&tree);
        assert_eq!(named(&update, "Telar").role(), AkRole::Label);
        assert_eq!(
            named(&update, "Bonjour tout le monde").language(),
            Some("fr")
        );
        let read: Vec<String> = labels(&update).into_iter().map(|(_, name)| name).collect();
        for gone in ["T", "e", "Decoration"] {
            assert!(
                !read.iter().any(|name| name == gone),
                "{gone:?} is hidden: {read:?}"
            );
        }
        let (_, root) = update.nodes.last().expect("the window");
        assert_eq!(root.language(), Some("en"));
    }

    /// Tab walks the controls in the order they were built, and the tree's focus follows each step, so a reader announces the control the keyboard is on.
    #[test]
    fn the_tree_s_focus_follows_the_tab_order() {
        let tree = mount();
        assert_eq!(
            published(&tree).focus,
            ROOT,
            "nothing focused is the window"
        );
        let mut walked = Vec::new();
        for _ in 0..8 {
            focus::focus_next();
            let update = published(&tree);
            let (_, node) = update
                .nodes
                .iter()
                .find(|(id, _)| *id == update.focus)
                .expect("the focus names a node in the tree");
            walked.push(node.label().unwrap_or_default().to_string());
        }
        assert_eq!(
            walked,
            [
                "Save",
                "Source code",
                "About",
                "Name",
                "Send me the newsletter",
                "Reduce motion",
                "Bold",
                "Save"
            ]
        );
    }
}

#[test]
fn the_workbench_roles_are_published_as_the_roles_a_reader_knows_them_by() {
    for (role, expected) in [
        (Role::Tree, AkRole::Tree),
        (Role::TreeItem, AkRole::TreeItem),
        (Role::TabList, AkRole::TabList),
        (Role::Toolbar, AkRole::Toolbar),
        (Role::Splitter, AkRole::Splitter),
        (Role::Status, AkRole::Status),
        (Role::Log, AkRole::Log),
        (Role::Menu, AkRole::Menu),
        (Role::MenuItem, AkRole::MenuItem),
        (Role::MenuItemCheckBox, AkRole::MenuItemCheckBox),
        (Role::MenuItemRadio, AkRole::MenuItemRadio),
        (Role::ListBox, AkRole::ListBox),
        (Role::Option, AkRole::ListBoxOption),
    ] {
        let update = tree_update(&[node(Some(1), role, "Item")], "Workbench", None);
        assert_eq!(update.nodes[0].1.role(), expected, "{role:?}");
    }
}

#[test]
fn a_splitter_publishes_its_value_and_its_orientation() {
    let splitter = AccessNode {
        value: Some(platform_core::NumericValue {
            now: 240.0,
            min: 180.0,
            max: 400.0,
        }),
        orientation: Some(Orientation::Vertical),
        ..node(Some(1), Role::Splitter, "Sidebar")
    };
    let published = tree_update(&[splitter], "Splitter", None).nodes[0]
        .1
        .clone();
    assert_eq!(published.role(), AkRole::Splitter);
    assert_eq!(published.numeric_value(), Some(240.0));
    assert_eq!(published.min_numeric_value(), Some(180.0));
    assert_eq!(published.max_numeric_value(), Some(400.0));
    assert_eq!(
        published.orientation(),
        Some(accesskit::Orientation::Vertical)
    );

    let bar = AccessNode {
        orientation: Some(Orientation::Horizontal),
        ..node(Some(2), Role::Splitter, "Panels")
    };
    let published = tree_update(&[bar], "Splitter", None).nodes[0].1.clone();
    assert_eq!(
        published.orientation(),
        Some(accesskit::Orientation::Horizontal)
    );
}

/// A control that runs the same command as a key publishes it as the node's keyboard shortcut; one with no such key publishes none.
#[test]
fn a_control_publishes_the_keys_that_run_its_command() {
    let nodes = [
        AccessNode {
            keyboard_shortcut: Some("Control+Z".to_string()),
            ..node(Some(1), Role::MenuItem, "Undo")
        },
        node(Some(2), Role::MenuItem, "Close"),
    ];
    let update = tree_update(&nodes, "Edit", None);
    assert_eq!(update.nodes[0].1.keyboard_shortcut(), Some("Control+Z"));
    assert_eq!(update.nodes[1].1.keyboard_shortcut(), None);
}

/// A data table reaches a reader as a grid of rows and cells, its chosen row selected, and the header of the column it is sorted by says which way.
#[test]
fn a_grid_says_its_rows_its_cells_and_how_it_is_sorted() {
    let header = AccessNode {
        sort: Some(SortDirection::Descending),
        ..node(Some(2), Role::ColumnHeader, "Name")
    };
    let unsorted = node(Some(3), Role::ColumnHeader, "Size");
    let row = AccessNode {
        toggled: Some(true),
        ..node(Some(4), Role::Row, "")
    };
    let nodes = [
        node(Some(1), Role::Grid, "Files"),
        header,
        unsorted,
        row,
        node(Some(5), Role::GridCell, "notes.txt"),
    ];
    let update = tree_update(&nodes, "Explorer", None);
    let roles: Vec<AkRole> = update.nodes[..5].iter().map(|(_, ak)| ak.role()).collect();
    assert_eq!(
        roles,
        [
            AkRole::Grid,
            AkRole::ColumnHeader,
            AkRole::ColumnHeader,
            AkRole::Row,
            AkRole::GridCell
        ]
    );
    assert_eq!(
        update.nodes[1].1.sort_direction(),
        Some(AkSortDirection::Descending)
    );
    assert_eq!(update.nodes[2].1.sort_direction(), None);
    assert_eq!(update.nodes[3].1.is_selected(), Some(true));
}

/// A list box that lets several of its options be chosen says so, and one that lets one be chosen does not.
#[test]
fn a_list_of_several_choices_publishes_that_it_is_multiselectable() {
    let nodes = [
        AccessNode {
            multiselectable: true,
            ..node(Some(1), Role::ListBox, "Recipients")
        },
        node(Some(2), Role::ListBox, "Sender"),
    ];
    let update = tree_update(&nodes, "Mail", None);
    assert!(update.nodes[0].1.is_multiselectable());
    assert!(!update.nodes[1].1.is_multiselectable());
}

/// An option or a menu's row says its place and no level, a grid's row its index among the grid's rows, and a role that is no item of a set neither.
#[test]
fn an_item_publishes_only_the_numbers_its_role_takes() {
    let placed = |role: Role| {
        let item = AccessNode {
            position: Some(platform_core::SetPosition {
                level: 1,
                position: 3,
                size: 40,
            }),
            ..node(Some(4), role, "Item")
        };
        let update = tree_update(&[item], "Window", None);
        let ak = &update.nodes[0].1;
        (ak.level(), ak.position_in_set(), ak.row_index())
    };
    assert_eq!(placed(Role::TreeItem), (Some(0), Some(2), None));
    assert_eq!(placed(Role::Option), (None, Some(2), None));
    assert_eq!(placed(Role::MenuItem), (None, Some(2), None));
    assert_eq!(placed(Role::Row), (None, None, Some(2)));
    assert_eq!(placed(Role::Button), (None, None, None));
}

/// A dialog inside the modal layer that is up says so, which is what keeps a reader inside it; any other dialog does not.
#[test]
fn a_dialog_in_a_modal_layer_is_published_modal() {
    let dialog = |modal: bool| {
        let node = AccessNode {
            modal,
            ..node(None, Role::Dialog, "Terms")
        };
        tree_update(&[node], "Window", None).nodes[0].1.is_modal()
    };
    assert!(dialog(true));
    assert!(!dialog(false));
}

/// A grid's header row is no control and has no name, and still reaches a reader as a row with its index, ahead of the headers it holds and with nothing to focus or press.
#[test]
fn a_header_row_with_no_name_is_a_row_with_its_index() {
    let header_row = AccessNode {
        position: Some(platform_core::SetPosition {
            level: 1,
            position: 1,
            size: 501,
        }),
        ..node(None, Role::Row, "")
    };
    let nodes = [
        node(Some(1), Role::Grid, "Files"),
        header_row,
        node(Some(2), Role::ColumnHeader, "Name"),
    ];
    let update = tree_update(&nodes, "Explorer", None);
    let row = &update.nodes[1].1;
    assert_eq!(row.role(), AkRole::Row);
    assert_eq!(row.row_index(), Some(0));
    assert!(!row.supports_action(Action::Focus) && !row.supports_action(Action::Click));
    assert_eq!(update.nodes[2].1.role(), AkRole::ColumnHeader);
}

fn inside(parent: usize, node: AccessNode) -> AccessNode {
    AccessNode {
        parent: Some(parent),
        ..node
    }
}

fn row_at(id: u64, position: u32, size: u32) -> AccessNode {
    AccessNode {
        position: Some(platform_core::SetPosition {
            level: 1,
            position,
            size,
        }),
        ..node(Some(id), Role::Row, "")
    }
}

fn update_of(update: &TreeUpdate, id: NodeId) -> &Node {
    &update
        .nodes
        .iter()
        .find(|(at, _)| *at == id)
        .unwrap_or_else(|| panic!("{id:?} is in the update"))
        .1
}

/// The window holds only the nodes nothing else holds, and every other node is a child of the one it sits in, in the order the snapshot reads them.
#[test]
fn each_node_is_a_child_of_the_one_it_sits_in() {
    let nodes = [
        node(None, Role::Label, "Above"),
        node(Some(1), Role::Grid, "Files"),
        inside(1, row_at(2, 1, 3)),
        inside(2, node(Some(3), Role::ColumnHeader, "Name")),
        inside(2, node(Some(4), Role::ColumnHeader, "Size")),
        inside(1, row_at(5, 2, 3)),
        inside(5, node(Some(6), Role::GridCell, "notes.txt")),
        inside(5, node(Some(7), Role::GridCell, "4 KB")),
    ];
    let update = tree_update(&nodes, "Explorer", None);
    let window = update_of(&update, ROOT);
    assert_eq!(window.children(), [NodeId(1), control_id(1)]);
    assert_eq!(
        update_of(&update, control_id(1)).children(),
        [control_id(2), control_id(5)]
    );
    assert_eq!(
        update_of(&update, control_id(2)).children(),
        [control_id(3), control_id(4)]
    );
    assert_eq!(
        update_of(&update, control_id(5)).children(),
        [control_id(6), control_id(7)]
    );
    assert!(update_of(&update, control_id(6)).children().is_empty());
}

/// A grid says how many rows it has from what its rows say, though only some are built, and how many columns from its widest row; each cell says its column and its row's place.
#[test]
fn a_grid_says_its_counts_and_each_cell_its_place() {
    let nodes = [
        node(Some(1), Role::Grid, "Files"),
        inside(0, row_at(2, 1, 501)),
        inside(1, node(Some(3), Role::ColumnHeader, "Name")),
        inside(1, node(Some(4), Role::ColumnHeader, "Size")),
        inside(0, row_at(5, 40, 501)),
        inside(4, node(Some(6), Role::GridCell, "notes.txt")),
        inside(4, node(Some(7), Role::GridCell, "4 KB")),
    ];
    let update = tree_update(&nodes, "Explorer", None);
    let grid = update_of(&update, control_id(1));
    assert_eq!(grid.row_count(), Some(501));
    assert_eq!(grid.column_count(), Some(2));
    assert_eq!(update_of(&update, control_id(5)).row_index(), Some(39));
    let size = update_of(&update, control_id(7));
    assert_eq!(size.column_index(), Some(1));
    assert_eq!(size.row_index(), Some(39));
    assert_eq!(update_of(&update, control_id(3)).column_index(), Some(0));
    assert_eq!(update_of(&update, control_id(3)).row_index(), Some(0));
}

/// A grid whose rows say no place counts the rows it holds, and each row's place is its place among them.
#[test]
fn a_grid_of_rows_that_say_no_place_counts_them() {
    let nodes = [
        node(Some(1), Role::Grid, "Scores"),
        inside(0, node(Some(2), Role::Row, "")),
        inside(1, node(Some(3), Role::GridCell, "Ada")),
        inside(0, node(Some(4), Role::Row, "")),
        inside(3, node(Some(5), Role::GridCell, "Alan")),
    ];
    let update = tree_update(&nodes, "Scores", None);
    assert_eq!(update_of(&update, control_id(1)).row_count(), Some(2));
    assert_eq!(update_of(&update, control_id(4)).row_index(), Some(1));
    assert_eq!(update_of(&update, control_id(5)).row_index(), Some(1));
}

/// A list box says how many options it has from what its options say, which a virtualised list builds only some of; a tree whose rows sit at different levels in sets of different sizes leaves each row's set to its level.
#[test]
fn a_set_says_its_size_where_its_items_agree_on_it() {
    let item = |id: u64, role: Role, level: u32, position: u32, size: u32| AccessNode {
        position: Some(platform_core::SetPosition {
            level,
            position,
            size,
        }),
        ..node(Some(id), role, "Item")
    };
    let nodes = [
        node(Some(1), Role::ListBox, "Fruit"),
        inside(0, item(2, Role::Option, 1, 7, 300)),
        inside(0, item(3, Role::Option, 1, 8, 300)),
        node(Some(4), Role::Tree, "Files"),
        inside(3, item(5, Role::TreeItem, 1, 1, 2)),
        inside(3, item(6, Role::TreeItem, 2, 1, 5)),
    ];
    let update = tree_update(&nodes, "Window", None);
    assert_eq!(update_of(&update, control_id(1)).size_of_set(), Some(300));
    assert_eq!(update_of(&update, control_id(4)).size_of_set(), None);
    assert_eq!(update_of(&update, control_id(3)).position_in_set(), Some(7));
}

/// A node grafted from a nested tree keeps the focus id it is addressed by, wherever it now sits, and its active descendant still names it.
#[test]
fn a_nested_node_keeps_its_id_and_its_active_descendant() {
    let nodes = [
        AccessNode {
            focused: true,
            active_descendant: Some(3),
            ..node(Some(1), Role::Tree, "Files")
        },
        inside(0, node(Some(3), Role::TreeItem, "src")),
    ];
    let update = tree_update(&nodes, "Window", None);
    assert_eq!(update.focus, control_id(1));
    let tree = update_of(&update, control_id(1));
    assert_eq!(tree.active_descendant(), Some(control_id(3)));
    assert_eq!(tree.children(), [control_id(3)]);
}
