use super::*;
use telar_parser::parse;

#[test]
fn links_only_existing_local_img_assets() {
    let dir = std::env::temp_dir().join("rsx_links_test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("logo.png"), b"x").unwrap();

    let src = "[view]\ncol\n    img src:\"logo.png\"\n    img src:\"missing.png\"\n    img src:\"https://x/y.png\"\n    img src:\"{dynamic}\"\n";
    let doc = parse(src).unwrap();
    let links = document_links(&doc, src, &dir);

    assert_eq!(links.len(), 1);
    let target = links[0].target.as_ref().unwrap().as_str();
    assert!(target.ends_with("logo.png"), "target: {target}");
    assert_eq!(
        links[0].range.end.character - links[0].range.start.character,
        8
    );
}

#[test]
fn links_existing_local_svg_assets_too() {
    let dir = std::env::temp_dir().join("rsx_links_svg_test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("icon.svg"), b"<svg></svg>").unwrap();

    let src = "[view]\ncol\n    svg src:\"icon.svg\"\n";
    let doc = parse(src).unwrap();
    let links = document_links(&doc, src, &dir);

    assert_eq!(links.len(), 1);
    let target = links[0].target.as_ref().unwrap().as_str();
    assert!(target.ends_with("icon.svg"), "target: {target}");
}
