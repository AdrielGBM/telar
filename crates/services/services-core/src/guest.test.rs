use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use platform_core::guest::UpperServices;
use renderer_core::{FontSource, GenericFamilies};

use super::*;
use crate::{FileFilter, MemoryStore};

fn nothing_served() -> Backends {
    Backends {
        clipboard: None,
        uri_opener: None,
        file_dialogs: None,
        preferences: Arc::new(MemoryStore::default()),
        app_paths: None,
        keyboard_convention: KeyboardConvention::native(),
        location_format: None,
        asset_base: None,
        layout_grid: LayoutGrid::default(),
        loop_waker: None,
        fonts: None,
    }
}

fn served(backends: Backends) -> ServedServices {
    ServedServices {
        backends: Box::new(backends),
    }
}

fn host(served: &ServedServices) -> Arc<Host> {
    Arc::new(Host(served.table()))
}

#[derive(Default)]
struct Selection(Mutex<Option<String>>);

impl Clipboard for Selection {
    fn text(&self) -> Option<String> {
        self.0.lock().unwrap().clone()
    }
    fn set_text(&self, text: &str) {
        *self.0.lock().unwrap() = Some(text.to_owned());
    }
}

#[derive(Default)]
struct Opened(Mutex<Vec<String>>);

impl UriOpener for Opened {
    fn open(&self, uri: &Uri) -> bool {
        self.0.lock().unwrap().push(uri.to_string());
        true
    }
    fn open_beside(&self, address: &RouteAddress) -> bool {
        self.0.lock().unwrap().push(address.to_string());
        true
    }
}

