//! The names `cargo telar` and the app it launches agree on: the environment variables it sets for a preview host, the lines it sends over the hot-reload channel, and where a running workshop can be reached.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::PageLocation;

/// The env var naming the preview to open first.
pub const PREVIEW_ID_VAR: &str = "TELAR_PREVIEW_ID";

/// The env var `cargo telar preview --component` sets: the component a host opens on, and what a workshop narrows its list to.
pub const PREVIEW_COMPONENT_VAR: &str = "TELAR_PREVIEW_COMPONENT";

/// The env var `cargo telar preview` sets to the workspace it runs from, where a workshop keeps what it remembers between runs.
pub const WORKSPACE_DIR_VAR: &str = "TELAR_WORKSPACE_DIR";

/// Starts a hot-reload channel line carrying the path of a freshly built dylib.
pub const HOT_RELOAD_PREFIX: &str = "hot:";

/// Starts a hot-reload channel line carrying a failed build's message, with its line breaks escaped.
pub const BUILD_ERROR_PREFIX: &str = "err:";

/// Starts a hot-reload channel line carrying a location for the running app to open.
pub const GOTO_PREFIX: &str = "goto:";

/// A whole hot-reload channel line, and a workshop channel request, asking the running app to bring its window forward where it is.
pub const FOCUS_LINE: &str = "focus";

/// The env var `cargo telar test` sets on the app it runs, which makes it test its previews instead of starting.
pub const TEST_VAR: &str = "TELAR_TEST";

/// The env var narrowing a test run to the previews, or matrix cells, whose id matches this glob (`*` and `?`).
pub const TEST_PREVIEW_VAR: &str = "TELAR_TEST_PREVIEW";

/// The env var saying what a test run does with accessibility findings: `off`, `warn` or `error`.
pub const TEST_A11Y_VAR: &str = "TELAR_TEST_A11Y";

/// The env var saying what a test run does with snapshots: `compare`, `update` or `off`.
pub const TEST_SNAPSHOTS_VAR: &str = "TELAR_TEST_SNAPSHOTS";

/// The env var naming the directory of the package under test, whose `previews/__snapshots__` holds the goldens.
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

/// The name of the test `rsx_modules!` emits in a library's preview build, which `cargo telar test -p <library>` runs.
pub const LIBRARY_TEST_NAME: &str = "telar_previews";

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
    let manifest = crate::cargo_manifest::read_manifest(package_dir)?;
    let package = manifest.get("package")?.get("name")?.as_str()?;
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

/// `title` in the one form that names it: its [`title_path`] joined by `/`.
pub fn canonical_title(title: &str) -> String {
    title_path(title).collect::<Vec<_>>().join("/")
}

/// Every page a workshop showing `previews` stands on below its root, in the order the previews are listed: each preview's canvas and matrix, and the docs of each title.
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
        let docs = page_at(std::iter::once("docs").chain(title_path(&preview.title)));
        if docs.segments.len() > 1 {
            add(docs);
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
#[path = "protocol_test.rs"]
mod tests;
