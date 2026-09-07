use super::*;
use crate::Color;

fn solid_path(shadow: Option<Shadow>) -> PathStyle {
    PathStyle {
        fill: Some(Paint::Solid(Color::rgba(1.0, 0.0, 0.0, 1.0))),
        stroke: None,
        shadow,
        fill_rule: FillRule::Winding,
    }
}

// `SvgData::id` is derived from this hasher, and renderer-assets used to carry a copy that skipped the shadow — two styles differing only there shared an id, so a cache could hand back the wrong display list.
#[test]
fn hash_path_style_distinguishes_a_shadow() {
    let shadow = Shadow::new(1.0, 2.0, 3.0, Color::rgba(0.0, 0.0, 0.0, 0.5));
    assert_ne!(
        hash_path_style(&solid_path(None)),
        hash_path_style(&solid_path(Some(shadow)))
    );
}

#[test]
fn hash_path_style_distinguishes_two_shadows() {
    let a = Shadow::new(1.0, 2.0, 3.0, Color::rgba(0.0, 0.0, 0.0, 0.5));
    let b = Shadow::new(1.0, 2.0, 4.0, Color::rgba(0.0, 0.0, 0.0, 0.5));
    assert_ne!(
        hash_path_style(&solid_path(Some(a))),
        hash_path_style(&solid_path(Some(b)))
    );
}

#[test]
fn hash_path_style_agrees_with_itself() {
    let shadow = Shadow::new(1.0, 2.0, 3.0, Color::rgba(0.0, 0.0, 0.0, 0.5));
    assert_eq!(
        hash_path_style(&solid_path(Some(shadow))),
        hash_path_style(&solid_path(Some(shadow)))
    );
}