#[derive(Default)]
struct Asked(Mutex<Vec<(&'static str, FileDialog)>>);

impl Asked {
    fn ask(&self, kind: &'static str, request: FileDialog) -> PathBuf {
        let path = request
            .directory
            .clone()
            .unwrap_or_default()
            .join(request.file_name.clone().unwrap_or_default());
        self.0.lock().unwrap().push((kind, request));
        path
    }
}

impl FileDialogs for Asked {
    fn open_file(&self, request: FileDialog) -> Option<PathBuf> {
        Some(self.ask("open_file", request))
    }
    fn open_files(&self, request: FileDialog) -> Vec<PathBuf> {
        vec![self.ask("open_files", request)]
    }
    fn save_file(&self, request: FileDialog) -> Option<PathBuf> {
        Some(self.ask("save_file", request))
    }
    fn pick_folder(&self, request: FileDialog) -> Option<PathBuf> {
        Some(self.ask("pick_folder", request))
    }
}

struct Broken;

impl Clipboard for Broken {
    fn text(&self) -> Option<String> {
        panic!("the selection owner went away")
    }
    fn set_text(&self, _: &str) {
        panic!("the selection owner went away")
    }
}

impl UriOpener for Broken {
    fn open(&self, _: &Uri) -> bool {
        panic!("the launcher went away")
    }
}

impl FileDialogs for Broken {
    fn open_file(&self, _: FileDialog) -> Option<PathBuf> {
        panic!("the portal went away")
    }
    fn open_files(&self, _: FileDialog) -> Vec<PathBuf> {
        panic!("the portal went away")
    }
    fn save_file(&self, _: FileDialog) -> Option<PathBuf> {
        panic!("the portal went away")
    }
    fn pick_folder(&self, _: FileDialog) -> Option<PathBuf> {
        panic!("the portal went away")
    }
}

#[test]
fn the_guest_copies_to_and_pastes_from_the_host_clipboard() {
    let selection = Arc::new(Selection::default());
    let served = served(Backends {
        clipboard: Some(selection.clone()),
        ..nothing_served()
    });
    let clipboard = HostClipboard(host(&served));
    assert_eq!(clipboard.text(), None);
    clipboard.set_text("copied in the guest");
    assert_eq!(
        selection.text().as_deref(),
        Some("copied in the guest"),
        "the host's selection holds it, and outlives the guest"
    );
    assert_eq!(clipboard.text().as_deref(), Some("copied in the guest"));
}

#[test]
fn the_guest_opens_links_and_routes_through_the_host_opener() {
    let opened = Arc::new(Opened::default());
    let served = served(Backends {
        uri_opener: Some(opened.clone()),
        ..nothing_served()
    });
    let opener = HostUriOpener(host(&served));
    assert!(opener.open(&Uri::parse("https://example.com/a?b=c").unwrap()));
    assert!(opener.open_beside(&RouteAddress::parse("/projects/telar#top").unwrap()));
    assert_eq!(
        *opened.0.lock().unwrap(),
        ["https://example.com/a?b=c", "/projects/telar#top"]
    );
}

#[test]
fn the_guest_asks_the_host_dialogs_with_the_whole_request() {
    let asked = Arc::new(Asked::default());
    let served = served(Backends {
        file_dialogs: Some(asked.clone()),
        ..nothing_served()
    });
    let dialogs = HostFileDialogs(host(&served));
    let request = FileDialog::new()
        .title("Export")
        .directory("/tmp/exports")
        .file_name("frame.png")
        .filter("Images", &["png", "jpg"]);
    let expected = PathBuf::from("/tmp/exports/frame.png");
    assert_eq!(dialogs.open_file(request.clone()), Some(expected.clone()));
    assert_eq!(dialogs.open_files(request.clone()), vec![expected.clone()]);
    assert_eq!(dialogs.save_file(request.clone()), Some(expected.clone()));
    assert_eq!(dialogs.pick_folder(FileDialog::new()), Some(PathBuf::new()));
    let asked = asked.0.lock().unwrap();
    let kinds: Vec<_> = asked.iter().map(|(kind, _)| *kind).collect();
    assert_eq!(
        kinds,
        ["open_file", "open_files", "save_file", "pick_folder"]
    );
    let (_, received) = &asked[0];
    assert_eq!(received.title.as_deref(), Some("Export"));
    assert_eq!(
        received.directory.as_deref(),
        Some(Path::new("/tmp/exports"))
    );
    assert_eq!(received.file_name.as_deref(), Some("frame.png"));
    let FileFilter { name, extensions } = &received.filters[0];
    assert_eq!(
        (name.as_str(), extensions.as_slice()),
        ("Images", &["png".to_owned(), "jpg".to_owned()][..])
    );
}

#[test]
fn the_guest_keeps_its_preferences_in_the_host_store() {
    let store = Arc::new(MemoryStore::default());
    let served = served(Backends {
        preferences: store.clone(),
        ..nothing_served()
    });
    let preferences = HostPreferenceStore(host(&served));
    preferences.set("telar.scheme", Some("dark"));
    assert_eq!(store.get("telar.scheme").as_deref(), Some("dark"));
    assert_eq!(preferences.get("telar.scheme").as_deref(), Some("dark"));
    preferences.set("telar.scheme", None);
    assert_eq!(store.get("telar.scheme"), None);
    assert_eq!(preferences.get("never.set"), None);
}

#[test]
fn a_host_backend_that_panics_answers_as_an_absent_one() {
    let served = served(Backends {
        clipboard: Some(Arc::new(Broken)),
        uri_opener: Some(Arc::new(Broken)),
        file_dialogs: Some(Arc::new(Broken)),
        ..nothing_served()
    });
    let host = host(&served);
    let clipboard = HostClipboard(host.clone());
    assert_eq!(clipboard.text(), None);
    clipboard.set_text("kept on this side");
    assert!(!HostUriOpener(host.clone()).open(&Uri::parse("https://example.com").unwrap()));
    let dialogs = HostFileDialogs(host);
    assert_eq!(dialogs.open_file(FileDialog::new()), None);
    assert_eq!(dialogs.open_files(FileDialog::new()), Vec::<PathBuf>::new());
}

#[test]
fn only_the_backends_the_host_has_are_in_its_table() {
    let table = served(nothing_served()).table();
    assert!(table.clipboard_text.is_none() && table.set_clipboard_text.is_none());
    assert!(table.open_uri.is_none() && table.open_beside.is_none());
    assert!(table.upper.open_file.is_none() && table.upper.pick_folder.is_none());
    assert!(table.wake_loop.is_none() && table.upper.install_fonts.is_none());
    assert!(
        table.stored_preference.is_some(),
        "the host always has a store, one in memory at least"
    );
    assert!(table.keyboard_convention.is_some() && table.layout_grid.is_some());
}

struct Paths;

impl AppPathsProvider for Paths {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(PathBuf::from("/home/u/.config"))
    }
    fn data_dir(&self) -> Option<PathBuf> {
        None
    }
    fn cache_dir(&self) -> Option<PathBuf> {
        Some(PathBuf::from("/home/u/.cache"))
    }
    fn system_fonts_dir(&self) -> Option<PathBuf> {
        Some(PathBuf::from("/system/fonts"))
    }
    fn sans_serif_candidates(&self) -> Vec<String> {
        vec!["Roboto".to_owned()]
    }
}

#[test]
fn the_guest_reads_the_host_app_name_and_directories() {
    let served = served(Backends {
        app_paths: Some(Installed {
            name: "notes".to_owned(),
            provider: Arc::new(Paths),
        }),
        ..nothing_served()
    });
    let dirs = host(&served)
        .ask(|table| table.upper.app_paths, &())
        .flatten()
        .expect("the host has paths");
    assert_eq!(dirs.name, "notes");
    assert_eq!(dirs.config_dir(), Paths.config_dir());
    assert_eq!(dirs.data_dir(), None);
    assert_eq!(dirs.cache_dir(), Paths.cache_dir());
    assert_eq!(
        dirs.state_dir(),
        None,
        "the provider's default, data, is none"
    );
    assert_eq!(dirs.system_fonts_dir(), Paths.system_fonts_dir());
    assert_eq!(dirs.sans_serif_candidates(), ["Roboto"]);
}

#[derive(Default)]
struct Database {
    calls: Mutex<Vec<String>>,
}

impl Database {
    fn sources(faces: Vec<FontAsset>) -> FontSources {
        FontSources {
            system_fonts: false,
            dirs: Vec::new(),
            faces,
            sans_serif_family_candidates: vec!["Telar Test".to_owned()],
            generic_families: GenericFamilies {
                serif: "Telar Test".to_owned(),
                sans_serif: "Telar Test".to_owned(),
                monospace: "Telar Test".to_owned(),
                cursive: "Telar Test".to_owned(),
                fantasy: "Telar Test".to_owned(),
            },
        }
    }
}

impl FontHost for Database {
    fn install(&self, config: FontConfig) -> Option<FontSources> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("install {}", config.faces.len()));
        Some(Self::sources(config.faces))
    }
    fn add_faces(&self, faces: Vec<FontAsset>) -> Option<FontSources> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("add {}", faces.len()));
        Some(Self::sources(faces))
    }
    fn install_only(
        &self,
        faces: Vec<FontAsset>,
        families: Vec<String>,
    ) -> Option<Result<FontSources, ExclusiveFontsError>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("only {} {}", faces.len(), families.join(",")));
        Some(Err(ExclusiveFontsError::OtherFacesLoaded))
    }
}

static FACE: &[u8] = b"a face compiled into the guest";

#[test]
fn the_guest_shaper_defers_to_the_host_database_through_bytes() {
    let database = Arc::new(Database::default());
    let served = served(Backends {
        fonts: Some(database.clone()),
        ..nothing_served()
    });
    let fonts = HostFonts(host(&served));
    let installed = fonts
        .install(FontConfig {
            faces: vec![FontAsset::embedded(FACE).named("Telar Test")],
            ..FontConfig::default()
        })
        .expect("the host answers with what it holds");
    let FontSource::Shared(bytes) = &installed.faces[0].source else {
        panic!("the guest holds a copy of its own of what the host holds");
    };
    assert_eq!(&bytes[..], FACE);
    assert_eq!(
        fonts.add_faces(vec![FontAsset::file("fonts/Inter.ttf")]),
        Some(Database::sources(vec![FontAsset::file("fonts/Inter.ttf")]))
    );
    assert_eq!(
        fonts.install_only(
            vec![FontAsset::embedded(FACE)],
            vec!["Telar Test".to_owned()]
        ),
        Some(Err(ExclusiveFontsError::OtherFacesLoaded))
    );
    assert_eq!(
        *database.calls.lock().unwrap(),
        ["install 1", "add 1", "only 1 Telar Test"]
    );
}

