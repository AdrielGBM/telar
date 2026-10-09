//! Fuzzy matching: whether a query's characters appear in a text in order, how well, and where.

use std::ops::Range;

const MATCH: i32 = 16;
const CONSECUTIVE: i32 = 8;
const AT_START: i32 = 12;
const AFTER_SEPARATOR: i32 = 10;
const AT_CAPITAL: i32 = 9;
const LEADING_GAP: i32 = -1;
const INNER_GAP: i32 = -1;

/// How well a query matched a text, and the byte ranges of the text it matched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuzzyMatch {
    /// Higher is a better match. Only comparable between matches of the same query.
    pub score: i32,
    /// Where the query landed in the text, runs of neighbouring characters merged into one range.
    pub ranges: Vec<Range<usize>>,
}

/// Matches `query` against `text`, ignoring case and the query's whitespace, which only separates the words someone typed.
///
/// Every character of the query has to appear in the text in order. Among the ways it can, the one kept is the one a person means: runs of consecutive characters, characters that start the text or a word in it, and as little skipped text as possible. An empty query matches everything equally.
pub fn fuzzy_match(query: &str, text: &str) -> Option<FuzzyMatch> {
    let needle: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(fold)
        .collect();
    if needle.is_empty() {
        return Some(FuzzyMatch {
            score: 0,
            ranges: Vec::new(),
        });
    }
    let hay: Vec<(usize, char)> = text.char_indices().collect();
    if needle.len() > hay.len() {
        return None;
    }
    let folded: Vec<char> = hay.iter().map(|&(_, c)| fold(c)).collect();
    let bonus: Vec<i32> = (0..hay.len()).map(|j| bonus_at(&hay, j)).collect();

    let (m, n) = (needle.len(), hay.len());
    let mut best: Vec<Vec<Option<i32>>> = vec![vec![None; n]; m];
    for (i, &wanted) in needle.iter().enumerate() {
        let mut carried: Option<i32> = None;
        for j in 0..n {
            if i > 0 && j > 0 {
                carried = max_opt(carried.map(|s| s + INNER_GAP), best[i - 1][j - 1]);
            }
            if folded[j] != wanted {
                continue;
            }
            let reached = if i == 0 {
                Some(LEADING_GAP * j as i32)
            } else {
                let consecutive = (j > 0)
                    .then(|| best[i - 1][j - 1].map(|s| s + CONSECUTIVE))
                    .flatten();
                max_opt(consecutive, carried)
            };
            best[i][j] = reached.map(|s| s + MATCH + bonus[j]);
        }
    }

    let (mut j, score) = best[m - 1]
        .iter()
        .enumerate()
        .filter_map(|(j, s)| s.map(|s| (j, s)))
        .max_by_key(|&(j, s)| (s, std::cmp::Reverse(j)))?;
    let mut matched = vec![j];
    for i in (1..m).rev() {
        let here = best[i][j].expect("a matched position has a score");
        let gained = MATCH + bonus[j];
        let previous =
            if j > 0 && best[i - 1][j - 1].map(|s| s + CONSECUTIVE + gained) == Some(here) {
                j - 1
            } else {
                (0..j.saturating_sub(1))
                    .rev()
                    .find(|&k| {
                        best[i - 1][k].map(|s| s + INNER_GAP * (j - k - 1) as i32 + gained)
                            == Some(here)
                    })
                    .expect("a matched position was reached from one before it")
            };
        matched.push(previous);
        j = previous;
    }
    matched.reverse();
    Some(FuzzyMatch {
        score,
        ranges: byte_ranges(&hay, text.len(), &matched),
    })
}

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn max_opt(a: Option<i32>, b: Option<i32>) -> Option<i32> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, None) => a,
        (None, b) => b,
    }
}

fn is_separator(c: char) -> bool {
    c.is_whitespace() || matches!(c, '/' | '-' | '_' | '.' | ':' | '\\' | '(' | '[')
}

fn bonus_at(hay: &[(usize, char)], j: usize) -> i32 {
    let current = hay[j].1;
    let Some(&(_, before)) = j.checked_sub(1).and_then(|k| hay.get(k)) else {
        return AT_START;
    };
    if is_separator(before) {
        AFTER_SEPARATOR
    } else if before.is_lowercase() && current.is_uppercase() {
        AT_CAPITAL
    } else {
        0
    }
}

/// The matched characters as byte ranges of the text, runs of neighbours merged into one.
fn byte_ranges(hay: &[(usize, char)], len: usize, matched: &[usize]) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for &j in matched {
        let start = hay[j].0;
        let end = hay.get(j + 1).map_or(len, |&(at, _)| at);
        match ranges.last_mut() {
            Some(last) if last.end == start => last.end = end,
            _ => ranges.push(start..end),
        }
    }
    ranges
}

#[cfg(test)]
#[path = "fuzzy_test.rs"]
mod tests;
