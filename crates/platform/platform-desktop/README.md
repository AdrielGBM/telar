# telar-platform-desktop

Desktop platform backend for Telar: winit event loop, system paths and OS preference detection.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

Reads the OS theme, `SPI_GETCLIENTAREAANIMATION`/`SPI_GETHIGHCONTRAST` on Windows,
`accessibilityDisplayShouldReduceMotion`/`accessibilityDisplayShouldIncreaseContrast` on macOS, and the
settings portal on Linux, into a `SystemPreferences` snapshot on theme change and on the window regaining
focus. See
[docs/system-preferences.md](https://github.com/AdrielGBM/telar/blob/main/docs/system-preferences.md).

**Applications depend on the [`telar`](https://crates.io/crates/telar) facade, not on this crate.** Telar
is split into small crates so a build carries only the target and the capabilities it named, and every one
of them has to be published for the facade to be. The facade re-exports what an application needs behind
feature flags; reach for this crate directly only if you are writing a frontend or a tool against Telar's
internals.

- API documentation: <https://docs.rs/telar-platform-desktop>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>
