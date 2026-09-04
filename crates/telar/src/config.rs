//! The renderer backend an app is built or configured for.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
/// Which renderer to build: `Auto` is hardware with a software fallback.
pub enum RendererBackend {
    #[default]
    Auto,
    Hardware,
    Software,
}

// Gated so a build without `runtime` does not warn it as dead code.
#[cfg(feature = "runtime")]
pub(crate) fn compile_time_backend() -> RendererBackend {
    match option_env!("TELAR_RENDERER_BACKEND") {
        Some("hardware") => RendererBackend::Hardware,
        Some("software") => RendererBackend::Software,
        Some("auto") | None => RendererBackend::Auto,
        Some(other) => panic!(
            "Unknown TELAR_RENDERER_BACKEND value: \"{other}\". \
             Expected \"auto\", \"hardware\", or \"software\"."
        ),
    }
}
