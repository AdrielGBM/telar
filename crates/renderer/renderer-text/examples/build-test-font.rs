//! Regenerates `test-fonts/` from Inter 4.1: Telar Test, the copies of it that declare another ascent and descent, and the copy that paints the letter L as a COLR v1 glyph. `cargo run -p telar-renderer-text --example build-test-font -- "$(nix build nixpkgs#inter --no-link --print-out-paths)/share/fonts/truetype/InterVariable.ttf"`

use std::error::Error;
use std::path::{Path, PathBuf};

use skera::{Plan, SubsetFlags, subset_font};
use write_fonts::FontBuilder;
use write_fonts::from_obj::ToOwnedTable;
use write_fonts::read::collections::IntSet;
use write_fonts::read::tables::glyf::Glyph;
use write_fonts::read::tables::{glyf::Glyf, loca::Loca};
use write_fonts::read::{FontRef, TableProvider};
use write_fonts::tables::colr::{
    BaseGlyphList, BaseGlyphPaint, Colr, Paint, PaintGlyph, PaintSolid,
};
use write_fonts::tables::cpal::{ColorRecord, Cpal};
use write_fonts::tables::head::Head;
use write_fonts::tables::hhea::Hhea;
use write_fonts::tables::maxp::Maxp;
use write_fonts::tables::name::{Name, NameRecord};
use write_fonts::tables::os2::Os2;
use write_fonts::types::{F2Dot14, GlyphId, GlyphId16, NameId, Tag};

const MICROSOFT_PLATFORM: u16 = 3;
const UNICODE_BMP_ENCODING: u16 = 1;
const ENGLISH_US: u16 = 0x409;

fn unicodes() -> Vec<u32> {
    let mut set: Vec<u32> = (0x20..0x7F).collect();
    set.extend([0xA0, 0x2002, 0x2003, 0x2009, 0x200A, 0x202F]);
    set.extend([0xA2, 0xA3, 0xA5, 0x20AC]);
    set.extend([0xA7, 0xB6, 0x2020, 0x2021, 0x2030, 0x2032, 0x2033]);
    set.extend([0xA9, 0xAE, 0x2122, 0xB0]);
    set.extend([0x2012, 0x2013, 0x2014, 0x2026, 0xB7, 0x2022]);
    set.extend([0xD7, 0xF7, 0xB1, 0x2212, 0x2248, 0x2260, 0x2264, 0x2265]);
    set.extend([0x2190, 0x2191, 0x2192, 0x2193]);
    set.extend([0x2018, 0x2019, 0x201C, 0x201D, 0xAB, 0xBB, 0x2039, 0x203A]);
    set.extend([0x2318, 0x2325, 0x21E7, 0x2303, 0x23CE, 0x232B, 0x21E5]);
    set
}

fn subset(source: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let font = FontRef::new(source)?;
    let mut codepoints = IntSet::<u32>::empty();
    codepoints.extend(unicodes());
    let mut layout_features = IntSet::<Tag>::empty();
    layout_features.insert(Tag::new(b"kern"));
    let name_ids: IntSet<NameId> = (0..=255).map(NameId::new).collect();
    let plan = Plan::new(
        &IntSet::<GlyphId>::empty(),
        &codepoints,
        &font,
        SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE | SubsetFlags::SUBSET_FLAGS_GLYPH_NAMES,
        &IntSet::<Tag>::empty(),
        &IntSet::<Tag>::all(),
        &layout_features,
        &name_ids,
        &IntSet::<u16>::all(),
    );
    Ok(subset_font(&font, &plan)?)
}

/// A face the example writes: Telar Test itself, a copy of it that declares another ascent and descent, for tests of where a line sits in its box whatever a face declares, or a copy that paints the letter L as a COLR v1 colour glyph.
struct Face {
    file: &'static str,
    family: &'static str,
    postscript: &'static str,
    /// The ascender and descender declared in place of Inter's, in font units.
    vertical: Option<(i16, i16)>,
    /// Whether the letter L is a COLR v1 glyph painted in `COLOR_GLYPH_PAINT` at `COLOR_GLYPH_ALPHA`.
    colored: bool,
}

const COLOR_GLYPH_PAINT: ColorRecord = ColorRecord {
    red: 255,
    green: 0,
    blue: 0,
    alpha: 255,
};
const COLOR_GLYPH_ALPHA: f32 = 0.5;

