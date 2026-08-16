pub fn attribute_key_before_colon(line: &str, word_start: usize) -> Option<&str> {
    let before_colon = line[..word_start.saturating_sub(1)].trim_end();
    let key_start = before_colon
        .rfind(char::is_whitespace)
        .map(|i| i + 1)
        .unwrap_or(0);
    Some(before_colon[key_start..].trim())
}
