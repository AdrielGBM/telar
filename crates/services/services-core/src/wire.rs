//! The byte spelling of what a guest asks its host's services for.

use wire_core::wire_struct;

use crate::{FileDialog, FileFilter};

wire_struct!(FileFilter { name, extensions });
wire_struct!(FileDialog {
    title,
    directory,
    file_name,
    filters
});

#[cfg(test)]
#[path = "wire_test.rs"]
mod tests;
