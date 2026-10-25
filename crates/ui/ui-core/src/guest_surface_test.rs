use geometry_core::Rect;
use platform_core::guest::{ReportsSurface, guest_entry};
use platform_core::{Key, ModifiersState, NamedKey, Role};

use super::*;

/// A guest's instance with one field, which takes focus when it arrives and lets it go on Tab, and panics when it is read while `panics` says so.
#[derive(Default)]
pub struct Field {
    focused: bool,
    actions: Vec<(u64, bool)>,
    panics: bool,
}

impl ReportsSurface for Field {
    fn access_snapshot(&self) -> Vec<AccessNode> {
        assert!(!self.panics, "the guest's snapshot panicked");
        vec![AccessNode {
            id: Some(3),
            parent: None,
            role: Role::TextInput,
            name: "Name".to_owned(),
            rect: Rect::new(0.0, 0.0, 80.0, 20.0),
            focused: self.focused,
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
        }]
    }

    fn perform_access_action(&mut self, id: u64, activate: bool) {
        self.actions.push((id, activate));
    }

    fn text_input(&self) -> Option<TextInput> {
        self.focused.then_some(TextInput {
            field: 3,
            caret: Rect::new(4.0, 2.0, 1.0, 16.0),
            secret: false,
        })
    }

    fn arrive(&mut self, _backwards: bool) -> bool {
        self.focused = true;
        true
    }

    fn tab(&mut self, _key: &Event) -> bool {
        self.focused = false;
        true
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

    fn keeps(&self, _keys: platform_core::ConsumedKeys) -> bool {
        self.focused
    }
}

mod guest {
    use platform_core::guest::{CallOutcome, HostServices};

    pub(super) extern "C" fn install_services(_: HostServices) -> CallOutcome {
        CallOutcome::OK
    }
}

platform_core::entry_table! {
    /// A guest library whose instances are [`Field`]s.
    pub struct Fields {
        kind: Embed,
        symbol: "telar_test_fields",
        guest: guest,
        settings: [],
        create: fn(*mut Handle<Field>) as CreateField;
        report: Field;
    }
}

unsafe extern "C" fn create_field(field: *mut Handle<Field>) -> CallOutcome {
    guest_entry(|| unsafe { field.write(Handle::new(Field::default())) })
}

/// A guest library this process serves itself, its one instance and the surface the host reads it as.
fn loaded() -> (
    Rc<GuestLibrary<Fields>>,
    Handle<Field>,
    GuestSurface<Fields>,
) {
    let library = Rc::new(GuestLibrary::start(
        Fields::serving(create_field),
        None,
        None,
    ));
    let field = library
        .answer(Handle::NULL, |entries, field| unsafe {
            (entries.create)(field)
        })
        .expect("the instance builds");
    let surface = GuestSurface::new(Rc::clone(&library), field);
    (library, field, surface)
}

fn tab() -> Event {
    Event::KeyPressed {
        key: Key::Named(NamedKey::Tab),
        modifiers: ModifiersState::default(),
        unmodified: None,
    }
}

#[test]
fn the_host_reads_what_the_instance_reports_of_its_surface() {
    let (_library, field, surface) = loaded();
    assert!(!surface.holds_focus());
    assert_eq!(surface.text_input(), None);

    assert!(surface.arrive(false), "the field takes focus as it arrives");
    assert!(surface.holds_focus());
    let nodes = surface.access_snapshot();
    assert_eq!(nodes.len(), 1);
    assert!(nodes[0].focused);
    assert_eq!(
        surface.text_input().map(|input| input.caret),
        Some(Rect::new(4.0, 2.0, 1.0, 16.0))
    );

    surface.perform_access_action(3, true);
    assert!(surface.tab(&tab()), "Tab past its one field leaves it");
    assert!(!surface.holds_focus());
    surface.arrive(true);
    surface.blur();
    assert!(!surface.holds_focus());
    assert_eq!(unsafe { field.get() }.actions, [(3, true)]);
    unsafe { field.release() };
}

/// Its owner releases the instance right after: nothing reads through the handle once it is forgotten.
#[test]
fn a_forgotten_instance_reports_nothing_and_takes_nothing() {
    let (_library, field, surface) = loaded();
    surface.forget();
    assert!(!surface.arrive(false));
    surface.perform_access_action(3, true);
    assert!(surface.access_snapshot().is_empty());
    assert_eq!(surface.text_input(), None);
    let field = unsafe { field.get() };
    assert!(
        !field.focused && field.actions.is_empty(),
        "nothing reached it"
    );
}

#[test]
fn once_the_guest_panics_its_surface_reports_nothing_and_holds_no_focus() {
    let (library, field, surface) = loaded();
    surface.arrive(false);
    unsafe { field.get() }.panics = true;

    assert!(surface.access_snapshot().is_empty());
    assert!(library.guest().has_panicked(), "precondition: it panicked");
    assert_eq!(surface.text_input(), None);
    assert!(!surface.holds_focus());
    assert!(
        unsafe { field.get() }.focused,
        "the guest is called no more"
    );
}

fn audit_of(
    _: &GuestLibrary<Fields>,
    _: Handle<Field>,
    background: Option<Color>,
) -> Option<AccessAudit> {
    (background == Some(Color::WHITE)).then_some(AccessAudit {
        nodes: Vec::new(),
        violations: Vec::new(),
    })
}

#[test]
fn an_instance_checks_itself_only_where_its_boundary_has_an_entry_for_it() {
    let (_library, _field, surface) = loaded();
    assert!(surface.access_audit(Some(Color::WHITE)).is_none());
    let surface = surface.audited_by(audit_of);
    assert!(surface.access_audit(Some(Color::WHITE)).is_some());
    surface.forget();
    assert!(surface.access_audit(Some(Color::WHITE)).is_none());
}
