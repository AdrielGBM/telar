//! `cargo telar dev`: the watch loop, the rebuild, and the hot-reload channel to the running app.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::{Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use telar_project::protocol::{
    BUILD_ERROR_PREFIX, HOT_RELOAD_PREFIX, HOT_REPLIES_LINE, HOT_TOKEN_VAR, HotOutcome, HotReply,
    TEST_TARGET_DIR_VAR, WORKSPACE_DIR_VAR, escape_lines,
};
use telar_project::{
    ASSET_KINDS, DEVTOOLS_PACKAGE, ManifestError, TelarManifest, WORKSHOP_PACKAGE,
};

use super::android::{android_install_and_launch, make_android_cmd};
use super::config::{
    ResolvedPackage, TelarSection, WindowSection, backend_as_str, missing_devtools_note,
    missing_workshop_note, resolve_package, split_android_flag, warn_if_tooling_unlocked,
};
use super::diagnostics;
use super::package::{package_bin_path, package_lib_path, profile_of};
use super::workshop_channel::WorkshopChannel;

fn inject_feature(args: &mut Vec<String>, feature: &str) {
    if let Some(pos) = args.iter().position(|a| a == "--features" || a == "-F")
        && pos + 1 < args.len()
    {
        args[pos + 1] = format!("{},{feature}", args[pos + 1]);
        return;
    }
    args.push("--features".to_string());
    args.push(feature.to_string());
}

/// Runs a cargo build and re-points whatever it says about generated Rust back onto the `.rsx` it came from.
///
/// stdout carries the JSON diagnostic stream and is consumed here; stderr is cargo's own progress and is left on the terminal, so a build still looks like a build. Every build goes through this, not only the failing ones — warnings used to be captured into a `String` that was read on the failure path alone, which made them invisible for a whole development session.
fn build_with_diagnostics(cmd: &mut Command) -> (bool, diagnostics::Report) {
    cmd.arg("--color=always")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let mut child = cmd.spawn().expect("[cargo-telar] failed to invoke cargo");
    // Drained before `wait`, or a report larger than the pipe buffer deadlocks the build it is reading.
    let report = match child.stdout.take() {
        Some(stdout) => diagnostics::collect(BufReader::new(stdout)),
        None => diagnostics::Report::default(),
    };
    let succeeded = child.wait().map(|status| status.success()).unwrap_or(false);
    (succeeded, report)
}

/// Adds `--message-format=json` unless the caller already chose a format, which cargo would reject as two of them rather than honour the later one.
fn with_json_messages(args: &mut Vec<String>) {
    if !args.iter().any(|a| a.starts_with("--message-format")) {
        args.push("--message-format=json".to_string());
    }
}

/// The dylib build, from the binary's own invocation.
///
/// Every flag that decides a feature has to come along: the two halves are swapped into one process, and a dylib built from another feature set is an application the host cannot drive. It used to copy `-p` and `--release` alone, so `--target tui` or a `-F` on the command line reached the binary and not the library beside it.
fn make_lib_build_args(args: &[String], features: &[&str]) -> Vec<String> {
    let mut lib_build_args = vec!["build".to_string(), "--lib".to_string()];
    for pair in args.windows(2) {
        if ["-p", "--package", "--features", "-F"].contains(&pair[0].as_str()) {
            lib_build_args.push(pair[0].clone());
            lib_build_args.push(pair[1].clone());
        }
    }
    for flag in ["--release", "--no-default-features"] {
        if args.iter().any(|arg| arg == flag) {
            lib_build_args.push(flag.to_string());
        }
    }
    for feature in features {
        inject_feature(&mut lib_build_args, feature);
    }
    with_json_messages(&mut lib_build_args);
    lib_build_args
}

fn apply_dev_window_env(envs: &mut Vec<(String, String)>, window: &WindowSection) {
    if let Some(title) = &window.title {
        envs.push(("TELAR_DEV_WINDOW_TITLE".to_string(), title.clone()));
    }
    if let Some(width) = window.width {
        envs.push(("TELAR_DEV_WINDOW_WIDTH".to_string(), width.to_string()));
    }
    if let Some(height) = window.height {
        envs.push(("TELAR_DEV_WINDOW_HEIGHT".to_string(), height.to_string()));
    }
    if let Some(decorations) = window.decorations {
        envs.push((
            "TELAR_DEV_WINDOW_DECORATIONS".to_string(),
            if decorations { "1" } else { "0" }.to_string(),
        ));
    }
    if let Some(resizable) = window.resizable {
        envs.push((
            "TELAR_DEV_WINDOW_RESIZABLE".to_string(),
            if resizable { "1" } else { "0" }.to_string(),
        ));
    }
    if let Some(transparent) = window.transparent {
        envs.push((
            "TELAR_DEV_WINDOW_TRANSPARENT".to_string(),
            if transparent { "1" } else { "0" }.to_string(),
        ));
    }
    if let Some(fullscreen) = &window.fullscreen {
        envs.push((
            "TELAR_DEV_WINDOW_FULLSCREEN".to_string(),
            fullscreen.clone(),
        ));
    }
    if let Some(position) = &window.position {
        envs.push(("TELAR_DEV_WINDOW_POSITION".to_string(), position.clone()));
    }
}

