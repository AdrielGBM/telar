//! What crosses between the host and an app dylib beyond [`platform_core::guest`]: the version of the exports, and the host services the dylib is handed, each catching its own panics before they reach a frame of the dylib's.

use std::path::PathBuf;
use std::sync::Arc;

use platform_core::guest::{BoundaryLayout, shield};
use services_core::{Clipboard, FileDialog, FileDialogs, UriOpener};

/// The shape of the exports a host can drive. A library reporting another one was built against a telar whose exports this host would misread, so it is refused rather than called. 2 is for `_rsx_hot_layout`, 3 for `_rsx_hot_loaded` and `_rsx_hot_unloading`, 4 for the keyboard convention `HostServices` hands over.
pub const GUEST_ABI: u32 = 4;

/// The fingerprint of what crosses by value, and of the features that shape it. `HostServices` carries a field only a `shaper` build has, which is the case it exists for: a host and a dylib that differ there would otherwise read the struct as the wrong shape.
pub const GUEST_LAYOUT: BoundaryLayout = platform_core::boundary_layout!(
    super::HostServices,
    platform_core::AccessAudit,
    platform_core::AppCtx<'static>,
    platform_core::Event,
    platform_core::KeyboardConvention,
    platform_core::Location,
    platform_core::RedrawWaker,
    platform_core::SystemPreferences,
    platform_core::WindowCommand,
    platform_core::WindowConfig,
    renderer_core::Color,
    renderer_core::DrawCommand,
    ui_tree::SegmentNodeInfo,
    Box<dyn crate::App>,
    &dyn crate::App,
    std::time::Instant,
    std::time::Duration;
    "shaper" => cfg!(feature = "shaper"),
    "dev" => cfg!(feature = "dev"),
);

/// Whether a library reporting `abi` and `layout` can be driven by this host, and if not, why not.
pub(crate) fn check_guest(abi: Option<u32>, layout: Option<BoundaryLayout>) -> Result<(), String> {
    if abi != Some(GUEST_ABI) {
        return Err(format!(
            "it was built against another telar than this host (hot-reload ABI {}, expected {GUEST_ABI}): restart the session",
            abi.map_or_else(|| "unknown".to_owned(), |abi| abi.to_string()),
        ));
    }
    let guest = layout
        .ok_or("it does not report the layout of what crosses the boundary: restart the session")?;
    GUEST_LAYOUT
        .accept(guest)
        .map_err(|mismatch| mismatch.to_string())
}

pub(crate) struct ShieldedClipboard(pub(crate) Arc<dyn Clipboard>);

impl Clipboard for ShieldedClipboard {
    fn text(&self) -> Option<String> {
        shield(None, || self.0.text())
    }

    fn set_text(&self, text: &str) {
        shield((), || self.0.set_text(text))
    }
}

pub(crate) struct ShieldedUriOpener(pub(crate) Arc<dyn UriOpener>);

impl UriOpener for ShieldedUriOpener {
    fn open(&self, uri: &str) -> bool {
        shield(false, || self.0.open(uri))
    }

    fn open_beside(&self, reference: &str) -> bool {
        shield(false, || self.0.open_beside(reference))
    }
}

pub(crate) struct ShieldedFileDialogs(pub(crate) Arc<dyn FileDialogs>);

impl FileDialogs for ShieldedFileDialogs {
    fn open_file(&self, request: FileDialog) -> Option<PathBuf> {
        shield(None, || self.0.open_file(request))
    }

    fn open_files(&self, request: FileDialog) -> Vec<PathBuf> {
        shield(Vec::new(), || self.0.open_files(request))
    }

    fn save_file(&self, request: FileDialog) -> Option<PathBuf> {
        shield(None, || self.0.save_file(request))
    }

    fn pick_folder(&self, request: FileDialog) -> Option<PathBuf> {
        shield(None, || self.0.pick_folder(request))
    }
}

#[cfg(test)]
#[path = "boundary_test.rs"]
mod tests;
