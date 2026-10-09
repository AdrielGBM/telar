//! The navigation hosts' previews, compiled only under `telar/previews`.

mod nav_host;
mod page;
mod tab_host;

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;

use telar::preview::PreviewEntry;

pub(crate) fn previews() -> Vec<PreviewEntry> {
    [nav_host::previews(), tab_host::previews()].concat()
}