static STARTED: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn counting_spawn(
    data: *const c_void,
    start: platform_core::guest::ThreadStart,
    thread: *mut HostThread,
) -> bool {
    STARTED.fetch_add(1, Ordering::Relaxed);
    unsafe { platform_core::guest::spawn_thread(data, start, thread) }
}

#[test]
fn installing_takes_the_host_values_wakes_the_host_loop_and_starts_threads_there() {
    static WOKEN: AtomicUsize = AtomicUsize::new(0);
    let host_convention = match KeyboardConvention::native() {
        KeyboardConvention::Mac => KeyboardConvention::Pc,
        KeyboardConvention::Pc => KeyboardConvention::Mac,
    };
    let served = served(Backends {
        keyboard_convention: host_convention,
        location_format: Some(LocationFormat::new("/portfolio").with_trailing_slash(true)),
        asset_base: Some("https://example.com/site/".to_owned()),
        layout_grid: LayoutGrid::default(),
        loop_waker: Some(Arc::new(|| {
            WOKEN.fetch_add(1, Ordering::Relaxed);
        })),
        fonts: Some(Arc::new(Database::default())),
        ..nothing_served()
    });
    let table = HostServices {
        stored_preference: None,
        store_preference: None,
        spawn_thread: Some(counting_spawn),
        ..served.table()
    };
    let fonts = unsafe { install_host_services(table) };
    assert_eq!(platform_core::keyboard_convention(), host_convention);
    assert_eq!(
        platform_core::installed_location_format(),
        Some(LocationFormat::new("/portfolio").with_trailing_slash(true))
    );
    assert_eq!(
        platform_core::asset_url("images/a.png"),
        "https://example.com/site/images/a.png"
    );
    platform_core::loop_waker().expect("the host's loop wake")();
    assert_eq!(WOKEN.load(Ordering::Relaxed), 1);
    assert!(fonts.is_some(), "the shaper is handed the host's database");
    let started = STARTED.load(Ordering::Relaxed);
    let task = reactive_local::threads::spawn("telar-task", || 6 * 7).expect("the host starts it");
    assert_eq!(task.join().ok(), Some(42));
    let scoped = reactive_local::threads::scope(|scope| {
        scope.spawn("telar-scoped", || 1).unwrap().join().ok()
    });
    assert_eq!(scoped, Some(1));
    assert!(
        STARTED.load(Ordering::Relaxed) >= started + 2,
        "every thread of the guest's runtime is started by the host"
    );
}

/// Where a store of the runtime keeps a backend or a value process-wide, by file and name, and the entries of the table that serve a guest's copy of it.
const BRIDGED: &[(&str, &str, &[&str])] = &[
    (
        "crates/services/services-core/src/clipboard.rs",
        "CLIPBOARD",
        &["clipboard_text", "set_clipboard_text"],
    ),
    (
        "crates/services/services-core/src/uri.rs",
        "OPENER",
        &["open_uri", "open_beside"],
    ),
    (
        "crates/services/services-core/src/dialogs.rs",
        "DIALOGS",
        &["open_file", "open_files", "save_file", "pick_folder"],
    ),
    (
        "crates/services/services-core/src/preferences.rs",
        "STORE",
        &["stored_preference", "store_preference"],
    ),
    (
        "crates/services/services-core/src/app_paths.rs",
        "INSTALLED",
        &["app_paths"],
    ),
    (
        "crates/platform/platform-core/src/chord.rs",
        "CONVENTION",
        &["keyboard_convention"],
    ),
    (
        "crates/platform/platform-core/src/location_format.rs",
        "CURRENT",
        &["location_format"],
    ),
    (
        "crates/platform/platform-core/src/asset_base.rs",
        "BASE",
        &["asset_base"],
    ),
    (
        "crates/platform/platform-core/src/loop_waker.rs",
        "LOOP_WAKER",
        &["wake_loop"],
    ),
    (
        "crates/geometry/geometry-core/src/grid.rs",
        "GRID_X",
        &["layout_grid"],
    ),
    (
        "crates/geometry/geometry-core/src/grid.rs",
        "GRID_Y",
        &["layout_grid"],
    ),
    (
        "crates/renderer/renderer-text/src/fonts.rs",
        "INSTALLED",
        &["install_fonts", "add_font_faces", "install_only_fonts"],
    ),
    (
        "crates/reactive/reactive-local/src/threads.rs",
        "SPAWNER",
        &["spawn_thread", "join_thread", "detach_thread"],
    ),
];

