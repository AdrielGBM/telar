use super::*;

/// The one thing a decoder must not do is accept bytes that are not a font: a file that fails to load and reports success installs nothing and leaves the caller a family name nothing answers to.
#[test]
fn bytes_that_are_not_a_font_fail() {
    assert!(FontDecoder.decode(b"not a font").is_err());
    assert!(FontDecoder.decode(&[]).is_err());
}

/// A real face reports the family its own name table declares — not one the caller chose, which is the whole reason the decoded value is a name at all.
#[test]
fn a_real_face_reports_the_family_it_declares() {
    let ttf = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/sandbox/assets/fonts/DejaVuSans.ttf");
    let Ok(bytes) = std::fs::read(&ttf) else {
        eprintln!("skipping: no face at {}", ttf.display());
        return;
    };
    let family = FontDecoder.decode(&bytes).expect("a real face decodes");
    assert_eq!(&*family, "DejaVu Sans");
}
