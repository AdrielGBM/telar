//! The end-to-end `[telar] library` fixture: a component written in `.rsx` that reads the theme's tokens, translates through its own catalog, draws a baked SVG and bakes an icon from an Iconify set of its own.
//!
//! It is packaged, unpacked read-only and compiled as a dependency of a generated application by `crates/tools/cargo-telar/src/rsx_library_test.rs`, which is the proof that a library published this way builds without the CLI ever running on it.

telar::rsx_modules!();

pub use greeting_card::{GreetingCardProps, greeting_card};
