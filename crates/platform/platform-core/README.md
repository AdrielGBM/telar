# telar-platform-core

Platform abstraction for Telar: window, event and event-loop traits.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

Besides windows and events it defines `SystemPreferences`: colour scheme, reduced motion, high contrast and
ordered preferred locales. Each backend sends this whole snapshot as `Event::SystemPreferencesChanged` before
a surface first resumes and again whenever any field changes. Every field can be unknown (`None`, or an
empty locale list), and a backend never fills an unknown field with a guess. `locales_from_env` turns a POSIX
environment into ordered BCP 47 tags. See
[docs/system-preferences.md](https://github.com/AdrielGBM/telar/blob/main/docs/system-preferences.md) for
where each target reads them.

**Applications depend on the [`telar`](https://crates.io/crates/telar) facade, not on this crate.** Telar
is split into small crates so a build carries only the target and the capabilities it named, and every one
of them has to be published for the facade to be. The facade re-exports what an application needs behind
feature flags; reach for this crate directly only if you are writing a frontend or a tool against Telar's
internals.

- API documentation: <https://docs.rs/telar-platform-core>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>
