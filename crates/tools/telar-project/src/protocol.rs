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
