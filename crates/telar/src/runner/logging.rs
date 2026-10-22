//! The `tracing` subscriber an app gets when it installed none, since this crate reports failures only through `tracing`; a terminal app writes to a file because its stderr is the frame being drawn.

#[cfg(any(feature = "tui", test))]
use std::ffi::OsString;
#[cfg(any(feature = "tui", test))]
use std::fs::File;
#[cfg(all(
    feature = "desktop-bare",
    not(target_os = "android"),
    not(target_arch = "wasm32")
))]
use std::io::IsTerminal;
#[cfg(any(feature = "tui", test))]
use std::path::{Path, PathBuf};
#[cfg(feature = "tui")]
use std::sync::Mutex;

#[cfg(any(feature = "tui", test))]
use services_core::AppPathsProvider;
use tracing_subscriber::EnvFilter;

const FILTER_VAR: &str = "TELAR_LOG";
#[cfg(feature = "tui")]
const FILE_VAR: &str = "TELAR_LOG_FILE";
const DEFAULT_DIRECTIVE: &str = "warn";

fn filter_from(directive: Option<&str>) -> EnvFilter {
    let Some(directive) = directive.filter(|d| !d.is_empty()) else {
        return EnvFilter::new(DEFAULT_DIRECTIVE);
    };
    EnvFilter::try_new(directive).unwrap_or_else(|e| {
        eprintln!("telar: ignoring invalid {FILTER_VAR}={directive:?}: {e}");
        EnvFilter::new(DEFAULT_DIRECTIVE)
    })
}

fn env_filter() -> EnvFilter {
    filter_from(std::env::var(FILTER_VAR).ok().as_deref())
}

#[cfg(any(feature = "tui", test))]
fn log_file_path(
    overridden: Option<OsString>,
    paths: &dyn AppPathsProvider,
    app_name: &str,
) -> Option<PathBuf> {
    match overridden.filter(|p| !p.is_empty()) {
        Some(path) => Some(PathBuf::from(path)),
        None => Some(
            paths
                .state_dir()?
                .join(app_name)
                .join(format!("{app_name}.log")),
        ),
    }
}

#[cfg(any(feature = "tui", test))]
fn open_log_file(path: &Path) -> std::io::Result<File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    File::options().create(true).append(true).open(path)
}

#[cfg(all(
    feature = "desktop-bare",
    not(target_os = "android"),
    not(target_arch = "wasm32")
))]
pub(super) fn install_stderr() {
    if tracing::dispatcher::has_been_set() {
        return;
    }
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter())
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .try_init();
}

#[cfg(feature = "tui")]
pub(super) fn install_file(paths: &dyn AppPathsProvider, app_name: &str) {
    if tracing::dispatcher::has_been_set() {
        return;
    }
    let Some(path) = log_file_path(std::env::var_os(FILE_VAR), paths, app_name) else {
        eprintln!("telar: no state directory to write the log to; set {FILE_VAR} to keep one");
        return;
    };
    let file = match open_log_file(&path) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("telar: cannot open log file {}: {e}", path.display());
            return;
        }
    };
    let installed = tracing_subscriber::fmt()
        .with_env_filter(env_filter())
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .try_init()
        .is_ok();
    if installed {
        install_panic_report(path);
    }
}

#[cfg(feature = "tui")]
fn install_panic_report(path: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("panic: {info}");
        previous(info);
        eprintln!("telar: the log is at {}", path.display());
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StateAt(Option<PathBuf>);

    impl AppPathsProvider for StateAt {
        fn config_dir(&self) -> Option<PathBuf> {
            None
        }
        fn data_dir(&self) -> Option<PathBuf> {
            None
        }
        fn cache_dir(&self) -> Option<PathBuf> {
            None
        }
        fn state_dir(&self) -> Option<PathBuf> {
            self.0.clone()
        }
    }

    #[test]
    fn log_file_lives_under_the_state_dir_by_app_name() {
        let paths = StateAt(Some(PathBuf::from("/state")));
        assert_eq!(
            log_file_path(None, &paths, "notes"),
            Some(PathBuf::from("/state/notes/notes.log"))
        );
    }

    #[test]
    fn log_file_override_wins_and_empty_is_ignored() {
        let paths = StateAt(Some(PathBuf::from("/state")));
        assert_eq!(
            log_file_path(Some("/tmp/x.log".into()), &paths, "notes"),
            Some(PathBuf::from("/tmp/x.log"))
        );
        assert_eq!(
            log_file_path(Some(OsString::new()), &paths, "notes"),
            Some(PathBuf::from("/state/notes/notes.log"))
        );
    }

    #[test]
    fn log_file_override_needs_no_state_dir() {
        assert_eq!(
            log_file_path(Some("/tmp/x.log".into()), &StateAt(None), "notes"),
            Some(PathBuf::from("/tmp/x.log"))
        );
        assert_eq!(log_file_path(None, &StateAt(None), "notes"), None);
    }

    #[test]
    fn opening_creates_missing_parents_and_appends() {
        let dir = std::env::temp_dir().join(format!("telar-log-test-{}", std::process::id()));
        let path = dir.join("a").join("b.log");
        {
            use std::io::Write;
            writeln!(open_log_file(&path).unwrap(), "one").unwrap();
            writeln!(open_log_file(&path).unwrap(), "two").unwrap();
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\ntwo\n");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn filter_defaults_to_warn_and_survives_a_bad_directive() {
        assert_eq!(filter_from(None).to_string(), "warn");
        assert_eq!(filter_from(Some("")).to_string(), "warn");
        assert_eq!(filter_from(Some("telar=debug")).to_string(), "telar=debug");
        assert_eq!(filter_from(Some("=[bad")).to_string(), "warn");
    }
}
