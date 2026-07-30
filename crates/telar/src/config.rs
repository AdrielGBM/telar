use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum RendererBackend {
    #[default]
    Auto,
    Hardware,
    Software,
}

// Only the runtime runner reads this; gated so `rsx` compiled without `runtime` (e.g. as a cargo-telar dependency) doesn't warn it as dead code.
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