/// The stores a guest's copy keeps for itself, by file and name, and why the host's has nothing to hand it.
const GUEST_OWN: &[(&str, &str, &str)] = &[
    (
        "crates/platform/platform-core/src/event_sink.rs",
        "EVENT_SINK",
        "it feeds the loop of the platform that installed it, and a guest runs none: what it reports back crosses through its own exports",
    ),
    (
        "crates/platform/platform-core/src/history.rs",
        "HUB",
        "the app's own history, which its host hands it through the boundary's exports",
    ),
    (
        "crates/platform/platform-core/src/primary_scroll.rs",
        "PRIMARY",
        "the app's own page scroll",
    ),
    (
        "crates/platform/platform-core/src/primary_scroll.rs",
        "NEXT_CLAIM",
        "the app's own page scroll",
    ),
    (
        "crates/platform/platform-core/src/guest.rs",
        "COPIES",
        "the loader's own count of the copies it opened",
    ),
    (
        "crates/platform/platform-core/src/guest.rs",
        "NEXT_CALL",
        "each copy catches its own panics",
    ),
    (
        "crates/platform/platform-core/src/guest.rs",
        "RECORDED",
        "each copy catches its own panics",
    ),
    (
        "crates/platform/platform-core/src/guest.rs",
        "CURRENT_CALL",
        "each copy catches its own panics",
    ),
    (
        "crates/platform/platform-core/src/guest.rs",
        "HOOK",
        "each copy catches its own panics",
    ),
    (
        "crates/platform/platform-core/src/guest/outcome.rs",
        "REPORT",
        "each copy lends its own panic reports",
    ),
    (
        "crates/platform/platform-core/src/guest/lent.rs",
        "ANSWER",
        "each copy lends its own answers",
    ),
    (
        "crates/platform/platform-core/src/guest/lifecycle.rs",
        "HOST_EPOCH",
        "the host's epoch as each copy adopted it at load, through `adopt_epoch`",
    ),
    (
        "crates/platform/platform-core/src/guest/contract.rs",
        "TELAR_GUEST_CONTRACT",
        "the stamp a guest exports for its host to read",
    ),
    (
        "crates/renderer/renderer-core/src/metrics.rs",
        "TEXT_METRICS",
        "a measurer is code: a guest installs its own, and a shaping one sets text in the host's faces through the font entries",
    ),
    (
        "crates/renderer/renderer-core/src/metrics.rs",
        "TEXT_METRICS_GENERATION",
        "follows this copy's own measurer",
    ),
    (
        "crates/renderer/renderer-core/src/perf.rs",
        "ENABLED",
        "each copy counts its own frame statistics",
    ),
    (
        "crates/renderer/renderer-core/src/perf.rs",
        "GLOBAL",
        "each copy counts its own frame statistics",
    ),
    (
        "crates/renderer/renderer-core/src/perf.rs",
        "FRAMES",
        "each copy counts its own frame statistics",
    ),
    (
        "crates/renderer/renderer-core/src/perf.rs",
        "CAPTURES",
        "each copy counts its own frame statistics",
    ),
    (
        "crates/renderer/renderer-core/src/perf.rs",
        "CAPTURING",
        "each copy counts its own frame statistics",
    ),
    (
        "crates/renderer/renderer-core/src/perf.rs",
        "CAPTURED",
        "each copy counts its own frame statistics",
    ),
    (
        "crates/renderer/renderer-core/src/gpu_sync.rs",
        "GPU_LIFECYCLE",
        "guards this copy's own GPU objects, and a guest draws on no device of the host's",
    ),
    (
        "crates/renderer/renderer-text/src/fonts.rs",
        "HOST",
        "where a guest keeps the font host the table's font entries are wrapped in",
    ),
    (
        "crates/renderer/renderer-text/src/fonts.rs",
        "RELEASED_ON_UNLOAD",
        "this copy's own release of its database",
    ),
    (
        "crates/renderer/renderer-text/src/fonts.rs",
        "FACES_GENERATION",
        "follows this copy's own database",
    ),
    (
        "crates/renderer/renderer-text/src/fonts.rs",
        "FACES_STABLE",
        "a lock this crate's tests share",
    ),
    (
        "crates/renderer/renderer-text/src/measure.rs",
        "MEASURE_SHAPER",
        "a per-thread cache over this copy's database",
    ),
    (
        "crates/i18n/i18n-core/src/installed.rs",
        "INSTALLED",
        "each image installs the catalog baked into it, which a host's would outlive",
    ),
    (
        "crates/i18n/i18n-core/src/plural.rs",
        "INSTALLED",
        "each image installs rules of its own from its setup, which a host's would outlive",
    ),
    (
        "crates/i18n/i18n-core/src/locale.rs",
        "locale",
        "the app's own locale, a signal of its reactive runtime",
    ),
    (
        "crates/semantics/semantics-core/src/uri.rs",
        "APP_SCHEMES",
        "each image registers its app's schemes from its setup, and the host parses every URI it is handed against its own",
    ),
    (
        "crates/services/services-core/src/guest.rs",
        "HOST_THREADS",
        "where a guest keeps the table its spawner starts threads through",
    ),
    (
        "crates/reactive/reactive-local/src/thread_exit.rs",
        "LEFT_TO_HOST",
        "whether this copy is a guest, which only its own loader can say",
    ),
    (
        "crates/reactive/reactive-local/src/thread_exit.rs",
        "UNLOADS",
        "what this copy frees as its host unloads it",
    ),
    (
        "crates/reactive/reactive-local/src/thread_exit.rs",
        "QUIESCES",
        "the threads this copy joins before its host unmaps it",
    ),
    (
        "crates/reactive/reactive-local/src/thread_exit.rs",
        "RELEASES",
        "what each thread frees of this copy's state",
    ),
    (
        "crates/reactive/reactive-local/src/thread_exit.rs",
        "AT_EXIT",
        "the thread-exit release of this copy's caches",
    ),
    (
        "crates/reactive/reactive-local/src/detached.rs",
        "DEPTH",
        "the app's own reactive ownership",
    ),
    (
        "crates/reactive/reactive-core/src/surface_scoped.rs",
        "THREAD",
        "the app's own surface-scoped state",
    ),
    (
        "crates/reactive/reactive-core/src/timer.rs",
        "TIMERS",
        "the app's own timers, which its host fires through the boundary's exports",
    ),
    (
        "crates/reactive/reactive-core/src/runtime/surface.rs",
        "CURRENT_SURFACE",
        "the app's own reactive runtime",
    ),
    (
        "crates/reactive/reactive-core/src/runtime/surface.rs",
        "ENTER_HOOK",
        "the app's own reactive runtime",
    ),
    (
        "crates/reactive/reactive-core/src/runtime/mod.rs",
        "RUNTIME",
        "the app's own reactive runtime",
    ),
    (
        "crates/reactive/reactive-core/src/task/pool/native.rs",
        "POOL",
        "the workers this copy runs its tasks on, each started by the host through `spawn_thread`",
    ),
    (
        "crates/reactive/reactive-core/src/task/pool/native.rs",
        "JOINED_BEFORE_UNMAP",
        "this copy's own registration of its pool's join",
    ),
    (
        "crates/reactive/reactive-core/src/task/mod.rs",
        "TASK_WAKER",
        "handed by the host's loop rather than at load, through each boundary's `install_task_waker` entry",
    ),
    (
        "crates/reactive/reactive-core/src/task/mod.rs",
        "TASKS",
        "the app's own tasks, which its host drains through the boundary's exports",
    ),
    (
        "crates/embed/telar-embed/src/guest.rs",
        "TELAR_EMBED_ENTRIES",
        "the export a guest declares for its host to read, which the guest never reads back",
    ),
    (
        "crates/layout/layout-reactive/src/direction.rs",
        "direction",
        "the app's own surface-scoped writing direction",
    ),
    (
        "crates/layout/layout-reactive/src/context.rs",
        "PARENTS",
        "the app's own per-surface layout tree",
    ),
    (
        "crates/layout/layout-reactive/src/context.rs",
        "SCROLL_HOSTS",
        "the app's own per-surface layout tree",
    ),
    (
        "crates/layout/layout-reactive/src/context.rs",
        "LAYOUT_RUNTIME",
        "the app's own per-surface layout tree",
    ),
    (
        "crates/layout/layout-reactive/src/context.rs",
        "NEXT_GENERATION",
        "the app's own layout generations",
    ),
    (
        "crates/layout/layout-reactive/src/context.rs",
        "RECORDINGS",
        "the app's own layout pass bookkeeping",
    ),
    (
        "crates/layout/layout-reactive/src/context.rs",
        "AFTER_LAYOUT",
        "the app's own layout pass bookkeeping",
    ),
    (
        "crates/layout/layout-reactive/src/context.rs",
        "MEASURE_AGAIN",
        "the app's own layout pass bookkeeping",
    ),
    (
        "crates/layout/layout-reactive/src/surface_size.rs",
        "SIDES",
        "the app's own per-surface size",
    ),
    (
        "crates/motion/motion-core/src/ticker.rs",
        "REGISTRY",
        "the app's own animation tickers",
    ),
    (
        "crates/platform/platform-core/src/window_command.rs",
        "WINDOW_COMMANDS",
        "the app's own per-surface window-command queue",
    ),
    (
        "crates/preferences/preferences-core/src/lib.rs",
        "COLOR_SCHEME",
        "the app's own reactive preference state",
    ),
    (
        "crates/preferences/preferences-core/src/lib.rs",
        "REDUCED_MOTION",
        "the app's own reactive preference state",
    ),
    (
        "crates/preferences/preferences-core/src/lib.rs",
        "HIGH_CONTRAST",
        "the app's own reactive preference state",
    ),
    (
        "crates/preferences/preferences-core/src/lib.rs",
        "LOCALES",
        "the app's own reactive preference state",
    ),
    (
        "crates/preferences/preferences-core/src/lib.rs",
        "REDUCED_MOTION_OVERRIDE",
        "the app's own reactive preference state",
    ),
    (
        "crates/preferences/preferences-core/src/lib.rs",
        "contrast",
        "the app's own reactive preference state",
    ),
    (
        "crates/reactive/reactive-core/src/surface_scoped.rs",
        "OVERRIDE",
        "the app's own per-surface override behind each surface-scoped value",
    ),
    (
        "crates/renderer/renderer-assets/src/svg/options.rs",
        "OPTIONS",
        "the app's own SVG parse options, built once from constants",
    ),
    (
        "crates/renderer/renderer-cache/src/registry.rs",
        "PUBLISHED",
        "this copy's own cache statistics, published under its own origin",
    ),
    (
        "crates/renderer/renderer-cache/src/registry.rs",
        "LAST_PUBLISH",
        "the throttle of this copy's own cache statistics",
    ),
    (
        "crates/telar/src/preview/matrix.rs",
        "INSTALLED",
        "this copy's own preview matrix, installed as it loads",
    ),
    (
        "crates/telar/src/preview/arg.rs",
        "LEAKED",
        "the app's own interned preview strings",
    ),
    (
        "crates/telar/src/direction.rs",
        "FOLLOW",
        "the binding this copy's own setup makes, replaced when the setup re-runs",
    ),
    (
        "crates/telar/src/runner/web.rs",
        "FOLLOW_LANGUAGE",
        "the binding this copy's own setup makes, replaced when the setup re-runs",
    ),
    (
        "crates/telar/src/location_locale.rs",
        "FOLLOW",
        "the binding this copy's own setup makes, replaced when the setup re-runs",
    ),
    (
        "crates/telar/src/raster.rs",
        "RASTERIZER",
        "this copy's own rasterizer, freed through thread exit",
    ),
    (
        "crates/telar/src/hot/guest.rs",
        "TELAR_HOT_ENTRIES",
        "the export a guest declares for its host to read, which the guest never reads back",
    ),
    (
        "crates/telar/src/hot/guest.rs",
        "HOST_BATCHES",
        "the batches this copy's hot boundary has open",
    ),
    (
        "crates/telar/src/licenses.rs",
        "ICON_LICENSES",
        "the app's own declared icon licenses",
    ),
    (
        "crates/telar/src/macros.rs",
        "V",
        "the app's own interned literal behind a macro",
    ),
    (
        "crates/telar/src/user_preferences.rs",
        "FOLLOWING",
        "this copy's own binding of the preference store",
    ),
    (
        "crates/telar/src/user_preferences.rs",
        "INSTALLED",
        "this copy's own once-per-process install of the preference store",
    ),
    (
        "crates/telar/src/system_locale.rs",
        "FOLLOW",
        "the binding this copy's own setup makes, replaced when the setup re-runs",
    ),
    (
        "crates/telar/src/hot_state.rs",
        "REGISTRY",
        "the app's own hot state",
    ),
    (
        "crates/telar/src/hot_state.rs",
        "PENDING",
        "the app's own hot state",
    ),
    (
        "crates/telar/src/hot_state.rs",
        "WARNED",
        "the app's own hot state",
    ),
    (
        "crates/ui/theme-core/src/mode.rs",
        "mode",
        "the app's own surface-scoped theme mode",
    ),
    (
        "crates/ui/theme-core/src/mode.rs",
        "MODES",
        "the app's own registered theme modes",
    ),
    (
        "crates/ui/theme-core/src/mode.rs",
        "MODE_THEMES",
        "the app's own registered theme modes",
    ),
    (
        "crates/ui/theme-core/src/mode.rs",
        "MODE_SCHEMES",
        "the app's own registered theme modes",
    ),
    (
        "crates/ui/theme-core/src/mode.rs",
        "SCHEME_PAIR",
        "the app's own registered theme modes",
    ),
    (
        "crates/ui/theme-core/src/mode.rs",
        "PREFERENCE",
        "the app's own scheme preference",
    ),
    (
        "crates/ui/theme-core/src/mode.rs",
        "FOLLOW",
        "the binding this copy's own setup makes, replaced when the setup re-runs",
    ),
    (
        "crates/ui/theme-core/src/context.rs",
        "THEME",
        "the app's own installed theme",
    ),
    (
        "crates/ui/theme-core/src/context.rs",
        "DEFAULT_TOKENS",
        "the app's own default theme tokens",
    ),
    (
        "crates/ui/ui-core/src/inherit.rs",
        "CASCADE",
        "the app's own per-surface style cascade",
    ),
    (
        "crates/ui/ui-core/src/drag.rs",
        "ACTIVE",
        "the app's own drag state",
    ),
    (
        "crates/ui/ui-core/src/drag.rs",
        "CLAIMED",
        "the app's own drag state",
    ),
    (
        "crates/ui/ui-core/src/drag.rs",
        "LIVE",
        "the app's own drag state",
    ),
    (
        "crates/ui/ui-core/src/drag.rs",
        "NEXT_LIVE",
        "the app's own drag state",
    ),
    (
        "crates/ui/ui-core/src/text_metrics.rs",
        "GENERATION",
        "the app's own text-measure generation",
    ),
    (
        "crates/ui/ui-core/src/keyboard.rs",
        "KEYBOARD",
        "the app's own keyboard state",
    ),
    (
        "crates/ui/ui-core/src/cursor.rs",
        "CURSORS",
        "the app's own pointer-shape claims",
    ),
    (
        "crates/ui/ui-core/src/cursor.rs",
        "DEPTH",
        "the app's own pointer-shape claims",
    ),
    (
        "crates/ui/ui-core/src/cursor.rs",
        "NEXT_CLAIM",
        "the app's own pointer-shape claims",
    ),
    (
        "crates/ui/ui-core/src/focus.rs",
        "FOCUS",
        "the app's own per-surface focus state",
    ),
    (
        "crates/ui/ui-core/src/foreign_surface.rs",
        "REGISTRY",
        "the app's own foreign surfaces",
    ),
    (
        "crates/ui/ui-core/src/scroll_viewports.rs",
        "VIEWPORTS",
        "the app's own per-surface scroll viewports",
    ),
    (
        "crates/ui/ui-core/src/dismiss.rs",
        "STACK",
        "the app's own per-surface dismiss stack",
    ),
    (
        "crates/ui/ui-core/src/dismiss.rs",
        "NEXT_ID",
        "the app's own per-surface dismiss stack",
    ),
    (
        "crates/ui/ui-core/src/disposal.rs",
        "DEPTH",
        "the app's own deferred disposal queue",
    ),
    (
        "crates/ui/ui-core/src/disposal.rs",
        "RETIRED",
        "the app's own deferred disposal queue",
    ),
    (
        "crates/ui/ui-core/src/disposal.rs",
        "LIVE_BUILDS",
        "the app's own deferred disposal queue",
    ),
    (
        "crates/ui/ui-core/src/presence.rs",
        "EXITS",
        "the app's own exiting presences, which its host waits on before unmapping",
    ),
    (
        "crates/ui/ui-core/src/annotation.rs",
        "ANNOTATIONS",
        "the app's own per-surface annotations",
    ),
    (
        "crates/ui/ui-core/src/link.rs",
        "ANCHORS",
        "the app's own links and their reveal state",
    ),
    (
        "crates/ui/ui-core/src/link.rs",
        "LINKS",
        "the app's own links and their reveal state",
    ),
    (
        "crates/ui/ui-core/src/link.rs",
        "RUNS",
        "the app's own links and their reveal state",
    ),
    (
        "crates/ui/ui-core/src/link.rs",
        "PENDING",
        "the app's own links and their reveal state",
    ),
    (
        "crates/ui/ui-core/src/link.rs",
        "REVEALER_INSTALLED",
        "the app's own links and their reveal state",
    ),
    (
        "crates/ui/ui-core/src/link.rs",
        "NEXT_TOKEN",
        "the app's own links and their reveal state",
    ),
    (
        "crates/ui/ui-core/src/surface_context.rs",
        "SURFACES",
        "the app's own surfaces",
    ),
    (
        "crates/ui/ui-core/src/surface_context.rs",
        "NEXT_HANDLE",
        "the app's own surfaces",
    ),
    (
        "crates/ui/ui-core/src/surface_context.rs",
        "HOOK_INSTALLED",
        "the app's own surfaces",
    ),
    (
        "crates/ui/ui-core/src/named_overlay.rs",
        "NAMED",
        "the app's own per-surface named overlays",
    ),
    (
        "crates/ui/ui-core/src/input_region.rs",
        "INPUT",
        "the app's own per-surface input regions",
    ),
    (
        "crates/ui/ui-core/src/input_region.rs",
        "NEXT_TOKEN",
        "the app's own per-surface input regions",
    ),
    (
        "crates/ui/ui-core/src/surface_font.rs",
        "DEFAULT_FAMILY",
        "the app's own per-surface default font family",
    ),
    (
        "crates/ui/ui-core/src/surface_title.rs",
        "TITLE",
        "the app's own per-surface title",
    ),
    (
        "crates/ui/ui-core/src/anchor_line.rs",
        "PLACES",
        "the app's own per-surface anchor places",
    ),
    (
        "crates/ui/ui-core/src/pointer.rs",
        "OCCLUDED",
        "the app's own pointer state during dispatch",
    ),
    (
        "crates/ui/ui-core/src/pointer.rs",
        "BUTTONS",
        "the app's own pointer state during dispatch",
    ),
    (
        "crates/ui/ui-core/src/pointer.rs",
        "POSITION",
        "the app's own pointer state during dispatch",
    ),
    (
        "crates/ui/ui-tree/src/render_node.rs",
        "NODE_VEC_POOL",
        "the app's own pool of render-node vectors",
    ),
    (
        "crates/ui/ui-tree/src/overlay_dispatch.rs",
        "OVERLAYS",
        "the app's own per-surface overlay registry",
    ),
    (
        "crates/ui/ui-tree/src/element.rs",
        "CAPTURE",
        "the app's own pointer-capture flag",
    ),
    (
        "crates/ui/ui-tree/src/wheel.rs",
        "SMOOTH",
        "the app's own wheel smoothing choice",
    ),
    (
        "plugins/telar-components/src/overlays/toast/toast.rs",
        "NEXT_ID",
        "the app's own toast ids",
    ),
    (
        "plugins/telar-components/src/overlays/toast/toast.rs",
        "SHARED",
        "the app's own shared toasts",
    ),
    (
        "plugins/telar-components/src/strings.rs",
        "CATALOG",
        "the component strings compiled into this copy",
    ),
    (
        "plugins/telar-icons/src/runtime.rs",
        "WARNED",
        "the icon names this copy has already warned about",
    ),
    (
        "plugins/telar-icons/src/runtime.rs",
        "INSTALLED",
        "the icon loader this copy installed, which owns the signals it handed out",
    ),
    (
        "plugins/telar-icons/src/icon.rs",
        "EMPTY",
        "the app's own placeholder for an icon with nothing to draw yet",
    ),
    (
        "crates/telar/src/preview/test_run/run.rs",
        "LIBRARY_RUN_STARTED",
        "the test binary's once-per-process guard for library previews, which a guest never runs",
    ),
];

