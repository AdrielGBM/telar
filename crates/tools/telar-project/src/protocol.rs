//! The names `cargo telar` and the app it launches agree on: the environment variables it sets and how each value is spelled in them, the lines it sends over the hot-reload channel, and where a running workshop can be reached.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{PageLocation, WindowSection};

/// A value no spelling of its kind matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownValue {
    pub value: String,
    /// The values that are accepted, as a sentence names them.
    pub expected: String,
}

impl fmt::Display for UnknownValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown value `{}`, expected {}",
            self.value, self.expected
        )
    }
}

impl std::error::Error for UnknownValue {}

/// The one of `all` that `as_str` spells as `word`.
pub(crate) fn parse_word<T: Copy>(
    all: &[T],
    as_str: fn(T) -> &'static str,
    word: &str,
) -> Result<T, UnknownValue> {
    all.iter()
        .copied()
        .find(|value| as_str(*value) == word)
        .ok_or_else(|| UnknownValue {
            value: word.to_string(),
            expected: one_of(&all.iter().map(|value| as_str(*value)).collect::<Vec<_>>()),
        })
}

fn one_of(words: &[&str]) -> String {
    let quoted: Vec<String> = words.iter().map(|word| format!("`{word}`")).collect();
    match quoted.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} or {last}", rest.join(", ")),
        _ => quoted.concat(),
    }
}

/// Which renderer the built application selects at startup: what `telar.toml`'s `backend` says, what [`RENDERER_BACKEND_VAR`] carries into the build, and what the preference store keeps a person's switch as.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(try_from = "String")]
pub enum RendererBackend {
    /// Hardware, falling back to software where there is no adapter.
    #[default]
    Auto,
    Hardware,
    Software,
}

impl RendererBackend {
    pub const ALL: [Self; 3] = [Self::Auto, Self::Hardware, Self::Software];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Hardware => "hardware",
            Self::Software => "software",
        }
    }

    pub fn parse(word: &str) -> Result<Self, UnknownValue> {
        parse_word(&Self::ALL, Self::as_str, word)
    }
}

impl TryFrom<String> for RendererBackend {
    type Error = UnknownValue;

    fn try_from(word: String) -> Result<Self, Self::Error> {
        Self::parse(&word)
    }
}

/// Whether the dev window covers the screen.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(try_from = "String")]
pub enum FullscreenMode {
    Disabled,
    Borderless,
    Exclusive,
}

impl FullscreenMode {
    pub const ALL: [Self; 3] = [Self::Disabled, Self::Borderless, Self::Exclusive];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Borderless => "borderless",
            Self::Exclusive => "exclusive",
        }
    }

    pub fn parse(word: &str) -> Result<Self, UnknownValue> {
        parse_word(&Self::ALL, Self::as_str, word)
    }
}

impl TryFrom<String> for FullscreenMode {
    type Error = UnknownValue;

    fn try_from(word: String) -> Result<Self, Self::Error> {
        Self::parse(&word)
    }
}

/// Where the dev window opens: `"centered"`, or `"<x>,<y>"` in screen coordinates.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(try_from = "String")]
pub enum WindowPosition {
    Centered,
    At { x: i32, y: i32 },
}

impl WindowPosition {
    const CENTERED: &str = "centered";

    pub fn encode(self) -> String {
        match self {
            Self::Centered => Self::CENTERED.to_string(),
            Self::At { x, y } => format!("{x},{y}"),
        }
    }

    pub fn parse(text: &str) -> Result<Self, UnknownValue> {
        if text == Self::CENTERED {
            return Ok(Self::Centered);
        }
        text.split_once(',')
            .and_then(|(x, y)| {
                Some(Self::At {
                    x: x.trim().parse().ok()?,
                    y: y.trim().parse().ok()?,
                })
            })
            .ok_or_else(|| UnknownValue {
                value: text.to_string(),
                expected: format!("`{}` or `<x>,<y>`", Self::CENTERED),
            })
    }
}

impl TryFrom<String> for WindowPosition {
    type Error = UnknownValue;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::parse(&text)
    }
}

/// The name of [`RENDERER_BACKEND_VAR`] as a literal, which is all `option_env!` takes.
#[macro_export]
macro_rules! renderer_backend_var {
    () => {
        "TELAR_RENDERER_BACKEND"
    };
}

/// The env var every build `cargo telar` runs carries the configured [`RendererBackend`] in, which the runtime reads at compile time.
pub const RENDERER_BACKEND_VAR: &str = renderer_backend_var!();

