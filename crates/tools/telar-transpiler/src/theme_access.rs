//! What a `$theme` read compiles to: a field of the application's own theme type, or, in a `[telar] library` that cannot name one, a token of the vocabulary every theme answers.

use telar_project::theme_tokens;

/// How the markup's `$theme` reaches a theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ThemeAccess {
    /// Through the `theme` handle each generated fn binds over the application's theme type: `$theme.x` is `theme.get().x`.
    Handle,
    /// Through `ThemeTokens`, for a library: `$theme.x` is `::telar::use_theme_tokens().x()`, and a name that is not a token is a `compile_error!` where it was written. No `theme` is bound, so nothing captures or clones one.
    Tokens,
}

impl ThemeAccess {
    pub(crate) fn for_library(library: bool) -> Self {
        match library {
            true => Self::Tokens,
            false => Self::Handle,
        }
    }
}

/// The facade function a library's `$theme` reads through. It subscribes the caller to the theme in force exactly as `theme.get()` does, so a read inside a closure follows a theme switch the same way in a library as in an application.
const TOKENS_FN: &str = "::telar::use_theme_tokens()";

/// A library's `$theme` read, from the text that follows `$theme`.
pub(crate) enum TokensRead<'a> {
    /// A bare `$theme`: the tokens themselves.
    Tokens,
    /// `$theme.name` naming a token, which starts one byte into the text; `called` is whether the author already wrote the parens.
    Token { name: &'a str, called: bool },
    /// `$theme.name` naming anything else.
    Unknown { name: &'a str },
}

impl<'a> TokensRead<'a> {
    pub(crate) fn parse(after: &'a str) -> Self {
        let Some(tail) = after.strip_prefix('.') else {
            return Self::Tokens;
        };
        let len = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(tail.len());
        let name = &tail[..len];
        if name.is_empty() {
            return Self::Tokens;
        }
        match theme_tokens::is_token(name) {
            true => Self::Token {
                name,
                called: tail[len..].starts_with('('),
            },
            false => Self::Unknown { name },
        }
    }

    /// How many bytes of the text after `$theme` this read stands for.
    pub(crate) fn consumed(&self) -> usize {
        match self {
            Self::Tokens => 0,
            Self::Token { name, .. } | Self::Unknown { name } => 1 + name.len(),
        }
    }

    /// The Rust this read compiles to, with `mark` placed immediately before a token's name so a caller can record where that name came from.
    pub(crate) fn emit(&self, mark: &str) -> String {
        match self {
            Self::Tokens => TOKENS_FN.to_string(),
            Self::Token { name, called } => {
                let parens = if *called { "" } else { "()" };
                format!("{TOKENS_FN}.{mark}{name}{parens}")
            }
            Self::Unknown { name } => format!(
                "compile_error!({})",
                crate::view::rust_str(&unknown_token_message(name))
            ),
        }
    }
}

fn unknown_token_message(name: &str) -> String {
    let tokens: Vec<&str> = theme_tokens::all().collect();
    format!(
        "`$theme.{name}`: `{name}` is not a `ThemeTokens` token. A `[telar] library` cannot name its application's theme type, so its `$theme` reads only the tokens every theme answers: {}",
        tokens.join(", ")
    )
}

#[cfg(test)]
#[path = "theme_access_test.rs"]
mod tests;
