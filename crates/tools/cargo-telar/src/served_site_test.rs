//! What the tests of an exported site share: where the repository is, and which file a static host answers an address of the site with.

use std::path::{Path, PathBuf};

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("cargo-telar sits three levels below the repository root")
        .to_path_buf()
}

/// The file a static host answers `address` with: the path percent-decoded, below the site, and a directory's `index.html`. `None` for an address that is not UTF-8 once decoded or that climbs out of the site.
pub fn served_at(site: &Path, address: &str) -> Option<PathBuf> {
    let path = address.split(['?', '#']).next().unwrap_or("/");
    let mut file = site.to_path_buf();
    for segment in path.split('/').filter(|segment| !segment.is_empty()) {
        let segment = percent_decode(segment)?;
        if segment == ".." || segment.contains(['/', '\\', '\0']) {
            return None;
        }
        file.push(segment);
    }
    match file.is_file() {
        true => Some(file),
        false => Some(file.join("index.html")),
    }
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let escaped = (bytes[i] == b'%')
            .then(|| text.get(i + 1..i + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escaped {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}
