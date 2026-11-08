//! Text written into markup.

use std::borrow::Cow;

/// `text` as markup that reads back as the same characters, as an element's text or as an attribute value in either kind of quotes.
pub fn escape_html(text: &str) -> Cow<'_, str> {
    if !text.contains(['&', '<', '>', '"', '\'', '\r']) {
        return Cow::Borrowed(text);
    }
    let mut escaped = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            '\r' => escaped.push_str("&#13;"),
            c => escaped.push(c),
        }
    }
    Cow::Owned(escaped)
}

/// ` name="value"`, the value escaped, as it is written inside a start tag.
pub(crate) fn attribute(out: &mut String, name: &str, value: &str) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    out.push_str(&escape_html(value));
    out.push('"');
}

#[cfg(test)]
#[path = "markup.test.rs"]
mod tests;
