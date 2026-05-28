use platform_core::{Event, EventHandler, Platform, Window, WindowConfig};
use platform_winit::{WinitPlatform, WinitWindow};
use reactive_core::{FlushNotifyHandle, begin_batch, end_batch, set_flush_notify};
use renderer_core::{RenderBackend, RendererError};
use renderer_hardware::HardwareRenderer;
use renderer_software::{RendererBudget, SoftwareRenderer};
use ui_core::ComponentTree;

use rsx_devtools::{DevAction, DevPlugin};

use crate::app::App;
use crate::config::{self, RendererBackend};
use crate::prefs::UserPrefs;
use crate::window_signals::WindowSignals;

struct AppHandler<D: DevPlugin> {
    app: Box<dyn App>,
    tree: Option<ComponentTree>,
    renderer: Option<Box<dyn RenderBackend>>,
    renderer_is_hardware: bool,
    backend: RendererBackend,
    prefs: UserPrefs,
    pending_restart: bool,
    _flush_notify: Option<FlushNotifyHandle>,
    window_signals: Option<WindowSignals>,
    app_name: String,
    last_frame: std::time::Instant,
    dev: D,
}

const FRAME_BUDGET: std::time::Duration = std::time::Duration::from_nanos(1_000_000_000 / 60);

impl<D: DevPlugin> EventHandler<WinitWindow> for AppHandler<D> {
    fn on_resume(&mut self, window: &WinitWindow) -> bool {
        let cache_path = hardware_cache_path(&self.app_name);
        match create_renderer(self.backend, window, cache_path.as_deref()) {
            Ok((renderer, is_hw)) => {
                self.renderer = Some(renderer);
                self.renderer_is_hardware = is_hw;
            }
            Err(e) => {
                tracing::error!("Failed to initialize renderer: {e}");
                return false;
            }
        }
        self.window_signals = Some(WindowSignals::new(
            window.width() as f32,
            window.height() as f32,
        ));
        self.tree = Some(ComponentTree::new(self.app.root()));

        let w = window.clone();
        self._flush_notify = Some(set_flush_notify(move || w.request_redraw()));
        window.request_redraw();
        true
    }

    fn on_event(&mut self, event: Event, window: &WinitWindow) {
        if let Event::WindowResized { width, height } = &event {
            if let Some(ref signals) = self.window_signals {
                signals.update(*width as f32, *height as f32);
            }
        }
        if let Event::KeyPressed { key, modifiers } = &event {
            match self.dev.on_key(key, *modifiers) {
                DevAction::Redraw => {
                    window.request_redraw();
                }
                DevAction::ToggleBackend => {
                    let next = match self.prefs.backend.unwrap_or(RendererBackend::Auto) {
                        RendererBackend::Hardware => RendererBackend::Software,
                        _ => RendererBackend::Hardware,
                    };
                    self.prefs.backend = Some(next);
                    if let Err(e) = self.prefs.save(&self.app_name) {
                        tracing::warn!("Could not save preferences: {e}");
                    }
                    self.pending_restart = true;
                }
                DevAction::None => {}
            }
        }
        if let Event::PointerPressed { x, y, .. } = &event {
            if self.dev.on_pointer_pressed(*x as f32, *y as f32) {
                window.request_redraw();
                return;
            }
        }
        if let Some(tree) = &mut self.tree {
            tree.on_event(&event);
        }
    }

    fn on_redraw(&mut self, window: &WinitWindow) {
        let mut redraw_requested = false;
        {
            let mut ctx = crate::app_context::AppCtx {
                app_name: &self.app_name,
                prefs: &mut self.prefs,
                pending_restart: &mut self.pending_restart,
                redraw_requested: &mut redraw_requested,
                window_signals: self.window_signals.as_ref(),
            };
            self.app.on_frame(&mut ctx);
        }
        if redraw_requested {
            window.request_redraw();
        }

        if self.pending_restart {
            self.pending_restart = false;
            self.backend = self
                .prefs
                .backend
                .unwrap_or_else(config::compile_time_backend);
            let cache_path = hardware_cache_path(&self.app_name);
            match create_renderer(self.backend, window, cache_path.as_deref()) {
                Ok((renderer, is_hw)) => {
                    self.renderer = Some(renderer);
                    self.renderer_is_hardware = is_hw;
                }
                Err(e) => tracing::error!("Failed to switch renderer: {e}"),
            }
        }

        let Some(renderer) = &mut self.renderer else {
            return;
        };

        let tree_dirty = self.tree.as_ref().map(|t| t.is_dirty()).unwrap_or(false);
        // HW renderer always calls render_frame (cheap idle-blit fast path avoids 1-2s GPU wake-up); SW skips when nothing changed unless dev plugin requests keepalive.
        let needs_keepalive = self.renderer_is_hardware || self.dev.keepalive_interval().is_some();
        if !tree_dirty && !needs_keepalive {
            return;
        }

        // 60 FPS cap: defer this redraw until the frame budget expires; about_to_wait() schedules the WaitUntil wakeup.
        if tree_dirty && self.last_frame.elapsed() < FRAME_BUDGET {
            return;
        }
        // Only update last_frame for content frames; keepalive blits must not reset the budget clock (would delay next content render by up to 16ms).
        if tree_dirty {
            self.last_frame = std::time::Instant::now();
        }

        let (w, h) = window.size();
        if let Err(e) = renderer.begin_frame(w, h) {
            tracing::error!("begin_frame failed: {e}");
            return;
        }
        let clear = self.app.clear_color();
        let commands_ref = self.tree.as_ref().map(|t| t.commands());
        let base_slice: &[renderer_core::DrawCommand] =
            commands_ref.as_deref().map(|r| r.as_slice()).unwrap_or(&[]);
        let frame_commands = self
            .dev
            .on_frame(base_slice, w as f32, h as f32, tree_dirty);
        if let Err(e) = renderer.as_mut().render_frame(&frame_commands, clear) {
            tracing::error!("render_frame failed: {e}");
        }
    }

