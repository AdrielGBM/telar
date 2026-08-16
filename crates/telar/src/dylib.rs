//! Opening a guest dylib, shared by the two guest models that do it.
//!
//! Hot reload loads a whole [`App`](crate::app::App) that owns the window; a plugin loads a composited
//! [`EmbeddedApp`](crate::plugin::EmbeddedApp). The models stay distinct — only the open is one thing.

use std::path::Path;

/// Opens `path` with the loading policy both guest models need: resolve every symbol now, and keep the guest's
/// symbols local so its own copy of the runtime never interposes on the host's. Two runtimes in one process is
/// the premise of both models, and a global-scope load would let the guest's copy of a `thread_local` accessor
/// answer for the host's.
pub(crate) fn open(path: &Path) -> Result<libloading::Library, libloading::Error> {
    #[cfg(unix)]
    {
        unsafe {
            libloading::os::unix::Library::open(
                Some(path.as_os_str()),
                libc::RTLD_NOW | libc::RTLD_LOCAL,
            )
            .map(libloading::Library::from)
        }
    }
    #[cfg(not(unix))]
    {
        unsafe { libloading::Library::new(path) }
    }
}
