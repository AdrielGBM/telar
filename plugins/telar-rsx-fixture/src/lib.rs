//! The end-to-end `[telar] library` fixture: a component written in `.rsx` that reads the theme's tokens, translates through its own catalog, draws a baked SVG and bakes an icon from an Iconify set of its own, one written in Rust with previews written in Rust, and one written in Rust drawing an icon it names with `telar_icons::icon!`.
//!
//! It is packaged, unpacked read-only and compiled as a dependency of a generated application by `crates/tools/cargo-telar/src/rsx_library_test.rs`, which is the proof that a library published this way builds without the CLI ever running on it.

telar::rsx_modules!();

mod marker;
mod tally;

pub use greeting_card::{GreetingCardProps, greeting_card};
pub use marker::marker;
pub use tally::{TallyProps, tally};

telar::__previews! {
    mod previews;

    /// Every preview this crate carries, its `.rsx` ones first: the one fn an application collects a library's previews through. Defining it here shadows the one `rsx_modules!` defines, which lists only the `.rsx` ones.
    pub fn telar_all_previews() -> Vec<telar::preview::PreviewEntry> {
        [telar_rsx_previews(), previews::previews()].concat()
    }
}
