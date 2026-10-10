//! The names `cargo telar` and the app it launches agree on: the environment variables it sets for a preview host, and the prefixes of the lines it sends over the hot-reload channel.

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
