use services_core::AppPathsProvider;

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

#[cfg(feature = "hardware")]
pub(super) fn hardware_cache_path(
    app_name: &str,
    paths: &dyn AppPathsProvider,
) -> Option<std::path::PathBuf> {
    paths.cache_dir().map(|d| d.join("telar").join(app_name))
}

/// What a hardware renderer built outside the runner starts from: the caller's own faces, the family it
/// named for them, and the platform's stack behind it.
///
/// No paths provider, because there is no window whose platform would supply one — a renderer built for a
/// texture is not a surface the OS knows about.
#[cfg(feature = "hardware")]
pub(crate) fn offscreen_hardware_font_config(
    font_paths: Vec<std::path::PathBuf>,
    font_data: Vec<Vec<u8>>,
    font_family: Option<String>,
) -> renderer_text::TextShaperConfig {
    build_hardware_font_config(
        font_paths,
        font_data,
        font_family,
        &SystemFonts {
            dir: None,
            sans_serif: Vec::new(),
        },
    )
}

#[cfg(feature = "hardware")]
pub(super) fn build_hardware_font_config(
    font_paths: Vec<std::path::PathBuf>,
    font_data: Vec<Vec<u8>>,
    font_family: Option<String>,
    fonts: &SystemFonts,
) -> renderer_text::TextShaperConfig {
    renderer_text::TextShaperConfig {
        font: build_font_config(font_paths, font_data, font_family, fonts),
        ..renderer_text::TextShaperConfig::default()
    }
}

/// The faces a surface loads and which of them its unstyled text shapes in.
///
/// `font_family` is the surface's own — it arrives with the rest of its configuration rather than from a
/// process-wide setting, so two surfaces in one process can render in two different faces, and neither can
/// change the other's by being built later.
pub(super) fn build_font_config(
    font_paths: Vec<std::path::PathBuf>,
    font_data: Vec<Vec<u8>>,
    font_family: Option<String>,
    fonts: &SystemFonts,
) -> renderer_core::FontConfig {
    // The OS system font stack (fixed dir + OEM family candidates) is a platform fact supplied by the
    // injected paths provider, so this stays OS-agnostic — the Android adapter fills them, desktop defaults.
    // The named family takes priority over the platform/OEM candidates, so a shell's theme font wins; the OS
    // candidates remain as fallbacks when the chosen family is not installed.
    let sans_serif_family_candidates = font_family
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

#[cfg(feature = "software")]
pub(super) fn build_software_renderer_config(
    font_paths: Vec<std::path::PathBuf>,
    font_data: Vec<Vec<u8>>,
    font_family: Option<String>,
    fonts: &SystemFonts,
    transparent: bool,
) -> renderer_software::SoftwareRendererConfig {
    renderer_software::SoftwareRendererConfig {
        font: build_font_config(font_paths, font_data, font_family, fonts),
        transparent,
        ..renderer_software::SoftwareRendererConfig::default()
    }
}
