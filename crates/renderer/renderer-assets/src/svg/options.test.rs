use renderer_core::ImageData;

use super::super::BakedSvg;
use super::super::bake::bake;

const RED_SVG: &str = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' width='10' height='10'><rect width='10' height='10' fill='%23ff0000'/></svg>";

fn baked(svg: &str) -> BakedSvg {
    bake(svg).unwrap().1
}

fn embeds_nothing(svg: &str) -> bool {
    match baked(svg) {
        BakedSvg::Vector(commands) => commands.is_empty(),
        BakedSvg::Raster { image, .. } => is_blank(&image),
    }
}

fn is_blank(image: &ImageData) -> bool {
    image.pixels().iter().all(|&byte| byte == 0)
}

fn image_svg(href: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10"><image href="{href}" width="10" height="10"/></svg>"##
    )
}

fn data_uri(svg: &str) -> String {
    format!(
        "data:image/svg+xml;utf8,{}",
        svg.replace('<', "%3C").replace('>', "%3E")
    )
}

#[test]
fn baking_an_absolute_file_href_embeds_nothing() {
    assert!(embeds_nothing(&image_svg("/etc/passwd")));
}

#[test]
fn baking_a_sibling_file_href_embeds_nothing() {
    let dir = std::env::temp_dir().join(format!("telar-bake-href-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("sibling.svg"), RED_SVG.replace("%23", "#")).unwrap();
    let svg = image_svg(&format!("{}/sibling.svg", dir.display()));
    let nothing = embeds_nothing(&svg);
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(nothing);
}

#[test]
fn baking_a_data_uri_image_under_the_cap_draws_it() {
    match baked(&image_svg(&data_uri(RED_SVG))) {
        BakedSvg::Raster { image, .. } => assert!(!is_blank(&image)),
        BakedSvg::Vector(commands) => panic!("data image was dropped: {} commands", commands.len()),
    }
}

#[test]
fn baking_a_data_uri_image_over_the_cap_drops_it() {
    let padded = format!("{RED_SVG}{}", "%20".repeat(8 * 1024 * 1024 + 1));
    assert!(embeds_nothing(&image_svg(&data_uri(&padded))));
}
