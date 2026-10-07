//! The parser: tokens into a syntax tree, by recursive descent, with every node carrying its span.

use std::sync::Arc;

use geometry_core::Color;

use crate::lexer::{Token, TokenKind, lex};
use crate::{Closing, Error, ErrorCode, Found, Reference, Span};

/// How deep an expression may nest, counting both brackets and chains of operators. The checker and the evaluator recurse over the tree, so an unbounded one could overflow the stack on hostile input; this bound is what keeps them total.
pub const MAX_DEPTH: u32 = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnaryOp {
    Negate,
    Plus,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BinaryOp {
    Or,
    And,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Add,
    Subtract,
    Multiply,
    Divide,
    Power,
}

impl BinaryOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinaryOp::Or => "||",
            BinaryOp::And => "&&",
            BinaryOp::Equal => "==",
            BinaryOp::NotEqual => "!=",
            BinaryOp::Less => "<",
            BinaryOp::LessEqual => "<=",
            BinaryOp::Greater => ">",
            BinaryOp::GreaterEqual => ">=",
            BinaryOp::Add => "+",
            BinaryOp::Subtract => "-",
            BinaryOp::Multiply => "*",
            BinaryOp::Divide => "/",
            BinaryOp::Power => "^",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ExprKind {
    Number(f64),
    Text(Arc<str>),
    Bool(bool),
    Color(Color),
    List(Vec<Expr>),
    Reference(Reference),
    Name(String),
    Percent(Box<Expr>),
    Unary(UnaryOp, Box<Expr>),
    Binary {
        op: BinaryOp,
        op_span: Span,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    If {
        condition: Box<Expr>,
        then: Box<Expr>,
        otherwise: Box<Expr>,
    },
    Call {
        name: String,
        name_span: Span,
        arguments: Vec<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Expr {
    pub kind: ExprKind,
    pub span: Span,
    depth: u32,
}

impl Expr {
    fn new(kind: ExprKind, span: Span) -> Self {
        let child = |expr: &Expr| expr.depth;
        let deepest = match &kind {
            ExprKind::Number(_)
            | ExprKind::Text(_)
            | ExprKind::Bool(_)
            | ExprKind::Color(_)
            | ExprKind::Reference(_)
            | ExprKind::Name(_) => 0,
            ExprKind::List(items) => items.iter().map(child).max().unwrap_or(0),
            ExprKind::Call { arguments, .. } => arguments.iter().map(child).max().unwrap_or(0),
            ExprKind::Percent(inner) | ExprKind::Unary(_, inner) => child(inner),
            ExprKind::Binary { left, right, .. } => child(left).max(child(right)),
            ExprKind::If {
                condition,
                then,
                otherwise,
            } => child(condition).max(child(then)).max(child(otherwise)),
        };
        Self {
            kind,
            span,
            depth: deepest + 1,
        }
    }

    /// Whether this is written as a percentage — `10%`, `-10%` — which is what makes `a + b%` relative to `a`.
    pub fn is_percent(&self) -> bool {
        match &self.kind {
            ExprKind::Percent(_) => true,
            ExprKind::Unary(UnaryOp::Negate | UnaryOp::Plus, inner) => inner.is_percent(),
            _ => false,
        }
    }
}

pub(crate) fn parse(source: &str) -> Result<Expr, Error> {
    let tokens = lex(source)?;
    if tokens.is_empty() {
        return Err(Error::syntax(
            Span::new(0, source.len()),
            ErrorCode::EmptyExpression,
        ));
    }
    let mut parser = Parser {
        tokens: &tokens,
        at: 0,
        end: source.len(),
        nesting: 0,
    };
    let expr = parser.expression()?;
    if let Some(token) = parser.peek() {
        return Err(Error::syntax(
            token.span,
            ErrorCode::TrailingToken {
                found: token.kind.found(),
            },
        ));
    }
    Ok(expr)
}

struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
    end: usize,
    nesting: u32,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn peek_kind(&self) -> Option<&TokenKind> {
        self.peek().map(|token| &token.kind)
    }

    fn eat(&mut self, kind: &TokenKind) -> Option<Span> {
        let token = self.peek().filter(|token| token.kind == *kind)?;
        let span = token.span;
        self.at += 1;
        Some(span)
    }

    fn here(&self) -> Span {
        self.peek()
            .map_or(Span::point(self.end), |token| token.span)
    }

    fn expect(&mut self, kind: &TokenKind, closing: Closing) -> Result<Span, Error> {
        if let Some(span) = self.eat(kind) {
            return Ok(span);
        }
        let found = self.peek().map_or(Found::End, |token| token.kind.found());
        Err(Error::syntax(
            self.here(),
            ErrorCode::ExpectedClosing {
                expected: kind.symbol().unwrap_or_default(),
                closing,
                found,
            },
        ))
    }

    fn bounded(&self, expr: Expr) -> Result<Expr, Error> {
        if expr.depth > MAX_DEPTH {
            return Err(Error::syntax(expr.span, ErrorCode::TooDeep));
        }
        Ok(expr)
    }

    fn expression(&mut self) -> Result<Expr, Error> {
        self.binary(0)
    }

    fn binary(&mut self, level: usize) -> Result<Expr, Error> {
        const LEVELS: [&[(TokenKind, BinaryOp)]; 6] = [
            &[(TokenKind::Or, BinaryOp::Or)],
            &[(TokenKind::And, BinaryOp::And)],
            &[
                (TokenKind::Equal, BinaryOp::Equal),
                (TokenKind::NotEqual, BinaryOp::NotEqual),
            ],
            &[
                (TokenKind::Less, BinaryOp::Less),
                (TokenKind::LessEqual, BinaryOp::LessEqual),
                (TokenKind::Greater, BinaryOp::Greater),
                (TokenKind::GreaterEqual, BinaryOp::GreaterEqual),
            ],
            &[
                (TokenKind::Plus, BinaryOp::Add),
                (TokenKind::Minus, BinaryOp::Subtract),
            ],
            &[
                (TokenKind::Star, BinaryOp::Multiply),
                (TokenKind::Slash, BinaryOp::Divide),
            ],
        ];
        let Some(operators) = LEVELS.get(level) else {
            return self.unary();
        };
        let mut left = self.binary(level + 1)?;
        loop {
            let Some((op, op_span)) = operators
                .iter()
                .find_map(|(kind, op)| self.eat(kind).map(|span| (*op, span)))
            else {
                return Ok(left);
            };
            let right = self.binary(level + 1)?;
            let span = left.span.to(right.span);
            left = self.bounded(Expr::new(
                ExprKind::Binary {
                    op,
                    op_span,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            ))?;
        }
    }

    fn unary(&mut self) -> Result<Expr, Error> {
        if self.nesting >= MAX_DEPTH {
            return Err(Error::syntax(self.here(), ErrorCode::TooDeep));
        }
        self.nesting += 1;
        let result = self.unary_inner();
        self.nesting -= 1;
        result
    }

    fn unary_inner(&mut self) -> Result<Expr, Error> {
        let op = match self.peek_kind() {
            Some(TokenKind::Minus) => Some(UnaryOp::Negate),
            Some(TokenKind::Plus) => Some(UnaryOp::Plus),
            Some(TokenKind::Bang) => Some(UnaryOp::Not),
            _ => None,
        };
        let Some(op) = op else {
            return self.power();
        };
        let op_span = self.here();
        self.at += 1;
        let operand = self.unary()?;
        let span = op_span.to(operand.span);
        self.bounded(Expr::new(ExprKind::Unary(op, Box::new(operand)), span))
    }

    /// Binds tighter than a leading minus, so `-2^2` is `-4`; right-associative, so `2^3^2` is `2^9`.
    fn power(&mut self) -> Result<Expr, Error> {
        let base = self.postfix()?;
        let Some(op_span) = self.eat(&TokenKind::Caret) else {
            return Ok(base);
        };
        let exponent = self.unary()?;
        let span = base.span.to(exponent.span);
        self.bounded(Expr::new(
            ExprKind::Binary {
                op: BinaryOp::Power,
                op_span,
                left: Box::new(base),
                right: Box::new(exponent),
            },
            span,
        ))
    }

    fn postfix(&mut self) -> Result<Expr, Error> {
        let value = self.primary()?;
        let Some(percent) = self.eat(&TokenKind::Percent) else {
            return Ok(value);
        };
        let span = value.span.to(percent);
        self.bounded(Expr::new(ExprKind::Percent(Box::new(value)), span))
    }

    fn primary(&mut self) -> Result<Expr, Error> {
        let Some(token) = self.peek().cloned() else {
            return Err(Error::syntax(
                Span::point(self.end),
                ErrorCode::ExpectedValue { found: Found::End },
            ));
        };
        self.at += 1;
        let span = token.span;
        let kind = match token.kind {
            TokenKind::Number(n) => ExprKind::Number(n),
            TokenKind::Text(text) => ExprKind::Text(text),
            TokenKind::Color(color) => ExprKind::Color(color),
            TokenKind::Reference(reference) => ExprKind::Reference(reference),
            TokenKind::OpenParen => return self.group(span, &TokenKind::CloseParen),
            TokenKind::OpenBracket => return self.group(span, &TokenKind::CloseBracket),
            TokenKind::OpenBrace => {
                let items = self.items(&TokenKind::CloseBrace, Closing::List)?;
                let close = self.tokens[self.at - 1].span;
                return self.bounded(Expr::new(ExprKind::List(items), span.to(close)));
            }
            TokenKind::Ident(name) => return self.named(name, span),
            other => {
                return Err(Error::syntax(
                    span,
                    ErrorCode::ExpectedValue {
                        found: other.found(),
                    },
                ));
            }
        };
        Ok(Expr::new(kind, span))
    }

    fn group(&mut self, open: Span, close: &TokenKind) -> Result<Expr, Error> {
        let mut inner = self.expression()?;
        let close = self.expect(close, Closing::Group)?;
        inner.span = open.to(close);
        Ok(inner)
    }

    fn items(&mut self, close: &TokenKind, closing: Closing) -> Result<Vec<Expr>, Error> {
        let mut items = Vec::new();
        loop {
            if self.eat(close).is_some() {
                return Ok(items);
            }
            items.push(self.expression()?);
            if self.eat(&TokenKind::Comma).is_none() {
                self.expect(close, closing)?;
                return Ok(items);
            }
        }
    }

    fn named(&mut self, name: String, span: Span) -> Result<Expr, Error> {
        let lowered = name.to_ascii_lowercase();
        match lowered.as_str() {
            "true" => return Ok(Expr::new(ExprKind::Bool(true), span)),
            "false" => return Ok(Expr::new(ExprKind::Bool(false), span)),
            _ => {}
        }
        if self.eat(&TokenKind::OpenParen).is_none() {
            if lowered == "if" {
                return Err(Error::syntax(span, ErrorCode::IfNeedsCall));
            }
            return Ok(Expr::new(ExprKind::Name(name), span));
        }
        let arguments = self.items(&TokenKind::CloseParen, Closing::Call)?;
        let call_span = span.to(self.tokens[self.at - 1].span);
        if lowered == "if" {
            let Ok([condition, then, otherwise]) = <[Expr; 3]>::try_from(arguments) else {
                return Err(Error::syntax(call_span, ErrorCode::IfArity));
            };
            return self.bounded(Expr::new(
                ExprKind::If {
                    condition: Box::new(condition),
                    then: Box::new(then),
                    otherwise: Box::new(otherwise),
                },
                call_span,
            ));
        }
        self.bounded(Expr::new(
            ExprKind::Call {
                name,
                name_span: span,
                arguments,
            },
            call_span,
        ))
    }
}

#[cfg(test)]
#[path = "syntax_test.rs"]
mod tests;
