//! Text read out of the author's source, shown back to them: one rule for taking off the indentation it was written at.

/// `lines` without the indentation every non-blank one shares and without trailing whitespace. Indentation is spaces and tabs, each one column, so the cut always falls between characters.
pub fn dedent<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let shared = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| indentation(line))
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|line| line.get(shared..).unwrap_or("").trim_end())
        .collect()
}

fn indentation(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

#[cfg(test)]
#[path = "text_test.rs"]
mod tests;
