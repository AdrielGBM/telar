use geometry_core::Rect;

use super::*;
use crate::{ConsumedKeys, Key, ModifiersState, NamedKey, Role};

fn field_node(focused: bool) -> AccessNode {
    AccessNode {
        id: Some(7),
        parent: None,
        role: Role::Button,
        name: "Field".to_owned(),
        rect: Rect::new(1.0, 2.0, 30.0, 10.0),
        focused,
        enabled: true,
        toggled: None,
        value: None,
        orientation: None,
        expanded: None,
        position: None,
        lang: None,
        url: None,
        current: None,
        active_descendant: None,
        controls: None,
        has_popup: None,
        keyboard_shortcut: None,
        sort: None,
        multiselectable: false,
        modal: false,
    }
}

/// A surface with one control, which holds focus once it arrives and lets it go on the second Tab.
#[derive(Default)]
struct Field {
    focused: bool,
    tabs: u32,
    actions: Vec<(u64, bool)>,
}

impl ReportsSurface for Field {
    fn access_snapshot(&self) -> Vec<AccessNode> {
        vec![field_node(self.focused)]
    }

    fn perform_access_action(&mut self, id: u64, activate: bool) {
        self.actions.push((id, activate));
    }

    fn text_input(&self) -> Option<TextInput> {
        self.focused.then_some(TextInput {
            field: 7,
            caret: Rect::new(3.0, 2.0, 1.0, 10.0),
            secret: false,
        })
    }

    fn arrive(&mut self, _backwards: bool) -> bool {
        self.focused = true;
        true
    }

    fn tab(&mut self, key: &Event) -> bool {
        assert!(matches!(
            key,
            Event::KeyPressed {
                key: Key::Named(NamedKey::Tab),
                ..
            }
        ));
        self.tabs += 1;
        self.focused = self.tabs < 2;
        !self.focused
    }

    fn blur(&mut self) {
        self.focused = false;
    }

    fn holds_focus(&self) -> bool {
        self.focused
    }

    fn text_entry_focused(&self) -> bool {
        self.focused
    }

    fn keeps(&self, keys: ConsumedKeys) -> bool {
        self.focused && ConsumedKeys::ENTER.contains(keys)
    }
}

const SERVED: SurfaceReport<Field> = SurfaceReport::serving();

fn tab() -> Vec<u8> {
    wire_core::encode(&Event::KeyPressed {
        key: Key::Named(NamedKey::Tab),
        modifiers: ModifiersState::default(),
        unmodified: None,
    })
}

fn asks(entry: impl FnOnce(*mut bool) -> CallOutcome) -> bool {
    let mut answer = false;
    assert_eq!(entry(&mut answer).status(), CallOutcome::RETURNED);
    answer
}

fn reads<T: wire_core::Wire>(entry: impl FnOnce(*mut LentBytes) -> CallOutcome) -> T {
    let mut lent = LentBytes::EMPTY;
    assert_eq!(entry(&mut lent).status(), CallOutcome::RETURNED);
    unsafe { lent.decode() }.expect("the answer reads back")
}

#[test]
fn every_answer_crosses_as_what_the_surface_said() {
    let surface = Handle::new(Field::default());
    unsafe {
        assert!(!asks(|holds| (SERVED.holds_focus)(surface, holds)));
        assert_eq!(
            reads::<Option<TextInput>>(|input| (SERVED.text_input)(surface, input)),
            None
        );
        assert!(asks(|took| (SERVED.arrive)(surface, false, took)));
        assert!(asks(|holds| (SERVED.holds_focus)(surface, holds)));
        assert!(asks(|typing| (SERVED.text_entry_focused)(surface, typing)));
        let enter = ConsumedKeys::ENTER.bits();
        assert!(asks(|kept| (SERVED.keeps)(surface, enter, kept)));
        let tab_key = ConsumedKeys::TAB.bits();
        assert!(!asks(|kept| (SERVED.keeps)(surface, tab_key, kept)));
        let nodes: Vec<AccessNode> = reads(|nodes| (SERVED.access_snapshot)(surface, nodes));
        assert_eq!(nodes.len(), 1);
        assert!(nodes[0].focused);
        assert_eq!(
            reads::<Option<TextInput>>(|input| (SERVED.text_input)(surface, input))
                .map(|input| input.caret),
            Some(Rect::new(3.0, 2.0, 1.0, 10.0))
        );

        let key = tab();
        assert!(!asks(|left| (SERVED.tab)(
            surface,
            LentBytes::lend(&key),
            left
        )));
        assert!(asks(|left| (SERVED.tab)(
            surface,
            LentBytes::lend(&key),
            left
        )));
        assert!(!asks(|holds| (SERVED.holds_focus)(surface, holds)));

        (SERVED.access_action)(surface, 7, true);
        (SERVED.blur)(surface);
        assert_eq!(surface.get().actions, [(7, true)]);
        surface.release();
    }
}

#[test]
fn the_report_is_hashed_entry_by_entry_with_what_each_takes() {
    assert_eq!(
        SURFACE_REPORT_LAYOUT.size,
        9 * std::mem::size_of::<unsafe extern "C" fn() -> CallOutcome>()
    );
    assert_eq!(SURFACE_REPORT_LAYOUT.fields.len(), 9);
    assert!(
        SURFACE_REPORT_LAYOUT
            .fields
            .iter()
            .all(|(field, _)| field.contains(": fn(Handle"))
    );
    assert!(
        SURFACE_REPORT_LAYOUT
            .fields
            .iter()
            .any(|(field, _)| *field == "keeps: fn(Handle, u16, *mut bool, )"),
        "each entry is hashed with the signature it is declared with"
    );
}
