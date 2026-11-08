//! The renderer backend an app is built or configured for.

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
/// Which renderer to build: `Auto` is hardware with a software fallback.
pub enum RendererBackend {
    #[default]
    Auto,
    Hardware,
    Software,
}

impl RendererBackend {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            RendererBackend::Auto => "auto",
            RendererBackend::Hardware => "hardware",
            RendererBackend::Software => "software",
        }
    }

    pub(crate) fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "auto" => RendererBackend::Auto,
            "hardware" => RendererBackend::Hardware,
            "software" => RendererBackend::Software,
            _ => return None,
        })
    }
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
