//! Opening what lives outside the app: a web page, a mail draft, a document another program owns.

use std::sync::{Arc, RwLock};

/// Hands a URI to whatever the system opens it with.
///
/// `Send + Sync` for the same reason [`Clipboard`](crate::Clipboard) is: a backend may do the opening on a thread of its own, since asking a desktop portal or a launcher is a round trip the UI should not wait on.
pub trait UriOpener: Send + Sync + 'static {
    /// Opens `uri`, an absolute URI. `false` when nothing could be asked to open it.
    fn open(&self, uri: &str) -> bool;

    /// Opens `reference`, an address in the app's own `LocationFormat`, as a second view of the app beside this one. `false` where there is no second view to open, which is every target but a browser.
    fn open_beside(&self, reference: &str) -> bool {
        let _ = reference;
        false
    }
}

static OPENER: RwLock<Option<Arc<dyn UriOpener>>> = RwLock::new(None);

/// Installs the backend URIs are opened with. Each runner installs its platform's at startup; a later call replaces it, which is how a test observes what would have been opened.
pub fn set_uri_opener(opener: Arc<dyn UriOpener>) {
    *OPENER.write().unwrap_or_else(|e| e.into_inner()) = Some(opener);
}

/// The installed backend, or `None` where nothing opens URIs — headless.
pub fn uri_opener() -> Option<Arc<dyn UriOpener>> {
    OPENER.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Opens `uri` with the system, reporting whether anything was asked to.
pub fn open_uri(uri: &str) -> bool {
    let opened = uri_opener().is_some_and(|opener| opener.open(uri));
    if !opened {
        tracing::warn!(uri, "nothing opened the URI");
    }
    opened
}

/// Opens the app's own `reference` in a view beside this one, reporting whether there was one to open. See [`UriOpener::open_beside`].
pub fn open_beside(reference: &str) -> bool {
    uri_opener().is_some_and(|opener| opener.open_beside(reference))
}

/// Opens URIs by starting the system's own launcher: `xdg-open` on Linux and the BSDs, `open` on macOS, the URL protocol handler on Windows.
///
/// The URI is one argument to the launcher and never passes through a shell. The launcher runs detached, and is reaped on a thread of its own so a finished one leaves no zombie behind.
#[cfg(feature = "system-opener")]
pub struct SystemOpener;

#[cfg(feature = "system-opener")]
impl UriOpener for SystemOpener {
    fn open(&self, uri: &str) -> bool {
        launch(uri)
            .inspect_err(|error| tracing::warn!(uri, %error, "the system launcher did not start"))
            .is_ok()
    }
}

/// Starts the system launcher on `uri`, for a backend whose own way of opening one failed.
#[cfg(feature = "system-opener")]
pub fn launch(uri: &str) -> std::io::Result<()> {
    let mut command = launcher(uri);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = command.spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(all(feature = "system-opener", target_os = "windows"))]
fn launcher(uri: &str) -> std::process::Command {
    let mut command = std::process::Command::new("rundll32");
    command.arg("url.dll,FileProtocolHandler").arg(uri);
    command
}

#[cfg(all(feature = "system-opener", target_os = "macos"))]
fn launcher(uri: &str) -> std::process::Command {
    let mut command = std::process::Command::new("open");
    command.arg(uri);
    command
}

#[cfg(all(
    feature = "system-opener",
    not(any(target_os = "windows", target_os = "macos"))
))]
fn launcher(uri: &str) -> std::process::Command {
    let mut command = std::process::Command::new("xdg-open");
    command.arg(uri);
    command
}

#[cfg(test)]
#[path = "uri_test.rs"]
mod tests;
