use super::*;

const SET: &str = r#"{
    "prefix": "demo",
    "width": 24,
    "height": 24,
    "info": {
        "name": "Demo Icons",
        "author": { "name": "Someone", "url": "https://example.com" },
        "license": { "title": "MIT", "spdx": "MIT", "url": "https://example.com/LICENSE" },
        "category": "General",
        "palette": false
    },
    "icons": {
        "home": { "body": "<path d=\"M1 1h22v22H1z\" fill=\"currentColor\"/>" },
        "wide": { "body": "<path d=\"M0 0h32v16H0z\"/>", "width": 32, "height": 16 },
        "shifted": { "body": "<path d=\"M0 0h1v1H0z\"/>", "left": -2, "top": 4 }
    },
    "aliases": {
        "house": { "parent": "home" },
        "house-left": { "parent": "home", "rotate": 3 },
        "house-mirrored": { "parent": "house-left", "hFlip": true, "rotate": 1 },
        "house-tall": { "parent": "home", "height": 48 },
        "loop-a": { "parent": "loop-b" },
        "loop-b": { "parent": "loop-a" },
        "orphan": { "parent": "missing" }
    },
    "lastModified": 1700000000
}"#;

fn set() -> IconSet {
    IconSet::from_json(SET).unwrap()
}

#[test]
fn an_icon_takes_the_sets_default_box() {
    let icon = set().icon("home").unwrap();
    assert_eq!(
        (icon.left, icon.top, icon.width, icon.height),
        (0.0, 0.0, 24.0, 24.0)
    );
    assert_eq!(icon.rotate, 0);
    assert!(!icon.h_flip && !icon.v_flip);
}

#[test]
fn an_icon_box_wins_over_the_sets() {
    let wide = set().icon("wide").unwrap();
    assert_eq!((wide.width, wide.height), (32.0, 16.0));
    let shifted = set().icon("shifted").unwrap();
    assert_eq!(
        (shifted.left, shifted.top, shifted.width),
        (-2.0, 4.0, 24.0)
    );
}

#[test]
fn a_set_without_a_box_is_iconifys_sixteen_square() {
    let set =
        IconSet::from_json(r#"{"prefix":"bare","icons":{"dot":{"body":"<circle r=\"1\"/>"}}}"#)
            .unwrap();
    let dot = set.icon("dot").unwrap();
    assert_eq!((dot.width, dot.height), (16.0, 16.0));
}

#[test]
fn an_alias_resolves_to_its_parents_body() {
    let set = set();
    assert_eq!(set.icon("house").unwrap(), set.icon("home").unwrap());
}

#[test]
fn an_alias_box_wins_over_its_parents() {
    let tall = set().icon("house-tall").unwrap();
    assert_eq!((tall.width, tall.height), (24.0, 48.0));
}

#[test]
fn transforms_compose_along_the_chain() {
    let left = set().icon("house-left").unwrap();
    assert_eq!(left.rotate, 3);
    let mirrored = set().icon("house-mirrored").unwrap();
    assert_eq!(
        mirrored.rotate, 0,
        "three quarter turns and one more wrap to none"
    );
    assert!(mirrored.h_flip);
}

#[test]
fn a_cycle_or_a_dangling_alias_is_not_an_icon() {
    let set = set();
    assert!(set.icon("loop-a").is_none());
    assert!(set.icon("orphan").is_none());
    assert!(set.icon("nothing").is_none());
}

#[test]
fn the_sets_info_is_read() {
    let info = set().info.unwrap();
    assert_eq!(info.name.as_deref(), Some("Demo Icons"));
    let license = info.license.unwrap();
    assert_eq!(license.spdx.as_deref(), Some("MIT"));
    assert_eq!(info.author.unwrap().name.as_deref(), Some("Someone"));
    assert_eq!(info.palette, Some(false));
}

#[test]
fn an_api_response_lists_what_it_does_not_have() {
    let set = IconSet::from_json(
        r#"{"prefix":"mdi","icons":{"home":{"body":"<g/>"}},"width":24,"height":24,"not_found":["nope"]}"#,
    )
    .unwrap();
    assert_eq!(set.not_found, vec!["nope".to_string()]);
}

#[test]
fn svg_is_sized_to_the_view_box() {
    let svg = set().icon("home").unwrap().to_svg();
    assert!(
        svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""),
        "{svg}"
    );
    assert!(
        svg.contains("width=\"24\" height=\"24\" viewBox=\"0 0 24 24\""),
        "{svg}"
    );
    assert!(
        svg.contains("<path d=\"M1 1h22v22H1z\" fill=\"currentColor\"/>"),
        "{svg}"
    );
    assert!(
        !svg.contains("<g transform"),
        "an untransformed icon gets no group: {svg}"
    );
}

#[test]
fn a_quarter_turn_swaps_the_box_and_wraps_the_body() {
    let svg = set()
        .icon("wide")
        .map(|icon| Icon { rotate: 1, ..icon })
        .unwrap()
        .to_svg();
    assert!(
        svg.contains("width=\"16\" height=\"32\" viewBox=\"0 0 16 32\""),
        "{svg}"
    );
    assert!(svg.contains("<g transform=\"rotate(90 8 8)\">"), "{svg}");
}

#[test]
fn a_horizontal_flip_mirrors_across_the_box() {
    let svg = set()
        .icon("home")
        .map(|icon| Icon {
            h_flip: true,
            ..icon
        })
        .unwrap()
        .to_svg();
    assert!(
        svg.contains("<g transform=\"translate(24 0) scale(-1 1)\">"),
        "{svg}"
    );
}

#[test]
fn both_flips_are_a_half_turn() {
    let svg = set()
        .icon("home")
        .map(|icon| Icon {
            h_flip: true,
            v_flip: true,
            ..icon
        })
        .unwrap()
        .to_svg();
    assert!(svg.contains("<g transform=\"rotate(180 12 12)\">"), "{svg}");
}