fn is_asset_extension(ext: &str) -> bool {
    ASSET_KINDS
        .iter()
        .any(|kind| kind.extensions.contains(&ext))
}

/// Everything an edit that should rebuild can come from.
pub(crate) struct WatchSet {
    /// Watched recursively: each member's `src/`, asset root and catalog directory.
    dirs: Vec<PathBuf>,
    /// Every `telar.toml` a member's settings are read from, present or not. A `prelude` or `theme` changes the Rust every `.rsx` becomes, so an edit there is a source edit.
    manifests: Vec<PathBuf>,
}

impl WatchSet {
    /// Canonical, because an event names the path the platform resolved — macOS reports `/private/var/…` for a watch on `/var/…` — and a set spelled any other way would match nothing.
    pub(crate) fn collect(workspace_root: &Path) -> Result<Self, ManifestError> {
        let canonical = |path: PathBuf| path.canonicalize().unwrap_or(path);
        Ok(Self {
            dirs: collect_watch_dirs(workspace_root)?
                .into_iter()
                .map(canonical)
                .collect(),
            manifests: collect_watch_manifests(workspace_root)
                .into_iter()
                .map(|manifest| match (manifest.parent(), manifest.file_name()) {
                    (Some(parent), Some(name)) => canonical(parent.to_path_buf()).join(name),
                    _ => manifest,
                })
                .collect(),
        })
    }
}

impl WatchScope for WatchSet {
    /// Assets need no special handling any more: the macro emits an `include_bytes!` per baked asset, so cargo sees the edit as a real dependency, and the bake before each rebuild refreshes the artifact it reads.
    ///
    /// A manifest's directory is watched only for the manifest, so anything else changing beside it — a `Cargo.lock` cargo rewrote, an editor's swap file — is not an edit to the project.
    fn wants(&self, event: &notify::Event) -> bool {
        if !matches!(
            event.kind,
            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
        ) {
            return false;
        }
        event.paths.iter().any(|p| {
            if self.manifests.contains(p) {
                return true;
            }
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            self.dirs.iter().any(|dir| p.starts_with(dir))
                && (matches!(ext, "rs" | "rsx" | "toml") || is_asset_extension(ext))
        })
    }

    /// A manifest is watched through its directory rather than as a file, because an editor that saves by writing a new file and renaming it over the old one would end a watch on the file itself; and not at all when a recursive watch already covers it.
    fn watches(&self) -> Vec<(PathBuf, RecursiveMode)> {
        let mut parents: Vec<PathBuf> = self
            .manifests
            .iter()
            .filter_map(|manifest| manifest.parent().map(Path::to_path_buf))
            .filter(|parent| parent.is_dir())
            .filter(|parent| !self.dirs.iter().any(|dir| parent.starts_with(dir)))
            .collect();
        parents.sort();
        parents.dedup();
        self.dirs
            .iter()
            .map(|dir| (dir.clone(), RecursiveMode::Recursive))
            .chain(
                parents
                    .into_iter()
                    .map(|parent| (parent, RecursiveMode::NonRecursive)),
            )
            .collect()
    }
}

/// The `telar.toml` of every member and of the workspace root — the files [`telar_project::TelarManifest::files`] reads for any member — whether or not each exists yet, so creating one is noticed too.
fn collect_watch_manifests(workspace_root: &Path) -> Vec<PathBuf> {
    let mut manifests: Vec<PathBuf> = telar_project::workspace_member_dirs(workspace_root)
        .into_iter()
        .chain(std::iter::once(workspace_root.to_path_buf()))
        .map(|dir| dir.join(telar_project::MANIFEST_FILENAME))
        .collect();
    manifests.sort();
    manifests.dedup();
    manifests
}

// Every directory an edit can come from: each member's `src/`, the asset root, and the catalog directory. The last two sit outside `src/` by default, so watching only `src/` meant editing an asset or a translation raised no event at all — not one that was handled badly, one that never arrived.
fn collect_watch_dirs(workspace_root: &Path) -> Result<Vec<PathBuf>, ManifestError> {
    let mut dirs = Vec::new();
    for member in telar_project::workspace_member_dirs(workspace_root) {
        let telar = TelarManifest::load(&member)?.telar;
        dirs.push(member.join("src"));
        dirs.push(telar.assets_root(&member));
        dirs.extend(telar.locales_root(&member));
        // The application's own icons, not its Iconify sets: those usually live in `node_modules`, which is no tree to watch recursively.
        dirs.extend(telar.icons.and_then(|icons| icons.svg_dir(&member)));
    }
    dirs.retain(|dir| dir.is_dir());
    dirs.sort();
    dirs.dedup();
    // Each watch is recursive, and `[telar] assets` may point inside `src/` — watching both would deliver every edit twice and rebuild twice for one keystroke.
    let nested: Vec<PathBuf> = dirs
        .iter()
        .filter(|dir| {
            dirs.iter()
                .any(|other| *dir != other && dir.starts_with(other))
        })
        .cloned()
        .collect();
    dirs.retain(|dir| !nested.contains(dir));
    Ok(dirs)
}