/// The env var naming the dylib a `cargo telar dev` host loads the app from.
pub const HOT_LIB_VAR: &str = "TELAR_HOT_LIB";

/// The env var naming the loopback port of the hot-reload channel.
pub const HOT_PORT_VAR: &str = "TELAR_HOT_PORT";

/// The env var whose presence makes a preview build show its previews instead of starting the app.
pub const PREVIEW_VAR: &str = "TELAR_PREVIEW";

/// The env var naming the directory `cargo telar preview --png` has the app render every preview into.
pub const PREVIEW_PNG_VAR: &str = "TELAR_PREVIEW_PNG";

/// The env var turning the devtools overlay off when it is [`encode_flag`]`(false)`.
pub const DEVTOOLS_VAR: &str = "TELAR_DEVTOOLS";

/// The env var naming the frontend a build carrying several runs; [`TERMINAL_TARGET`] is the one a binary is asked for by name.
pub const TARGET_VAR: &str = "TELAR_TARGET";

pub const TERMINAL_TARGET: &str = "tui";

pub const DEV_WINDOW_TITLE_VAR: &str = "TELAR_DEV_WINDOW_TITLE";
pub const DEV_WINDOW_WIDTH_VAR: &str = "TELAR_DEV_WINDOW_WIDTH";
pub const DEV_WINDOW_HEIGHT_VAR: &str = "TELAR_DEV_WINDOW_HEIGHT";
pub const DEV_WINDOW_DECORATIONS_VAR: &str = "TELAR_DEV_WINDOW_DECORATIONS";
pub const DEV_WINDOW_RESIZABLE_VAR: &str = "TELAR_DEV_WINDOW_RESIZABLE";
pub const DEV_WINDOW_TRANSPARENT_VAR: &str = "TELAR_DEV_WINDOW_TRANSPARENT";
pub const DEV_WINDOW_FULLSCREEN_VAR: &str = "TELAR_DEV_WINDOW_FULLSCREEN";
pub const DEV_WINDOW_POSITION_VAR: &str = "TELAR_DEV_WINDOW_POSITION";

const FLAG_ON: &str = "1";
const FLAG_OFF: &str = "0";

pub fn encode_flag(on: bool) -> &'static str {
    if on { FLAG_ON } else { FLAG_OFF }
}

pub fn decode_flag(text: &str) -> Result<bool, UnknownValue> {
    match text {
        FLAG_ON => Ok(true),
        FLAG_OFF => Ok(false),
        _ => Err(UnknownValue {
            value: text.to_string(),
            expected: one_of(&[FLAG_ON, FLAG_OFF]),
        }),
    }
}

fn decode_pixels(text: &str) -> Result<u32, UnknownValue> {
    text.parse().map_err(|_| UnknownValue {
        value: text.to_string(),
        expected: "a whole number of pixels".to_string(),
    })
}

/// An env var `cargo telar` sets whose value the app could not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvError {
    pub var: &'static str,
    pub error: UnknownValue,
}

impl fmt::Display for EnvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.var, self.error)
    }
}

impl std::error::Error for EnvError {}

/// The env vars `[telar.dev.window]` reaches a dev session's app through, one for each key it sets.
pub fn dev_window_env(window: &WindowSection) -> Vec<(&'static str, String)> {
    let WindowSection {
        title,
        width,
        height,
        decorations,
        resizable,
        transparent,
        fullscreen,
        position,
    } = window;
    let flag = |on: &Option<bool>| on.map(|on| encode_flag(on).to_string());
    [
        (DEV_WINDOW_TITLE_VAR, title.clone()),
        (DEV_WINDOW_WIDTH_VAR, width.map(|n| n.to_string())),
        (DEV_WINDOW_HEIGHT_VAR, height.map(|n| n.to_string())),
        (DEV_WINDOW_DECORATIONS_VAR, flag(decorations)),
        (DEV_WINDOW_RESIZABLE_VAR, flag(resizable)),
        (DEV_WINDOW_TRANSPARENT_VAR, flag(transparent)),
        (
            DEV_WINDOW_FULLSCREEN_VAR,
            fullscreen.map(|mode| mode.as_str().to_string()),
        ),
        (
            DEV_WINDOW_POSITION_VAR,
            position.map(WindowPosition::encode),
        ),
    ]
    .into_iter()
    .filter_map(|(var, value)| Some((var, value?)))
    .collect()
}

