//! The catalogue's previews written in Rust, compiled only under `telar/previews`.

mod button;
#[cfg(feature = "overlays")]
mod modal;
mod select;
mod tabs;
mod text_field;

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;

use telar::preview::PreviewEntry;

pub(crate) fn previews() -> Vec<PreviewEntry> {
    let mut entries = Vec::new();
    entries.extend(button::previews());
    entries.extend(text_field::previews());
    entries.extend(select::previews());
    #[cfg(feature = "overlays")]
    entries.extend(modal::previews());
    entries.extend(tabs::previews());
    entries
}
