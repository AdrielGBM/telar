//! Opening a dylib that carries its own copy of the runtime.

use std::path::Path;

/// Opens `path` with the loading policy a guest needs: resolve every symbol now, and keep the guest's symbols **local** so its own copy of the runtime never interposes on the host's.
///
/// Two runtimes in one process is the premise of every model that loads one — hot reload swapping a whole application, a plugin compositing into the host's frame — and each is only sound while the two stay apart. A global-scope load lets the guest's copy of a `thread_local` accessor answer for the host's, and what that looks like is not a crash: it is a registry the host fills and the guest never reads, silently, for the life of the process.
pub fn open(path: &Path) -> Result<libloading::Library, libloading::Error> {
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