/// The `[telar.dev.window]` [`dev_window_env`] wrote, read through `var`.
pub fn dev_window_from_env(
    var: impl Fn(&str) -> Option<String>,
) -> Result<WindowSection, EnvError> {
    Ok(WindowSection {
        title: var(DEV_WINDOW_TITLE_VAR),
        width: decoded(&var, DEV_WINDOW_WIDTH_VAR, decode_pixels)?,
        height: decoded(&var, DEV_WINDOW_HEIGHT_VAR, decode_pixels)?,
        decorations: decoded(&var, DEV_WINDOW_DECORATIONS_VAR, decode_flag)?,
        resizable: decoded(&var, DEV_WINDOW_RESIZABLE_VAR, decode_flag)?,
        transparent: decoded(&var, DEV_WINDOW_TRANSPARENT_VAR, decode_flag)?,
        fullscreen: decoded(&var, DEV_WINDOW_FULLSCREEN_VAR, FullscreenMode::parse)?,
        position: decoded(&var, DEV_WINDOW_POSITION_VAR, WindowPosition::parse)?,
    })
}

fn decoded<T>(
    var: &impl Fn(&str) -> Option<String>,
    name: &'static str,
    decode: fn(&str) -> Result<T, UnknownValue>,
) -> Result<Option<T>, EnvError> {
    var(name)
        .map(|text| decode(&text).map_err(|error| EnvError { var: name, error }))
        .transpose()
}

/// The env var naming the preview to open first.
pub const PREVIEW_ID_VAR: &str = "TELAR_PREVIEW_ID";

/// The env var `cargo telar preview --component` sets: the component a host opens on, and what a workshop narrows its list to.
pub const PREVIEW_COMPONENT_VAR: &str = "TELAR_PREVIEW_COMPONENT";

/// The env var `cargo telar preview` sets to the workspace it runs from, where a workshop keeps what it remembers between runs.
pub const WORKSPACE_DIR_VAR: &str = "TELAR_WORKSPACE_DIR";

/// The env var `cargo telar dev` hands the app the hot-reload channel's secret in. The app sends it as the channel's first line, and a connection that does not is never adopted, so no other local process can drive the app.
pub const HOT_TOKEN_VAR: &str = "TELAR_HOT_TOKEN";

/// Starts a hot-reload channel line carrying the path of a freshly built dylib.
pub const HOT_RELOAD_PREFIX: &str = "hot:";

/// Starts a hot-reload channel line carrying a failed build's message, with its line breaks escaped.
pub const BUILD_ERROR_PREFIX: &str = "err:";

/// Starts a hot-reload channel line carrying a location for the running app to open.
pub const GOTO_PREFIX: &str = "goto:";

/// A whole hot-reload channel line, and a workshop channel request, asking the running app to bring its window forward where it is.
pub const FOCUS_LINE: &str = "focus";

/// The line the app sends right after its token when it answers each [`HOT_RELOAD_PREFIX`] line with one [`HotReply`]; a `cargo telar` that does not know this version treats the app as one that does not answer.
pub const HOT_REPLIES_LINE: &str = "replies:3";

pub const HOT_LOADED_PREFIX: &str = "loaded:";

pub const HOT_PANICKED_PREFIX: &str = "panicked:";

pub const HOT_REFUSED_PREFIX: &str = "refused:";

pub const HOT_UNREADABLE_PREFIX: &str = "unreadable:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotReply {
    /// The path exactly as the reload line named it, so `cargo telar` can tell which reload this answers.
    pub library: String,
    pub outcome: HotOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotOutcome {
    Loaded,
    /// The panic's report: nothing of the app runs until a later build loads.
    Panicked(String),
    /// Why, while the app keeps running the build before.
    Refused(String),
    /// What the host could not read of the build's answers: built from sources the host was not, nothing of it runs until the session restarts.
    Unreadable(String),
}

impl HotReply {
    /// The prefix, then the library and any report as [`escape_lines`] fields split by a tab, which escaping leaves in neither.
    pub fn line(&self) -> String {
        let library = escape_lines(&self.library);
        match &self.outcome {
            HotOutcome::Loaded => format!("{HOT_LOADED_PREFIX}{library}"),
            HotOutcome::Panicked(report) => {
                format!("{HOT_PANICKED_PREFIX}{library}\t{}", escape_lines(report))
            }
            HotOutcome::Refused(reason) => {
                format!("{HOT_REFUSED_PREFIX}{library}\t{}", escape_lines(reason))
            }
            HotOutcome::Unreadable(report) => {
                format!("{HOT_UNREADABLE_PREFIX}{library}\t{}", escape_lines(report))
            }
        }
    }

