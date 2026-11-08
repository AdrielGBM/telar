//! Text written into CSS as a quoted string.

use std::fmt::Write;

/// `text` as a CSS string that reads back as the same characters in a `style` attribute or a `<style>` element: control characters are hex escapes because a raw newline ends the string and lets the rest run as declarations, and so is `<` because `</style>` closes the element whatever CSS makes of it.
pub fn css_string(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('"');
    for c in text.chars() {
        match c {
            '"' | '\\' => {
                quoted.push('\\');
                quoted.push(c);
            }
            '<' => hex_escape(&mut quoted, c),
            c if c.is_control() => hex_escape(&mut quoted, c),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

fn hex_escape(out: &mut String, c: char) {
    let _ = write!(out, "\\{:x} ", u32::from(c));
}

#[cfg(test)]
#[path = "css_string.test.rs"]
mod tests;
