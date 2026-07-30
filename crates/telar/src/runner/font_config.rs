use std::sync::RwLock;

use renderer_software::SoftwareRendererConfig;
use services_core::AppPathsProvider;

// Process-wide default sans-serif family (a shell's theme font). Every surface builds its own font system
// from `build_font_config`, so routing the family through this global reaches all of them — bars, drawers,
// popups — without threading it through each surface's paths provider.
static DEFAULT_FONT_FAMILY: RwLock<Option<String>> = RwLock::new(None);

/// Sets the process-wide default font family every surface's text renders in (its sans-serif face). `None`
/// restores the platform default. Call once before building surfaces — e.g. a shell passing its theme's
/// `font_family`; a later call takes effect on surfaces built afterwards (a config reload rebuilds them).
pub fn set_default_font_family(family: Option<String>) {
    *DEFAULT_FONT_FAMILY.write().unwrap() = family;
}

fn default_font_family() -> Option<String> {
    DEFAULT_FONT_FAMILY.read().unwrap().clone()
}

// The OS system-font facts pulled once from the injected paths provider (a fixed font dir + OEM family
// candidates). Kept as owned data so it can move into the background renderer-build thread; empty on desktop.
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

pub(super) fn hardware_cache_path(
    app_name: &str,
    paths: &dyn AppPathsProvider,
) -> Option<std::path::PathBuf> {
    paths.cache_dir().map(|d| d.join("telar").join(app_name))
}

pub(super) fn build_hardware_font_config(
    font_paths: Vec<std::path::PathBuf>,
    font_data: Vec<Vec<u8>>,
    fonts: &SystemFonts,
) -> renderer_text::TextShaperConfig {
    renderer_text::TextShaperConfig {
        font: build_font_config(font_paths, font_data, fonts),
        ..renderer_text::TextShaperConfig::default()
    }
}

pub(super) fn build_font_config(
    font_paths: Vec<std::path::PathBuf>,
    font_data: Vec<Vec<u8>>,
    fonts: &SystemFonts,
) -> renderer_core::FontConfig {
    // The OS system font stack (fixed dir + OEM family candidates) is a platform fact supplied by the
    // injected paths provider, so this stays OS-agnostic — the Android adapter fills them, desktop defaults.
    // The process-wide default family takes priority over the platform/OEM candidates, so a shell's theme
    // font wins; the OS candidates remain as fallbacks when the chosen family is not installed.
    let sans_serif_family_candidates = default_font_family()
        .into_iter()
        .chain(fonts.sans_serif.iter().cloned())
        .collect();
    renderer_core::FontConfig {
        extra_font_paths: font_paths,
        font_data,
        system_fonts_dir: fonts.dir.clone(),
        sans_serif_family_candidates,
    }
}

pub(super) fn build_software_renderer_config(
    font_paths: Vec<std::path::PathBuf>,
    font_data: Vec<Vec<u8>>,
    fonts: &SystemFonts,
    transparent: bool,
) -> SoftwareRendererConfig {
    SoftwareRendererConfig {
        font: build_font_config(font_paths, font_data, fonts),
        transparent,
        ..SoftwareRendererConfig::default()
    }
}
