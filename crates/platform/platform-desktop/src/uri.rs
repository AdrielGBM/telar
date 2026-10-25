//! Opening a URI with the system: the freedesktop `OpenURI` portal, `ShellExecuteW` or `NSWorkspace`.

use std::sync::Arc;

use platform_core::Uri;
use services_core::UriOpener;

/// The desktop's own way to open a URI, falling back to the system launcher only where that way does not exist.
///
/// On Linux the portal is asked first, because it is what a sandboxed app (Flatpak, Snap) is allowed to use and what picks the handler the user chose; `xdg-open` answers whenever the portal cannot be asked or does not answer (no session bus, no portal or backend running, a timeout, a dropped connection, or a build with no D-Bus client). Only an explicit refusal (access denied, invalid arguments, not allowed) is final: it is logged, never routed around. `open` returns once the request is handed to that thread. Windows hands the URI to `ShellExecuteW` and macOS to `NSWorkspace`, which is what a click on a link does in any native application there.
pub struct DesktopUriOpener;

impl DesktopUriOpener {
    /// Installs this as the app's URI opener.
    pub fn install() {
        services_core::set_uri_opener(Arc::new(DesktopUriOpener));
    }
}

impl UriOpener for DesktopUriOpener {
    fn open(&self, uri: &Uri) -> bool {
        open(uri)
    }
}

#[cfg(all(target_os = "linux", feature = "system-theme"))]
fn open(uri: &Uri) -> bool {
    let uri = uri.clone();
    let spawned = std::thread::Builder::new()
        .name("open-uri".into())
        .spawn(move || match open_through_portal(&uri) {
            Ok(()) => {}
            Err(PortalError::Unavailable(error)) => {
                tracing::debug!(%error, "no OpenURI portal to ask; trying xdg-open");
                launch(&uri);
            }
            Err(PortalError::Refused(error)) => {
                tracing::warn!(%uri, %error, "the OpenURI portal refused the URI");
            }
        });
    spawned
        .inspect_err(
            |error| tracing::warn!(%error, "the thread that asks the OpenURI portal did not start"),
        )
        .is_ok()
}

#[cfg(all(target_os = "linux", feature = "system-theme"))]
enum PortalError {
    Unavailable(zbus::Error),
    Refused(zbus::Error),
}

#[cfg(all(target_os = "linux", feature = "system-theme"))]
impl From<zbus::Error> for PortalError {
    fn from(error: zbus::Error) -> Self {
        const REFUSALS: [&str; 3] = [
            "org.freedesktop.DBus.Error.AccessDenied",
            "org.freedesktop.DBus.Error.InvalidArgs",
            "org.freedesktop.portal.Error.NotAllowed",
        ];
        let refused = match &error {
            zbus::Error::MethodError(name, _, _) => REFUSALS.contains(&name.as_str()),
            zbus::Error::FDO(error) => matches!(
                **error,
                zbus::fdo::Error::AccessDenied(_) | zbus::fdo::Error::InvalidArgs(_)
            ),
            _ => false,
        };
        if refused {
            Self::Refused(error)
        } else {
            Self::Unavailable(error)
        }
    }
}

#[cfg(all(target_os = "linux", feature = "system-theme"))]
fn open_through_portal(uri: &Uri) -> Result<(), PortalError> {
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
    proxy.call_method("OpenURI", &("", uri.as_str(), options))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn open(uri: &Uri) -> bool {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (verb, file) = (wide("open"), wide(uri.as_str()));
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
            %uri,
            code = result as isize,
            "ShellExecuteW did not open the URI"
        );
    }
    opened
}

#[cfg(target_os = "macos")]
fn open(uri: &Uri) -> bool {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSString, NSURL};

    let Some(url) = NSURL::URLWithString(&NSString::from_str(uri.as_str())) else {
        tracing::warn!(%uri, "NSURL could not read the URI");
        return false;
    };
    NSWorkspace::sharedWorkspace().openURL(&url)
}

#[cfg(not(any(
    target_os = "windows",
    target_os = "macos",
    all(target_os = "linux", feature = "system-theme")
)))]
fn open(uri: &Uri) -> bool {
    launch(uri)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn launch(uri: &Uri) -> bool {
    services_core::uri::launch(uri)
        .inspect_err(|error| tracing::warn!(%uri, %error, "xdg-open did not start"))
        .is_ok()
}

#[cfg(all(test, target_os = "linux", feature = "system-theme"))]
#[path = "uri_test.rs"]
mod tests;
