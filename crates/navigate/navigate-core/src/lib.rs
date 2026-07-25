//! Page-stack navigation for rsx: a reactive [`Navigator`] stack over an app-defined route type, and a
//! [`NavHost`] container that renders the top of that stack as a page — lazily built, cached, and swapped
//! with an optional [`NavTransition`].
//!
//! [`Navigator`] is the primitive: a `Vec<R>` behind a reactive signal, with push/pop history. [`NavHost`]
//! is the view: it owns one layout container, builds each page once from a factory, shows only the active
//! one (`set_display`), and reconciles navigation on each event — the same mechanism the runtime host uses
//! to switch tabs, generalized over routes. A page is any [`NavPage`] (a `LayoutItem` with `on_enter` /
//! `on_relayout` lifecycle hooks); [`SimplePage`] wraps a hook-less widget as one.
//!
//! [`TabStacks`] and [`TabHost`] are the same pair one level up: one stack *per tab* rather than one shared
//! stack, which is the native model (`UITabBarController`, a nested `Navigator`, a nested nav graph) and what
//! lets a tab you leave stay several screens deep until you come back to it.

//!
//! Routes are whatever type the app chooses — normally a small enum, which is what makes navigation
//! type-safe and exhaustively matched. [`Routable`] and [`RouteTable`] are an *optional* layer on top for
//! the places a route must survive as text (a deep link, a restored session); nothing in the runtime needs
//! them.

mod host;
mod navigator;
mod page;
mod route;
mod tabs;
mod transition;

pub use host::NavHost;
pub use navigator::Navigator;
pub use page::{NavPage, PagePolicy, SimplePage};
pub use route::{Routable, RouteTable};
pub use tabs::{TabHost, TabStacks};
pub use transition::NavTransition;
