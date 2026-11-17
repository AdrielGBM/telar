use super::*;

#[test]
fn a_continuation_line_names_attributes_of_the_element_above() {
    let src = "[view]\nFeatureCard title:\"x\"\n        icon:\"star\" @card\n";
    assert!(matches!(
        view_token_at(src, 2, 9),
        Some(ViewToken::Attr {
            tag: "FeatureCard",
            key: "icon"
        })
    ));
    assert!(matches!(
        view_token_at(src, 2, 23),
        Some(ViewToken::Class("card"))
    ));
    assert!(matches!(
        view_token_at(src, 1, 3),
        Some(ViewToken::Tag("FeatureCard"))
    ));
}
