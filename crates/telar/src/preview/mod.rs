//! Previews: components rendered in isolation, outside the application that uses them.
//!
//! A preview is written in Rust with [`preview!`], which returns a [`Preview`] to go on configuring with its setters, or generated from a `[preview]` block in `.rsx`. Its body reads its inputs from the [`PreviewCtx`] it is handed: [`PreviewCtx::arg`] for a value handed to a prop at build, [`PreviewCtx::signal`] for one the tree reads as it runs. Each arg's type decides its control through [`PreviewArg`], refined by the prop of the same name.
//!
//! Everything here exists only under the `previews` feature, and items a crate declares for previews are gated with [`crate::__previews!`]. A crate lists every preview it carries from one fn at its root, which is how an application reaches a library's previews by name:
//!
//! ```ignore
//! telar::__previews! {
//!     mod previews;
//!
//!     pub fn telar_all_previews() -> Vec<telar::preview::Preview> {
//!         previews::previews()
//!     }
//! }
//! ```
//!
//! What mounts previews and gives them controls is in [`host`].

#[doc(hidden)]
#[path = "probe.rs"]
pub mod __probe;
mod arg;
mod args;
mod authoring;
mod control;
mod entry;
pub mod host;
mod matrix;
mod mount;
mod play;
mod props;
mod value;

#[cfg(all(
    any(feature = "preview", feature = "preview-headless"),
    not(target_os = "android")
))]
mod app;
#[cfg(not(target_os = "android"))]
mod runner;

pub use arg::PreviewArg;
pub use args::{ArgBinding, ArgSpec};
pub use authoring::IntoPreviewRoot;
pub use control::{ControlKind, NumberControl};
pub use entry::{
    BuildFn, Decorator, Layout, PreviewCtx, PreviewEntry, PreviewEnv, PreviewSurface, SourceSpan,
};
pub use matrix::Matrix;
pub use play::{Play, PlayError, PlayFn, PlayResult};
pub use props::{HasPropsSchema, PropDefault, PropField, PropsSchema};
pub use telar_macros::PreviewArg;
pub use value::{ArgValue, ArgValueParseError};

#[doc(inline)]
pub use crate::__preview_entry as preview;
#[doc(hidden)]
pub use telar_macros::{__preview_file_of, __preview_names, __preview_source};

/// A preview, as [`preview!`] returns it and its setters configure it: another name for [`PreviewEntry`], so `vec![preview!(…).title(…), …]` is already the `Vec<Preview>` a crate lists.
pub type Preview = PreviewEntry;
