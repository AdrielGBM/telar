//! Greedy word wrap, which is what a browser does for text with no hyphenation or `text-wrap: pretty`.
//!
//! Separate from the measurer that uses it because only *how wide a run is* needs a browser. Where the line breaks is arithmetic, and arithmetic that walks byte offsets through a string is worth being able to run on a machine that has no canvas.

/// The widest line `text` wraps to at `max_width`, and how many lines that is.
///
/// `width_of` measures one run in whatever the caller considers the current style. An infinite `max_width` wraps nothing, which is what text that must stay on one line asks for.
pub fn greedy(text: &str, max_width: f32, width_of: impl Fn(&str) -> f32) -> (f32, usize) {
    let mut widest: f32 = 0.0;
    let mut lines = 0usize;

    // A newline at the very end closes the last line rather than opening another: a document lays out no line box for it, and counting one made every text that ends in a break a line taller than it is drawn.
    let body = text.strip_suffix('\n').unwrap_or(text);
    for hard in body.split('\n') {
        lines += 1;
        let mut line_start = 0usize;
        let mut last_break: Option<usize> = None;
        for (offset, c) in hard.char_indices() {
            // A break moves the line's start past the whitespace it broke on, which is ahead of where this walk still is. What it skipped belongs to the line already measured — and slicing from a start that is past the cursor is not a short line, it is a panic.
            if offset < line_start {
                continue;
            }
            if c.is_whitespace() {
                last_break = Some(offset);
            }
            // Trailing space does not count towards the fit. A space at a break hangs past the edge rather than pushing the word after it onto the next line, which is what every text engine does and what a document does with `pre-wrap` — measuring it broke one word early, and a paragraph came out a line taller than the page it was drawn on.
            let candidate = &hard[line_start..offset + c.len_utf8()];
            if width_of(candidate.trim_end()) <= max_width {
                continue;
            }
            let cut = match last_break {
                Some(at) if at > line_start => at,
                // A word wider than the column breaks inside itself, as `overflow-wrap` does.
                _ => offset.max(line_start + c.len_utf8()),
            };
            widest = widest.max(width_of(hard[line_start..cut].trim_end()));
            line_start = hard[cut..]
                .find(|c: char| !c.is_whitespace())
                .map(|skip| cut + skip)
                .unwrap_or(hard.len());
            last_break = None;
            lines += 1;
        }
        widest = widest.max(width_of(hard[line_start..].trim_end()));
    }

    (widest, lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ten units a character, so a column is a character count and the arithmetic is readable.
    fn monospace(run: &str) -> f32 {
        run.chars().count() as f32 * 10.0
    }

    #[test]
    fn text_that_fits_is_one_line() {
        assert_eq!(greedy("abcd", 100.0, monospace), (40.0, 1));
    }

    #[test]
    fn a_line_breaks_at_the_last_space_that_fits() {
        assert_eq!(greedy("ab cd ef", 50.0, monospace), (50.0, 2));
    }

    #[test]
    fn a_run_of_spaces_at_a_break_is_not_carried_onto_the_next_line() {
        // The walk is at the second space while the line already starts at `c`. Sliced the other way round this is `&"ab   cd"[5..4]`, which took the whole page down the moment a label had two spaces in it.
        assert_eq!(greedy("ab   cd", 20.0, monospace), (20.0, 2));
    }

    /// A space at a break hangs past the edge; it is not what pushes the next word down.
    #[test]
    fn a_trailing_space_does_not_take_a_word_to_the_next_line() {
        // Four characters in a four-character column, and then a space. Without the hang the space pushes past the edge, the walk breaks on it, and the line count comes out one too high — which is the twenty pixels a paragraph disagreed by, once per wrapped line.
        assert_eq!(greedy("abcd ", 40.0, monospace), (40.0, 1));
        // The hang is not a licence to overflow: a word that genuinely does not fit still goes down.
        assert_eq!(greedy("ab cd", 40.0, monospace), (20.0, 2));
    }

    #[test]
    fn a_word_wider_than_the_column_breaks_inside_itself() {
        assert_eq!(greedy("abcdef", 20.0, monospace), (20.0, 3));
    }

    #[test]
    fn a_hard_break_is_a_line_of_its_own() {
        assert_eq!(greedy("ab\ncd\nef", 100.0, monospace), (20.0, 3));
    }

    /// A source listing ends in one, and counting it made every such text a line taller than it is drawn.
    #[test]
    fn a_break_at_the_very_end_closes_a_line_rather_than_opening_one() {
        assert_eq!(greedy("ab\ncd\n", 100.0, monospace), (20.0, 2));
        // One in the middle still is a line: it is a blank line somebody wrote.
        assert_eq!(greedy("ab\n\ncd", 100.0, monospace), (20.0, 3));
    }

    #[test]
    fn an_unbounded_column_wraps_nothing() {
        assert_eq!(greedy("ab cd ef", f32::INFINITY, monospace), (80.0, 1));
    }

    #[test]
    fn nothing_is_still_one_line() {
        assert_eq!(greedy("", 100.0, monospace), (0.0, 1));
    }

    #[test]
    fn a_multibyte_word_breaks_on_a_character_boundary() {
        // Four two-byte characters: a cut taken on bytes rather than characters would split one in half.
        assert_eq!(greedy("ñññññ", 20.0, monospace), (20.0, 3));
    }
}