// TCP loopback rather than a unix socket, so hot reload works on non-Unix hosts. cargo-telar binds and the app connects once at startup, opens with the token, then reads line events and writes back what it did with each reload.
struct HotChannel {
    adopted: mpsc::Receiver<TcpStream>,
    stream: Option<TcpStream>,
    port: u16,
    /// What the app is handed in [`HOT_TOKEN_VAR`] and has to open the channel with: any local process can reach a loopback port, and only the app was told this.
    token: String,
    reloads: Reloads,
}

impl HotChannel {
    fn bind() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .expect("[cargo-telar] failed to bind hot reload port");
        let port = listener
            .local_addr()
            .expect("[cargo-telar] failed to read hot reload port")
            .port();
        let token = hot_token();
        let (handed, adopted) = mpsc::channel();
        let (heard_tx, heard) = mpsc::channel();
        let expected = token.clone();
        std::thread::Builder::new()
            .name("cargo-telar-hot-accept".to_string())
            .spawn(move || adopt_first_authenticated(listener, &expected, &handed, &heard_tx))
            .expect("[cargo-telar] failed to start the hot reload listener");
        HotChannel {
            adopted,
            stream: None,
            port,
            token,
            reloads: Reloads::new(heard),
        }
    }

    fn notify_hot_reload(&mut self, lib_path: &str) {
        if self.send(&format!("{HOT_RELOAD_PREFIX}{lib_path}")) {
            self.reloads.sent(lib_path, Instant::now());
        }
    }

    fn notify_build_error(&mut self, message: &str) {
        self.send(&format!("{BUILD_ERROR_PREFIX}{}", escape_lines(message)));
    }

    /// Answers whether the line reached the app.
    fn send(&mut self, message: &str) -> bool {
        self.accept_waiting();
        if self.stream.is_none() {
            eprintln!("[cargo-telar] App not connected to the hot reload channel.");
            return false;
        }
        self.deliver(message)
    }

    /// [`Self::send`] for a line that waits for the app: not having connected yet is not worth saying.
    fn deliver(&mut self, message: &str) -> bool {
        use std::io::Write;
        self.accept_waiting();
        let Some(stream) = &mut self.stream else {
            return false;
        };
        match writeln!(stream, "{message}") {
            Ok(()) => true,
            Err(e) => {
                eprintln!("[cargo-telar] Failed to write to hot reload channel: {e}");
                self.stream = None;
                false
            }
        }
    }

    fn accept_waiting(&mut self) {
        if let Ok(stream) = self.adopted.try_recv() {
            self.stream = Some(stream);
        }
    }
}

/// How long a connection has to send its token before it is dropped. The app sends it as soon as it connects.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(2);

/// 256 bits from the system's randomness, as hex.
fn hot_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .expect("[cargo-telar] could not draw the hot reload token from the system's randomness");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Takes connections until one opens with `token`, hands that one on and stops listening, so nothing can connect once the app has; then hears out what the app says on that one connection, and only on it.
fn adopt_first_authenticated(
    listener: TcpListener,
    token: &str,
    handed: &mpsc::Sender<TcpStream>,
    heard_tx: &mpsc::Sender<Heard>,
) {
    let Some(stream) = listener
        .incoming()
        .flatten()
        .find(|stream| opens_with(stream, token))
    else {
        return;
    };
    drop(listener);
    let reader = stream.try_clone();
    if handed.send(stream).is_err() {
        return;
    }
    match reader {
        Ok(reader) => hear_out(reader, heard_tx),
        Err(e) => eprintln!(
            "[cargo-telar] warning: the app's answers to reloads cannot be read ({e}); whether a reload loads will not be known."
        ),
    }
}

#[derive(Debug, PartialEq)]
enum Heard {
    AnswersReloads,
    Reply(HotReply),
}

/// `None` for a line of another version of the replies, which is skipped.
fn heard_from(line: &str) -> Option<Heard> {
    if line == HOT_REPLIES_LINE {
        return Some(Heard::AnswersReloads);
    }
    HotReply::parse(line).map(Heard::Reply)
}

/// Returns once the app closes the channel, which drops `heard_tx`.
fn hear_out(stream: TcpStream, heard_tx: &mpsc::Sender<Heard>) {
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        if let Some(heard) = heard_from(line.trim())
            && heard_tx.send(heard).is_err()
        {
            break;
        }
    }
}

/// A window that is hidden may not load a reload until it is shown, so an answer after this is still printed.
const REPLY_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, PartialEq)]
enum ReloadOutcome {
    Answered(HotOutcome),
    Unanswered { app_answers: bool },
    ChannelClosed,
}

