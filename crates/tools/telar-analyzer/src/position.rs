pub use telar_parser::Section;

use telar_parser::header_section;

pub fn parser_line_to_lsp_range(parser_line: usize) -> lsp_types::Range {
    let line = parser_line.saturating_sub(1) as u32;
    lsp_types::Range {
        start: lsp_types::Position { line, character: 0 },
        end: lsp_types::Position {
            line,
            character: u32::MAX,
        },
    }
}

pub fn find_section_at(source: &str, lsp_line: u32) -> Section {
    let target = lsp_line as usize;
    let mut current = Section::Unknown;
    for (i, line) in source.lines().enumerate() {
        if let Some(section) = header_section(line.trim()) {
            current = section;
        }
        if i == target {
            return current;
        }
    }
    current
}
