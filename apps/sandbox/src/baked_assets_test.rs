//! The artifact `cargo telar bake` writes, compiled and evaluated as the app itself compiles it.
//!
//! Every other test of the asset chain stops one step short. `renderer-assets` proves a baked display list
//! equals the dynamic one, and `telar-baker` proves the baker emits the right Rust *as a string* — but
//! nothing evaluated that string. That gap is not theoretical: the generated module imported only each
//! kind's `data_ty`, so it named `VectorCommand` and `PathData` without importing them and had never
//! compiled, which went unnoticed for as long as nothing included it.
//!
//! This runs against `.telar/assets.rs` for real, so it needs the artifact to exist — which is what
//! `cargo telar bake` (and every `cargo telar` subcommand that compiles) produces, and what CI runs first.

use telar::{Color, ObjectFit, SvgData};

/// `static_name_for_path("badge.svg")`, which is FNV-1a over the path and so fixed by the algorithm rather
/// than by a counter — writing it out is what checks the transpiler still emits the name the baker chose.
use sandbox::__rsx_assets::{ASSET_327B2D38 as BADGE_SVG, ASSET_A2C18B25 as DOT_PNG};

const BADGE_SOURCE: &str = include_str!("../assets/badge.svg");

#[test]
fn the_baked_svg_draws_what_the_dynamic_one_draws() {
    let dynamic = SvgData::from_str(BADGE_SOURCE).expect("the fixture parses");

    for (w, h) in [(24.0, 24.0), (48.0, 24.0), (24.0, 48.0)] {
        for tint in [None, Some(Color::rgba(1.0, 0.0, 0.0, 1.0))] {
            for fit in [ObjectFit::Contain, ObjectFit::Cover, ObjectFit::Fill] {
                assert_eq!(
                    *BADGE_SVG.commands_for(w, h, tint, None, fit),
                    *dynamic.commands_for(w, h, tint, None, fit),
                    "baked != dynamic at ({w}x{h}) tint={tint:?} fit={fit:?}"
                );
            }
        }
    }
}

#[test]
fn the_baked_svg_keeps_the_documents_own_size() {
    let (w, h) = BADGE_SVG.intrinsic_size();
    let (dw, dh) = SvgData::from_str(BADGE_SOURCE).unwrap().intrinsic_size();
    assert!((w - dw).abs() < 1e-3 && (h - dh).abs() < 1e-3, "{w}x{h}");
}

/// No decoder is linked here, so the pixels cannot be re-derived — but geometry surviving the round trip
/// through generated source is what a wrong `Arc::new(..)` or a truncated payload would break.
#[test]
fn the_baked_image_carries_the_files_dimensions() {
    let png = include_bytes!("../assets/dot.png");
    let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(png[20..24].try_into().unwrap());

    assert_eq!((DOT_PNG.width, DOT_PNG.height), (width, height));
}
