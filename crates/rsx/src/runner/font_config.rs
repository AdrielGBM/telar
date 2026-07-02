use renderer_software::SoftwareRendererConfig;
use services_core::AppPathsProvider;

pub(super) fn hardware_cache_path(
    app_name: &str,
    paths: &dyn AppPathsProvider,
) -> Option<std::path::PathBuf> {
    paths.cache_dir().map(|d| d.join("rsx").join(app_name))
}

pub(super) fn build_hardware_font_config(
    font_paths: Vec<std::path::PathBuf>,
    font_data: Vec<Vec<u8>>,
    android: bool,
) -> renderer_text::TextShaperConfig {
    renderer_text::TextShaperConfig {
        font: build_font_config(font_paths, font_data, android),
        ..renderer_text::TextShaperConfig::default()
    }
}

pub(super) fn build_font_config(
    font_paths: Vec<std::path::PathBuf>,
    font_data: Vec<Vec<u8>>,
    android: bool,
) -> renderer_core::FontConfig {
    renderer_core::FontConfig {
        extra_font_paths: font_paths,
        font_data,
        system_fonts_dir: android.then(|| std::path::PathBuf::from("/system/fonts")),
        sans_serif_family_candidates: if android {
            super::android::android_sans_serif_candidates()
        } else {
            vec![]
        },
    }
}

pub(super) fn build_software_renderer_config(
    font_paths: Vec<std::path::PathBuf>,
    font_data: Vec<Vec<u8>>,
    android: bool,
) -> SoftwareRendererConfig {
    SoftwareRendererConfig {
        font: build_font_config(font_paths, font_data, android),
        ..SoftwareRendererConfig::default()
    }
}
