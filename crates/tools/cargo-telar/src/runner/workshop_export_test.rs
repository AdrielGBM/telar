use super::*;

#[test]
fn an_export_is_for_the_web_whether_or_not_the_target_says_so() {
    assert!(matches!(export_target(None), Ok(Target::Web)));
    assert!(matches!(export_target(Some(Target::Web)), Ok(Target::Web)));
    for other in [Target::Desktop, Target::Tui, Target::Android] {
        assert!(export_target(Some(other)).is_err());
    }
}

#[test]
fn a_base_names_where_the_site_is_served_in_place_of_the_configured_one() {
    let mut config = TelarSection::default();
    config.web.base = Some("/docs/".to_string());
    assert_eq!(
        with_base(config.clone(), None).unwrap().web.base_path(),
        "/docs/"
    );
    assert_eq!(
        with_base(config, Some("workshop")).unwrap().web.base_path(),
        "/workshop/"
    );
}

#[test]
fn a_base_no_site_can_be_served_under_is_refused() {
    let error = with_base(TelarSection::default(), Some("/a/../b/")).unwrap_err();
    assert!(error.starts_with("`--base /a/../b/`"), "{error}");
    let mut cloudflare = TelarSection::default();
    cloudflare.web.host = Some(telar_project::WebHost::CloudflarePages);
    let error = with_base(cloudflare, Some("/workshop/")).unwrap_err();
    assert!(error.contains("cloudflare-pages"), "{error}");
}
