//! Page-stack navigation for rsx: a reactive [`Navigator`] stack over an app-defined route type, and a [`NavHost`] container that renders the top of that stack as a page — lazily built, cached, and swapped with an optional [`NavTransition`].
//!
//! [`Navigator`] is the primitive: a `Vec<R>` behind a reactive signal, with push/pop history. [`NavHost`] is the view: it owns one layout container, builds each page once from a factory, shows only the active one (`set_display`), and reconciles navigation on each event — the same mechanism the runtime host uses to switch tabs, generalized over routes. A page is any [`NavPage`] (a `LayoutItem` with `on_enter` / `on_relayout` lifecycle hooks); [`SimplePage`] wraps a hook-less widget as one.
//!
//! A route type stays a plain `Clone` enum unless it needs an address outside the process. Implementing [`Route`] on it adds [`to_location`](Route::to_location) / [`from_location`](Route::from_location) against the platform-neutral [`Location`] (path segments, an optional in-page-anchor fragment, query-style params — not a URL), which unlocks [`Navigator::location`] and [`Navigator::locations`] for whichever target adapter serializes those further (web history, a desktop deep link, an Android intent, a TUI argument).
//!
//! [`TabStacks`] and [`TabHost`] are the same pair one level up: one stack *per tab* rather than one shared stack, which is the native model (`UITabBarController`, a nested `Navigator`, a nested nav graph) and what lets a tab you leave stay several screens deep until you come back to it.

#![warn(rustdoc::broken_intra_doc_links)]

mod host;
mod navigator;
mod page;
mod tabs;
mod transition;

pub use host::NavHost;
pub use navigator::Navigator;
pub use page::{NavPage, PagePolicy, SimplePage};
pub use platform_core::Location;
pub use platform_core::Route;
pub use tabs::{TabHost, TabStacks};
pub use transition::NavTransition;
