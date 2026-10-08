//! [`Globals`]: the environment a host gives a preview's canvas, held in one place for its toolbar to set and its canvas to follow.

use std::fmt;
use std::rc::Rc;

use reactive_core::{RwSignal, effect, signal};

use crate::{Color, ControlSize, Direction, Size, SurfaceCanvas};

use super::PreviewEnv;

/// The environment of one canvas: its mode, locale, direction, viewport, background and control size. `None` leaves the canvas with whatever the host around it has.
///
/// A cheap `Copy` handle over one signal per setting, so a toolbar writes the same signals a canvas reads. A preview's own [`PreviewEnv`] seeds them, and acts only as their default: [`seed`](Self::seed) sets what it names and leaves the rest as the host had them. The signals belong to the owner that was active when the globals were made.
#[derive(Clone, Copy)]
pub struct Globals {
    mode: RwSignal<Option<String>>,
    locale: RwSignal<Option<String>>,
    direction: RwSignal<Option<Direction>>,
    viewport: RwSignal<Option<Size>>,
    background: RwSignal<Option<Color>>,
    control_size: RwSignal<Option<ControlSize>>,
}

impl Globals {
    /// Globals that set nothing.
    pub fn new() -> Self {
        Self {
            mode: signal(None),
            locale: signal(None),
            direction: signal(None),
            viewport: signal(None),
            background: signal(None),
            control_size: signal(None),
        }
    }

    /// Globals holding what `env` asks for.
    pub fn seeded(env: &PreviewEnv) -> Self {
        let globals = Self::new();
        globals.seed(env);
        globals
    }

    /// Sets each global `env` names, and leaves the ones it does not.
    pub fn seed(&self, env: &PreviewEnv) {
        if let Some(mode) = env.mode {
            self.mode.set(Some(mode.to_string()));
        }
        if let Some(locale) = env.locale {
            self.locale.set(Some(locale.to_string()));
        }
        if let Some(direction) = env.direction {
            self.direction.set(Some(direction));
        }
        if let Some(viewport) = env.viewport {
            self.viewport.set(Some(viewport));
        }
        if let Some(background) = env.background {
            self.background.set(Some(background));
        }
    }

    /// A mode registered with `register_mode`, by id.
    pub fn mode(&self) -> RwSignal<Option<String>> {
        self.mode
    }

    /// A BCP 47 language tag.
    pub fn locale(&self) -> RwSignal<Option<String>> {
        self.locale
    }

    pub fn direction(&self) -> RwSignal<Option<Direction>> {
        self.direction
    }

    /// The canvas size, in logical px.
    pub fn viewport(&self) -> RwSignal<Option<Size>> {
        self.viewport
    }

    pub fn background(&self) -> RwSignal<Option<Color>> {
        self.background
    }

    pub fn control_size(&self) -> RwSignal<Option<ControlSize>> {
        self.control_size
    }

    /// Keeps `canvas`'s direction, locale, control size and, while one is set, size in step with these globals, for as long as the current owner lives.
    ///
    /// The mode and the background are left to the host: they are drawn around the canvas rather than set on it.
    pub fn bind(&self, canvas: &Rc<SurfaceCanvas>) {
        let follow = |apply: Box<dyn Fn(&SurfaceCanvas)>| {
            let canvas = Rc::downgrade(canvas);
            effect(move || {
                if let Some(canvas) = canvas.upgrade() {
                    apply(&canvas);
                }
            });
        };
        let Self {
            direction,
            locale,
            control_size,
            viewport,
            ..
        } = *self;
        follow(Box::new(move |canvas| {
            canvas.set_direction(direction.get())
        }));
        follow(Box::new(move |canvas| {
            canvas.set_locale(locale.get().as_deref())
        }));
        follow(Box::new(move |canvas| {
            canvas.set_control_size(control_size.get())
        }));
        follow(Box::new(move |canvas| {
            if let Some(size) = viewport.get() {
                canvas.resize(size);
            }
        }));
    }
}

impl Default for Globals {
    fn default() -> Self {
        Self::new()
    }
}

impl From<&PreviewEnv> for Globals {
    fn from(env: &PreviewEnv) -> Self {
        Self::seeded(env)
    }
}

impl fmt::Debug for Globals {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Globals")
            .field("mode", &self.mode.peek())
            .field("locale", &self.locale.peek())
            .field("direction", &self.direction.peek())
            .field("viewport", &self.viewport.peek())
            .field("background", &self.background.peek())
            .field("control_size", &self.control_size.peek())
            .finish()
    }
}

#[cfg(test)]
#[path = "globals_test.rs"]
mod tests;
