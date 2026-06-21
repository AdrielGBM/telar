pub fn word_at_cursor(line: &str, ch: usize) -> (usize, &str) {
    let ch = ch.min(line.len());
    let start = line[..ch]
        .rfind(|c: char| c.is_whitespace() || c == ':' || c == '"')
        .map(|i| i + 1)
        .unwrap_or(0);
    let end = line[ch..]
        .find(|c: char| c.is_whitespace() || c == ':' || c == '"')
        .map(|i| ch + i)
        .unwrap_or(line.len());
    (start, &line[start..end])
}

pub fn attr_key_before_colon(line: &str, word_start: usize) -> Option<&str> {
    let before_colon = line[..word_start.saturating_sub(1)].trim_end();
    let key_start = before_colon
        .rfind(char::is_whitespace)
        .map(|i| i + 1)
        .unwrap_or(0);
    Some(before_colon[key_start..].trim())
}
