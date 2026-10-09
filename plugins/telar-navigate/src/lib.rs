//! Page-stack navigation for rsx: a reactive [`Navigator`] stack over an app-defined route type, and a [`NavHost`] container that renders the top of that stack as a page — lazily built, cached, and swapped with an optional [`NavTransition`].
//!
//! [`Navigator`] is the primitive: a `Vec<R>` behind a reactive signal, with push/pop history. [`NavHost`] is the view: it owns one layout container, builds each page once from a factory, shows only the active one (`set_display`), and reconciles navigation on each event — the same mechanism the runtime host uses to switch tabs, generalized over routes. A page is any [`NavPage`] (a `LayoutItem` with `on_enter` / `on_relayout` lifecycle hooks); [`SimplePage`] wraps a hook-less widget as one.
//!
//! A route type stays a plain `Clone` enum unless it needs an address outside the process. Implementing [`Route`] on it adds [`to_location`](Route::to_location) / [`from_location`](Route::from_location) against the platform-neutral [`Location`] (path segments, an optional in-page-anchor fragment, query-style params — not a URL), which unlocks [`Navigator::location`] and [`Navigator::locations`] for whichever target adapter serializes those further (web history, a desktop deep link, an Android intent, a TUI argument).
//!
//! [`TabStacks`] and [`TabHost`] are the same pair one level up: one stack *per tab* rather than one shared stack, which is the native model (`UITabBarController`, a nested `Navigator`, a nested nav graph) and what lets a tab you leave stay several screens deep until you come back to it.
//!
//! An application depends on it beside `telar`:
//!
//! ```toml
//! # Cargo.toml
//! telar-navigate = "0.2.2"
//!
//! # telar.toml
//! [telar]
//! prelude = ["telar_navigate"]
//! ```
//!
//! The prelude entry is what lets `.rsx` name `Navigator` or `NavTransition` without a `use`: generated code glob-imports it after `telar`'s own items. [`Location`] and [`Route`] are re-exported here as the very items `telar` exports, so the two globs name one item and never clash.
//!
//! The mechanism under the stack stays in `telar`: the app's address and its history, `to:` links, and the [`HistoryFollower`](telar::HistoryFollower) seam a [`Navigator`] follows the address through. This crate is one opinionated route stack written against that seam, the way an application could write its own.
//!
//! **Keep this crate on the same version as `telar`.** Every kernel type a page here takes or returns is reached through the facade, so a mismatch resolves two copies of the kernel and a page built here stops being the `LayoutItem` a tree over there accepts — a type error naming one trait twice. It is the same lockstep `telar` and `telar-macros` already have, and for the same reason.

#![warn(rustdoc::broken_intra_doc_links)]

mod host;
mod navigator;
mod page;
mod tabs;
mod transition;

pub use host::NavHost;
pub use navigator::Navigator;
pub use page::{NavPage, PagePolicy, SimplePage};
pub use tabs::{TabHost, TabStacks};
pub use telar::{Location, Route};
pub use transition::NavTransition;

telar::__previews! {
    mod previews;

    /// Every preview this crate carries: the one fn an application collects a library's previews through.
    pub fn telar_all_previews() -> Vec<telar::preview::PreviewEntry> {
        previews::previews()
    }
}
