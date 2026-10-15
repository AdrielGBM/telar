use std::cell::RefCell;

use telar::testing::{centre, key_with, lay_out, named, press, release, route};
use telar::{AccessNode, ComponentList, DrawCommand, Event, relayout_if_dirty};

use super::*;

struct Mounted {
    tree: ComponentList,
    value: RwSignal<String>,
    log: Rc<RefCell<Vec<String>>>,
}

impl Mounted {
    fn new(props: SearchFieldPropsBuilder) -> Self {
        crate::test_support::fresh_layout_runtime();
        focus::clear();
        let value = signal(String::new());
        let log = Rc::new(RefCell::new(Vec::new()));
        let (submitted, cleared) = (log.clone(), log.clone());
        let item = search_field(
            props
                .value(value)
                .on_submit(Rc::new(move |query: &str| {
                    submitted.borrow_mut().push(format!("submit {query}"))
                }))
                .on_clear(Rc::new(move || cleared.borrow_mut().push("clear".into())))
                .build(),
            Children::default(),
        )
        .unwrap();
        lay_out(item.layout_node(), 400.0, 200.0);
        let tree = ComponentList::new(item);
        let mut mounted = Self { tree, value, log };
        mounted.settle();
        mounted
    }

    fn settle(&mut self) {
        relayout_if_dirty();
        let _ = self.tree.commands();
        relayout_if_dirty();
        let _ = self.tree.commands();
    }

    fn send(&mut self, event: Event) {
        route(&mut self.tree, &event);
        self.settle();
    }

    fn read(&self) -> Vec<AccessNode> {
        ui_core::accessibility::snapshot(&self.tree.commands())
    }

    fn node(&self, role: Role) -> Option<AccessNode> {
        self.read().into_iter().find(|node| node.role == role)
    }

    fn field(&self) -> AccessNode {
        self.node(Role::TextInput).expect("a text field")
    }

    fn focus_field(&mut self) {
        let at = centre(self.field().rect);
        self.send(press(at.0, at.1));
        self.send(release(at.0, at.1));
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.send(key_with(Key::Char(c), Default::default()));
        }
    }

    fn marks(&self) -> usize {
        self.tree
            .commands()
            .iter()
            .filter(|command| matches!(command, DrawCommand::Path { .. }))
            .count()
    }
}

#[test]
fn an_empty_field_is_called_search_and_shows_only_its_glass() {
    let mounted = Mounted::new(SearchFieldProps::props());
    assert_eq!(mounted.field().name, "Search");
    assert!(telar::testing::find_text(&mounted.tree, "Search"));
    assert_eq!(mounted.marks(), 1, "the magnifying glass");
    assert!(mounted.node(Role::Button).is_none(), "nothing to clear yet");
}

#[test]
fn a_query_brings_a_clear_button_that_empties_the_field_and_keeps_the_keyboard_there() {
    let mut mounted = Mounted::new(SearchFieldProps::props().label("Find files"));
    mounted.focus_field();
    mounted.type_text("main");
    assert_eq!(mounted.value.get(), "main");
    let clear = mounted.node(Role::Button).expect("a clear button");
    assert_eq!(clear.name, "Clear search");
    assert_eq!(mounted.marks(), 2, "the glass and the cross");
    assert!(
        focus::tab_order()
            .iter()
            .all(|stop| Some(stop.id) != clear.id),
        "the button is no Tab stop: Escape does the same"
    );

    let at = centre(clear.rect);
    mounted.send(press(at.0, at.1));
    mounted.send(release(at.0, at.1));
    assert_eq!(mounted.value.get(), "");
    assert_eq!(*mounted.log.borrow(), ["clear"]);
    assert!(mounted.node(Role::Button).is_none());
    assert_eq!(focus::current(), mounted.field().id);
}

#[test]
fn escape_clears_a_query_and_only_then_lets_the_field_go() {
    let mut mounted = Mounted::new(SearchFieldProps::props());
    mounted.focus_field();
    mounted.type_text("kite");
    mounted.send(named(NamedKey::Escape));
    assert_eq!(mounted.value.get(), "");
    assert_eq!(*mounted.log.borrow(), ["clear"]);
    assert_eq!(
        focus::current(),
        mounted.field().id,
        "the first Escape only clears"
    );
    mounted.send(named(NamedKey::Escape));
    assert_ne!(
        focus::current(),
        mounted.field().id,
        "with nothing to clear, Escape leaves the field"
    );
}

#[test]
fn enter_submits_the_query_as_it_stands() {
    let mut mounted = Mounted::new(SearchFieldProps::props());
    mounted.focus_field();
    mounted.type_text("telar");
    mounted.send(named(NamedKey::Enter));
    assert_eq!(*mounted.log.borrow(), ["submit telar"]);
}

#[test]
fn a_list_under_the_field_hears_its_keys_first() {
    let heard = Rc::new(RefCell::new(Vec::new()));
    let sink = heard.clone();
    let mut mounted = Mounted::new(SearchFieldProps::props().on_key(Rc::new(
        move |key: &Key, _modifiers: ModifiersState| {
            sink.borrow_mut().push(format!("{key:?}"));
            matches!(key, Key::Named(NamedKey::ArrowDown | NamedKey::Escape))
        },
    )));
    mounted.focus_field();
    mounted.type_text("a");
    mounted.send(named(NamedKey::ArrowDown));
    mounted.send(named(NamedKey::Escape));
    assert_eq!(heard.borrow().len(), 3);
    assert_eq!(
        mounted.value.get(),
        "a",
        "the list took Escape, so nothing was cleared"
    );
}

#[test]
fn the_focus_id_is_published_and_requesting_it_focuses_the_line() {
    let published = signal(None);
    let _mounted = Mounted::new(SearchFieldProps::props().focus_id(published));
    let id = published.get().expect("the field publishes its id");
    focus::request(id);
    assert!(focus::text_entry_focused());
}
