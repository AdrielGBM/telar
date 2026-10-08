//! Cutting a source file into its `[section]` zones, so a rewrite only ever sees the one it belongs to.

pub(super) use telar_parser::Section;
use telar_parser::section_opened_by;

pub(super) struct Zone<'a> {
    pub(super) section: Section,
    /// The `[section]` line itself, kept out of the body so a rewrite can never touch it.
    pub(super) header: &'a str,
    pub(super) body: &'a str,
}

pub(super) fn zones(source: &str) -> Vec<Zone<'_>> {
    let mut out = Vec::new();
    let (mut section, mut header_at, mut body_at) = (Section::Unknown, 0usize, 0usize);
    let mut at = 0usize;
    for line in source.split_inclusive('\n') {
        if let Some(next) = section_opened_by(line.trim()) {
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
