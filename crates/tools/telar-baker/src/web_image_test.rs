use super::*;

const DOT_PNG: &[u8] = include_bytes!("../fixtures/dot.png");

#[test]
fn files_are_named_by_the_content_they_hold() {
    let web = web_image(DOT_PNG).unwrap();
    let hash = telar_project::content_hash(DOT_PNG);
    assert_eq!(web.full.path, format!("images/{hash}.png"));
    assert_eq!(web.full.bytes, DOT_PNG);
    assert!(web.copies.is_empty());
}
