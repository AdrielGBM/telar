//! [`licenses`]: the icon licence notice the application carries in its binary.

/// The licence notice of the icons this application baked, and of the libraries it is built with that baked any: per set its licence and author, the icons used, and the crate each was baked into. `None` when neither the application nor a library it is built with baked an icon.
///
/// The same text `cargo telar bake` writes to `.telar/ICONS-LICENSES.txt`, compiled into the application as it loads, so a platform whose bundle cannot carry the file beside it — Android, iOS, the web — can still show it, in an "Open source licences" screen:
///
/// ```ignore
/// if let Some(notice) = telar_icons::licenses() {
///     // draw `notice` in a scrollable text
/// }
/// ```
///
/// A `runtime`-mode icon resolves as the application runs and is in no notice: its licence is the provider's to state.
pub fn licenses() -> Option<&'static str> {
    telar::icon_licenses()
}
