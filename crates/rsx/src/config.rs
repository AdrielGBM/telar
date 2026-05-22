use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum RendererBackend {
    #[default]
    Auto,
    Hardware,
    Software,
}

pub fn compile_time_backend() -> RendererBackend {
    match option_env!("RSX_RENDERER_BACKEND") {
        Some("hardware") => RendererBackend::Hardware,
        Some("software") => RendererBackend::Software,
        Some("auto") | None => RendererBackend::Auto,
        Some(other) => panic!(
            "Unknown RSX_RENDERER_BACKEND value: \"{other}\". \
             Expected \"auto\", \"hardware\", or \"software\"."
        ),
    }
}