/// The crates a guest is built from, each its own workspace root.
const GUEST_ROOTS: &[&str] = &["fixtures/embed-guest", "fixtures/hot-panic-guest"];

/// Every process-global store in the runtime is either served to a guest through the table or listed as the guest's own with a reason, so a store added without a bridge fails here rather than in a hot-reloaded app that finds it empty.
#[test]
fn every_global_store_a_guest_reaches_for_is_served_or_its_own() {
    let root = workspace_root();
    let mut found = Vec::new();
    for dir in guest_source_dirs(&root) {
        collect_sources(&root, &dir, &mut found);
    }
    let mut unbridged = Vec::new();
    let mut seen = Vec::new();
    for path in found {
        let source = std::fs::read_to_string(root.join(&path)).expect("read a source file");
        for (line, store) in stores(&source) {
            let listed = BRIDGED
                .iter()
                .map(|(file, name, _)| (*file, *name))
                .chain(GUEST_OWN.iter().map(|(file, name, _)| (*file, *name)))
                .any(|(file, name)| file == path && name == store);
            if listed {
                seen.push((path.clone(), store));
            } else {
                unbridged.push(format!("{path}:{line} `{store}`"));
            }
        }
    }
    assert!(
        unbridged.is_empty(),
        "serve these to a guest through `HostServices` (platform-core) and `ServedServices`/`install_host_services` (services-core), or list them in `GUEST_OWN` with why a guest keeps its own:\n{}",
        unbridged.join("\n")
    );
    for (file, name) in BRIDGED
        .iter()
        .map(|(file, name, _)| (*file, *name))
        .chain(GUEST_OWN.iter().map(|(file, name, _)| (*file, *name)))
    {
        assert!(
            seen.iter()
                .any(|(path, store)| path == file && store == name),
            "`{name}` in {file} is listed but no longer declared there: drop it"
        );
    }
}

