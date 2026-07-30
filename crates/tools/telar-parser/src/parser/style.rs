//! `[style]` section: constants and classes.

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
                let class = self.parse_style_class()?;
                section.classes.push(class);
            } else {
                let constant = self.parse_style_const()?;
                section.constants.push(constant);
                self.pos += 1;
            }
        }

        Ok(section)
    }

    fn parse_style_const(&self) -> Result<StyleConstant, ParseError> {
        let line = &self.lines[self.pos];
        let (name, value) = split_once_colon(&line.content).ok_or_else(|| ParseError {
            message: format!(
                "expected `name: value` in style constant, got `{}`",
                line.content
            ),
            line: line.number,
        })?;

        let name = name.trim().to_string();
        let value = value.trim();
        if value.is_empty() {
            return Err(ParseError {
                message: format!("style constant `{name}` is missing a value after `:`"),
                line: line.number,
            });
        }
        check_hex_value(value, line.number)?;

        Ok(StyleConstant {
            name,
            value: parse_style_value(value),
            line: line.number,
        })
    }

    /// Parses either an inline `@class: k:v k:v` or a multi-line `@class` with indented props.
    fn parse_style_class(&mut self) -> Result<StyleClass, ParseError> {
        let header = &self.lines[self.pos];
        let header_line = header.number;
        let header_indent = header.indent;
        // Strip the leading `@` sigil; the rest is the class name (and any inline props).
        let after_sigil = header.content[1..].to_string();
        self.pos += 1;

        // Inline form: `@badge: padding_x:6 padding_y:2 radius:6`
        if let Some((name, rest)) = split_once_colon(&after_sigil) {
            let name = name.trim().to_string();
            let props = parse_inline_props(rest.trim(), header_line)?;
            return Ok(StyleClass {
                name,
                props,
                line: header_line,
            });
        }

        // Multi-line form: name is the rest of the header, props are indented lines.
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
        })
    }
}

/// A hex color body (the part after `#`) is valid only at the lengths the transpiler expands:
/// `#rgb`, `#rrggbb`, `#rrggbbaa`, all hex digits.
fn is_valid_hex(hex: &str) -> bool {
    matches!(hex.len(), 3 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `#` is reserved for hex colors in `.rsx`, so any `#`-prefixed value must be a well-formed hex.
/// Catches typos (`#zzz`, `#12`) at parse time instead of silently rendering them as black.
/// `pub(super)` because `view::parse_element_header` validates attribute values with it too.
pub(super) fn check_hex_value(value: &str, line: usize) -> Result<(), ParseError> {
    if let Some(hex) = value.strip_prefix('#')
        && !is_valid_hex(hex)
    {
        return Err(ParseError {
            message: format!("invalid hex color `{value}`: expected #rgb, #rrggbb or #rrggbbaa"),
            line,
        });
    }
    Ok(())
}

/// Validates a style-class property `key: value`: the value must be present and, when it is a hex
/// color, well-formed.
fn check_style_prop_value(key: &str, value: &str, line: usize) -> Result<(), ParseError> {
    if value.is_empty() {
        return Err(ParseError {
            message: format!("style property `{key}` is missing a value after `:`"),
            line,
        });
    }
    check_hex_value(value, line)
}

/// Classifies a style constant value.
fn parse_style_value(raw: &str) -> StyleValue {
    if let Some(hex) = raw.strip_prefix('#') {
        return StyleValue::Hex(format!("#{hex}"));
    }
    if let Ok(n) = raw.parse::<f32>() {
        return StyleValue::Number(n);
    }
    StyleValue::Raw(raw.to_string())
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
