//! What a host and the guests it loads share beyond the contract `platform_core::guest` declares, since it reaches into stores and a draw list that crate does not link: [`GuestLibrary`], which drives any guest a host loaded and keeps what it was served; [`ServedServices`], the backends a host serves through the contract's [`HostServices`] table, and [`install_host_services`], which fills a guest's own stores with backends that ask the host through it; and [`LentFrames`], the frames a guest's instance lends.
//!
//! A guest links its own copy of every store here, of `platform-core`'s and of the shaper's font database, and each starts empty: without the table a guest copies to no clipboard, opens no link, keeps no preference past its unload, has no directories of its own, shows its shortcuts for the system it was compiled on, sets its text in faces the host does not draw it in, and starts its threads with its own std, which keeps its library mapped for as long as the host thread that started one lives. Nothing owned crosses: each request and answer is spelled into bytes the spelling side lends, and the host's backends stay behind `data`, which the guest never reads.

mod frames;
mod library;

pub use frames::LentFrames;
pub use library::GuestLibrary;

use std::ffi::c_void;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, PoisonError, RwLock};

use geometry_core::LayoutGrid;
use platform_core::guest::{HostServices, HostThread, LentBytes, Reply, Service, serve};
use platform_core::{KeyboardConvention, LocationFormat};
use reactive_local::threads::{Joinable, Spawner};
use renderer_core::{ExclusiveFontsError, FontAsset, FontConfig, FontHost, FontSources};
use semantics_core::{RouteAddress, Uri};
use wire_core::{Wire, wire_struct};

use crate::app_paths::Installed;
use crate::{
    AppPathsProvider, Clipboard, FileDialog, FileDialogs, PreferenceStore, UriOpener, app_paths,
};

type LoopWaker = Arc<dyn Fn() + Send + Sync>;

/// The backends a host serves its guests, as it had them installed when it loaded one, behind the [`HostServices`] table [`table`](Self::table) builds over them.
///
/// The host keeps it until the guest it was handed to is unloaded, or for the rest of the process when that guest's threads could not be joined, as [`GuestLibrary`] does: the guest's backends call into it from any of the guest's threads until then. Each entry runs the host's backend under [`shield`](platform_core::guest::shield), so a panic in one stops on this side and the guest reads it as no answer.
pub struct ServedServices {
    backends: Box<Backends>,
}

struct Backends {
    clipboard: Option<Arc<dyn Clipboard>>,
    uri_opener: Option<Arc<dyn UriOpener>>,
    file_dialogs: Option<Arc<dyn FileDialogs>>,
    preferences: Arc<dyn PreferenceStore>,
    app_paths: Option<Installed>,
    keyboard_convention: KeyboardConvention,
    location_format: Option<LocationFormat>,
    asset_base: Option<String>,
    layout_grid: LayoutGrid,
    loop_waker: Option<LoopWaker>,
    fonts: Option<Arc<dyn FontHost>>,
}

impl ServedServices {
    /// What this side has installed, served as it is now. `fonts` is this side's font database for a guest's shaper to defer to, `renderer_text::fonts::Served` in a build that shapes its own text; `None` where there is none, and for a guest that is this process's own, whose text already sets in the one database.
    pub fn installed(fonts: Option<Arc<dyn FontHost>>) -> Self {
        Self {
            backends: Box::new(Backends {
                clipboard: crate::clipboard(),
                uri_opener: crate::uri_opener(),
                file_dialogs: crate::file_dialogs(),
                preferences: crate::preference_store(),
                app_paths: app_paths::installed(),
                keyboard_convention: platform_core::keyboard_convention(),
                location_format: platform_core::installed_location_format(),
                asset_base: platform_core::asset_base(),
                layout_grid: geometry_core::layout_grid(),
                loop_waker: platform_core::loop_waker(),
                fonts,
            }),
        }
    }