    fn on_suspend(&mut self) {}

    fn new_events(&mut self) {
        begin_batch();
    }

    fn about_to_wait(&mut self) -> Option<std::time::Duration> {
        end_batch();
        let tree_dirty = self.tree.as_ref().map(|t| t.is_dirty()).unwrap_or(false);
        if tree_dirty {
            // Return the time remaining in the current frame budget so the platform wakes us up exactly when the next 60fps slot opens (or immediately if already past it).
            Some(FRAME_BUDGET.saturating_sub(self.last_frame.elapsed()))
        } else {
            let dev_keepalive = self.dev.keepalive_interval();
            if self.renderer_is_hardware || dev_keepalive.is_some() {
                // Hardware: 1fps minimum to keep the GPU in an active power state; dev plugin: honor its requested keepalive cadence (e.g. FPS counter tick-down).
                Some(dev_keepalive.unwrap_or(std::time::Duration::from_millis(1000)))
            } else {
                None
            }
        }
    }
}

fn hardware_cache_path(app_name: &str) -> Option<std::path::PathBuf> {
    dirs::cache_dir().map(|d| d.join("rsx").join(app_name))
}

fn create_renderer(
    backend: RendererBackend,
    window: &WinitWindow,
    cache_path: Option<&std::path::Path>,
) -> Result<(Box<dyn RenderBackend>, bool), RendererError> {
    match backend {
        RendererBackend::Auto => match HardwareRenderer::new(window.clone(), cache_path) {
            Ok(renderer) => {
                tracing::info!("Using hardware renderer");
                Ok((Box::new(renderer), true))
            }
            Err(e) => {
                tracing::warn!(
                    "Hardware renderer unavailable ({e}), falling back to software renderer"
                );
                SoftwareRenderer::new(window.clone(), window.clone(), RendererBudget::default())
                    .map(|r| (Box::new(r) as Box<dyn RenderBackend>, false))
            }
        },
        RendererBackend::Hardware => HardwareRenderer::new(window.clone(), cache_path).map(|r| {
            tracing::info!("Using hardware renderer");
            (Box::new(r) as Box<dyn RenderBackend>, true)
        }),
        RendererBackend::Software => {
            tracing::info!("Using software renderer");
            SoftwareRenderer::new(window.clone(), window.clone(), RendererBudget::default())
                .map(|r| (Box::new(r) as Box<dyn RenderBackend>, false))
        }
    }
}

fn run_with_plugin<A: App, D: DevPlugin>(config: WindowConfig, app: A, app_name: &str) {
    let prefs = UserPrefs::load(app_name);
    let backend = prefs.backend.unwrap_or_else(config::compile_time_backend);

    let platform = match WinitPlatform::try_new() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to create event loop: {e}");
            return;
        }
    };
    if let Err(e) = platform.run(
        config,
        AppHandler::<D> {
            app: Box::new(app),
            tree: None,
            renderer: None,
            renderer_is_hardware: false,
            backend,
            prefs,
            pending_restart: false,
            _flush_notify: None,
            window_signals: None,
            app_name: app_name.to_owned(),
            last_frame: std::time::Instant::now(),
            dev: D::default(),
        },
    ) {
        tracing::error!("Event loop exited with error: {e}");
    }
}

pub fn run_app_with_name<A: App>(config: WindowConfig, app: A, app_name: &str) {
    #[cfg(feature = "dev")]
    run_with_plugin::<A, rsx_devtools::DevTools>(config, app, app_name);
    #[cfg(not(feature = "dev"))]
    run_with_plugin::<A, ()>(config, app, app_name);
}
