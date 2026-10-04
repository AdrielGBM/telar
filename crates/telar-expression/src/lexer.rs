//! The lexer: source text into located tokens, refusing any character the language gives no meaning.

use std::sync::Arc;

use geometry_core::Color;

use crate::{Error, ErrorCode, Found, Reference, Span};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TokenKind {
    Number(f64),
    Text(Arc<str>),
    Color(Color),
    Ident(String),
    Reference(Reference),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Percent,
    OpenParen,
    CloseParen,
    OpenBracket,
    CloseBracket,
    OpenBrace,
    CloseBrace,
    Comma,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    Bang,
}

impl TokenKind {
    pub(crate) fn symbol(&self) -> Option<&'static str> {
        Some(match self {
            TokenKind::Number(_)
            | TokenKind::Text(_)
            | TokenKind::Color(_)
            | TokenKind::Ident(_)
            | TokenKind::Reference(_) => return None,
            TokenKind::Plus => "+",
            TokenKind::Minus => "-",
            TokenKind::Star => "*",
            TokenKind::Slash => "/",
            TokenKind::Caret => "^",
            TokenKind::Percent => "%",
            TokenKind::OpenParen => "(",
            TokenKind::CloseParen => ")",
            TokenKind::OpenBracket => "[",
            TokenKind::CloseBracket => "]",
            TokenKind::OpenBrace => "{",
            TokenKind::CloseBrace => "}",
            TokenKind::Comma => ",",
            TokenKind::Equal => "==",
            TokenKind::NotEqual => "!=",
            TokenKind::Less => "<",
            TokenKind::LessEqual => "<=",
            TokenKind::Greater => ">",
            TokenKind::GreaterEqual => ">=",
            TokenKind::And => "&&",
            TokenKind::Or => "||",
            TokenKind::Bang => "!",
        })
    }

    pub(crate) fn found(&self) -> Found {
        match self {
            TokenKind::Number(_) => Found::Number,
            TokenKind::Text(_) => Found::Text,
            TokenKind::Color(_) => Found::Color,
            TokenKind::Ident(name) => Found::Name(name.clone()),
            TokenKind::Reference(reference) => Found::Reference(reference.dotted()),
            other => Found::Symbol(other.symbol().unwrap_or_default()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

pub(crate) fn lex(source: &str) -> Result<Vec<Token>, Error> {
    Lexer {
        source,
        at: 0,
        tokens: Vec::new(),
    }
    .run()
}

struct Lexer<'a> {
    source: &'a str,
    at: usize,
    tokens: Vec<Token>,
}

fn starts_ident(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn continues_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Every `$name.path` written in `source`, in order, with the bytes it is written at: what a host renaming or inlining a name it declares rewrites, leaving everything else as written. A text that only looks like a reference — `"$5"` — is not one.
pub fn references_in(source: &str) -> Result<Vec<(Span, Reference)>, Error> {
    Ok(lex(source)?
        .into_iter()
        .filter_map(|token| match token.kind {
            TokenKind::Reference(reference) => Some((token.span, reference)),
            _ => None,
        })
        .collect())
}

/// Whether `name` is one name as an expression writes it: letters, digits and `_`, not starting with a digit. It is what may follow `$` in a reference and what a function or a constant is called.
///
/// A host that lets its user name things an expression reads — a source, a variable — checks those names with this, so every name it accepts is one an expression can spell.
pub fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(starts_ident) && chars.all(continues_ident)
}

impl Lexer<'_> {
    fn peek(&self) -> Option<char> {
        self.source[self.at..].chars().next()
    }

    fn peek_second(&self) -> Option<char> {
        self.source[self.at..].chars().nth(1)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += c.len_utf8();
        Some(c)
    }

    fn eat_while(&mut self, keep: impl Fn(char) -> bool) {
        while self.peek().is_some_and(&keep) {
            self.bump();
        }
    }

    fn push(&mut self, kind: TokenKind, start: usize) {
        self.tokens.push(Token {
            kind,
            span: Span::new(start, self.at),
        });
    }

    fn run(mut self) -> Result<Vec<Token>, Error> {
        while let Some(c) = self.peek() {
            let start = self.at;
            if c.is_whitespace() {
                self.bump();
            } else if c.is_ascii_digit()
                || (c == '.' && self.peek_second().is_some_and(|c| c.is_ascii_digit()))
            {
                self.number(start)?;
            } else if starts_ident(c) {
                self.eat_while(continues_ident);
                let name = self.source[start..self.at].to_string();
                self.push(TokenKind::Ident(name), start);
            } else if c == '$' {
                self.reference(start)?;
            } else if c == '"' || c == '\'' {
                self.text(start, c)?;
            } else if c == '#' {
                self.color(start)?;
            } else {
                self.bump();
                let kind = self.operator(c, start)?;
                self.push(kind, start);
            }
        }
        Ok(self.tokens)
    }

    fn number(&mut self, start: usize) -> Result<(), Error> {
        self.eat_while(|c| c.is_ascii_digit() || c == '.' || c == '_');
        // An exponent counts only when a digit follows, so `2e` is not a number and `2 e` keeps `e` the constant.
        if let Some('e' | 'E') = self.peek() {
            let mark = self.at;
            self.bump();
            if let Some('+' | '-') = self.peek() {
                self.bump();
            }
            if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.eat_while(|c| c.is_ascii_digit());
            } else {
                self.at = mark;
            }
        }
        let written = &self.source[start..self.at];
        let digits: String = written.chars().filter(|c| *c != '_').collect();
        let span = Span::new(start, self.at);
        let number: f64 = digits.parse().map_err(|_| {
            Error::syntax(
                span,
                ErrorCode::InvalidNumber {
                    written: written.to_string(),
                },
            )
        })?;
        if !number.is_finite() {
            return Err(Error::syntax(
                span,
                ErrorCode::NumberTooLarge {
                    written: written.to_string(),
                },
            ));
        }
        self.push(TokenKind::Number(number), start);
        Ok(())
    }

    fn ident(&mut self) -> Option<String> {
        let start = self.at;
        if !self.peek().is_some_and(starts_ident) {
            return None;
        }
        self.eat_while(continues_ident);
        Some(self.source[start..self.at].to_string())
    }

    fn reference(&mut self, start: usize) -> Result<(), Error> {
        self.bump();
        let Some(name) = self.ident() else {
            return Err(Error::syntax(
                Span::new(start, self.at),
                ErrorCode::ReferenceNeedsName,
            ));
        };
        let mut path = Vec::new();
        while self.peek() == Some('.') && self.peek_second().is_some_and(starts_ident) {
            self.bump();
            path.extend(self.ident());
        }
        self.push(TokenKind::Reference(Reference { name, path }), start);
        Ok(())
    }

    fn text(&mut self, start: usize, quote: char) -> Result<(), Error> {
        self.bump();
        let mut text = String::new();
        loop {
            let escape_start = self.at;
            match self.bump() {
                None => {
                    return Err(Error::syntax(
                        Span::new(start, self.at),
                        ErrorCode::UnclosedText { quote },
                    ));
                }
                Some(c) if c == quote => break,
                Some('\\') => {
                    let escaped = match self.bump() {
                        Some('n') => '\n',
                        Some('t') => '\t',
                        Some(c @ ('\\' | '"' | '\'')) => c,
                        _ => {
                            return Err(Error::syntax(
                                Span::new(escape_start, self.at),
                                ErrorCode::UnknownEscape,
                            ));
                        }
                    };
                    text.push(escaped);
                }
                Some(c) => text.push(c),
            }
        }
        self.push(TokenKind::Text(text.into()), start);
        Ok(())
    }

    fn color(&mut self, start: usize) -> Result<(), Error> {
        self.bump();
        self.eat_while(|c| c.is_alphanumeric());
        let written = &self.source[start..self.at];
        let color = Color::from_hex(written).ok_or_else(|| {
            Error::syntax(
                Span::new(start, self.at),
                ErrorCode::InvalidColor {
                    written: written.to_string(),
                },
            )
        })?;
        self.push(TokenKind::Color(color), start);
        Ok(())
    }

    fn operator(&mut self, c: char, start: usize) -> Result<TokenKind, Error> {
        let mut pair = |second: char, paired: TokenKind| {
            if self.peek() == Some(second) {
                self.bump();
                Some(paired)
            } else {
                None
            }
        };
        let kind = match c {
            '+' => TokenKind::Plus,
            '-' | '−' => TokenKind::Minus,
            '*' | '×' | '·' => TokenKind::Star,
            '/' | '÷' => TokenKind::Slash,
            '^' => TokenKind::Caret,
            '%' => TokenKind::Percent,
            '(' => TokenKind::OpenParen,
            ')' => TokenKind::CloseParen,
            '[' => TokenKind::OpenBracket,
            ']' => TokenKind::CloseBracket,
            '{' => TokenKind::OpenBrace,
            '}' => TokenKind::CloseBrace,
            ',' => TokenKind::Comma,
            '<' => pair('=', TokenKind::LessEqual).unwrap_or(TokenKind::Less),
            '>' => pair('=', TokenKind::GreaterEqual).unwrap_or(TokenKind::Greater),
            '!' => pair('=', TokenKind::NotEqual).unwrap_or(TokenKind::Bang),
            '=' => pair('=', TokenKind::Equal).ok_or_else(|| {
                Error::syntax(
                    Span::new(start, self.at),
                    ErrorCode::LoneOperator {
                        found: '=',
                        suggestion: "==",
                    },
                )
            })?,
            '&' => pair('&', TokenKind::And).ok_or_else(|| {
                Error::syntax(
                    Span::new(start, self.at),
                    ErrorCode::LoneOperator {
                        found: '&',
                        suggestion: "&&",
                    },
                )
            })?,
            '|' => pair('|', TokenKind::Or).ok_or_else(|| {
                Error::syntax(
                    Span::new(start, self.at),
                    ErrorCode::LoneOperator {
                        found: '|',
                        suggestion: "||",
                    },
                )
            })?,
            other => {
                return Err(Error::syntax(
                    Span::new(start, self.at),
                    ErrorCode::UnexpectedCharacter { found: other },
                ));
            }
        };
        Ok(kind)
    }
}

#[cfg(test)]
#[path = "lexer_test.rs"]
mod tests;
