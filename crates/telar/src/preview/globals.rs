//! [`Globals`]: the environment a host gives a preview's canvas, held in one place for its toolbar to set and its canvas to follow.

use std::fmt;
use std::rc::Rc;

use reactive_core::{RwSignal, effect, signal};

use crate::{
    Color, Direction, Insets, LayoutError, LayoutItem, LocaleTag, ModeId, NestedCanvas, Size,
    SurfaceCanvas, SurfaceEnv, UnknownMode,
};

use super::PreviewEnv;

/// The environment of one canvas: its mode, locale, direction, viewport, safe area, background and high contrast. `None` leaves the canvas with whatever the host around it has.
///
/// A cheap `Copy` handle over one signal per setting, so a toolbar writes the same signals a canvas reads. A preview's own [`PreviewEnv`] seeds them, and acts only as their default: [`seed`](Self::seed) sets what it names and leaves the rest as the host had them. The signals belong to the owner that was active when the globals were made.
#[derive(Clone, Copy)]
pub struct Globals {
    mode: RwSignal<Option<ModeId>>,
    locale: RwSignal<Option<LocaleTag>>,
    direction: RwSignal<Option<Direction>>,
    viewport: RwSignal<Option<Size>>,
    safe_area: RwSignal<Insets>,
    background: RwSignal<Option<Color>>,
    high_contrast: RwSignal<Option<bool>>,
}

impl Globals {
    /// Globals that set nothing.
    pub fn new() -> Self {
        Self {
            mode: signal(None),
            locale: signal(None),
            direction: signal(None),
            viewport: signal(None),
            safe_area: signal(Insets::default()),
            background: signal(None),
            high_contrast: signal(None),
        }
    }

    /// Globals holding what these hold now, on signals of their own: writing one leaves the other as it was.
    pub(super) fn detached(&self) -> Self {
        Self {
            mode: signal(self.mode.peek()),
            locale: signal(self.locale.peek()),
            direction: signal(self.direction.peek()),
            viewport: signal(self.viewport.peek()),
            safe_area: signal(self.safe_area.peek()),
            background: signal(self.background.peek()),
            high_contrast: signal(self.high_contrast.peek()),
        }
    }

    /// Globals holding what `env` asks for, or why they cannot: `env` names a mode the application has not registered.
    pub fn seeded(env: &PreviewEnv) -> Result<Self, UnknownMode> {
        let globals = Self::new();
        globals.seed(env)?;
        Ok(globals)
    }

    /// Sets each global `env` names, and leaves the ones it does not; sets none where `env` names a mode the application has not registered.
    pub fn seed(&self, env: &PreviewEnv) -> Result<(), UnknownMode> {
        let mode = env.mode.map(ModeId::registered).transpose()?;
        if let Some(mode) = mode {
            self.mode.set(Some(mode));
        }
        if let Some(locale) = env.locale {
            self.locale.set(Some(locale.clone()));
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
        if let Some(high) = env.high_contrast {
            self.high_contrast.set(Some(high));
        }
        Ok(())
    }

    /// A mode the application registered.
    pub fn mode(&self) -> RwSignal<Option<ModeId>> {
        self.mode
    }

    /// The locale the canvas is shown in, where one was asked for.
    pub fn locale(&self) -> RwSignal<Option<LocaleTag>> {
        self.locale
    }

    /// The writing direction, whatever the locale's own.
    pub fn direction(&self) -> RwSignal<Option<Direction>> {
        self.direction
    }

    /// The canvas size, in logical px.
    pub fn viewport(&self) -> RwSignal<Option<Size>> {
        self.viewport
    }

    /// How far in from each edge the system keeps the canvas for itself, as a device's notch and home indicator do. Zero keeps nothing.
    pub fn safe_area(&self) -> RwSignal<Insets> {
        self.safe_area
    }

    /// The canvas background colour.
    pub fn background(&self) -> RwSignal<Option<Color>> {
        self.background
    }

    /// `Some(true)` for more contrast, `Some(false)` for the regular palette.
    pub fn high_contrast(&self) -> RwSignal<Option<bool>> {
        self.high_contrast
    }

    /// The environment the globals hold now, as a canvas is built in it.
    pub fn env(&self) -> SurfaceEnv {
        SurfaceEnv {
            mode: self.mode.peek(),
            locale: self.locale.peek(),
            direction: self.direction.peek(),
            high_contrast: self.high_contrast.peek(),
            safe_area: self.safe_area.peek(),
            ..SurfaceEnv::default()
        }
    }

    /// Builds `build`'s content on a new canvas of `size` already in this environment, so what the content reads of it while building is the canvas's and not the host's, and keeps the canvas's mode, direction, locale, high contrast, safe area and, while one is set, size in step with these globals for as long as the current owner lives.
    ///
    /// The background is left to the host: it is drawn around the canvas rather than set on it.
    ///
    /// The canvas is a [`NestedCanvas`], as every preview canvas is drawn inside a tree around it: Tab past its last focusable leaves nothing focused rather than wrapping, the same in the workshop as in a play run headless.
    pub fn canvas(
        &self,
        size: Size,
        build: impl FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError>,
    ) -> Result<Rc<NestedCanvas>, LayoutError> {
        let canvas = Rc::new(NestedCanvas::new_in(size, &self.env(), build)?);
        self.bind(&canvas);
        Ok(canvas)
    }

    fn bind(&self, canvas: &Rc<NestedCanvas>) {
        let follow = |apply: Box<dyn Fn(&SurfaceCanvas)>| {
            let canvas = Rc::downgrade(canvas);
            effect(move || {
                if let Some(canvas) = canvas.upgrade() {
                    apply(&canvas);
                }
            });
        };
        let Self {
            mode,
            direction,
            locale,
            high_contrast,
            viewport,
            safe_area,
            ..
        } = *self;
        follow(Box::new(move |canvas| canvas.set_mode(mode.get().as_ref())));
        follow(Box::new(move |canvas| {
            canvas.set_direction(direction.get())
        }));
        follow(Box::new(move |canvas| {
            canvas.set_locale(locale.get().as_ref())
        }));
        follow(Box::new(move |canvas| {
            canvas.set_high_contrast(high_contrast.get())
        }));
        follow(Box::new(move |canvas| {
            canvas.set_safe_area(safe_area.get())
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

impl fmt::Debug for Globals {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Globals")
            .field("mode", &self.mode.peek())
            .field("locale", &self.locale.peek())
            .field("direction", &self.direction.peek())
            .field("viewport", &self.viewport.peek())
            .field("safe_area", &self.safe_area.peek())
            .field("background", &self.background.peek())
            .field("high_contrast", &self.high_contrast.peek())
            .finish()
    }
}

#[cfg(test)]
#[path = "globals.test.rs"]
mod tests;
