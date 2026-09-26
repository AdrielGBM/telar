//! Opening a URI with the system: the freedesktop `OpenURI` portal, `ShellExecuteW` or `NSWorkspace`.

use std::sync::Arc;

use services_core::UriOpener;

/// The desktop's own way to open a URI, falling back to the system launcher where it is unavailable.
///
/// On Linux the portal is asked first, because it is what a sandboxed app (Flatpak, Snap) is allowed to use and what picks the handler the user chose; `xdg-open` answers when the portal is not running or the build has no D-Bus client. Windows hands the URI to `ShellExecuteW` and macOS to `NSWorkspace`, which is what a click on a link does in any native application there.
pub struct DesktopUriOpener;

impl DesktopUriOpener {
    /// Installs this as the app's URI opener.
    pub fn install() {
        services_core::set_uri_opener(Arc::new(DesktopUriOpener));
    }
}

impl UriOpener for DesktopUriOpener {
    fn open(&self, uri: &str) -> bool {
        open(uri)
    }
}

#[cfg(all(target_os = "linux", feature = "system-theme"))]
fn open(uri: &str) -> bool {
    let uri = uri.to_string();
    // A portal call is a D-Bus round trip, and the UI should not wait for the desktop to answer it.
    std::thread::spawn(move || {
        if let Err(error) = open_through_portal(&uri) {
            tracing::debug!(%error, "the OpenURI portal did not open the URI; trying xdg-open");
            launch(&uri);
        }
    });
    true
}

#[cfg(all(target_os = "linux", feature = "system-theme"))]
fn open_through_portal(uri: &str) -> zbus::Result<()> {
    use std::collections::HashMap;

    use zbus::zvariant::Value;

    let connection = zbus::blocking::Connection::session()?;
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.OpenURI",
    )?;
    let options: HashMap<&str, Value> = HashMap::new();
    proxy.call_method("OpenURI", &("", uri, options))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn open(uri: &str) -> bool {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (verb, file) = (wide("open"), wide(uri));
    // Safety: both strings are NUL-terminated UTF-16 that outlive the call, and every other pointer is null, which the API accepts.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // Documented: anything above 32 is success, anything else an error code.
    let opened = result as isize > 32;
    if !opened {
        tracing::warn!(
            uri,
            code = result as isize,
            "ShellExecuteW did not open the URI"
        );
    }
    opened
}

#[cfg(target_os = "macos")]
fn open(uri: &str) -> bool {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSString, NSURL};

    let Some(url) = NSURL::URLWithString(&NSString::from_str(uri)) else {
        tracing::warn!(uri, "NSURL could not read the URI");
        return false;
    };
    NSWorkspace::sharedWorkspace().openURL(&url)
}

#[cfg(not(any(
    target_os = "windows",
    target_os = "macos",
    all(target_os = "linux", feature = "system-theme")
)))]
fn open(uri: &str) -> bool {
    launch(uri)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn launch(uri: &str) -> bool {
    services_core::uri::launch(uri)
        .inspect_err(|error| tracing::warn!(uri, %error, "xdg-open did not start"))
        .is_ok()
}
