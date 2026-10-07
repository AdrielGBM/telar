//! Assembling the renderer's font configuration from the app's and the platform's.

use services_core::AppPathsProvider;

/// The faces a surface loads, and which family its text shapes in where nothing above it names one.
///
/// One value because the two always travel together: they arrive from an [`AppConfig`](crate::AppConfig), cross the handler, and reach the builders that load the faces and the surface that takes the family.
#[derive(Clone, Default)]
pub struct FontSetup {
    pub faces: Vec<renderer_core::FontAsset>,
    /// What this surface's text shapes in where nothing above it names a family: the root of its text cascade, seeded as the surface opens and changed while it runs with [`set_font_family`](crate::set_font_family). A property of *this* surface, so a second one built later renders in its own rather than in whichever was configured last.
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

/// Loads the application's faces into the shaper and installs the shaper's measurer, for a process that lays text out without ever building a renderer.
///
/// No paths provider, as in [`prerender_page`](super::prerender_page): a measurement-only run has no platform to ask where the system fonts are.
#[cfg(all(feature = "previews", not(target_os = "android")))]
pub(super) fn install_shaper_fonts(config: crate::AppConfig) {
    let crate::AppConfig {
        fonts: faces,
        font_family: family,
        ..
    } = config;
    renderer_text::fonts::install(build_font_config(
        FontSetup { faces, family },
        &SystemFonts::from_provider(&services_core::NoPaths),
    ));
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

/// The faces a surface loads, behind the platform's own sans-serif candidates.
///
/// The family a surface names is not routed here: the shaper's database is the process's, and a family chosen there would be every surface's — layout measures each surface's text with one shaper. It is the root of the surface's text cascade instead, so the family travels in each text's style and measuring and drawing agree per surface.
pub(super) fn build_font_config(
    fonts: FontSetup,
    system: &SystemFonts,
) -> renderer_core::FontConfig {
    // A platform fact supplied by the injected paths provider, so this stays OS-agnostic.
    renderer_core::FontConfig {
        faces: fonts.faces,
        system_fonts_dir: system.dir.clone(),
        sans_serif_family_candidates: system.sans_serif.clone(),
    }
}

#[cfg(feature = "software")]
pub(super) fn build_software_renderer_config(
    fonts: FontSetup,
    system: &SystemFonts,
    transparent: bool,
    retains_presented_contents: bool,
) -> renderer_software::SoftwareRendererConfig {
    renderer_software::SoftwareRendererConfig {
        font: build_font_config(fonts, system),
        transparent,
        retains_presented_contents,
    }
}