const FACES: [Face; 4] = [
    Face {
        file: "TelarTest.ttf",
        family: "Telar Test",
        postscript: "TelarTest",
        vertical: None,
        colored: false,
    },
    // Room above the capitals for stacked accents, as a platform's UI face reserves, so a line centred on what it declares hangs its descenders below a 1.2 em line box.
    Face {
        file: "TelarTestHighAscent.ttf",
        family: "Telar Test High Ascent",
        postscript: "TelarTestHighAscent",
        vertical: Some((2560, -494)),
        colored: false,
    },
    // A descent so deep that a line centred on what it declares lifts its ascenders above a 1.2 em line box.
    Face {
        file: "TelarTestDeepDescent.ttf",
        family: "Telar Test Deep Descent",
        postscript: "TelarTestDeepDescent",
        vertical: Some((1984, -2048)),
        colored: false,
    },
    Face {
        file: "TelarTestColor.ttf",
        family: "Telar Test Color",
        postscript: "TelarTestColor",
        vertical: None,
        colored: true,
    },
];

fn rename(subset: &[u8], face: &Face) -> Result<Vec<u8>, Box<dyn Error>> {
    let font = FontRef::new(subset)?;
    let mut name: Name = font.name()?.to_owned_table();
    let copyright = name
        .name_record
        .iter()
        .find(|record| {
            record.name_id == NameId::COPYRIGHT_NOTICE
                && record.platform_id == MICROSOFT_PLATFORM
                && record.language_id == ENGLISH_US
        })
        .map(|record| (*record.string).trim_end_matches('.').to_owned())
        .ok_or("the source font has no English copyright record")?;
    let Face {
        family, postscript, ..
    } = face;
    let altered = if face.vertical.is_some() {
        ", with its declared ascent and descent changed"
    } else if face.colored {
        ", with the letter L painted as a COLR v1 colour glyph"
    } else {
        ""
    };
    let replacements = [
        (
            NameId::COPYRIGHT_NOTICE,
            format!(
                "{copyright}. {family} is a Basic Latin and typographic-symbol subset of Inter 4.1, renamed{altered}."
            ),
        ),
        (NameId::FAMILY_NAME, (*family).to_owned()),
        (
            NameId::UNIQUE_ID,
            format!("{postscript}-Regular;subset of Inter 4.1"),
        ),
        (NameId::FULL_NAME, format!("{family} Regular")),
        (NameId::POSTSCRIPT_NAME, format!("{postscript}-Regular")),
        (NameId::new(25), (*postscript).to_owned()),
    ];
    for (name_id, value) in replacements {
        name.name_record
            .retain(|record| !is_english_record(record, name_id));
        name.name_record.push(NameRecord::new(
            MICROSOFT_PLATFORM,
            UNICODE_BMP_ENCODING,
            ENGLISH_US,
            name_id,
            value.into(),
        ));
    }
    name.name_record.sort_by_key(|record| {
        (
            record.platform_id,
            record.encoding_id,
            record.language_id,
            record.name_id,
        )
    });
    let mut builder = FontBuilder::new();
    builder.add_table(&name)?;
    add_recomputed_metrics(&font, &mut builder, face.vertical)?;
    if face.colored {
        add_color_tables(&font, &mut builder)?;
    }
    builder.copy_missing_tables(font);
    Ok(builder.build())
}

fn is_english_record(record: &NameRecord, name_id: NameId) -> bool {
    record.name_id == name_id
        && record.platform_id == MICROSOFT_PLATFORM
        && record.encoding_id == UNICODE_BMP_ENCODING
        && record.language_id == ENGLISH_US
}

fn main() -> Result<(), Box<dyn Error>> {
    let source = std::env::args()
        .nth(1)
        .ok_or("usage: build-test-font <InterVariable.ttf> [directory]")?;
    let directory = std::env::args().nth(2).map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("test-fonts"),
        PathBuf::from,
    );
    let subset = subset(&std::fs::read(source)?)?;
    for face in &FACES {
        std::fs::write(directory.join(face.file), rename(&subset, face)?)?;
    }
    Ok(())
}

const LATIN_1_CODE_PAGE: u32 = 1;

fn point_and_contour_counts(
    glyf: &Glyf,
    loca: &Loca,
    glyph_id: GlyphId,
) -> Result<(u32, u32), Box<dyn Error>> {
    Ok(match loca.get_glyf(glyph_id, glyf)? {
        None => (0, 0),
        Some(Glyph::Simple(simple)) => (
            simple.num_points() as u32,
            simple.number_of_contours() as u32,
        ),
        Some(Glyph::Composite(composite)) => {
            let mut total = (0, 0);
            for component in composite.components() {
                let (points, contours) = point_and_contour_counts(glyf, loca, component.glyph)?;
                total = (total.0 + points, total.1 + contours);
            }
            total
        }
    })
}

