//! `[style]` section: the named property bundles a view reuses.

use super::{Parser, split_once_colon};
use crate::ast::*;
use crate::error::ParseError;
use crate::lexer::Section;

impl Parser {
    pub(super) fn parse_style(&mut self) -> Result<StyleSection, ParseError> {
        let mut section = StyleSection::default();

        while let Some(line) = self.lines.get(self.pos) {
            if line.section != Section::Style {
                break;
            }
            if line.is_blank() {
                self.pos += 1;
                continue;
            }

            if line.content.starts_with('@') {
                let class = self.parse_style_class(false)?;
                section.classes.push(class);
            } else if let Some(class) = line.content.strip_prefix("pub ") {
                if !class.trim_start().starts_with('@') {
                    return Err(ParseError {
                        message: format!(
                            "`pub` in `[style]` makes a class importable: write `pub @name`, got `{}`",
                            line.content
                        ),
                        line: line.number,
                    });
                }
                let class = self.parse_style_class(true)?;
                section.classes.push(class);
            } else if line.content.starts_with("use ") {
                section
                    .imports
                    .push(parse_style_import(&line.content, line.number)?);
                self.pos += 1;
            } else {
                // `[style]` is classes — named bundles of properties, which are reuse. A constant was a second evaluation model with a namespace of its own, and `[logic]` is already Rust.
                let name = line.content.split(':').next().unwrap_or("").trim();
                return Err(ParseError {
                    message: format!(
                        "`[style]` holds classes, not constants — move `{name}` to `[logic]` as a `const`"
                    ),
                    line: line.number,
                });
            }
        }

        Ok(section)
    }

    /// Parses either an inline `@class: k:v k:v` or a multi-line `@class` with indented props, `public` when the header was written `pub @class`.
    fn parse_style_class(&mut self, public: bool) -> Result<StyleClass, ParseError> {
        let header = &self.lines[self.pos];
        let header_line = header.number;
        let header_indent = header.indent;
        let content = header.content.trim_start_matches("pub").trim_start();
        let after_sigil = content[1..].to_string();
        self.pos += 1;

        if let Some((name, rest)) = split_once_colon(&after_sigil) {
            let name = name.trim().to_string();
            let props = parse_inline_props(rest.trim(), header_line)?;
            return Ok(StyleClass {
                name,
                props,
                line: header_line,
                public,
            });
        }

        let name = after_sigil.trim().to_string();
        if name.is_empty() {
            return Err(ParseError {
                message: "style class is missing a name after `@`".to_string(),
                line: header_line,
            });
        }

        let mut props = Vec::new();
        while let Some(line) = self.lines.get(self.pos) {
            if line.section != Section::Style {
                break;
            }
            if line.is_blank() {
                self.pos += 1;
                continue;
            }
            // A child property must be indented deeper than the class header.
            if line.indent <= header_indent {
                break;
            }
            // A nested class would start a new definition.
            if line.content.starts_with('@') {
                break;
            }

            let (key, value) = split_once_colon(&line.content).ok_or_else(|| ParseError {
                message: format!(
                    "expected `key: value` in style class `{name}`, got `{}`",
                    line.content
                ),
                line: line.number,
            })?;
            let (key, value) = (key.trim(), value.trim());
            check_style_prop_value(key, value, line.number)?;
            props.push(StyleProp {
                key: key.to_string(),
                value: value.to_string(),
            });
            self.pos += 1;
        }

        Ok(StyleClass {
            name,
            props,
            line: header_line,
            public,
        })
    }
}

/// Parses `use crate::styles::{mono, line as rule};`, `use super::styles::mono;` or `use crate::styles::*;`: Rust's own spelling, with class names where Rust has items.
fn parse_style_import(content: &str, line: usize) -> Result<StyleImport, ParseError> {
    let error = |why: &str| ParseError {
        message: format!("{why}: `{content}`"),
        line,
    };
    let body = content["use ".len()..].trim();
    let body = body
        .strip_suffix(';')
        .ok_or_else(|| error("a class import ends with `;`, as a `use` does"))?
        .trim();
    let (path, items) = match body.strip_suffix('}') {
        Some(braced) => {
            let open = braced
                .find("::{")
                .ok_or_else(|| error("expected `module::{name, …}`"))?;
            let names = braced[open + 3..]
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(|name| import_name(name).ok_or_else(|| error("expected a class name")))
                .collect::<Result<Vec<_>, _>>()?;
            if names.is_empty() {
                return Err(error("the import names no class"));
            }
            (&braced[..open], StyleImportItems::Names(names))
        }
        None => {
            let (path, last) = body
                .rsplit_once("::")
                .ok_or_else(|| error("expected `module::name`"))?;
            let items = match last.trim() {
                "*" => StyleImportItems::Glob,
                name => StyleImportItems::Names(vec![
                    import_name(name).ok_or_else(|| error("expected a class name"))?,
                ]),
            };
            (path, items)
        }
    };
    let module: Vec<String> = path.split("::").map(|s| s.trim().to_string()).collect();
    let is_segment =
        |s: &String| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !module.iter().all(is_segment) || !matches!(module[0].as_str(), "crate" | "super" | "self") {
        return Err(error(
            "a class import names a module of this crate: start its path with `crate`, `super` or `self`",
        ));
    }
    Ok(StyleImport {
        module,
        items,
        line,
    })
}

/// `name` or `name as alias`, with class names written as they are declared, without the `@`.
fn import_name(text: &str) -> Option<StyleImportName> {
    let ident = |s: &str| {
        let s = s.trim();
        (!s.is_empty()
            && s.chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-'))
        .then(|| s.to_string())
    };
    match text.split_once(" as ") {
        Some((name, alias)) => Some(StyleImportName {
            name: ident(name)?,
            alias: Some(ident(alias)?),
        }),
        None => Some(StyleImportName {
            name: ident(text)?,
            alias: None,
        }),
    }
}

/// `#` is reserved for hex colors in `.rsx`, so any `#`-prefixed value must be a well-formed hex. Catches typos (`#zzz`, `#12`) at parse time instead of silently rendering them as black. `pub(super)` because `view::parse_element_header` validates attribute values with it too.
pub(super) fn check_hex_value(value: &str, line: usize) -> Result<(), ParseError> {
    if value.starts_with('#') && crate::parse_hex(value).is_none() {
        return Err(ParseError {
            message: format!(
                "invalid hex color `{value}`: expected #rgb, #rgba, #rrggbb or #rrggbbaa"
            ),
            line,
        });
    }
    Ok(())
}

/// Validates a style-class property `key: value`: the value must be present and, when it is a hex color, well-formed.
fn check_style_prop_value(key: &str, value: &str, line: usize) -> Result<(), ParseError> {
    if value.is_empty() {
        return Err(ParseError {
            message: format!("style property `{key}` is missing a value after `:`"),
            line,
        });
    }
    check_hex_value(value, line)
}

/// Parses inline class props: `padding_x:6  padding_y:2  radius:6`.
fn parse_inline_props(s: &str, line: usize) -> Result<Vec<StyleProp>, ParseError> {
    let mut props = Vec::new();
    for token in s.split_whitespace() {
        let (key, value) = split_once_colon(token).ok_or_else(|| ParseError {
            message: format!("expected `key:value` in inline style class, got `{token}`"),
            line,
        })?;
        let (key, value) = (key.trim(), value.trim());
        check_style_prop_value(key, value, line)?;
        props.push(StyleProp {
            key: key.to_string(),
            value: value.to_string(),
        });
    }
    Ok(props)
}