#[test]
fn every_entry_of_the_table_serves_a_store_and_every_store_names_an_entry() {
    let entries: Vec<&str> = HostServices::<UpperTable>::LAYOUT
        .fields
        .iter()
        .chain(UpperTable::LAYOUT.fields)
        .map(|(name, _)| *name)
        .filter(|name| !matches!(*name, "data" | "upper"))
        .collect();
    for (file, store, served_by) in BRIDGED {
        for entry in *served_by {
            assert!(
                entries.contains(entry),
                "`{store}` in {file} names `{entry}`, which `HostServices` has no field for"
            );
        }
    }
    for entry in entries {
        assert!(
            BRIDGED
                .iter()
                .any(|(_, _, served_by)| served_by.contains(&entry)),
            "`HostServices::{entry}` serves no store listed in `BRIDGED`"
        );
    }
}

#[test]
fn a_store_is_found_however_it_is_declared() {
    let source = "static A: u8 = 0;\npub static B: u8 = 0;\npub(crate) static C: u8 = 0;\n    static D: Cell<u8> = const { Cell::new(0) };\n// static E: u8 = 0;\n/// static F: u8 = 0;\n    scoped locale: String as Option<String> = None;\n    slot CASCADE: Cascade = Cascade::default();\nlet statics: u8 = 0;\nstatic mut G: u8 = 0;\n";
    let stores: Vec<_> = stores(source).into_iter().map(|(_, store)| store).collect();
    assert_eq!(stores, ["A", "B", "C", "D", "locale", "CASCADE", "G"]);
}