    /// The table a guest is handed, valid for as long as `self` lives; an entry is missing where this side has no such backend.
    pub fn table(&self) -> HostServices {
        let backends = &*self.backends;
        let some = |present: bool, entry: Entry| present.then_some(entry);
        HostServices {
            data: std::ptr::from_ref(backends).cast(),
            clipboard_text: some(backends.clipboard.is_some(), clipboard_text),
            set_clipboard_text: some(backends.clipboard.is_some(), set_clipboard_text),
            open_uri: some(backends.uri_opener.is_some(), open_uri),
            open_beside: some(backends.uri_opener.is_some(), open_beside),
            open_file: some(backends.file_dialogs.is_some(), open_file),
            open_files: some(backends.file_dialogs.is_some(), open_files),
            save_file: some(backends.file_dialogs.is_some(), save_file),
            pick_folder: some(backends.file_dialogs.is_some(), pick_folder),
            stored_preference: Some(stored_preference),
            store_preference: Some(store_preference),
            app_paths: Some(app_dirs),
            keyboard_convention: Some(keyboard_convention),
            location_format: Some(location_format),
            asset_base: Some(asset_base),
            layout_grid: Some(layout_grid),
            wake_loop: some(backends.loop_waker.is_some(), wake_loop),
            font_sources: some(backends.fonts.is_some(), font_sources),
            install_fonts: some(backends.fonts.is_some(), install_fonts),
            add_font_faces: some(backends.fonts.is_some(), add_font_faces),
            install_only_fonts: some(backends.fonts.is_some(), install_only_fonts),
            spawn_thread: Some(platform_core::guest::spawn_thread),
            join_thread: Some(platform_core::guest::join_thread),
            detach_thread: Some(platform_core::guest::detach_thread),
        }
    }
}

type Entry = unsafe extern "C" fn(*const c_void, LentBytes, Reply);

unsafe extern "C" fn clipboard_text(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe {
        serve(data, request, reply, |host: &Backends, ()| {
            host.clipboard
                .as_ref()
                .and_then(|clipboard| clipboard.text())
        })
    }
}

unsafe extern "C" fn set_clipboard_text(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe {
        serve(data, request, reply, |host: &Backends, text: String| {
            if let Some(clipboard) = &host.clipboard {
                clipboard.set_text(&text);
            }
        })
    }
}

unsafe extern "C" fn open_uri(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe {
        serve(data, request, reply, |host: &Backends, uri: Uri| {
            host.uri_opener
                .as_ref()
                .is_some_and(|opener| opener.open(&uri))
        })
    }
}

unsafe extern "C" fn open_beside(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe {
        serve(
            data,
            request,
            reply,
            |host: &Backends, address: RouteAddress| {
                host.uri_opener
                    .as_ref()
                    .is_some_and(|opener| opener.open_beside(&address))
            },
        )
    }
}

unsafe extern "C" fn open_file(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe {
        serve(
            data,
            request,
            reply,
            |host: &Backends, dialog: FileDialog| {
                host.file_dialogs
                    .as_ref()
                    .and_then(|dialogs| dialogs.open_file(dialog))
            },
        )
    }
}

unsafe extern "C" fn open_files(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe {
        serve(
            data,
            request,
            reply,
            |host: &Backends, dialog: FileDialog| {
                host.file_dialogs
                    .as_ref()
                    .map_or_else(Vec::new, |dialogs| dialogs.open_files(dialog))
            },
        )
    }
}

unsafe extern "C" fn save_file(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe {
        serve(
            data,
            request,
            reply,
            |host: &Backends, dialog: FileDialog| {
                host.file_dialogs
                    .as_ref()
                    .and_then(|dialogs| dialogs.save_file(dialog))
            },
        )
    }
}

unsafe extern "C" fn pick_folder(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe {
        serve(
            data,
            request,
            reply,
            |host: &Backends, dialog: FileDialog| {
                host.file_dialogs
                    .as_ref()
                    .and_then(|dialogs| dialogs.pick_folder(dialog))
            },
        )
    }
}

unsafe extern "C" fn stored_preference(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe {
        serve(data, request, reply, |host: &Backends, key: String| {
            host.preferences.get(&key)
        })
    }
}

