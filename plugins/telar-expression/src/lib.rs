//! A small typed expression language: the formulas a user writes against an application's live readings — `fmt("{}%", round($battery.level))`, `if($media.playing, $theme.accent, #888)` — checked before they ever run, and bound to signals so each recomputes only when something it read moves.
//!
//! **Pure and total.** An expression reads values and computes one. It cannot run a command, touch a file or look at a clock, so an expression arriving in a shared bundle is safe to evaluate. Nor can it crash its host: every failure — a syntax error, a type mismatch, a division by zero at run time, a nesting deeper than [`MAX_DEPTH`] — is an [`Error`] carrying the [`Span`] it is about, which [`render`] draws with a caret under it.
//!
//! **The language.** Literals: numbers (`1_000`, `2.5e-3`), percentages (`10%`), texts (`"…"` or `'…'`, with `\n`, `\t`, `\\` and quote escapes), `true` and `false`, colours (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`) and lists (`{1, 2, 3}`). References: `$name` and `$name.path.to.field`, whatever the host declares. Operators from loosest to tightest: `||`; `&&`; `==` `!=`; `<` `<=` `>` `>=`; `+` `-`; `*` `/`; a leading `-`, `+` or `!`; `^`, right-associative and tighter than a leading minus, so `-2^2` is `-4`; and the postfix `%`. `+` also joins two texts. `(…)` and `[…]` both group. The conditional is `if(condition, then, else)`, which evaluates only the branch it takes, and `&&` and `||` short-circuit. Function, constant and keyword names ignore ASCII case.
//!
//! **Percentages work as on a calculator.** A percentage on the right of `+` or `-` is taken of the left side, so `200 + 10%` is 220 and `200 - 10%` is 180. Anywhere else it is a hundredth: `50 * 10%` is 5.
//!
//! **Typed before it runs.** [`compile`] parses an expression and checks it against the host's [`Environment`], which declares the type of every reference, and a [`Registry`] of functions and constants, so a mistake is reported where it is written instead of on the first evaluation that happens to reach it. [`Compiled::evaluate`] then reads current values through a [`Resolver`].
//!
//! **Functions come in families.** [`Registry::standard`] carries [`families::text`], [`families::math`], [`families::list`] and [`families::color`]. A host adds its own the same way: a function that registers forms with typed [`Signature`]s.
//!
//! **Reactive binding.** [`bind`] turns a compiled expression and a resolver reading the host's signals into a [`Memo`](reactive_core::Memo) whose dependencies are exactly what the last evaluation read, and [`bind_held`] keeps the last good value through an error.
//!
//! **Localizing errors.** An [`Error`] carries an [`ErrorCode`], an enum whose variants hold the values the message is built from — the types expected and found, the name, the function and its signatures, the limit. [`ErrorCode::key`] is a stable snake_case id (`"unknown_name"`, `"argument_count"`), and the variant's public fields are the arguments (types, patterns, signatures and references implement `Display`), so a host maps key and fields to an entry in its own catalogue and keeps [`Error::span`] for the caret. `Display` for [`ErrorCode`] and [`Error`], [`Error::message`] and [`render`] are the English wording, so a host that does not localize needs nothing more. A failure only the host knows — an unknown field on its source, a reading with no value yet — is a [`HostError`]: [`Environment::reference`], [`Resolver::read`] and a family's [`CallError`] return one (as [`ErrorCode::Host`]), carrying the host's own catalogue key, its arguments and the English message. The crate prints that message and never interprets the key.
//!
//! **A calculator for free.** [`calculate`] evaluates a closed numeric expression with the standard functions. It is strict on purpose — a trailing word, an unknown name or a non-finite result is an error, not a guess — so text that is not a sum is told apart from one, and [`format_number`] prints the answer the way a calculator does.
//!
//! **A second dependency, like `telar-dynamic`.** It knows nothing of `telar`, and `telar` neither re-exports it nor names it in a feature, since most applications never bind a property to a formula. One that does names this crate beside `telar`, and keeps the two on one version — the same lockstep `telar` and `telar-macros` already have.
//!
//! # Feature flags
#![cfg_attr(
    feature = "document-features",
    doc = document_features::document_features!()
)]
#![warn(rustdoc::broken_intra_doc_links)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

use std::sync::LazyLock;

mod bind;
mod code;
mod compile;
mod error;
mod eval;
pub mod families;
mod lexer;
mod reference;
mod registry;
mod span;
mod syntax;
mod types;
mod value;

pub use bind::{Held, bind, bind_held};
pub use code::{Closing, Context, ErrorCode, Found, HostError, ValueKind};
pub use compile::{Compiled, compile};
pub use error::{Error, ErrorKind, Errors, render};
pub use lexer::{is_identifier, references_in};
pub use reference::{Closed, Environment, Reference, Resolver};
pub use registry::{Arguments, CallError, Constant, Function, Implementation, Registry};
pub use span::Span;
pub use syntax::MAX_DEPTH;
pub use types::{Pattern, Signature, Type};
pub use value::{MAX_TEXT_BYTES, Value, format_number};

/// Evaluates `source` as a closed numeric expression — no references, the standard functions and constants — and answers the number, or the first thing wrong with it.
///
/// Strict, which is the point of it for a launcher evaluating every keystroke: `2 + 3 firefox`, `sqrt` without brackets, `1/0` and anything that is not a number are errors, so text that is really an application's name is never mistaken for a sum.
pub fn calculate(source: &str) -> Result<f64, Error> {
    static STANDARD: LazyLock<Registry> = LazyLock::new(Registry::standard);
    let compiled = compile(source, &Closed, &STANDARD)
        .map_err(Errors::into_first)?
        .require(&Type::Number)?;
    match compiled.evaluate(&Closed)? {
        Value::Number(n) => Ok(n),
        other => Err(Error::evaluation(
            Span::new(0, source.len()),
            ErrorCode::ExpectedType {
                expected: Type::Number,
                found: other.type_of(),
            },
        )),
    }
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
