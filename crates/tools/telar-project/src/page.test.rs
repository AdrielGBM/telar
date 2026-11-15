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
        "unknown value `webgpu`, expected `canvas` or `dom`"
    );
}

#[test]
fn a_page_that_names_no_renderer_draws_as_a_document() {
    assert_eq!(PageRenderer::from_setting(None), Ok(PageRenderer::Dom));
    assert_eq!(PageRenderer::from_setting(Some(" ")), Ok(PageRenderer::Dom));
    assert_eq!(
        PageRenderer::from_setting(Some(" canvas ")),
        Ok(PageRenderer::Canvas)
    );
    assert!(PageRenderer::from_setting(Some("auto")).is_err());
}
