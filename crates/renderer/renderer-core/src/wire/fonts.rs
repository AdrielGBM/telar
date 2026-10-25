//! How the fonts a shaper loads are spelled, for a guest's copy of the shaper to load what its host's did and to hand the host the faces it adds.
//!
//! A face's bytes cross whole, however the sending side holds them: bytes compiled into a guest live in an image its host unmaps, so the reading side always holds a copy of its own.

use std::sync::Arc;

use wire_core::{Reader, Wire, WireError, Writer, wire_enum, wire_struct};

use crate::{
    ExclusiveFontsError, FontAsset, FontAxis, FontConfig, FontSource, FontSources, FontWeight,
    GenericFamilies,
};

const BYTES: u8 = 0;
const FILE: u8 = 1;

impl Wire for FontSource {
    fn put(&self, w: &mut Writer) {
        match self {
            FontSource::Static(bytes) => {
                w.u8(BYTES);
                w.bytes(bytes);
            }
            FontSource::Shared(bytes) => {
                w.u8(BYTES);
                w.bytes(bytes);
            }
            FontSource::File(path) => {
                w.u8(FILE);
                path.put(w);
            }
        }
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        match r.u8()? {
            BYTES => Ok(FontSource::Shared(Arc::from(r.bytes()?))),
            FILE => Wire::take(r).map(FontSource::File),
            tag => Err(WireError::Tag {
                of: "FontSource",
                tag,
            }),
        }
    }
}

wire_struct!(FontWeight { min, max });
wire_struct!(FontAxis { tag, min, max });
wire_struct!(FontAsset {
    source,
    family,
    weight,
    style,
    axes
});
wire_struct!(FontConfig {
    faces,
    system_fonts_dir,
    sans_serif_family_candidates
});
wire_struct!(GenericFamilies {
    serif,
    sans_serif,
    monospace,
    cursive,
    fantasy
});
wire_struct!(FontSources {
    system_fonts,
    dirs,
    faces,
    sans_serif_family_candidates,
    generic_families
});
wire_enum!(ExclusiveFontsError {
    NoFaces = 0,
    OtherFacesLoaded = 1
});

#[cfg(test)]
#[path = "fonts_test.rs"]
mod tests;
