# telar-platform-android

Android platform backend for Telar, built on android-activity.

The window's title is the task's description (`Activity.setTaskDescription`), the label the recents screen
shows. See [docs/surface-title.md](https://github.com/AdrielGBM/telar/blob/main/docs/surface-title.md).

With the `a11y` feature, on by default, a screen reader reads the window: the accessibility tree the desktop
publishes, hung on the activity's decor view through AccessKit's Android adapter (API 26 and later), and what
the reader asks of it carried back to the UI as the actions a tap or a key takes. See
[docs/accessibility.md](https://github.com/AdrielGBM/telar/blob/main/docs/accessibility.md#the-android-tree).

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

**Applications depend on the [`telar`](https://crates.io/crates/telar) facade, not on this crate.** Telar
is split into small crates so a build carries only the target and the capabilities it named, and every one
of them has to be published for the facade to be. The facade re-exports what an application needs behind
feature flags; reach for this crate directly only if you are writing a frontend or a tool against Telar's
internals.

- API documentation: <https://docs.rs/telar-platform-android>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>
