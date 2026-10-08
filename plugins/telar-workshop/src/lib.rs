//! The component workshop `cargo telar preview` opens: every preview an application carries, listed in a sidebar, rendered one at a time in a canvas of its own, with panels to edit its args live.
//!
//! [`WorkshopApp`] is the whole of what an application sees. `telar::app!` builds it from the application's previews when the package declares `telar-workshop` as an optional dependency and the feature is on, which `cargo telar preview` does:
//!
//! ```ignore
//! #[cfg(feature = "telar-workshop")]
//! let app = telar_workshop::WorkshopApp::new(telar_all_preview_entries());
//! ```
//!
//! The workshop's chrome draws in the workbench theme from `telar-devtools`, scoped to the chrome, and never changes the application's own theme. What it remembers across a hot reload — the selection, the open groups, the pane sizes, the view, the search and each preview's args — is kept in `@workshop/…` hot signals.
//!
//! Its text is in English under the `telar_workshop` namespace, which an application translates or overrides from its own catalog with `telar_workshop.<key>`.
#![warn(rustdoc::broken_intra_doc_links)]

mod app;
mod canvas;
mod panels;
mod shell;
mod sidebar;
mod state;
mod strings;
mod top_bar;

pub use app::WorkshopApp;
