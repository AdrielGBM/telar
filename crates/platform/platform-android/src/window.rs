//! The activity's window: winit's surface, plus what only the activity can do for it.

use std::sync::Arc;

use android_activity::AndroidApp;
use jni::objects::JValue;
use jni::{jni_sig, jni_str};
use platform_core::Window;
use platform_winit::WinitWindow;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};

/// A winit window on an activity, which keeps the activity at hand for what winit cannot reach on Android: the title is the task's description, the label the recents screen shows the task under.
#[derive(Clone)]
pub struct AndroidWindow {
    surface: WinitWindow,
    app: AndroidApp,
}

impl AndroidWindow {
    pub fn new(window: Arc<winit::window::Window>, app: AndroidApp) -> Self {
        Self {
            surface: WinitWindow(window),
            app,
        }
    }

    /// The winit window this draws on.
    pub fn surface(&self) -> &WinitWindow {
        &self.surface
    }
}

impl HasWindowHandle for AndroidWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.surface.window_handle()
    }
}

impl HasDisplayHandle for AndroidWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.surface.display_handle()
    }
}

impl Window for AndroidWindow {
    fn redraw_waker(&self) -> Option<Arc<dyn Fn() + Send + Sync>> {
        Some(platform_core::window_waker(self))
    }

    fn width(&self) -> u32 {
        self.surface.width()
    }

    fn height(&self) -> u32 {
        self.surface.height()
    }

    fn request_redraw(&self) {
        self.surface.request_redraw();
    }

    fn scale_factor(&self) -> f64 {
        self.surface.scale_factor()
    }

    fn retains_presented_contents(&self) -> bool {
        self.surface.retains_presented_contents()
    }

    fn set_title(&self, title: &str) {
        if let Err(error) = describe_task(&self.app, title) {
            tracing::warn!(%error, "could not set the task description");
        }
    }
}

// `TaskDescription(String)` rather than its builder, which only exists from API 33.
fn describe_task(app: &AndroidApp, label: &str) -> jni::errors::Result<()> {
    crate::intent::with_activity(app, |env, activity| {
        let label = env.new_string(label)?;
        let description = env.new_object(
            jni_str!("android/app/ActivityManager$TaskDescription"),
            jni_sig!("(Ljava/lang/String;)V"),
            &[JValue::Object(&label)],
        )?;
        env.call_method(
            activity,
            jni_str!("setTaskDescription"),
            jni_sig!("(Landroid/app/ActivityManager$TaskDescription;)V"),
            &[JValue::Object(&description)],
        )?;
        Ok(())
    })
}