    pub fn parse(line: &str) -> Option<Self> {
        let (library, outcome) = if let Some(library) = line.strip_prefix(HOT_LOADED_PREFIX) {
            (library, HotOutcome::Loaded)
        } else if let Some(fields) = line.strip_prefix(HOT_PANICKED_PREFIX) {
            let (library, report) = fields.split_once('\t')?;
            (library, HotOutcome::Panicked(unescape_lines(report)))
        } else if let Some(fields) = line.strip_prefix(HOT_UNREADABLE_PREFIX) {
            let (library, report) = fields.split_once('\t')?;
            (library, HotOutcome::Unreadable(unescape_lines(report)))
        } else {
            let (library, reason) = line.strip_prefix(HOT_REFUSED_PREFIX)?.split_once('\t')?;
            (library, HotOutcome::Refused(unescape_lines(reason)))
        };
        (!library.contains('\t')).then(|| HotReply {
            library: unescape_lines(library),
            outcome,
        })
    }
}

/// `message` as one field of a channel line: backslashes doubled, line breaks and tabs written as `\n` and `\t`, carriage returns dropped. Escaped rather than replaced: a code frame is full of `|`, so a ` | ` separator would be cut apart at every gutter.
pub fn escape_lines(message: &str) -> String {
    // Backslashes first, or an escape already in the message would decode as a line break.
    message
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace('\r', "")
}

/// Undoes [`escape_lines`]. A trailing lone backslash cannot come from it and is passed through rather than dropped, so a malformed message is still shown.
pub fn unescape_lines(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut chars = message.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// The env var `cargo telar test` sets on the app it runs, which makes it test its previews instead of starting.
pub const TEST_VAR: &str = "TELAR_TEST";

/// The env var narrowing a test run to the previews, or matrix cells, whose id matches this glob (`*` and `?`).
pub const TEST_PREVIEW_VAR: &str = "TELAR_TEST_PREVIEW";

/// The env var saying what a test run does with accessibility findings: `off`, `warn` or `error`.
pub const TEST_A11Y_VAR: &str = "TELAR_TEST_A11Y";

/// The env var saying what a test run does with snapshots: `compare`, `update` or `off`.
pub const TEST_SNAPSHOTS_VAR: &str = "TELAR_TEST_SNAPSHOTS";

/// The env var naming the directory of the package under test, whose `snapshots/` holds the goldens.
pub const TEST_PACKAGE_DIR_VAR: &str = "TELAR_TEST_PACKAGE_DIR";

/// The env var naming the Cargo target directory, under which a test run writes `telar-snapshots/report.json` and its diffs.
pub const TEST_TARGET_DIR_VAR: &str = "TELAR_TEST_TARGET_DIR";

/// The env var with the per-channel snapshot tolerance `[telar.previews.snapshots]` sets, 0–255.
pub const TEST_TOLERANCE_CHANNEL_VAR: &str = "TELAR_TEST_TOLERANCE_CHANNEL";

/// The env var with how many pixels may differ in a snapshot that still matches, from `[telar.previews.snapshots]`.
pub const TEST_TOLERANCE_PIXELS_VAR: &str = "TELAR_TEST_TOLERANCE_PIXELS";

/// The directory under the Cargo target directory where a test run writes its report and what did not match.
pub const TEST_OUT_DIR: &str = "telar-snapshots";

/// The report a test run writes in [`TEST_OUT_DIR`].
pub const TEST_REPORT_FILE: &str = "report.json";

/// The name of the test `rsx_modules!` emits in a library's preview build, which `cargo telar test -p <library>` runs by this name alone, wherever the macro sits in the module tree: no other test should contain it.
pub const LIBRARY_TEST_NAME: &str = "__telar_library_previews";

/// Where the session running `package`'s workshop names the loopback port it takes `goto:` and [`FOCUS_LINE`] requests on.
pub fn workshop_channel_file(workspace_root: &Path, package: &str) -> PathBuf {
    workshop_dir(workspace_root).join(format!("{package}.channel"))
}

/// Where `cargo telar preview --target web` names the address it serves `package`'s workshop at, one line like `http://localhost:8080/`, which a location such as [`preview_location`] is appended to. While the session is still building it holds [`starting_server_line`] instead, so the editor can tell a session that is booting from one that died.
pub fn workshop_server_file(workspace_root: &Path, package: &str) -> PathBuf {
    workshop_dir(workspace_root).join(format!("{package}.server"))
}

/// Starts the line a session writes to its server file as it launches, before it serves anything.
pub const SERVER_STARTING_PREFIX: &str = "starting ";

/// What a server file says about the session that wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerFile {
    /// Still building, as the process `pid`.
    Starting { pid: u32 },
    /// Serving the workshop at this address.
    Serving(String),
}

