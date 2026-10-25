use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use platform_core::guest::HOST_SERVICES_LAYOUT;
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
    assert!(table.open_file.is_none() && table.pick_folder.is_none());
    assert!(table.wake_loop.is_none() && table.font_sources.is_none());
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
        .ask::<Option<AppDirs>>(|table| table.app_paths, &())
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
    fn sources(&self) -> Option<FontSources> {
        self.calls.lock().unwrap().push("sources".to_owned());
        None
    }
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
    assert_eq!(
        fonts.sources(),
        None,
        "a host with no faces yet answers none"
    );
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
        ["sources", "install 1", "add 1", "only 1 Telar Test"]
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
        &[
            "font_sources",
            "install_fonts",
            "add_font_faces",
            "install_only_fonts",
        ],
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
        "crates/services/services-core/src/preferences.rs",
        "WRITES",
        "a counter naming this copy's temporary files",
    ),
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
];

/// The crates that keep the stores a guest's copy of the runtime reaches for.
const SCANNED: &[&str] = &[
    "crates/reactive/reactive-local/src",
    "crates/reactive/reactive-core/src",
    "crates/services/services-core/src",
    "crates/platform/platform-core/src",
    "crates/renderer/renderer-core/src",
    "crates/renderer/renderer-text/src",
    "crates/i18n/i18n-core/src",
    "crates/semantics/semantics-core/src",
    "crates/geometry/geometry-core/src",
];

/// Every process-global store in the runtime is either served to a guest through the table or listed as the guest's own with a reason, so a store added without a bridge fails here rather than in a hot-reloaded app that finds it empty.
#[test]
fn every_global_store_a_guest_reaches_for_is_served_or_its_own() {
    let root = workspace_root();
    let mut found = Vec::new();
    for dir in SCANNED {
        collect_sources(&root, &root.join(dir), &mut found);
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
    let entries: Vec<&str> = HOST_SERVICES_LAYOUT
        .fields
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| *name != "data")
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
    let source = "static A: u8 = 0;\npub static B: u8 = 0;\npub(crate) static C: u8 = 0;\n    static D: Cell<u8> = const { Cell::new(0) };\n// static E: u8 = 0;\n/// static F: u8 = 0;\n    scoped locale: String as Option<String> = None;\nlet statics: u8 = 0;\nstatic mut G: u8 = 0;\n";
    let stores: Vec<_> = stores(source).into_iter().map(|(_, store)| store).collect();
    assert_eq!(stores, ["A", "B", "C", "D", "locale", "G"]);
}

/// Every `static` item `source` declares, thread-locals included, and every slot a `surface_scoped!` declares, by the line it is on.
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
            .or_else(|| item.strip_prefix("scoped "));
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

fn collect_sources(root: &Path, dir: &Path, found: &mut Vec<String>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("{} is scanned but cannot be read: {error}", dir.display()));
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            collect_sources(root, &path, found);
        } else if name.ends_with(".rs") && !name.ends_with("_test.rs") {
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
