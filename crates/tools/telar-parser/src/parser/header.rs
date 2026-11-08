use std::ops::Range;

pub(super) struct Tokens<'a> {
    source: &'a str,
    spans: Vec<Range<usize>>,
}

impl<'a> Tokens<'a> {
    pub(super) fn of(source: &'a str) -> Self {
        let bytes = source.as_bytes();
        let mut spans = Vec::new();
        let mut start = None;
        let mut at = 0;
        while at < bytes.len() {
            if bytes[at].is_ascii_whitespace() {
                if let Some(start) = start.take() {
                    spans.push(start..at);
                }
                at += 1;
            } else {
                start.get_or_insert(at);
                at = literal_end(bytes, at).unwrap_or(at + 1);
            }
        }
        if let Some(start) = start {
            spans.push(start..bytes.len());
        }
        Self { source, spans }
    }

    pub(super) fn len(&self) -> usize {
        self.spans.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    pub(super) fn get(&self, index: usize) -> &'a str {
        &self.source[self.spans[index].clone()]
    }

    pub(super) fn position(&self, within: Range<usize>, keyword: &str) -> Option<usize> {
        within.into_iter().find(|&i| self.get(i) == keyword)
    }

    pub(super) fn slice(&self, tokens: Range<usize>) -> &'a str {
        &self.source[self.spans[tokens.start].start..self.spans[tokens.end - 1].end]
    }
}

fn literal_end(bytes: &[u8], at: usize) -> Option<usize> {
    match bytes[at] {
        b'"' => Some(string_end(bytes, at + 1)),
        b'\'' => char_end(bytes, at),
        b'r' | b'b' if at == 0 || !is_identifier_byte(bytes[at - 1]) => raw_string_end(bytes, at),
        _ => None,
    }
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn string_end(bytes: &[u8], mut at: usize) -> usize {
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b'"' => return at + 1,
            _ => at += 1,
        }
    }
    bytes.len()
}

fn char_end(bytes: &[u8], quote: usize) -> Option<usize> {
    let first = quote + 1;
    if *bytes.get(first)? == b'\\' {
        let closing = bytes.get(first + 2..)?.iter().position(|&b| b == b'\'')?;
        return Some(first + 2 + closing + 1);
    }
    let width = match bytes.get(first)? {
        0..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    };
    (bytes.get(first + width) == Some(&b'\'') && bytes[first] != b'\'').then_some(first + width + 1)
}

fn raw_string_end(bytes: &[u8], at: usize) -> Option<usize> {
    let mut at = at;
    if bytes[at] == b'b' {
        at += 1;
    }
    if bytes.get(at) != Some(&b'r') {
        return None;
    }
    at += 1;
    let hashes = bytes[at..].iter().take_while(|&&b| b == b'#').count();
    at += hashes;
    if bytes.get(at) != Some(&b'"') {
        return None;
    }
    let body = at + 1;
    let closing: Vec<u8> = std::iter::once(b'"')
        .chain(std::iter::repeat_n(b'#', hashes))
        .collect();
    Some(
        bytes[body..]
            .windows(closing.len())
            .position(|window| window == closing)
            .map_or(bytes.len(), |found| body + found + closing.len()),
    )
}
