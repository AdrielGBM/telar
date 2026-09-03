//! Greedy word wrap, which is what a browser does for text with no hyphenation or `text-wrap: pretty`.
//!
//! Separate from the measurer that uses it because only *how wide a run is* needs a browser. Where the line
//! breaks is arithmetic, and arithmetic that walks byte offsets through a string is worth being able to run
//! on a machine that has no canvas.

/// The widest line `text` wraps to at `max_width`, and how many lines that is.
///
/// `width_of` measures one run in whatever the caller considers the current style. An infinite `max_width`
/// wraps nothing, which is what text that must stay on one line asks for.
pub fn greedy(text: &str, max_width: f32, width_of: impl Fn(&str) -> f32) -> (f32, usize) {
    let mut widest: f32 = 0.0;
    let mut lines = 0usize;

    for hard in text.split('\n') {
        lines += 1;
        let mut line_start = 0usize;
        let mut last_break: Option<usize> = None;
        for (offset, c) in hard.char_indices() {
            // A break moves the line's start past the whitespace it broke on, which is ahead of where this
            // walk still is. What it skipped belongs to the line already measured — and slicing from a start
            // that is past the cursor is not a short line, it is a panic.
            if offset < line_start {
                continue;
            }
            if c.is_whitespace() {
                last_break = Some(offset);
            }
            if width_of(&hard[line_start..offset + c.len_utf8()]) <= max_width {
                continue;
            }
            let cut = match last_break {
                Some(at) if at > line_start => at,
                // A word wider than the column breaks inside itself, as `overflow-wrap` does.
                _ => offset.max(line_start + c.len_utf8()),
            };
            widest = widest.max(width_of(&hard[line_start..cut]));
            line_start = hard[cut..]
                .find(|c: char| !c.is_whitespace())
                .map(|skip| cut + skip)
                .unwrap_or(hard.len());
            last_break = None;
            lines += 1;
        }
        widest = widest.max(width_of(&hard[line_start..]));
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
        // The walk is at the second space while the line already starts at `c`. Sliced the other way round
        // this is `&"ab   cd"[5..4]`, which took the whole page down the moment a label had two spaces in it.
        assert_eq!(greedy("ab   cd", 20.0, monospace), (20.0, 2));
    }

    #[test]
    fn a_word_wider_than_the_column_breaks_inside_itself() {
        assert_eq!(greedy("abcdef", 20.0, monospace), (20.0, 3));
    }

    #[test]
    fn a_hard_break_is_a_line_of_its_own() {
        assert_eq!(greedy("ab\ncd\nef", 100.0, monospace), (20.0, 3));
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
