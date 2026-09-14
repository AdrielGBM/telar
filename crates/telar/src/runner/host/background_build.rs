use std::sync::mpsc::{Receiver, TryRecvError, channel};

use renderer_core::RendererError;

pub(super) struct BackgroundBuild<T> {
    delivered: Receiver<Result<T, RendererError>>,
}

impl<T: Send + 'static> BackgroundBuild<T> {
    pub(super) fn spawn(
        build: impl FnOnce() -> Result<T, RendererError> + Send + 'static,
        wake: impl FnOnce() + Send + 'static,
    ) -> Self {
        let (deliver, delivered) = channel();
        std::thread::Builder::new()
            .name("telar-renderer-build".to_string())
            .spawn(move || {
                let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(build))
                    .unwrap_or_else(|_| Err(build_panicked()));
                // Delivered before the wake, so the frame the wake asks for finds the result whether or not this thread has exited yet.
                let _ = deliver.send(built);
                wake();
            })
            .expect("failed to spawn renderer build thread");
        Self { delivered }
    }

    pub(super) fn try_take(self) -> Result<Result<T, RendererError>, Self> {
        match self.delivered.try_recv() {
            Ok(built) => Ok(built),
            Err(TryRecvError::Empty) => Err(self),
            Err(TryRecvError::Disconnected) => Ok(Err(build_panicked())),
        }
    }
}

fn build_panicked() -> RendererError {
    RendererError::Backend("renderer build thread panicked".to_string())
}

#[cfg(test)]
#[path = "background_build_test.rs"]
mod tests;