/// Every `static` item `source` declares, thread-locals included, and every slot a `surface_scoped!` or `surface_local!` declares, by the line it is on.
fn stores(source: &str) -> Vec<(usize, String)> {
    let mut stores = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let line = line.trim_start();
        if line.starts_with("//") {
            continue;
        }
        let item = ["pub(crate) ", "pub(super) ", "pub "]
            .iter()
            .find_map(|visibility| line.strip_prefix(visibility))
            .unwrap_or(line);
        let declared = item
            .strip_prefix("static mut ")
            .or_else(|| item.strip_prefix("static "))
            .or_else(|| item.strip_prefix("scoped "))
            .or_else(|| item.strip_prefix("slot "));
        let Some((name, _)) = declared.and_then(|rest| rest.split_once(':')) else {
            continue;
        };
        let name = name.trim();
        if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            stores.push((index + 1, name.to_owned()));
        }
    }
    stores
}

struct GuestClosure<'a> {
    packages: HashMap<&'a str, &'a serde_json::Value>,
    features: HashMap<&'a str, HashSet<String>>,
    active: HashSet<&'a str>,
}

impl<'a> GuestClosure<'a> {
    fn activate(&mut self, name: &str, features: &[String], default: bool) {
        let Some((&name, &package)) = self.packages.get_key_value(name) else {
            return;
        };
        let newly_active = self.active.insert(name);
        if newly_active {
            for dependency in package["dependencies"].as_array().expect("dependencies") {
                if dependency["kind"].is_null()
                    && !dependency["optional"].as_bool().expect("optional")
                {
                    self.activate_dependency(dependency);
                }
            }
        }
        if default {
            self.enable(name, "default");
        }
        for feature in features {
            self.enable(name, feature);
        }
        if newly_active {
            let enabled: Vec<String> = self
                .features
                .get(name)
                .into_iter()
                .flatten()
                .cloned()
                .collect();
            for feature in enabled {
                self.enable(name, &feature);
            }
        }
    }

