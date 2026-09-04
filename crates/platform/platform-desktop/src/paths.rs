//! Where a desktop app's config, cache and data live.

/// The desktop's application directories.
///
/// An alias rather than an implementation: these are the host OS's user directories, which every backend running as an ordinary process wants and which therefore live in `services-core`. Kept under this name so an app that names it does not have to change.
pub use services_core::SystemPaths as DesktopPathsProvider;
