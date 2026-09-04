//! The fonts a renderer is built with: extra faces, raw bytes, and the families to prefer.

/// Shared font configuration for both software and hardware renderers. Lets callers supply extra fonts, raw font bytes, a system fonts directory, and preferred sans-serif families without duplicating these fields across renderer-specific config structs.
#[derive(Clone)]
pub struct FontConfig {
    pub extra_font_paths: Vec<std::path::PathBuf>,
    pub font_data: Vec<Vec<u8>>,
    pub system_fonts_dir: Option<std::path::PathBuf>,
    pub sans_serif_family_candidates: Vec<String>,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            extra_font_paths: Vec::new(),
            font_data: Vec::new(),
            system_fonts_dir: None,
            sans_serif_family_candidates: Vec::new(),
        }
    }
}