unsafe extern "C" fn store_preference(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe {
        serve(
            data,
            request,
            reply,
            |host: &Backends, (key, value): (String, Option<String>)| {
                host.preferences.set(&key, value.as_deref());
            },
        )
    }
}

unsafe extern "C" fn app_dirs(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe {
        serve(data, request, reply, |host: &Backends, ()| {
            host.app_paths.as_ref().map(AppDirs::of)
        })
    }
}

unsafe extern "C" fn keyboard_convention(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe {
        serve(data, request, reply, |host: &Backends, ()| {
            host.keyboard_convention
        })
    }
}

unsafe extern "C" fn location_format(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe {
        serve(data, request, reply, |host: &Backends, ()| {
            host.location_format.clone()
        })
    }
}

unsafe extern "C" fn asset_base(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe {
        serve(data, request, reply, |host: &Backends, ()| {
            host.asset_base.clone()
        })
    }
}

unsafe extern "C" fn layout_grid(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe { serve(data, request, reply, |host: &Backends, ()| host.layout_grid) }
}

unsafe extern "C" fn wake_loop(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe {
        serve(data, request, reply, |host: &Backends, ()| {
            if let Some(wake) = &host.loop_waker {
                wake();
            }
        })
    }
}

unsafe extern "C" fn font_sources(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe {
        serve(data, request, reply, |host: &Backends, ()| {
            host.fonts.as_ref().and_then(|fonts| fonts.sources())
        })
    }
}

unsafe extern "C" fn install_fonts(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe {
        serve(
            data,
            request,
            reply,
            |host: &Backends, config: FontConfig| {
                host.fonts.as_ref().and_then(|fonts| fonts.install(config))
            },
        )
    }
}

unsafe extern "C" fn add_font_faces(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe {
        serve(
            data,
            request,
            reply,
            |host: &Backends, faces: Vec<FontAsset>| {
                host.fonts.as_ref().and_then(|fonts| fonts.add_faces(faces))
            },
        )
    }
}

unsafe extern "C" fn install_only_fonts(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe {
        serve(
            data,
            request,
            reply,
            |host: &Backends, (faces, families): (Vec<FontAsset>, Vec<String>)| {
                host.fonts
                    .as_ref()
                    .and_then(|fonts| fonts.install_only(faces, families))
            },
        )
    }
}

/// The host's app name and the directories its provider answered, read once: a guest nests its own under the same name, so both keep their files in one place.
#[derive(Clone, Debug, Default, PartialEq)]
struct AppDirs {
    name: String,
    config: Option<PathBuf>,
    data: Option<PathBuf>,
    cache: Option<PathBuf>,
    state: Option<PathBuf>,
    runtime: Option<PathBuf>,
    system_fonts: Option<PathBuf>,
    sans_serif_candidates: Vec<String>,
}

wire_struct!(AppDirs {
    name,
    config,
    data,
    cache,
    state,
    runtime,
    system_fonts,
    sans_serif_candidates
});

impl AppDirs {
    fn of(installed: &Installed) -> Self {
        let provider = installed.provider.as_ref();
        Self {
            name: installed.name.clone(),
            config: provider.config_dir(),
            data: provider.data_dir(),
            cache: provider.cache_dir(),
            state: provider.state_dir(),
            runtime: provider.runtime_dir(),
            system_fonts: provider.system_fonts_dir(),
            sans_serif_candidates: provider.sans_serif_candidates(),
        }
    }
}

impl AppPathsProvider for AppDirs {
    fn config_dir(&self) -> Option<PathBuf> {
        self.config.clone()
    }

    fn data_dir(&self) -> Option<PathBuf> {
        self.data.clone()
    }

    fn cache_dir(&self) -> Option<PathBuf> {
        self.cache.clone()
    }

    fn state_dir(&self) -> Option<PathBuf> {
        self.state.clone()
    }

    fn runtime_dir(&self) -> Option<PathBuf> {
        self.runtime.clone()
    }

    fn system_fonts_dir(&self) -> Option<PathBuf> {
        self.system_fonts.clone()
    }

