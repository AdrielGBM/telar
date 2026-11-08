use super::*;

#[test]
fn text_without_markup_characters_is_written_as_it_is() {
    assert_eq!(
        escape_html("Tom and Jerry, 日本語"),
        "Tom and Jerry, 日本語"
    );
}

#[test]
fn an_attribute_value_cannot_end_its_quotes_or_open_a_tag() {
    let mut out = String::new();
    attribute(&mut out, "aria-label", "Card \"one\" & 'two' <b>\nnext");
    assert_eq!(
        out,
        " aria-label=\"Card &quot;one&quot; &amp; &#39;two&#39; &lt;b&gt;\nnext\""
    );
}

#[test]
fn a_carriage_return_survives_the_parser_that_would_turn_it_into_a_newline() {
    assert_eq!(escape_html("a\r\nb"), "a&#13;\nb");
}
