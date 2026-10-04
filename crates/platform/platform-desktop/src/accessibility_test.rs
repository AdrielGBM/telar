use geometry_core::Rect;

use super::*;

fn node(id: Option<u64>, role: Role, name: &str) -> AccessNode {
    AccessNode {
        id,
        role,
        name: name.to_string(),
        rect: Rect::new(0.0, 0.0, 10.0, 10.0),
        focused: false,
        enabled: true,
        toggled: None,
        value: None,
        lang: None,
        url: None,
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

    let slider = published(toggled(Role::Slider, true));
    assert_eq!(
        slider.toggled(),
        None,
        "a slider has no on/off state to say"
    );
    assert_eq!(slider.is_selected(), None);
}

mod from_a_screen {
    use std::cell::Cell;
    use std::rc::Rc;

    use accesskit::{Action, Node, Role as AkRole, Toggled};
    use geometry_core::Size;
    use layout_core::{LayoutError, LayoutStyle};
    use platform_core::{Event, Key, Location, ModifiersState, NamedKey, external};
    use renderer_core::{Color, RectStyle, TextStyle};
    use ui_components::{ButtonProps, CheckboxProps, TextFieldProps, button, checkbox, text_field};
    use ui_core::{
        Accessible, Children, ComponentList, Container, LayoutItem, StyledContainer, Text, TextRun,
        WindowRoot, box_item, focus,
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

    /// The same screen the document's axe audit runs over (`renderer-dom/src/audit_test.rs`), so the two targets are held to one fixture.
    fn screen() -> Result<Box<dyn LayoutItem>, LayoutError> {
        let heading = StyledContainer::new(
            LayoutStyle::new(),
            |_| RectStyle::default(),
            vec![box_item(words("Settings")?)],
        )?
        .role(platform_core::Role::Heading(1));
        let save = button(
            ButtonProps::props()
                .label("Save")
                .on_press(Rc::new(|| {}))
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