    fn sans_serif_candidates(&self) -> Vec<String> {
        self.sans_serif_candidates.clone()
    }
}

/// The table as a guest holds it, shared by every backend [`install_host_services`] makes of it and freed with the last of them.
struct Host(HostServices);

impl Host {
    fn ask<T: Wire>(&self, entry: fn(&HostServices) -> Service, request: &impl Wire) -> Option<T> {
        // The table came through `install_host_services`, whose caller vouched that the host keeps it until this copy is unloaded, and every backend holding this is freed by then.
        unsafe { self.0.ask(entry(&self.0), request) }
    }
}

/// The table every thread of a guest's is started through, kept as the plain data it is: what [`install_host_services`] routes this copy's spawns to holds nothing the host's unload has to free, so a thread started while the guest is being released still starts on the host.
struct HostThreads(RwLock<Option<HostServices>>);

static HOST_THREADS: HostThreads = HostThreads(RwLock::new(None));

// Each body goes to the host's `spawn_thread`, which runs it once or answers that it never will, and is dropped here then; joining waits on the host's own handle.
unsafe impl Spawner for HostThreads {
    fn spawn(&self, name: String, main: Box<dyn FnOnce() + Send>) -> io::Result<Box<dyn Joinable>> {
        let table = self
            .0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .unwrap_or(HostServices::NONE);
        // The table came through `install_host_services`, whose caller vouched that the host keeps it until this copy is unloaded, and every thread this copy started is joined or let go by then.
        let thread = unsafe { HostThread::start(&table, &name, main) }
            .ok_or_else(|| io::Error::other("the host could not start the thread"))?;
        Ok(Box::new(OnHost {
            table,
            thread: Some(thread),
        }))
    }
}

/// A thread the host started for this copy, handed back to it exactly once: joined, or let go when dropped unjoined.
struct OnHost {
    table: HostServices,
    thread: Option<HostThread>,
}

impl Joinable for OnHost {
    fn join(mut self: Box<Self>) {
        if let Some(thread) = self.thread.take() {
            unsafe { thread.join(&self.table) }
        }
    }
}

impl Drop for OnHost {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            unsafe { thread.detach(&self.table) }
        }
    }
}

struct HostClipboard(Arc<Host>);

impl Clipboard for HostClipboard {
    fn text(&self) -> Option<String> {
        self.0
            .ask::<Option<String>>(|table| table.clipboard_text, &())
            .flatten()
    }

    fn set_text(&self, text: &str) {
        self.0
            .ask::<()>(|table| table.set_clipboard_text, &text.to_owned());
    }
}

struct HostUriOpener(Arc<Host>);

impl UriOpener for HostUriOpener {
    fn open(&self, uri: &Uri) -> bool {
        self.0.ask(|table| table.open_uri, uri).unwrap_or(false)
    }

    fn open_beside(&self, address: &RouteAddress) -> bool {
        self.0
            .ask(|table| table.open_beside, address)
            .unwrap_or(false)
    }
}

struct HostFileDialogs(Arc<Host>);

impl FileDialogs for HostFileDialogs {
    fn open_file(&self, request: FileDialog) -> Option<PathBuf> {
        self.0.ask(|table| table.open_file, &request).flatten()
    }

    fn open_files(&self, request: FileDialog) -> Vec<PathBuf> {
        self.0
            .ask(|table| table.open_files, &request)
            .unwrap_or_default()
    }

    fn save_file(&self, request: FileDialog) -> Option<PathBuf> {
        self.0.ask(|table| table.save_file, &request).flatten()
    }

    fn pick_folder(&self, request: FileDialog) -> Option<PathBuf> {
        self.0.ask(|table| table.pick_folder, &request).flatten()
    }
}

struct HostPreferenceStore(Arc<Host>);

impl PreferenceStore for HostPreferenceStore {
    fn get(&self, key: &str) -> Option<String> {
        self.0
            .ask(|table| table.stored_preference, &key.to_owned())
            .flatten()
    }