impl ReloadOutcome {
    fn message(&self) -> String {
        match self {
            ReloadOutcome::Answered(HotOutcome::Loaded) => "Hot reloaded.".to_owned(),
            ReloadOutcome::Answered(HotOutcome::Panicked(report)) => {
                format!("The rebuilt library loaded, but the app panicked while starting: {report}")
            }
            ReloadOutcome::Answered(HotOutcome::Refused(reason)) => {
                format!("Hot reload refused, the app keeps running the build before: {reason}")
            }
            ReloadOutcome::Unanswered { app_answers: true } => format!(
                "The app has not said within {}s whether it loaded the rebuilt library; its answer is printed when it comes.",
                REPLY_TIMEOUT.as_secs()
            ),
            ReloadOutcome::Unanswered { app_answers: false } => "Rebuilt library sent; this app does not say whether it loads one (it is built against a telar from before reload answers).".to_owned(),
            ReloadOutcome::ChannelClosed => "The hot reload channel closed before the app said whether it loaded the rebuilt library.".to_owned(),
        }
    }
}

struct PendingReload {
    library: String,
    deadline: Instant,
    overdue: bool,
}

/// The reloads sent that the app has not answered yet, oldest first, each settled by the first answer that names its library.
struct Reloads {
    heard: mpsc::Receiver<Heard>,
    app_answers: bool,
    pending: VecDeque<PendingReload>,
}

impl Reloads {
    fn new(heard: mpsc::Receiver<Heard>) -> Self {
        Self {
            heard,
            app_answers: false,
            pending: VecDeque::new(),
        }
    }

    fn sent(&mut self, library: &str, now: Instant) {
        self.pending.push_back(PendingReload {
            library: library.to_owned(),
            deadline: now + REPLY_TIMEOUT,
            overdue: false,
        });
    }

    /// What is known by `now`: each answer in the order it came, a late one included, then each reload that went past its deadline unanswered, or all of them once the channel closed.
    fn settle(&mut self, now: Instant) -> Vec<ReloadOutcome> {
        let mut outcomes = Vec::new();
        loop {
            match self.heard.try_recv() {
                Ok(Heard::AnswersReloads) => self.app_answers = true,
                Ok(Heard::Reply(reply)) => {
                    self.answered(&reply.library);
                    outcomes.push(ReloadOutcome::Answered(reply.outcome));
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.give_up(&mut outcomes);
                    return outcomes;
                }
            }
        }
        for reload in &mut self.pending {
            if !reload.overdue && now >= reload.deadline {
                reload.overdue = true;
                outcomes.push(ReloadOutcome::Unanswered {
                    app_answers: self.app_answers,
                });
            }
        }
        if !self.app_answers {
            self.pending.retain(|reload| !reload.overdue);
        }
        outcomes
    }

    /// [`Self::settle`] for an app that has exited: whatever it left unanswered it never will answer.
    fn finish(&mut self) -> Vec<ReloadOutcome> {
        let mut outcomes = self.settle(Instant::now());
        self.give_up(&mut outcomes);
        outcomes
    }

    fn answered(&mut self, library: &str) {
        if let Some(index) = self
            .pending
            .iter()
            .position(|reload| reload.library == library)
        {
            self.pending.remove(index);
        }
    }

    fn give_up(&mut self, outcomes: &mut Vec<ReloadOutcome>) {
        if !self.pending.is_empty() {
            outcomes.push(ReloadOutcome::ChannelClosed);
        }
        self.pending.clear();
    }
}

fn report_reloads(outcomes: Vec<ReloadOutcome>) {
    for outcome in outcomes {
        eprintln!("[cargo-telar] {}", outcome.message());
    }
}

fn opens_with(mut stream: &TcpStream, token: &str) -> bool {
    use std::io::Read;
    if stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT)).is_err() {
        return false;
    }
    let mut line = Vec::with_capacity(token.len());
    let mut byte = [0u8];
    while line.len() <= token.len() {
        match stream.read(&mut byte) {
            Ok(1) if byte[0] == b'\n' => break,
            Ok(1) => line.push(byte[0]),
            _ => return false,
        }
    }
    let matches = line.len() == token.len()
        && line
            .iter()
            .zip(token.as_bytes())
            .fold(0, |differ, (a, b)| differ | (a ^ b))
            == 0;
    matches && stream.set_read_timeout(None).is_ok()
}

/// How long a burst of file events has to go quiet before it is acted on: an editor saving one file raises several, and a tool rewriting a directory raises one per file.
const DEBOUNCE: Duration = Duration::from_millis(200);

/// How long a loop waits on the watcher before it looks at the app again.
const POLL: Duration = Duration::from_millis(50);

/// What a watch loop watches: the directories it hands the watcher, and which of their events call for a rebuild.
pub(crate) trait WatchScope {
    fn watches(&self) -> Vec<(PathBuf, RecursiveMode)>;
    fn wants(&self, event: &notify::Event) -> bool;
}

/// A watcher over a [`WatchScope`] and the messages it sends. Every loop reads them through here, so none drops a watcher failure or a notice that events were lost.
pub(crate) struct ChangeFeed<S> {
    scope: S,
    messages: mpsc::Receiver<notify::Result<notify::Event>>,
    _watcher: RecommendedWatcher,
}

