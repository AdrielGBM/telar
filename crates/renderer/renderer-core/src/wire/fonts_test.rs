use std::path::PathBuf;

use wire_core::{decode, encode};

use super::*;
use crate::FontStyle;

static FACE: &[u8] = b"not a real face, only its bytes";

fn sources() -> FontSources {
    FontSources {
        system_fonts: true,
        dirs: vec![PathBuf::from("/usr/share/fonts")],
        faces: vec![
            FontAsset::bytes(FACE.to_vec())
                .named("Telar Test")
                .with_weight(FontWeight::range(100, 900))
                .with_style(FontStyle::Italic)
                .with_axis(FontAxis::new(*b"wght", 100.0, 900.0)),
            FontAsset::file("fonts/Inter.ttf"),
        ],
        sans_serif_family_candidates: vec!["Inter".to_owned()],
        generic_families: GenericFamilies {
            serif: "Serif".to_owned(),
            sans_serif: "Inter".to_owned(),
            monospace: "Mono".to_owned(),
            cursive: "Cursive".to_owned(),
            fantasy: "Fantasy".to_owned(),
        },
    }
}

#[test]
fn what_a_shaper_was_loaded_from_reads_back_as_it_was_spelled() {
    let sources = sources();
    assert_eq!(decode::<FontSources>(&encode(&sources)), Ok(sources));
}

#[test]
fn a_face_compiled_into_one_side_arrives_as_bytes_the_other_holds_itself() {
    let read = decode::<FontAsset>(&encode(&FontAsset::embedded(FACE))).unwrap();
    let FontSource::Shared(bytes) = &read.source else {
        panic!("expected bytes of its own, got {:?}", read.source);
    };
    assert_eq!(&bytes[..], FACE);
    assert_ne!(bytes.as_ptr(), FACE.as_ptr());
}

#[test]
fn a_configuration_and_an_exclusive_answer_read_back() {
    let config = FontConfig {
        faces: sources().faces,
        system_fonts_dir: Some(PathBuf::from("/system/fonts")),
        sans_serif_family_candidates: vec!["Roboto".to_owned()],
    };
    let read = decode::<FontConfig>(&encode(&config)).unwrap();
    assert_eq!(read.faces, config.faces);
    assert_eq!(read.system_fonts_dir, config.system_fonts_dir);
    assert_eq!(
        read.sans_serif_family_candidates,
        config.sans_serif_family_candidates
    );
    for answer in [
        Some(Ok(sources())),
        Some(Err(ExclusiveFontsError::NoFaces)),
        Some(Err(ExclusiveFontsError::OtherFacesLoaded)),
        None,
    ] {
        assert_eq!(
            decode::<Option<Result<FontSources, ExclusiveFontsError>>>(&encode(&answer)),
            Ok(answer)
        );
    }
}

// A path is spelled as its platform stores it (UTF-16 on Windows), so these bytes are pinned for the Unix spelling only.
#[cfg(unix)]
#[test]
fn the_bytes_of_the_font_vocabulary_are_pinned_to_the_wire_version() {
    wire_core::assert_golden!("src/wire/fonts_golden/sources.hex", &encode(&sources()));
    let config = FontConfig {
        faces: sources().faces,
        system_fonts_dir: Some(PathBuf::from("/system/fonts")),
        sans_serif_family_candidates: vec!["Roboto".to_owned()],
    };
    wire_core::assert_golden!("src/wire/fonts_golden/config.hex", &encode(&config));
    let answers = vec![
        Some(Ok(sources())),
        Some(Err(ExclusiveFontsError::NoFaces)),
        Some(Err(ExclusiveFontsError::OtherFacesLoaded)),
        None,
    ];
    wire_core::assert_golden!("src/wire/fonts_golden/exclusive.hex", &encode(&answers));
}
