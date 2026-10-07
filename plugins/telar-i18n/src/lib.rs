//! Locale data for Telar from ICU4X: the CLDR plural rules a catalog's plural messages are selected by, and number and date formatting that follows the active locale.
//!
//! `telar` selects plurals with a small built-in table and formats nothing. This crate is the opt-in battery behind both: [`install`] makes every catalog plural select by [`CldrPluralRules`], and [`format_number`] and [`format_date`] write a value the way the active locale does. Both formatters read the locale with `use_locale()`, so text built from them inside a widget's content closure re-renders on a language switch.
//!
//! ```toml
//! # Cargo.toml
//! telar-i18n = "0.2.1"
//! ```
//!
//! ```
//! use telar_i18n::{Date, Length, format_date, format_number};
//!
//! telar_i18n::install();
//! telar::set_locale("de");
//! assert_eq!(format_number(1_234_567), "1.234.567");
//! assert_eq!(format_date(Date::try_new_iso(2024, 3, 15).unwrap(), Length::Long), "15. März 2024");
//! ```
//!
//! An application calls [`install`] once, from `telar::app!`'s setup block rather than from `main`: the block runs as the application starts and again in every hot-reload dylib, which carries its own copy of the catalog runtime and would otherwise keep selecting plurals by the built-in table. It is a call rather than something that happens on linking the crate, so a binary that only formats keeps the built-in plural table until it asks.
//!
//! The data is ICU4X's compiled CLDR data, linked per component, so a binary carries plural rules, number symbols and Gregorian date patterns and nothing else ICU4X has. Relative time ("3 days ago") is not here: ICU4X ships it only in `icu_experimental`, whose API is unstable across minor releases.
//!
//! **Keep this crate on the same version as `telar`.** The rules it installs implement a trait reached through the facade, so a mismatch resolves two copies of the catalog runtime and the rules land in the one the application's catalog does not read. It is the same lockstep `telar` and `telar-macros` already have, and for the same reason.

#![warn(rustdoc::broken_intra_doc_links)]

mod format;
mod locale;
mod plural;

pub use fixed_decimal::{Decimal, FloatPrecision};
pub use format::{format_date, format_number};
pub use icu_calendar::{Date, Iso};
pub use icu_datetime::options::Length;
pub use plural::{CldrPluralRules, install};
