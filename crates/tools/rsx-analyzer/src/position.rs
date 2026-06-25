pub use rsx_parser::Section;

use rsx_parser::header_section;

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

pub fn logic_zone_start(source: &str) -> Option<u32> {
    for (i, line) in source.lines().enumerate() {
        if line.trim() == "[logic]" {
            return Some(i as u32 + 1);
        }
    }
    None
}

pub fn rsx_to_rs_line(source: &str, rsx_line: u32) -> Option<u32> {
    let start = logic_zone_start(source)?;
    if rsx_line < start {
        return None;
    }
    Some(rsx_line - start)
}

pub fn rs_to_rsx_line(source: &str, rs_line: u32) -> u32 {
    let start = logic_zone_start(source).unwrap_or(0);
    rs_line + start
}
