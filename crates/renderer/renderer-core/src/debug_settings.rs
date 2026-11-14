//! The switches a developer flips to measure or A/B the renderers, read once by the runner from wherever its platform keeps them.

/// One switch, named the way each place a runner reads it from spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugFlag {
    /// Turns on the rolling [`perf`](crate::perf) dump.
    Perf,
    /// `0` turns off damage tracking.
    DamageTracking,
    /// `0` turns off the scroll blit.
    ScrollBlit,
}

impl DebugFlag {
    /// The environment variable a desktop or terminal process reads it from.
    pub fn env_var(self) -> &'static str {
        match self {
            Self::Perf => "TELAR_PERF",
            Self::DamageTracking => "TELAR_HW_DAMAGE",
            Self::ScrollBlit => "TELAR_HW_SCROLL_BLIT",
        }
    }

    /// The Android system property it is read from, since an app process there inherits no shell environment and `adb shell setprop` sets these without root.
    pub fn system_property(self) -> &'static str {
        match self {
            Self::Perf => "debug.telar.perf",
            Self::DamageTracking => "debug.telar.hw_damage",
            Self::ScrollBlit => "debug.telar.scroll_blit",
        }
    }
}

/// What the switches were set to for this run. [`Default`] is a run where none was set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DebugSettings {
    /// Logs the per-phase frame timings of [`perf`](crate::perf) about once a second.
    pub perf: bool,
    /// Lets the hardware renderer repaint only the region a frame changed rather than all of it.
    pub damage_tracking: bool,
    /// Lets the hardware renderer move a scrolled region's pixels rather than repaint them.
    pub scroll_blit: bool,
}

impl Default for DebugSettings {
    fn default() -> Self {
        Self {
            perf: false,
            damage_tracking: true,
            scroll_blit: true,
        }
    }
}

impl DebugSettings {
    /// Reads each flag through `value_of`, which answers `None` for one that is unset. An empty value counts as unset.
    pub fn read(mut value_of: impl FnMut(DebugFlag) -> Option<String>) -> Self {
        let mut value = |flag| value_of(flag).filter(|v| !v.is_empty());
        let off = |v: &Option<String>| v.as_deref() == Some("0");
        let perf = value(DebugFlag::Perf);
        Self {
            perf: perf.is_some() && !off(&perf),
            damage_tracking: !off(&value(DebugFlag::DamageTracking)),
            scroll_blit: !off(&value(DebugFlag::ScrollBlit)),
        }
    }
}

#[cfg(test)]
#[path = "debug_settings.test.rs"]
mod tests;
