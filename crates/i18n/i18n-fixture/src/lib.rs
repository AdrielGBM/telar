//! The end-to-end i18n test, in the smallest crate that can hold it.
//!
//! `t!` resolves `crate::__rsx_i18n::CATALOG`, which only exists in a crate whose build ran the baker over a `locales/` directory — so this cannot be a test under `telar-transpiler`, which has neither. A fixture member crate is the whole apparatus: `rsx_modules!` bakes the catalog (the same `transpile_project` pass `app!` runs), and nothing here needs a theme, a root component or a window.

#![warn(rustdoc::broken_intra_doc_links)]

telar::rsx_modules!();

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
