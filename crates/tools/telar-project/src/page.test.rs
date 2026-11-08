use super::*;

#[test]
fn every_page_renderer_reads_back_from_its_own_spelling() {
    for renderer in PageRenderer::ALL {
        assert_eq!(PageRenderer::parse(renderer.as_str()), Ok(renderer));
    }
}

#[test]
fn an_unknown_page_renderer_is_refused_with_the_choices() {
    let error = PageRenderer::parse("webgpu").expect_err("not a renderer the page can name");
    assert_eq!(
        error.to_string(),
        "unknown value `webgpu`, expected `auto`, `canvas` or `dom`"
    );
}
