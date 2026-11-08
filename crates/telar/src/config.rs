//! The renderer backend an app is built for.

use telar_project::protocol::{RENDERER_BACKEND_VAR, RendererBackend};

pub(crate) fn compile_time_backend() -> RendererBackend {
    match option_env!(telar_project::renderer_backend_var!()) {
        None => RendererBackend::Auto,
        Some(word) => RendererBackend::parse(word)
            .unwrap_or_else(|error| panic!("{RENDERER_BACKEND_VAR}: {error}")),
    }
}
