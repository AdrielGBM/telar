//! Shared identifier conversions between RSX names and generated Rust names.

/// Returns true if `c` is a word-separator: `.`, `-`, `_`, or whitespace.
fn is_separator(c: char) -> bool {
    matches!(c, '.' | '-' | '_' | ' ' | '\t')
}

/// Converts an RSX name (`card-title`, `btn.primary`) into a snake_case identifier. Separators (`.`, `-`, `_`, whitespace) become `_`. Leading digits are prefixed with `_` to produce a valid Rust identifier.
pub fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 1);
    let mut prev_was_sep = false;
    for (i, c) in name.chars().enumerate() {
        if is_separator(c) {
            if !out.is_empty() {
                prev_was_sep = true;
            }
        } else if c.is_ascii_alphanumeric() {
            if i == 0 && c.is_ascii_digit() {
                out.push('_');
            }
            if prev_was_sep {
                out.push('_');
                prev_was_sep = false;
            }
            out.push(c.to_ascii_lowercase());
        }
    }
    out
}

/// Converts an RSX name (`shape_card`, `info.card`) into PascalCase (`ShapeCard`, `InfoCard`). Separators (`.`, `-`, `_`, whitespace) trigger capitalization of the next word. Non-alphanumeric, non-separator chars are stripped. A leading digit is prefixed with `_`.
pub fn to_pascal_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut next_upper = true;
    let mut first_char = true;
    for c in name.chars() {
        if is_separator(c) {
            next_upper = true;
        } else if c.is_ascii_alphanumeric() {
            if first_char && c.is_ascii_digit() {
                out.push('_');
            }
            first_char = false;
            if next_upper && !c.is_ascii_digit() {
                out.extend(c.to_uppercase());
            } else {
                out.push(c);
            }
            next_upper = false;
        }
    }
    out
}

/// Generated `LayoutStyle` constructor name for a style class: `card` -> `style_card`.
pub fn style_function_name(class: &str) -> String {
    format!("style_{}", to_snake_case(class))
}

/// Generated color/number constant name: `card-border` -> `COLOR_CARD_BORDER`.
pub fn constant_name(prefix: &str, name: &str) -> String {
    format!("{prefix}{}", to_snake_case(name).to_ascii_uppercase())
}

/// Generated preview entries const name for a file stem: `card` -> `CARD_PREVIEW_ENTRIES`. This must match the name emitted by the transpiler in the generated `.rs` file.
pub fn preview_entries_const_name(stem: &str) -> String {
    format!(
        "{}_PREVIEW_ENTRIES",
        to_snake_case(stem).to_ascii_uppercase()
    )
}

/// The name part of a preview id: `"Landing — full page"` -> `landing-full-page`. Letters and digits are kept, lowercased; every run of anything else becomes one `-`, and none leads or trails.
pub fn preview_slug(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut gap = false;
    for c in name.chars() {
        if c.is_alphanumeric() {
            if gap && !slug.is_empty() {
                slug.push('-');
            }
            gap = false;
            slug.extend(c.to_lowercase());
        } else {
            gap = true;
        }
    }
    slug
}

/// Why a preview has no id.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreviewIdError {
    /// The name slugs to nothing.
    NoLetterOrDigit { name: String },
}

impl std::fmt::Display for PreviewIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoLetterOrDigit { name } => write!(
                f,
                "preview \"{name}\" needs a letter or digit in its name to form its id"
            ),
        }
    }
}

impl std::error::Error for PreviewIdError {}

/// The `--<component>--<slug(name)>` that follows the crate name in a preview's id, the same for a `.rsx` preview and a Rust one.
pub fn preview_id_suffix(component: &str, name: &str) -> Result<String, PreviewIdError> {
    let slug = preview_slug(name);
    if slug.is_empty() {
        return Err(PreviewIdError::NoLetterOrDigit {
            name: name.to_string(),
        });
    }
    Ok(format!("--{component}--{slug}"))
}

/// The Rust expression a preview entry records its file with: `CARGO_MANIFEST_DIR` joined to `package_path`, the file's path relative to its package, so every preview names its file absolutely and the same way.
pub fn preview_file_expr(package_path: &str) -> String {
    let path = format!("/{}", package_path.trim_start_matches('/'));
    format!("concat!(env!(\"CARGO_MANIFEST_DIR\"), {path:?})")
}

/// Whether `b` may appear inside a Rust identifier.
pub fn is_ident_byte(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

/// Whether `s` is a valid Rust identifier: starts with `_` or a letter, rest `_`/alphanumeric.
pub fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

#[cfg(test)]
#[path = "naming_test.rs"]
mod tests;