    fn activate_dependency(&mut self, dependency: &serde_json::Value) {
        let features: Vec<String> = dependency["features"]
            .as_array()
            .expect("dependency features")
            .iter()
            .map(|feature| feature.as_str().expect("a feature").to_owned())
            .collect();
        let default = dependency["uses_default_features"]
            .as_bool()
            .expect("default flag");
        self.activate(
            dependency["name"].as_str().expect("a name"),
            &features,
            default,
        );
    }

    fn enable(&mut self, name: &'a str, feature: &str) {
        if !self
            .features
            .entry(name)
            .or_default()
            .insert(feature.to_owned())
        {
            return;
        }
        if !self.active.contains(name) {
            return;
        }
        let package = self.packages[name];
        let implied: Vec<String> = package["features"][feature]
            .as_array()
            .into_iter()
            .flatten()
            .map(|item| item.as_str().expect("a feature item").to_owned())
            .collect();
        for item in implied {
            if let Some(optional) = item.strip_prefix("dep:") {
                self.activate_optional(package, optional);
            } else if let Some((target, feature)) = item.split_once('/') {
                let weak = target.ends_with('?');
                let target = target.trim_end_matches('?');
                if !weak {
                    self.activate_optional(package, target);
                }
                if let Some(dependency) = Self::dependency_named(package, target) {
                    let dependency_name = dependency["name"].as_str().expect("a name");
                    if let Some((&dependency_name, _)) =
                        self.packages.get_key_value(dependency_name)
                    {
                        self.enable(dependency_name, feature);
                    }
                }
            } else {
                self.enable(name, &item);
            }
        }
    }

    fn activate_optional(&mut self, package: &'a serde_json::Value, target: &str) {
        if let Some(dependency) = Self::dependency_named(package, target) {
            self.activate_dependency(dependency);
        }
    }

    fn dependency_named(
        package: &'a serde_json::Value,
        target: &str,
    ) -> Option<&'a serde_json::Value> {
        package["dependencies"]
            .as_array()
            .expect("dependencies")
            .iter()
            .find(|dependency| {
                dependency["kind"].is_null()
                    && dependency["rename"]
                        .as_str()
                        .unwrap_or(dependency["name"].as_str().unwrap_or_default())
                        == target
            })
    }
}

fn guest_source_dirs(root: &Path) -> Vec<PathBuf> {
    let output = std::process::Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--offline",
            "--no-deps",
            "--manifest-path",
        ])
        .arg(root.join("Cargo.toml"))
        .output()
        .expect("run `cargo metadata`");
    assert!(
        output.status.success(),
        "`cargo metadata` failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("the metadata is JSON");
    let packages: HashMap<&str, &serde_json::Value> = metadata["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .map(|package| (package["name"].as_str().expect("a package name"), package))
        .collect();
    let mut closure = GuestClosure {
        packages: packages.clone(),
        features: HashMap::new(),
        active: HashSet::new(),
    };
    for guest in GUEST_ROOTS {
        let manifest: toml::Table = std::fs::read_to_string(root.join(guest).join("Cargo.toml"))
            .expect("read a guest manifest")
            .parse()
            .expect("a guest manifest is TOML");
        let dependencies = manifest["dependencies"]
            .as_table()
            .expect("a guest declares dependencies");
        for (key, dependency) in dependencies {
            let table = dependency
                .as_table()
                .unwrap_or_else(|| panic!("{guest} declares `{key}` as a table with a path"));
            assert!(
                table.contains_key("path"),
                "{guest} depends on `{key}` by path, in this workspace"
            );
            let name = table
                .get("package")
                .and_then(toml::Value::as_str)
                .unwrap_or(key);
            let features: Vec<String> = table
                .get("features")
                .and_then(toml::Value::as_array)
                .into_iter()
                .flatten()
                .map(|feature| feature.as_str().expect("a feature").to_owned())
                .collect();
            let default = table
                .get("default-features")
                .and_then(toml::Value::as_bool)
                .unwrap_or(true);
            assert!(
                packages.contains_key(name),
                "`{name}` of {guest} is not a workspace member"
            );
            closure.activate(name, &features, default);
        }
    }
    closure
        .active
        .into_iter()
        .map(|name| packages[name])
        .filter(|package| {
            !package["targets"]
                .as_array()
                .expect("targets")
                .iter()
                .any(|target| {
                    target["kind"]
                        .as_array()
                        .is_some_and(|kinds| kinds.iter().any(|kind| kind == "proc-macro"))
                })
        })
        .map(|package| {
            let manifest = Path::new(package["manifest_path"].as_str().expect("a manifest"));
            manifest.parent().expect("a manifest folder").join("src")
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn collect_sources(root: &Path, dir: &Path, found: &mut Vec<String>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("{} is scanned but cannot be read: {error}", dir.display()));
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            collect_sources(root, &path, found);
        } else if name.ends_with(".rs") && !name.ends_with(".test.rs") {
            let relative = path.strip_prefix(root).expect("under the workspace root");
            found.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn workspace_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let manifest =
        std::fs::read_to_string(root.join("Cargo.toml")).expect("the workspace manifest");
    assert!(
        manifest.contains("[workspace]"),
        "this crate sits at crates/services/services-core in its workspace"
    );
    root.canonicalize().expect("the workspace root")
}

// A path is spelled as its platform stores it (UTF-16 on Windows), so these bytes are pinned for the Unix spelling only.
#[cfg(unix)]
#[test]
fn the_bytes_of_the_host_app_dirs_are_pinned_to_the_wire_version() {
    let dirs = AppDirs {
        name: "Telar".to_owned(),
        config: Some(PathBuf::from("/home/user/.config/telar")),
        data: Some(PathBuf::from("/home/user/.local/share/telar")),
        cache: None,
        state: Some(PathBuf::from("/home/user/.local/state/telar")),
        runtime: None,
        system_fonts: Some(PathBuf::from("/usr/share/fonts")),
        sans_serif_candidates: vec!["Inter".to_owned(), "DejaVu Sans".to_owned()],
    };
    assert_eq!(
        wire_core::decode::<AppDirs>(&wire_core::encode(&dirs)).unwrap(),
        dirs
    );
    wire_core::assert_golden!("src/wire_golden/app_dirs.hex", &wire_core::encode(&dirs));
}
