//! Where a run's [`DebugSettings`] come from: Android system properties on Android, the environment everywhere else.

use std::sync::OnceLock;

use renderer_core::{DebugFlag, DebugSettings};

/// This process's settings, read and installed by whichever entry point asks first; every runner and surface then passes this one value along.
pub(crate) fn process_debug_settings() -> DebugSettings {
    // Process-wide because its source is: a window opened at runtime or a texture UI has no runner of its own to be handed the value by.
    static SETTINGS: OnceLock<DebugSettings> = OnceLock::new();
    *SETTINGS.get_or_init(|| {
        let settings = DebugSettings::read(flag_value);
        renderer_core::perf::set_dump(settings.perf);
        settings
    })
}

#[cfg(all(feature = "android-bare", target_os = "android"))]
fn flag_value(flag: DebugFlag) -> Option<String> {
    platform_android::read_sys_prop(flag.system_property())
}

#[cfg(not(all(feature = "android-bare", target_os = "android")))]
fn flag_value(flag: DebugFlag) -> Option<String> {
    std::env::var(flag.env_var()).ok()
}