fn add_recomputed_metrics(
    font: &FontRef,
    builder: &mut FontBuilder,
    vertical: Option<(i16, i16)>,
) -> Result<(), Box<dyn Error>> {
    let glyf = font.glyf()?;
    let loca = font.loca(None)?;
    let hmtx = font.hmtx()?;
    let mut head: Head = font.head()?.to_owned_table();
    let mut hhea: Hhea = font.hhea()?.to_owned_table();
    let mut maxp: Maxp = font.maxp()?.to_owned_table();
    let mut os2: Os2 = font.os2()?.to_owned_table();

    let (mut x_min, mut y_min, mut x_max, mut y_max) = (i16::MAX, i16::MAX, i16::MIN, i16::MIN);
    let (mut min_lsb, mut min_rsb, mut max_extent) = (i16::MAX, i16::MAX, i16::MIN);
    let mut advance_max = 0;
    let (mut max_points, mut max_contours) = (0, 0);
    let (mut max_composite_points, mut max_composite_contours, mut max_components) = (0, 0, 0);
    for index in 0..maxp.num_glyphs {
        let glyph_id = GlyphId::from(index);
        let advance = hmtx.advance(glyph_id).ok_or("missing advance")?;
        advance_max = advance_max.max(advance);
        let Some(glyph) = loca.get_glyf(glyph_id, &glyf)? else {
            continue;
        };
        let (points, contours) = point_and_contour_counts(&glyf, &loca, glyph_id)?;
        let (glyph_x_min, glyph_x_max, glyph_y_min, glyph_y_max, is_composite) = match &glyph {
            Glyph::Simple(g) => (g.x_min(), g.x_max(), g.y_min(), g.y_max(), false),
            Glyph::Composite(g) => (g.x_min(), g.x_max(), g.y_min(), g.y_max(), true),
        };
        if is_composite {
            max_composite_points = max_composite_points.max(points);
            max_composite_contours = max_composite_contours.max(contours);
            if let Glyph::Composite(g) = &glyph {
                max_components = max_components.max(g.components().count());
            }
        } else {
            max_points = max_points.max(points);
            max_contours = max_contours.max(contours);
        }
        let lsb = hmtx.side_bearing(glyph_id).ok_or("missing bearing")?;
        let width = glyph_x_max - glyph_x_min;
        x_min = x_min.min(glyph_x_min);
        y_min = y_min.min(glyph_y_min);
        x_max = x_max.max(glyph_x_max);
        y_max = y_max.max(glyph_y_max);
        min_lsb = min_lsb.min(lsb);
        min_rsb = min_rsb.min(i16::try_from(advance)? - lsb - width);
        max_extent = max_extent.max(lsb + width);
    }

    (head.x_min, head.y_min, head.x_max, head.y_max) = (x_min, y_min, x_max, y_max);
    hhea.advance_width_max = advance_max.into();
    hhea.min_left_side_bearing = min_lsb.into();
    hhea.min_right_side_bearing = min_rsb.into();
    hhea.x_max_extent = max_extent.into();
    maxp.max_points = Some(max_points as u16);
    maxp.max_contours = Some(max_contours as u16);
    maxp.max_composite_points = Some(max_composite_points as u16);
    maxp.max_composite_contours = Some(max_composite_contours as u16);
    maxp.max_component_elements = Some(max_components as u16);
    os2.ul_code_page_range_1 = Some(LATIN_1_CODE_PAGE);
    if let Some((ascender, descender)) = vertical {
        hhea.ascender = ascender.into();
        hhea.descender = descender.into();
        os2.s_typo_ascender = ascender;
        os2.s_typo_descender = descender;
        os2.us_win_ascent = u16::try_from(ascender)?;
        os2.us_win_descent = u16::try_from(-descender)?;
    }
    builder
        .add_table(&head)?
        .add_table(&hhea)?
        .add_table(&maxp)?
        .add_table(&os2)?;
    Ok(())
}

fn add_color_tables(font: &FontRef, builder: &mut FontBuilder) -> Result<(), Box<dyn Error>> {
    let letter = font
        .cmap()?
        .map_codepoint('L')
        .ok_or("the subset has no letter L")?;
    let glyph = GlyphId16::try_from(letter)?;
    let paint: Paint = PaintGlyph::new(
        PaintSolid::new(0, F2Dot14::from_f32(COLOR_GLYPH_ALPHA)).into(),
        glyph,
    )
    .into();
    let mut colr = Colr::default();
    colr.base_glyph_list.set(BaseGlyphList::new(
        1,
        vec![BaseGlyphPaint::new(glyph, paint)],
    ));
    let mut cpal = Cpal {
        num_palette_entries: 1,
        num_palettes: 1,
        num_color_records: 1,
        color_record_indices: vec![0],
        ..Default::default()
    };
    cpal.color_records_array.set(vec![COLOR_GLYPH_PAINT]);
    builder.add_table(&colr)?.add_table(&cpal)?;
    Ok(())
}