/// The line, without its line break, that a session of process `pid` writes to its server file as it launches.
pub fn starting_server_line(pid: u32) -> String {
    format!("{SERVER_STARTING_PREFIX}{pid}")
}

/// What the text of a server file says, or `None` when it says neither.
pub fn parse_server_file(text: &str) -> Option<ServerFile> {
    let line = text.trim();
    if let Some(pid) = line.strip_prefix(SERVER_STARTING_PREFIX) {
        return pid
            .trim()
            .parse()
            .ok()
            .map(|pid| ServerFile::Starting { pid });
    }
    (!line.is_empty()).then(|| ServerFile::Serving(line.to_string()))
}

fn workshop_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".telar").join("workshop")
}

/// Where the workshops `cargo telar preview` runs for one package can be reached: the window's channel and the page's server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkshopFiles {
    pub channel: PathBuf,
    pub server: PathBuf,
}

/// The files the workshops of the package at `package_dir` are reached through. `None` when its manifest names no package.
pub fn package_workshop_files(package_dir: &Path) -> Option<WorkshopFiles> {
    let manifest = crate::CargoManifest::read(package_dir)?;
    let package = manifest.package_name()?;
    let workspace_root =
        crate::find_workspace_root(package_dir).unwrap_or_else(|| package_dir.to_path_buf());
    Some(WorkshopFiles {
        channel: workshop_channel_file(&workspace_root, package),
        server: workshop_server_file(&workspace_root, package),
    })
}

/// The location a workshop opens to show the preview `id`.
pub fn preview_location(id: &str) -> String {
    let mut location = String::from("/preview/");
    for byte in id.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'=' | b'+') {
            location.push(byte as char);
        } else {
            location.push_str(&format!("%{byte:02X}"));
        }
    }
    location
}

/// The env var asking the app to list its previews and exit instead of starting.
pub const PREVIEW_LIST_VAR: &str = "TELAR_PREVIEW_LIST";

/// The env var naming a file a listing writes as a JSON array of [`ListedPreview`], in place of printing one id per line.
pub const PREVIEW_LIST_FILE_VAR: &str = "TELAR_PREVIEW_LIST_FILE";

/// A preview as a listing written to [`PREVIEW_LIST_FILE_VAR`] names it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ListedPreview {
    pub id: String,
    /// The package the preview is written in. Empty when unknown.
    #[serde(default)]
    pub package: String,
    /// The `Group/Title` path the preview is listed under.
    pub title: String,
}

/// The groups a preview title is listed under, outermost first: it is cut at each `/`, each piece trimmed and the empty ones dropped. The sidebar, the docs address and the export all name a title by this path.
pub fn title_path(title: &str) -> impl Iterator<Item = &str> {
    title
        .split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
}

/// Where the docs page of the component `package` lists under `title` stands below `docs`: the package, unless it is unknown, then the title's [`title_path`]. Empty when the title is, which has no docs page.
pub fn docs_path<'a>(package: &'a str, title: &'a str) -> Vec<&'a str> {
    let title: Vec<&str> = title_path(title).collect();
    if title.is_empty() {
        return title;
    }
    (!package.is_empty())
        .then_some(package)
        .into_iter()
        .chain(title)
        .collect()
}

pub fn docs_key(package: &str, title: &str) -> String {
    docs_path(package, title).join("/")
}

/// Every page a workshop showing `previews` stands on below its root, in the order the previews are listed: each preview's canvas and matrix, and the docs of each component.
pub fn workshop_pages(previews: &[ListedPreview]) -> Vec<PageLocation> {
    let mut pages = Vec::new();
    let mut add = |page: PageLocation| {
        if !pages.contains(&page) {
            pages.push(page);
        }
    };
    for preview in previews {
        for kind in ["preview", "matrix"] {
            add(page_at([kind, preview.id.as_str()]));
        }
        let docs = docs_path(&preview.package, &preview.title);
        if !docs.is_empty() {
            add(page_at(std::iter::once("docs").chain(docs)));
        }
    }
    pages
}

fn page_at<'a>(segments: impl IntoIterator<Item = &'a str>) -> PageLocation {
    PageLocation {
        segments: segments.into_iter().map(str::to_string).collect(),
        locale: None,
    }
}

#[cfg(test)]
#[path = "protocol.test.rs"]
mod tests;
