use super::*;

#[test]
fn a_path_resolves_under_the_installed_base() {
    assert_eq!(asset_url("images/a.png"), "images/a.png");
    set_asset_base("https://example.com/site");
    assert_eq!(
        asset_url("images/a.png"),
        "https://example.com/site/images/a.png"
    );
    set_asset_base("https://example.com/");
    assert_eq!(asset_url("./b.png"), "https://example.com/b.png");
}
