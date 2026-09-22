//! The desktop's [`SystemPreferences`]: read from each OS's own source, and kept current as one snapshot the runners deliver whole.
//!
//! | OS | Colour scheme | Reduced motion | High contrast | Locales | Changes |
//! | --- | --- | --- | --- | --- | --- |
//! | Linux | settings portal | portal `reduced-motion` | portal `contrast` | `LANGUAGE`/`LC_*`/`LANG` | portal `SettingChanged` |
//! | Windows | winit | `SPI_GETCLIENTAREAANIMATION` | `SPI_GETHIGHCONTRAST` | `GetUserPreferredUILanguages` | `WM_SETTINGCHANGE` to a hidden listener window, winit theme change |
//! | macOS | winit | `accessibilityDisplayShouldReduceMotion` | `accessibilityDisplayShouldIncreaseContrast` | `NSLocale.preferredLanguages` | accessibility-options and current-locale notifications, winit theme change |

#[cfg(any(test, all(target_os = "linux", feature = "system-theme")))]
pub mod appearance;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(all(target_os = "linux", feature = "system-theme"))]
pub mod portal;
#[cfg(any(test, target_os = "windows"))]
mod wide;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
pub use macos::watch;
#[cfg(target_os = "windows")]
pub use windows::watch;

use platform_core::{ColorScheme, SystemPreferences};
use platform_winit::WinitWindow;

/// Everything the OS answers right now. Blocking on Linux, where it is a D-Bus round trip.
pub fn read(window: &WinitWindow) -> SystemPreferences {
    let winit_scheme = window.0.theme().map(|theme| match theme {
        winit::window::Theme::Dark => ColorScheme::Dark,
        winit::window::Theme::Light => ColorScheme::Light,
    });
    read_os(winit_scheme)
}

#[cfg(all(target_os = "linux", feature = "system-theme"))]
fn read_os(winit_scheme: Option<ColorScheme>) -> SystemPreferences {
    let portal = portal::read();
    SystemPreferences {
        color_scheme: portal.color_scheme.or(winit_scheme),
        locales: platform_core::system_locales_from_env(),
        ..portal
    }
}

#[cfg(target_os = "windows")]
fn read_os(winit_scheme: Option<ColorScheme>) -> SystemPreferences {
    SystemPreferences {
        color_scheme: winit_scheme,
        reduced_motion: windows::reduced_motion(),
        high_contrast: windows::high_contrast(),
        locales: windows::locales(),
    }
}

#[cfg(target_os = "macos")]
fn read_os(winit_scheme: Option<ColorScheme>) -> SystemPreferences {
    SystemPreferences {
        color_scheme: winit_scheme,
        reduced_motion: macos::reduced_motion(),
        high_contrast: macos::high_contrast(),
        locales: macos::locales(),
    }
}

#[cfg(not(any(
    all(target_os = "linux", feature = "system-theme"),
    target_os = "windows",
    target_os = "macos"
)))]
fn read_os(winit_scheme: Option<ColorScheme>) -> SystemPreferences {
    SystemPreferences {
        color_scheme: winit_scheme,
        locales: platform_core::system_locales_from_env(),
        ..SystemPreferences::default()
    }
}

/// The snapshot last delivered, so a re-read or a single changed key only produces an event when something moved.
#[derive(Default)]
pub struct PreferencesTracker {
    current: Option<SystemPreferences>,
}

impl PreferencesTracker {
    /// Reads afresh and remembers it, for a surface about to resume: it is owed the snapshot whether or not it changed.
    pub fn refresh(&mut self, window: &WinitWindow) -> SystemPreferences {
        let preferences = read(window);
        self.current = Some(preferences.clone());
        preferences
    }

    /// The remembered snapshot, reading it first if there is none yet.
    pub fn current(&mut self, window: &WinitWindow) -> SystemPreferences {
        match &self.current {
            Some(preferences) => preferences.clone(),
            None => self.refresh(window),
        }
    }

    /// Reads again, answering the new snapshot only if it differs from the remembered one.
    pub fn reread(&mut self, window: &WinitWindow) -> Option<SystemPreferences> {
        self.update(read(window))
    }

    pub fn update(&mut self, next: SystemPreferences) -> Option<SystemPreferences> {
        if self.current.as_ref() == Some(&next) {
            return None;
        }
        self.current = Some(next.clone());
        Some(next)
    }

    /// Changes one part of the remembered snapshot, answering the whole of it if that changed anything. `None` before the first read: whichever surface resumes first reads everything then.
    #[cfg(any(test, all(target_os = "linux", feature = "system-theme")))]
    pub fn modify(
        &mut self,
        change: impl FnOnce(&mut SystemPreferences),
    ) -> Option<SystemPreferences> {
        let mut next = self.current.clone()?;
        change(&mut next);
        self.update(next)
    }
}

#[cfg(test)]
#[path = "tracker_test.rs"]
mod tests;
