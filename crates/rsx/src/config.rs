use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum RendererBackend {
    #[default]
    Auto,
    Gpu,
    Cpu,
}

#[derive(Deserialize, Default)]
pub struct RendererConfig {
    #[serde(default)]
    pub backend: RendererBackend,
}

#[derive(Deserialize, Default)]
pub struct RsxConfig {
    #[serde(default)]
    pub renderer: RendererConfig,
}

pub fn compile_time_backend() -> RendererBackend {
    match option_env!("RSX_RENDERER_BACKEND") {
        Some("gpu") => RendererBackend::Gpu,
        Some("cpu") => RendererBackend::Cpu,
        _ => RendererBackend::Auto,
    }
}
