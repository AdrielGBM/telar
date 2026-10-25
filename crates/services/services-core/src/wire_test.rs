use std::path::PathBuf;

use wire_core::{decode, encode};

use crate::{FileDialog, FileFilter};

fn dialog() -> FileDialog {
    FileDialog {
        title: Some("Open a document".to_owned()),
        directory: Some(PathBuf::from("/home/user/documents")),
        file_name: Some("notes.md".to_owned()),
        filters: vec![
            FileFilter::new("Markdown", &["md", "markdown"]),
            FileFilter::new("Anything", &[]),
        ],
    }
}

#[test]
fn a_dialog_request_reads_back_as_it_was_spelled() {
    let request = dialog();
    let read = decode::<FileDialog>(&encode(&request)).unwrap();
    assert_eq!(read.title, request.title);
    assert_eq!(read.directory, request.directory);
    assert_eq!(read.file_name, request.file_name);
    assert_eq!(read.filters.len(), 2);
    assert_eq!(read.filters[0].name, "Markdown");
    assert_eq!(read.filters[0].extensions, ["md", "markdown"]);
    assert!(read.filters[1].extensions.is_empty());
}

// A path is spelled as its platform stores it (UTF-16 on Windows), so these bytes are pinned for the Unix spelling only.
#[cfg(unix)]
#[test]
fn the_bytes_of_a_dialog_request_are_pinned_to_the_wire_version() {
    wire_core::assert_golden!("src/wire_golden/dialog.hex", &encode(&dialog()));
}