impl<S: WatchScope> ChangeFeed<S> {
    /// Fails only when no watcher can be made at all; a directory that cannot be watched is reported and left out.
    pub(crate) fn start(scope: S) -> notify::Result<Self> {
        let (tx, messages) = mpsc::channel();
        let mut watcher = RecommendedWatcher::new(tx, NotifyConfig::default())?;
        for (dir, mode) in scope.watches() {
            if let Err(error) = watcher.watch(&dir, mode) {
                let error = match error.paths.is_empty() {
                    true => error.add_path(dir),
                    false => error,
                };
                eprintln!("[cargo-telar] {}", watch_failure(&error));
            }
        }
        Ok(Self {
            scope,
            messages,
            _watcher: watcher,
        })
    }

    /// Blocks until a change arrives.
    pub(crate) fn wait(&self) {
        loop {
            match self.messages.recv() {
                Ok(message) if self.is_change(&message) => return,
                Ok(_) => {}
                Err(_) => {
                    eprintln!(
                        "[cargo-telar] warning: the file watcher stopped; edits are no longer picked up."
                    );
                    idle();
                }
            }
        }
    }

    /// Whether a change arrives within `timeout`.
    pub(crate) fn changed_within(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match self.messages.recv_timeout(left) {
                Ok(message) if self.is_change(&message) => return true,
                Ok(_) => {}
                Err(_) => return false,
            }
        }
        false
    }

    /// Takes every message already waiting and answers whether any was a change.
    pub(crate) fn drain(&self) -> bool {
        let mut changed = false;
        for message in self.messages.try_iter() {
            changed |= self.is_change(&message);
        }
        changed
    }

    /// Takes messages until none has arrived for `quiet`, so one burst of edits makes one rebuild.
    pub(crate) fn settle(&self, quiet: Duration) {
        while let Ok(message) = self.messages.recv_timeout(quiet) {
            self.is_change(&message);
        }
    }

    /// A rescan notice means the platform dropped events, any of which may have been an edit, so it counts as one.
    fn is_change(&self, message: &notify::Result<notify::Event>) -> bool {
        match message {
            Ok(event) if event.need_rescan() => {
                eprintln!(
                    "[cargo-telar] The file watcher lost events; rebuilding in case one was an edit."
                );
                true
            }
            Ok(event) => self.scope.wants(event),
            Err(error) => {
                eprintln!("[cargo-telar] {}", watch_failure(error));
                false
            }
        }
    }
}

/// What to tell the developer about a watcher failure: where, why, and, for the inotify limit, how to raise it.
fn watch_failure(error: &notify::Error) -> String {
    let reason = match &error.kind {
        notify::ErrorKind::Generic(message) => message.clone(),
        notify::ErrorKind::Io(io) => io.to_string(),
        notify::ErrorKind::PathNotFound => "it does not exist".to_string(),
        notify::ErrorKind::WatchNotFound => "it is not being watched".to_string(),
        notify::ErrorKind::InvalidConfig(config) => {
            format!("the watcher configuration {config:?} is invalid")
        }
        notify::ErrorKind::MaxFilesWatch => {
            let limit = max_user_watches().map_or_else(String::new, |limit| format!(" ({limit})"));
            format!(
                "the system limit on watched directories{limit} is reached; raise `fs.inotify.max_user_watches` with sysctl"
            )
        }
    };
    let paths: Vec<String> = error
        .paths
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    match paths.is_empty() {
        true => format!(
            "warning: the file watcher failed: {reason}. Some edits may not trigger a rebuild."
        ),
        false => format!(
            "warning: could not watch {}: {reason}. Edits there will not trigger a rebuild.",
            paths.join(", ")
        ),
    }
}

fn max_user_watches() -> Option<String> {
    std::fs::read_to_string("/proc/sys/fs/inotify/max_user_watches")
        .ok()
        .map(|limit| limit.trim().to_string())
}

/// Parks the thread for good: what a loop does once it can never act again, leaving the process to whatever else it serves until it is interrupted.
pub(crate) fn idle() -> ! {
    loop {
        std::thread::park();
    }
}

/// The app the hot-reload loop runs: the binary and what it is started with.
struct HotLaunch {
    bin_path: PathBuf,
    lib_path: PathBuf,
    args: Vec<String>,
    envs: Vec<(String, String)>,
}

