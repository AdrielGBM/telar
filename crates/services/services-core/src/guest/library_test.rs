use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use platform_core::guest::{Handle, Lifecycle, ReportsSurface, guest_entry};
use renderer_core::{ExclusiveFontsError, FontAsset, FontConfig, FontSources};

use super::*;

thread_local! {
    static SERVED: Cell<Option<HostServices>> = const { Cell::new(None) };
    static LIFECYCLE_CALLS: Cell<u32> = const { Cell::new(0) };
}

mod guest {
    use super::*;

    pub(super) extern "C" fn install_services(services: HostServices) -> CallOutcome {
        SERVED.set(Some(services));
        CallOutcome::OK
    }
}

/// A surface with nothing on it.
pub struct Blank;

impl ReportsSurface for Blank {
    fn access_snapshot(&self) -> Vec<platform_core::AccessNode> {
        Vec::new()
    }

    fn perform_access_action(&mut self, _id: u64, _activate: bool) {}

    fn text_input(&self) -> Option<platform_core::TextInput> {
        None
    }

    fn arrive(&mut self, _backwards: bool) -> bool {
        false
    }

    fn tab(&mut self, _key: &platform_core::Event) -> bool {
        false
    }

    fn blur(&mut self) {}

    fn holds_focus(&self) -> bool {
        false
    }

    fn text_entry_focused(&self) -> bool {
        false
    }

    fn keeps(&self, _: platform_core::ConsumedKeys) -> bool {
        false
    }
}

platform_core::entry_table! {
    /// A guest with nothing to it but its lifecycle and the report of a blank surface.
    pub struct Bare {
        kind: Embed,
        symbol: "telar_bare_entries",
        guest: guest,
        settings: [],
        create: fn(*mut Handle<()>) as CreateBare;
        report: Blank;
    }
}

extern "C" fn create_nothing(_: *mut Handle<()>) -> CallOutcome {
    CallOutcome::OK
}

extern "C" fn returns() -> CallOutcome {
    LIFECYCLE_CALLS.set(LIFECYCLE_CALLS.get() + 1);
    CallOutcome::OK
}

extern "C" fn stuck() -> CallOutcome {
    guest_entry(|| panic!("a job would not stop"))
}

#[derive(Default)]
struct Fonts(AtomicUsize);

impl FontHost for Fonts {
    fn sources(&self) -> Option<FontSources> {
        self.0.fetch_add(1, Ordering::Relaxed);
        None
    }

    fn install(&self, _config: FontConfig) -> Option<FontSources> {
        None
    }

    fn add_faces(&self, _faces: Vec<FontAsset>) -> Option<FontSources> {
        None
    }

    fn install_only(
        &self,
        _faces: Vec<FontAsset>,
        _families: Vec<String>,
    ) -> Option<Result<FontSources, ExclusiveFontsError>> {
        None
    }
}

struct Mapping(Arc<AtomicBool>);

impl Drop for Mapping {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Entries whose lifecycle touches nothing of this process but what `quiesce` does.
fn bare(quiesce: unsafe extern "C" fn() -> CallOutcome) -> Bare {
    let served = Bare::serving(create_nothing);
    Bare {
        lifecycle: Lifecycle {
            loaded: returns,
            unloading: returns,
            quiesce,
            ..served.lifecycle
        },
        ..served
    }
}

fn loaded(
    quiesce: unsafe extern "C" fn() -> CallOutcome,
) -> (GuestLibrary<Bare>, Arc<Fonts>, Arc<AtomicBool>) {
    let entries = bare(quiesce);
    let fonts = Arc::new(Fonts::default());
    let unmapped = Arc::new(AtomicBool::new(false));
    let library = GuestLibrary::start(
        entries,
        Some(Box::new(Mapping(Arc::clone(&unmapped)))),
        Some(Arc::clone(&fonts) as Arc<dyn FontHost>),
    );
    (library, fonts, unmapped)
}

#[test]
fn a_guest_quiesced_is_unmapped_and_its_services_freed() {
    let (library, fonts, unmapped) = loaded(returns);
    assert_eq!(Arc::strong_count(&fonts), 2, "precondition: served");
    drop(library);
    assert!(unmapped.load(Ordering::Relaxed));
    assert_eq!(Arc::strong_count(&fonts), 1, "its services are freed");
}

/// A thread `quiesce` could not join may still run the guest's code, and that code may still ask the host for a service: the backends behind the table stay with the mapping.
#[test]
fn a_guest_whose_threads_could_not_be_joined_keeps_its_services_with_its_library() {
    let (library, fonts, unmapped) = loaded(stuck);
    let table = SERVED.get().expect("the guest was served");
    drop(library);
    assert!(
        !unmapped.load(Ordering::Relaxed),
        "its library stays mapped"
    );
    assert_eq!(Arc::strong_count(&fonts), 2, "and its services kept");
    let answered = unsafe { table.ask::<Option<FontSources>>(table.font_sources, &()) };
    assert_eq!(answered, Some(None), "a service still answers");
    assert_eq!(fonts.0.load(Ordering::Relaxed), 1);
}

#[test]
fn entries_this_process_serves_itself_are_served_nothing_and_never_unloaded() {
    LIFECYCLE_CALLS.set(0);
    drop(GuestLibrary::start(bare(returns), None, None));
    assert!(
        SERVED.get().is_some_and(|table| table.data.is_null()),
        "served nothing"
    );
    assert_eq!(LIFECYCLE_CALLS.get(), 0, "never told it loads or unloads");
}
