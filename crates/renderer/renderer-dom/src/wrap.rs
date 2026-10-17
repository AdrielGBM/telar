//! Greedy word wrap, which is what a browser does for text with no hyphenation or `text-wrap: pretty`.
//!
//! Separate from the measurer that uses it because only *how wide a run is* needs a browser. Where the line breaks is arithmetic, and arithmetic that walks byte offsets through a string is worth being able to run on a machine that has no canvas.

/// The widest line `text` wraps to at `max_width`, and how many lines that is.
///
/// `width_of` measures one run in whatever the caller considers the current style. An infinite `max_width` wraps nothing, which is what text that must stay on one line asks for.
pub fn greedy(text: &str, max_width: f32, width_of: impl Fn(&str) -> f32) -> (f32, usize) {
    let mut widest: f32 = 0.0;
    let mut lines = 0usize;
    each_line(text, max_width, &width_of, |line| {
        widest = widest.max(width_of(line));
        lines += 1;
    });
    (widest, lines)
}

/// The widest line `text` holds when only its `\n`s break it, and how many lines that is: what a document lays out for `white-space: pre`, where a space at the end of a line is a space like any other and takes its room.
pub fn unbroken(text: &str, width_of: impl Fn(&str) -> f32) -> (f32, usize) {
    let body = text.strip_suffix('\n').unwrap_or(text);
    body.split('\n')
        .fold((0.0f32, 0usize), |(widest, lines), line| {
            (widest.max(width_of(line)), lines + 1)
        })
}

/// Hands `on_line` each line `text` wraps to at `max_width`, in order and without the space it broke on.
pub fn each_line<'a>(
    text: &'a str,
    max_width: f32,
    width_of: impl Fn(&str) -> f32,
    mut on_line: impl FnMut(&'a str),
) {
    // A newline at the very end closes the last line rather than opening another: a document lays out no line box for it, and counting one made every text that ends in a break a line taller than it is drawn.
    let body = text.strip_suffix('\n').unwrap_or(text);
    for hard in body.split('\n') {
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
            on_line(hard[line_start..cut].trim_end());
            line_start = hard[cut..]
                .find(|c: char| !c.is_whitespace())
                .map(|skip| cut + skip)
                .unwrap_or(hard.len());
            last_break = None;
        }
        on_line(hard[line_start..].trim_end());
    }
}

/// The byte offset in `text` each line [`each_line`] hands out begins at.
pub fn line_starts(text: &str, max_width: f32, width_of: impl Fn(&str) -> f32) -> Vec<usize> {
    let mut starts = Vec::new();
    each_line(text, max_width, width_of, |line| {
        starts.push(line.as_ptr() as usize - text.as_ptr() as usize);
    });
    if starts.is_empty() {
        starts.push(0);
    }
    starts
}

/// The widest run `text` has between spaces: the narrowest column [`greedy`] wraps it into without breaking inside a word.
pub fn widest_word(text: &str, width_of: impl Fn(&str) -> f32) -> f32 {
    text.split_whitespace().map(width_of).fold(0.0, f32::max)
}

#[cfg(test)]
#[path = "wrap_test.rs"]
mod tests;
