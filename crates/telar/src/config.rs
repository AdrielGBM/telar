//! The renderer backend an app is built for.

use telar_project::protocol::RendererBackend;

/// What [`RENDERER_BACKEND_VAR`](telar_project::protocol::RENDERER_BACKEND_VAR) carried into this build, decided while compiling.
pub(crate) const COMPILE_TIME_BACKEND: RendererBackend =
    match option_env!(telar_project::renderer_backend_var!()) {
        None => RendererBackend::Auto,
        Some(word) => match RendererBackend::named(word) {
            Some(backend) => backend,
            // A constant's panic carries no formatted text, so the build stops on this line, which says what the word it names is: a renderer backend that `TELAR_RENDERER_BACKEND` misspells.
            None => panic!("{}", word),
        },
    };