fn watch_and_hot_reload(
    build_args: Vec<String>,
    launch: HotLaunch,
    mut channel: HotChannel,
    workshop: Option<WorkshopChannel>,
    changes: ChangeFeed<WatchSet>,
) -> ! {
    let HotLaunch {
        bin_path,
        lib_path,
        args,
        envs,
    } = launch;

    eprintln!("[cargo-telar] Starting with hot reload...");
    let mut child = Command::new(&bin_path)
        .args(&args)
        .env("TELAR_HOT_LIB", lib_path.to_str().unwrap_or_default())
        .env("TELAR_HOT_PORT", channel.port.to_string())
        .env(HOT_TOKEN_VAR, &channel.token)
        .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .spawn()
        .expect("[cargo-telar] failed to spawn app binary");

    let mut last_event = Instant::now();
    let mut pending_rebuild = false;
    let mut waiting = None;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                report_reloads(channel.reloads.finish());
                if status.success() {
                    eprintln!("[cargo-telar] App exited.");
                } else {
                    eprintln!("[cargo-telar] App exited abnormally ({status}).");
                }
                // `exit` runs no destructors, and the channel's removes the file naming a workshop that is gone.
                drop(workshop);
                std::process::exit(if status.success() { 0 } else { 1 });
            }
            Ok(None) => {}
            Err(e) => eprintln!("[cargo-telar] error: {e}"),
        }

        if let Some(workshop) = &workshop {
            waiting = workshop.take(waiting);
            if waiting.as_deref().is_some_and(|line| channel.deliver(line)) {
                waiting = None;
            }
        }

        report_reloads(channel.reloads.settle(Instant::now()));

        if changes.drain() {
            last_event = Instant::now();
            pending_rebuild = true;
        }

        if pending_rebuild && last_event.elapsed() >= DEBOUNCE {
            pending_rebuild = false;
            eprintln!("[cargo-telar] Change detected, rebuilding...");
            // `super::run` prepares once, before the loop starts. Every rebuild after that invokes cargo directly, so without this an edited asset compiles against the artifact from startup — and since the macro checks its hash, that is a build failure rather than a stale drawing.
            super::bake::bake_workspace();
            super::transpile::transpile_workspace();
            let mut cmd = Command::new("cargo");
            cmd.args(&build_args)
                .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())));
            let (succeeded, report) = build_with_diagnostics(&mut cmd);
            if !report.is_empty() {
                eprintln!();
                eprint!("{}", report.render(true));
            }
            if succeeded {
                channel.notify_hot_reload(lib_path.to_str().unwrap_or_default());
            } else {
                eprintln!("[cargo-telar] Build failed, waiting for changes...");
                channel.notify_build_error(&report.render(false));
            }
        }

        if changes.changed_within(POLL) {
            last_event = Instant::now();
            pending_rebuild = true;
        }
    }
}

fn watch_and_run(
    cargo_args: Vec<String>,
    envs: Vec<(String, String)>,
    changes: ChangeFeed<WatchSet>,
) -> ! {
    loop {
        eprintln!("[cargo-telar] Starting...");
        super::bake::bake_workspace();
        super::transpile::transpile_workspace();
        let mut child = Command::new("cargo")
            .args(&cargo_args)
            .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .spawn()
            .expect("[cargo-telar] failed to spawn cargo");

        let mut last_event = Instant::now();
        let mut pending_restart = false;

        'watch: loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    // No code means a signal ended it, as Ctrl-C does, and the shell expects 130 for that.
                    let Some(code) = status.code() else {
                        std::process::exit(130);
                    };
                    if code == 0 {
                        std::process::exit(0);
                    }
                    eprintln!("[cargo-telar] Process exited ({code}). Watching for changes...");
                    changes.wait();
                    changes.settle(DEBOUNCE);
                    eprintln!("[cargo-telar] Change detected, restarting...");
                    break 'watch;
                }
                Ok(None) => {}
                Err(e) => eprintln!("[cargo-telar] error: {e}"),
            }

            if changes.drain() {
                last_event = Instant::now();
                pending_restart = true;
            }

            if pending_restart && last_event.elapsed() >= DEBOUNCE {
                eprintln!("[cargo-telar] Change detected, restarting...");
                child.kill().ok();
                child.wait().ok();
                break 'watch;
            }

            if changes.changed_within(POLL) {
                last_event = Instant::now();
                pending_restart = true;
            }
        }
    }
}

pub(crate) enum HotMode {
    Dev,
    Preview,
}

impl HotMode {
    fn is_preview(&self) -> bool {
        matches!(self, HotMode::Preview)
    }

    /// What the loop builds with, on top of the frontend `select_frontend` already named.
    ///
    /// No window among them: `telar/dev` used to carry `desktop-bare` so that a project which declared no frontend still got one, and that window then rode along into `--target tui` — a terminal session linking winit for a frame it never draws. The frontend is the target's business now, and a build that names none reaches the same "no frontend to run on" panic `cargo telar build` has always produced.
    fn features(&self) -> &'static [&'static str] {
        match self {
            HotMode::Dev => &["telar/dev"],
            // `telar/preview` names `desktop-bare` itself: the preview host is a window by definition, and a `--target tui` preview still goes through it to reach the entry point that renders the blocks.
            HotMode::Preview => &["telar/preview", "telar/dev"],
        }
    }

    /// What the dylib half of the loop adds. Named on the build rather than pushed through `RUSTFLAGS` as a `--cfg`: rustflags are hashed into every unit in the graph, so the old spelling recompiled all four hundred dependencies on each switch between this loop and `cargo telar check`, to change three crates. Cargo tracks a feature just as well and scopes it to the crates that enable it.
    fn hot_features(&self) -> &'static [&'static str] {
        match self {
            HotMode::Dev => &["telar/dev", "telar/hot-reload"],
            HotMode::Preview => &["telar/preview", "telar/dev", "telar/hot-reload"],
        }
    }

    /// What a browser build of the loop adds to the page's frontend: [`Self::features`] short of `telar/dev`, the dylib host, which a page cannot load and has no use for, since a rebuild reloads the whole page.
    fn web_features(&self) -> &'static [&'static str] {
        match self {
            HotMode::Dev => &[],
            HotMode::Preview => &["telar/preview"],
        }
    }
}

