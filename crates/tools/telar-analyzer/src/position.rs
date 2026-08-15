pub use telar_parser::{Section, find_section_at};

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
