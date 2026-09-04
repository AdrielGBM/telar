//! Helpers shared by the software backend's test binaries.

#![allow(dead_code)]

/// Installs the glyph measurer layout sizes text with, which a gallery test has no runner to do for it.
pub fn install_text_metrics() {
    renderer_core::set_default_text_metrics(renderer_text::ShaperMetrics);
}

/// Writes an RGBA8 buffer out as a PNG, for the visual harnesses that exist to be looked at.
pub fn save_png(path: &str, w: u32, h: u32, rgba: &[u8]) {
    image::RgbaImage::from_raw(w, h, rgba.to_vec())
        .expect("rgba length matches w*h*4")
        .save(path)
        .expect("write PNG");
    eprintln!("wrote {path}");
}

/// [`save_png`] to whatever `env_key` names, or nothing when it is unset — for a real test that also wants to dump what it rendered.
pub fn save_png_if_requested(env_key: &str, w: u32, h: u32, rgba: &[u8]) {
    if let Ok(path) = std::env::var(env_key) {
        save_png(&path, w, h, rgba);
    }
}