/// The package's own feature for the devtools overlay, qualified so it reaches the package from a workspace root too. `None` when the session turned the overlay off, or when the package declares no `telar-devtools` to turn on, which is said with the command that adds it.
fn devtools_feature(resolved: &ResolvedPackage, config: &TelarSection) -> Option<String> {
    if config.dev.devtools == Some(false) {
        return None;
    }
    let package = resolved.name();
    if !telar_project::declares_optional_dependency(&resolved.package_dir, DEVTOOLS_PACKAGE) {
        eprintln!("[cargo-telar] {}", missing_devtools_note(&package));
        return None;
    }
    Some(format!("{package}/{DEVTOOLS_PACKAGE}"))
}

/// The package's own feature for the workshop `cargo telar preview` shows the previews in. `None` in a dev session, and when the package declares no `telar-workshop`, which is said with the command that adds it: the previews then open in the plain page.
fn workshop_feature(mode: &HotMode, resolved: &ResolvedPackage) -> Option<String> {
    if !mode.is_preview() {
        return None;
    }
    let package = resolved.name();
    if !telar_project::declares_optional_dependency(&resolved.package_dir, WORKSHOP_PACKAGE) {
        eprintln!("[cargo-telar] {}", missing_workshop_note(&package));
        return None;
    }
    Some(format!("{package}/{WORKSHOP_PACKAGE}"))
}

