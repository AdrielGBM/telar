//! The terminal's event loop.

use std::time::Duration;

use platform_core::{
    Event, EventHandler, Key, ModifiersState, Platform, PlatformError, WindowConfig,
};

use crate::map::Mapper;
use crate::term::{self, TerminalMode};
use crate::window::TuiWindow;

/// How long the loop waits for input before looking at the frame again when nothing is animating.
///
/// A terminal offers no way to interrupt a blocking read from another thread, so a reactive change with no keystroke behind it — a finished task, a timer — is picked up on the next turn rather than immediately. Long enough that an idle app costs nothing measurable, short enough that the delay is not felt.
const IDLE_POLL: Duration = Duration::from_millis(30);

#[derive(Clone, Debug)]
/// How the terminal surface is sized and coloured.
pub struct TuiPlatformConfig {
    /// How many logical pixels one cell stands for. Must match what the renderer was built with, or the window reports a size the frames do not fill.
    pub cell_width: f32,
    pub cell_height: f32,
    pub mouse: bool,
    /// Whether Ctrl+C asks the application to close.
    ///
    /// On by default, because raw mode turns Ctrl+C from a signal into an ordinary key: an app that does not bind it would otherwise have no way out at all, and the user would need another terminal to kill it. An app that wants the chord for itself turns this off and handles the key.
    pub quit_on_ctrl_c: bool,
}

impl Default for TuiPlatformConfig {
    fn default() -> Self {
        Self {
            cell_width: 8.0,
            cell_height: 16.0,
            mouse: true,
            quit_on_ctrl_c: true,
        }
    }
}

/// The terminal backend: an alternate screen, raw mode, and a cell grid for a surface.
pub struct TuiPlatform {
    config: TuiPlatformConfig,
}

impl TuiPlatform {
    pub fn new(config: TuiPlatformConfig) -> Self {
        Self { config }
    }
}

impl Default for TuiPlatform {
    fn default() -> Self {
        Self::new(TuiPlatformConfig::default())
    }
}

fn is_ctrl_c(event: &Event) -> bool {
    matches!(
        event,
        Event::KeyPressed {
            key: Key::Char('c'),
            modifiers: ModifiersState { is_ctrl: true, .. }
        }
    )
}

impl Platform for TuiPlatform {
    type Window = TuiWindow;

    fn run<H: EventHandler<TuiWindow>>(
        self,
        _config: WindowConfig,
        mut handler: H,
    ) -> Result<(), PlatformError> {
        let mode = TerminalMode::enter(self.config.mouse).map_err(|e| {
            PlatformError(format!(
                "could not put the terminal into application mode: {e}"
            ))
        })?;
        let (cols, rows) = term::size();
        let window = TuiWindow::new(cols, rows, self.config.cell_width, self.config.cell_height);
        let mut mapper = Mapper::new(
            self.config.cell_width,
            self.config.cell_height,
            mode.reports_key_releases(),
        );

        let result = self.drive(&mut handler, &window, &mut mapper);

        // The handler owns the renderer, whose thread writes to the terminal this is about to restore. Dropping it here joins that thread first, so the last frame cannot land after the alternate screen is gone.
        handler.on_suspend();
        drop(handler);
        drop(mode);
        result
    }
}

impl TuiPlatform {
    fn drive<H: EventHandler<TuiWindow>>(
        &self,
        handler: &mut H,
        window: &TuiWindow,
        mapper: &mut Mapper,
    ) -> Result<(), PlatformError> {
        handler.new_events();
        let resumed = handler.on_resume(window);
        handler.about_to_wait();
        if !resumed {
            return Err(PlatformError(
                "the terminal handler refused to resume (the renderer could not be built)".into(),
            ));
        }

        let mut events = Vec::new();
        loop {
            handler.new_events();

            events.clear();
            while crossterm::event::poll(Duration::ZERO)
                .map_err(|e| PlatformError(format!("could not read terminal input: {e}")))?
            {
                let event = crossterm::event::read()
                    .map_err(|e| PlatformError(format!("could not read terminal input: {e}")))?;
                mapper.map(event, &mut events);
            }

            let mut quit = false;
            for event in events.drain(..) {
                if self.config.quit_on_ctrl_c && is_ctrl_c(&event) {
                    quit = true;
                    continue;
                }
                if let Event::WindowResized { width, height } = event {
                    window.set_grid(
                        (width as f32 / self.cell_width()).round() as u16,
                        (height as f32 / self.cell_height()).round() as u16,
                    );
                }
                handler.on_event(event, window);
            }

            handler.on_redraw(window);
            if quit || handler.take_exit_request() {
                return Ok(());
            }

            let wait = handler.about_to_wait().unwrap_or(IDLE_POLL).min(IDLE_POLL);
            // A redraw asked for by a reactive flush is served on the next turn rather than waited out.
            if !window.take_redraw_request() {
                crossterm::event::poll(wait)
                    .map_err(|e| PlatformError(format!("could not wait on terminal input: {e}")))?;
            }
        }
    }

    fn cell_width(&self) -> f32 {
        self.config.cell_width
    }

    fn cell_height(&self) -> f32 {
        self.config.cell_height
    }
}
