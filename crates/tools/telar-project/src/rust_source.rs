#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Ident { name: String, raw: bool },
    Str(String),
    Punct(char),
}

/// A token and the number of `{` open around it; a brace counts as outside the block it opens and closes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) depth: usize,
}

impl Token {
    pub(crate) fn is_word(&self, word: &str) -> bool {
        matches!(&self.kind, TokenKind::Ident { name, raw: false } if name == word)
    }

    pub(crate) fn is_punct(&self, punct: char) -> bool {
        self.kind == TokenKind::Punct(punct)
    }

    pub(crate) fn ident(&self) -> Option<&str> {
        match &self.kind {
            TokenKind::Ident { name, .. } => Some(name),
            _ => None,
        }
    }

    pub(crate) fn string(&self) -> Option<&str> {
        match &self.kind {
            TokenKind::Str(value) => Some(value),
            _ => None,
        }
    }
}

/// The identifiers, string values and punctuation of Rust source, with comments, character literals, lifetimes and numbers dropped. Input that ends inside a comment or literal ends the stream: rustc reports that error itself.
pub(crate) fn tokens(source: &str) -> Vec<Token> {
    let mut lexer = Lexer {
        chars: source.chars().collect(),
        at: 0,
        depth: 0,
        out: Vec::new(),
    };
    lexer.run();
    lexer.out
}

struct Lexer {
    chars: Vec<char>,
    at: usize,
    depth: usize,
    out: Vec<Token>,
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

impl Lexer {
    fn peek(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.at + ahead).copied()
    }

    fn push(&mut self, kind: TokenKind) {
        self.out.push(Token {
            kind,
            depth: self.depth,
        });
    }

    fn run(&mut self) {
        while let Some(c) = self.peek(0) {
            match c {
                '/' if self.peek(1) == Some('/') => self.skip_line_comment(),
                '/' if self.peek(1) == Some('*') => self.skip_block_comment(),
                '"' => self.string(),
                '\'' => self.quote(),
                c if is_ident_start(c) => self.word(),
                c if c.is_ascii_digit() => self.number(),
                c if c.is_whitespace() => self.at += 1,
                c => {
                    self.at += 1;
                    match c {
                        '{' => {
                            self.push(TokenKind::Punct(c));
                            self.depth += 1;
                            continue;
                        }
                        '}' => self.depth = self.depth.saturating_sub(1),
                        _ => {}
                    }
                    self.push(TokenKind::Punct(c));
                }
            }
        }
    }

    fn skip_line_comment(&mut self) {
        while self.peek(0).is_some_and(|c| c != '\n') {
            self.at += 1;
        }
    }

    fn skip_block_comment(&mut self) {
        self.at += 2;
        let mut nesting = 1;
        while nesting > 0 {
            match (self.peek(0), self.peek(1)) {
                (None, _) => return,
                (Some('/'), Some('*')) => {
                    nesting += 1;
                    self.at += 2;
                }
                (Some('*'), Some('/')) => {
                    nesting -= 1;
                    self.at += 2;
                }
                _ => self.at += 1,
            }
        }
    }

    fn number(&mut self) {
        while self.peek(0).is_some_and(is_ident_continue) {
            self.at += 1;
        }
    }

    fn string(&mut self) {
        self.at += 1;
        let mut value = String::new();
        loop {
            match self.peek(0) {
                None => return,
                Some('"') => {
                    self.at += 1;
                    break;
                }
                Some('\\') => {
                    self.at += 1;
                    match self.peek(0) {
                        None => return,
                        Some('\n') => {
                            while self.peek(0).is_some_and(char::is_whitespace) {
                                self.at += 1;
                            }
                        }
                        Some(c @ ('"' | '\\')) => {
                            value.push(c);
                            self.at += 1;
                        }
                        Some(c) => {
                            value.push('\\');
                            value.push(c);
                            self.at += 1;
                        }
                    }
                }
                Some(c) => {
                    value.push(c);
                    self.at += 1;
                }
            }
        }
        self.push(TokenKind::Str(value));
    }

    fn raw_string(&mut self, hashes: usize) {
        self.at += hashes + 1;
        let mut value = String::new();
        loop {
            match self.peek(0) {
                None => return,
                Some('"') if (1..=hashes).all(|i| self.peek(i) == Some('#')) => {
                    self.at += hashes + 1;
                    break;
                }
                Some(c) => {
                    value.push(c);
                    self.at += 1;
                }
            }
        }
        self.push(TokenKind::Str(value));
    }

    fn quote(&mut self) {
        let is_char_literal = matches!(
            (self.peek(1), self.peek(2)),
            (Some('\\'), _) | (Some(_), Some('\''))
        );
        self.at += 1;
        if is_char_literal {
            self.char_literal_body();
        } else {
            while self.peek(0).is_some_and(is_ident_continue) {
                self.at += 1;
            }
        }
    }

    fn char_literal_body(&mut self) {
        while let Some(c) = self.peek(0) {
            self.at += 1;
            match c {
                '\\' => self.at += 1,
                '\'' => return,
                _ => {}
            }
        }
    }

    fn word(&mut self) {
        let start = self.at;
        while self.peek(0).is_some_and(is_ident_continue) {
            self.at += 1;
        }
        let name: String = self.chars[start..self.at].iter().collect();
        match (name.as_str(), self.peek(0)) {
            ("b" | "c", Some('"')) => return self.string(),
            ("b", Some('\'')) => {
                self.at += 1;
                return self.char_literal_body();
            }
            ("r" | "br" | "cr", Some('#' | '"')) => {
                let hashes = (0..).take_while(|&i| self.peek(i) == Some('#')).count();
                if self.peek(hashes) == Some('"') {
                    return self.raw_string(hashes);
                }
                if name == "r" && hashes == 1 && self.peek(1).is_some_and(is_ident_start) {
                    self.at += 1;
                    let start = self.at;
                    while self.peek(0).is_some_and(is_ident_continue) {
                        self.at += 1;
                    }
                    let name = self.chars[start..self.at].iter().collect();
                    return self.push(TokenKind::Ident { name, raw: true });
                }
            }
            _ => {}
        }
        self.push(TokenKind::Ident { name, raw: false });
    }
}

#[cfg(test)]
#[path = "rust_source.test.rs"]
mod tests;