fn with_tooling<'a>(features: &[&'a str], tooling: &[&'a str]) -> Vec<&'a str> {
    features.iter().chain(tooling).copied().collect()
}

/// `args` with what a browser build of `mode` turns on: [`HotMode::web_features`] and, for a preview, the package's workshop. Not its devtools: a page draws no overlay, and the workshop brings the panels it reuses itself.
pub(crate) fn with_web_features(mode: &HotMode, mut args: Vec<String>) -> Vec<String> {
    warn_if_tooling_unlocked(&args, mode.web_features());
    let workshop = workshop_feature(mode, &resolve_package(&args));
    for feature in mode
        .web_features()
        .iter()
        .copied()
        .chain(workshop.as_deref())
    {
        inject_feature(&mut args, feature);
    }
    args
}

pub(crate) struct HotLoopOpts {
    pub(crate) args: Vec<String>,
    /// What the app binary itself is started with, after cargo's own arguments.
    pub(crate) app_args: Vec<String>,
    pub(crate) config: TelarSection,
    pub(crate) no_hot_reload: bool,
    /// The channel the session's workshop takes requests on, opened before anything was built. Handed requests on once the workshop runs under hot reload, and closed in a session that does not.
    pub(crate) workshop: Option<WorkshopChannel>,
}

/// `cargo run`'s arguments with `app_args` handed on to the binary.
fn with_app_args(mut cargo_args: Vec<String>, app_args: &[String]) -> Vec<String> {
    if app_args.is_empty() {
        return cargo_args;
    }
    if !cargo_args.iter().any(|arg| arg == "--") {
        cargo_args.push("--".to_string());
    }
    cargo_args.extend(app_args.iter().cloned());
    cargo_args
}

pub(crate) fn run_hot_loop(mode: HotMode, opts: HotLoopOpts) -> ! {
    let HotLoopOpts {
        args,
        app_args,
        config,
        no_hot_reload,
        workshop,
    } = opts;

    let (android, rest) = split_android_flag(args);
    let resolved = resolve_package(&rest);
    let tooling_features: Vec<String> = devtools_feature(&resolved, &config)
        .into_iter()
        .chain(workshop_feature(&mode, &resolved))
        .collect();
    let tooling: Vec<&str> = tooling_features.iter().map(String::as_str).collect();
    let features = with_tooling(mode.features(), &tooling);
    let backend_value = backend_as_str(config.backend.unwrap_or_default());
    let is_preview = mode.is_preview();

    if android {
        warn_if_tooling_unlocked(&rest, mode.features());
        // `cargo apk run --lib` crashes on UID parsing when launching; work around by doing build → adb install → adb shell am start manually.
        let mut build_args = vec!["apk".to_string(), "build".to_string(), "--lib".to_string()];
        build_args.extend(rest.iter().cloned());
        for feature in &features {
            inject_feature(&mut build_args, feature);
        }

        let status = make_android_cmd(build_args, config)
            .status()
            .expect("[cargo-telar] failed to invoke cargo");
        if !status.success() {
            std::process::exit(status.code().unwrap_or(1));
        }

        android_install_and_launch(&rest);
        std::process::exit(0);
    }

    let mut launch_envs = vec![(
        "TELAR_RENDERER_BACKEND".to_string(),
        backend_value.to_string(),
    )];
    if is_preview {
        launch_envs.push(("TELAR_PREVIEW".to_string(), "1".to_string()));
        launch_envs.push((
            WORKSPACE_DIR_VAR.to_string(),
            resolved.workspace_root.display().to_string(),
        ));
        launch_envs.push((
            TEST_TARGET_DIR_VAR.to_string(),
            telar_project::find_target_dir(&resolved.workspace_root)
                .display()
                .to_string(),
        ));
    }
    if config.dev.devtools == Some(false) {
        launch_envs.push(("TELAR_DEVTOOLS".to_string(), "0".to_string()));
    }
    if let Some(window) = &config.dev.window {
        apply_dev_window_env(&mut launch_envs, window);
    }

    let mut cargo_args = vec!["run".to_string()];
    cargo_args.extend(rest.clone());
    for feature in &features {
        inject_feature(&mut cargo_args, feature);
    }

    let workspace_root = resolved.workspace_root.clone();
    let profile = profile_of(&rest);

    // Gated on the manifest rather than the built artifact: the dylib build differs in both RUSTFLAGS and generated sources, so running it for a package that can never produce one compiles the crate graph twice per `cargo telar dev` and leaves each half stale.
    let hot_reload = !no_hot_reload && resolved.produces_cdylib;
    if !no_hot_reload && !hot_reload {
        eprintln!(
            "[cargo-telar] Hot reload off: `{}` declares no `[lib] crate-type = [\"cdylib\", ..]`. Restarting the process on change instead.",
            resolved.name()
        );
    }
    warn_if_tooling_unlocked(
        &rest,
        if hot_reload {
            mode.hot_features()
        } else {
            mode.features()
        },
    );

    let watched = match WatchSet::collect(&workspace_root) {
        Ok(watched) => watched,
        Err(e) => {
            eprintln!("[cargo-telar] error: {e}");
            drop(workshop);
            std::process::exit(1);
        }
    };
    let changes = match ChangeFeed::start(watched) {
        Ok(changes) => changes,
        Err(e) => {
            eprintln!("[cargo-telar] could not watch for changes: {e}");
            drop(workshop);
            std::process::exit(1);
        }
    };

    if hot_reload {
        let hot_features = with_tooling(mode.hot_features(), &tooling);
        let package_name = resolved.name();
        let lib_path = package_lib_path(&workspace_root, &package_name, profile);
        let bin_path = package_bin_path(&workspace_root, &package_name, profile);

        let mut build_args = vec!["build".to_string()];
        build_args.extend(rest.clone());
        for feature in &hot_features {
            inject_feature(&mut build_args, feature);
        }
        with_json_messages(&mut build_args);
        let build_app = || {
            let mut build_cmd = Command::new("cargo");
            build_cmd
                .args(&build_args)
                // `telar`'s backend is resolved with `option_env!` and so is a tracked build input: omitting it would compile a different backend than the hot rebuilds.
                .env("TELAR_RENDERER_BACKEND", backend_value);
            let (succeeded, report) = build_with_diagnostics(&mut build_cmd);
            if !report.is_empty() {
                eprintln!();
                eprint!("{}", report.render(true));
            }
            succeeded
        };
        eprintln!("[cargo-telar] Building...");
        if !build_app() {
            eprintln!("[cargo-telar] Initial build failed. Watching for changes...");
            rebuild_until_built(&changes, || {
                super::bake::bake_workspace();
                super::transpile::transpile_workspace();
                build_app()
            });
        }

        if bin_path.exists() && lib_path.exists() {
            let lib_build_args = make_lib_build_args(&rest, &hot_features);
            watch_and_hot_reload(
                lib_build_args,
                HotLaunch {
                    bin_path,
                    lib_path,
                    args: app_args,
                    envs: launch_envs,
                },
                HotChannel::bind(),
                workshop,
                changes,
            );
        }
        eprintln!(
            "[cargo-telar] Hot reload off: the build succeeded but left no {} or no {} to load. Restarting the process on change instead.",
            bin_path.display(),
            lib_path.display()
        );
    }

    // An app restarted on every change has no channel to hand a request on, so a later invocation is left to open a workshop of its own.
    drop(workshop);
    watch_and_run(with_app_args(cargo_args, &app_args), launch_envs, changes);
}

/// Rebuilds on every change until `rebuild` succeeds, so a session whose first build failed starts the app with hot reload once the error is fixed, instead of falling back to restarts for good.
fn rebuild_until_built(changes: &ChangeFeed<impl WatchScope>, mut rebuild: impl FnMut() -> bool) {
    loop {
        changes.wait();
        changes.settle(DEBOUNCE);
        eprintln!("[cargo-telar] Change detected, rebuilding...");
        if rebuild() {
            return;
        }
        eprintln!("[cargo-telar] Build failed, waiting for changes...");
    }
}

#[cfg(test)]
#[path = "watch_test.rs"]
mod tests;
