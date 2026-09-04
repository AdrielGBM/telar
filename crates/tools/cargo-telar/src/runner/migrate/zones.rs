//! Cutting a source file into its `[section]` zones, so a rewrite only ever sees the one it belongs to.

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Section {
    None,
    Logic,
    Style,
    View,
    Preview,
}

pub(super) struct Zone<'a> {
    pub(super) section: Section,
    /// The `[section]` line itself, kept out of the body so a rewrite can never touch it.
    pub(super) header: &'a str,
    pub(super) body: &'a str,
}

pub(super) fn zones(source: &str) -> Vec<Zone<'_>> {
    let mut out = Vec::new();
    let (mut section, mut header_at, mut body_at) = (Section::None, 0usize, 0usize);
    let mut at = 0usize;
    for line in source.split_inclusive('\n') {
        if let Some(next) = section_of(line) {
            out.push(Zone {
                section,
                header: &source[header_at..body_at],
                body: &source[body_at..at],
            });
            (section, header_at, body_at) = (next, at, at + line.len());
        }
        at += line.len();
    }
    out.push(Zone {
        section,
        header: &source[header_at..body_at],
        body: &source[body_at..],
    });
    out
}

pub(super) fn section_of(line: &str) -> Option<Section> {
    let t = line.trim();
    match t {
        "[logic]" => Some(Section::Logic),
        "[style]" => Some(Section::Style),
        "[view]" => Some(Section::View),
        _ if t.starts_with("[preview") && t.ends_with(']') => Some(Section::Preview),
        _ => None,
    }
}
