//! Assembling the renderer's font configuration from the app's and the platform's.

use services_core::AppPathsProvider;

/// The faces a surface loads, and which of them its unstyled text shapes in.
///
/// One value because the three always travel together: they arrive from an [`AppConfig`](crate::AppConfig), cross the handler, and reach four builders that each took them as three positional arguments. `multi.rs` had already given the triple a name — as a tuple alias — which is the shape asking to be a struct.
#[derive(Clone, Default)]
pub struct FontSetup {
    pub paths: Vec<std::path::PathBuf>,
    /// Faces carried in the binary, for a target with no font directory behind it.
    pub data: Vec<Vec<u8>>,
    /// The family this surface's unstyled text shapes in. A property of *this* surface, so a second one built later renders in its own rather than in whichever was configured last.
    pub family: Option<String>,
}

// Pulled once from the injected paths provider. Owned, so it can move into the background renderer-build thread; empty on desktop.
pub(super) struct SystemFonts {
    dir: Option<std::path::PathBuf>,
    sans_serif: Vec<String>,
}

impl SystemFonts {
    pub(super) fn from_provider(paths: &dyn AppPathsProvider) -> Self {
        Self {
            dir: paths.system_fonts_dir(),
            sans_serif: paths.sans_serif_candidates(),
        }
    }
}

#[cfg(feature = "hardware")]
pub(super) fn hardware_cache_path(
    app_name: &str,
    paths: &dyn AppPathsProvider,
) -> Option<std::path::PathBuf> {
    paths.cache_dir().map(|d| d.join("telar").join(app_name))
}

/// What a hardware renderer built outside the runner starts from: the caller's own faces, the family it named for them, and the platform's stack behind it.
///
/// No paths provider, because there is no window whose platform would supply one — a renderer built for a texture is not a surface the OS knows about.
#[cfg(feature = "hardware")]
pub(crate) fn offscreen_hardware_font_config(fonts: FontSetup) -> renderer_text::TextShaperConfig {
    build_hardware_font_config(
        fonts,
        &SystemFonts {
            dir: None,
            sans_serif: Vec::new(),
        },
    )
}

#[cfg(feature = "hardware")]
pub(super) fn build_hardware_font_config(
    fonts: FontSetup,
    system: &SystemFonts,
) -> renderer_text::TextShaperConfig {
    renderer_text::TextShaperConfig {
        font: build_font_config(fonts, system),
        ..renderer_text::TextShaperConfig::default()
    }
}

/// The faces a surface loads and which of them its unstyled text shapes in.
///
/// `font_family` is the surface's own — it arrives with the rest of its configuration rather than from a process-wide setting, so two surfaces in one process can render in two different faces, and neither can change the other's by being built later.
pub(super) fn build_font_config(
    fonts: FontSetup,
    system: &SystemFonts,
) -> renderer_core::FontConfig {
    // A platform fact supplied by the injected paths provider, so this stays OS-agnostic. The named family takes priority, so a shell's theme font wins and the OS candidates remain as fallbacks.
    let sans_serif_family_candidates = fonts
        .family
        .into_iter()
        .chain(system.sans_serif.iter().cloned())
        .collect();
    renderer_core::FontConfig {
        extra_font_paths: fonts.paths,
        font_data: fonts.data,
        system_fonts_dir: system.dir.clone(),
        sans_serif_family_candidates,
    }
}

#[cfg(feature = "software")]
pub(super) fn build_software_renderer_config(
    fonts: FontSetup,
    system: &SystemFonts,
    transparent: bool,
) -> renderer_software::SoftwareRendererConfig {
    renderer_software::SoftwareRendererConfig {
        font: build_font_config(fonts, system),
        transparent,
        ..renderer_software::SoftwareRendererConfig::default()
    }
}