    fn set(&self, key: &str, value: Option<&str>) {
        self.0.ask::<()>(
            |table| table.store_preference,
            &(key.to_owned(), value.map(str::to_owned)),
        );
    }
}

struct HostFonts(Arc<Host>);

impl FontHost for HostFonts {
    fn sources(&self) -> Option<FontSources> {
        self.0.ask(|table| table.font_sources, &()).flatten()
    }

    fn install(&self, config: FontConfig) -> Option<FontSources> {
        self.0.ask(|table| table.install_fonts, &config).flatten()
    }

    fn add_faces(&self, faces: Vec<FontAsset>) -> Option<FontSources> {
        self.0.ask(|table| table.add_font_faces, &faces).flatten()
    }

    fn install_only(
        &self,
        faces: Vec<FontAsset>,
        families: Vec<String>,
    ) -> Option<Result<FontSources, ExclusiveFontsError>> {
        self.0
            .ask(|table| table.install_only_fonts, &(faces, families))
            .flatten()
    }
}

/// Installs, in this copy's own stores, a backend per entry `table` has, each asking the host through it, and the values the host serves: its app name and directories, keyboard convention, location format, asset base and layout grid. A store whose entry is missing keeps what this copy had. Every thread this copy's runtime starts from then on (`reactive_local::threads`) is started by the host, with its own std.
///
/// Answers with the host's font database, as a [`FontHost`] this copy's shaper is to defer to (`renderer_text::fonts::defer_to`), or `None` where the host serves none: the shaper is the one store this crate cannot reach.
///
/// What it installs is freed when the host unloads this copy, through the stores' own release, since each is a static of this copy that registered one when it was filled.
///
/// # Safety
///
/// `table` must be a host's [`ServedServices::table`], and the host must keep that [`ServedServices`] until it has unloaded this copy. Call it after the guest's `loaded` entry has run, so what it installs is released before the library is unmapped.
pub unsafe fn install_host_services(table: HostServices) -> Option<Arc<dyn FontHost>> {
    let host = Arc::new(Host(table));
    if table.clipboard_text.is_some() {
        crate::set_clipboard(Arc::new(HostClipboard(host.clone())));
    }
    if table.open_uri.is_some() {
        crate::set_uri_opener(Arc::new(HostUriOpener(host.clone())));
    }
    if table.open_file.is_some() {
        crate::set_file_dialogs(Arc::new(HostFileDialogs(host.clone())));
    }
    if table.stored_preference.is_some() {
        crate::set_preference_store(Arc::new(HostPreferenceStore(host.clone())));
    }
    if let Some(dirs) = host
        .ask::<Option<AppDirs>>(|table| table.app_paths, &())
        .flatten()
    {
        app_paths::install(dirs.name.clone(), Arc::new(dirs));
    }
    if let Some(convention) = host.ask(|table| table.keyboard_convention, &()) {
        platform_core::set_keyboard_convention(convention);
    }
    if let Some(format) = host
        .ask::<Option<LocationFormat>>(|table| table.location_format, &())
        .flatten()
    {
        platform_core::set_location_format(format);
    }
    if let Some(base) = host
        .ask::<Option<String>>(|table| table.asset_base, &())
        .flatten()
    {
        platform_core::set_asset_base(&base);
    }
    if let Some(grid) = host.ask(|table| table.layout_grid, &()) {
        geometry_core::set_layout_grid(grid);
    }
    if table.spawn_thread.is_some() && table.join_thread.is_some() && table.detach_thread.is_some()
    {
        *HOST_THREADS
            .0
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(table);
        reactive_local::threads::set_spawner(&HOST_THREADS);
    }
    if table.wake_loop.is_some() {
        let waker = host.clone();
        platform_core::set_loop_waker(Arc::new(move || {
            waker.ask::<()>(|table| table.wake_loop, &());
        }));
    }
    table
        .font_sources
        .is_some()
        .then(|| Arc::new(HostFonts(host)) as Arc<dyn FontHost>)
}

#[cfg(test)]
#[path = "guest_test.rs"]
mod tests;
