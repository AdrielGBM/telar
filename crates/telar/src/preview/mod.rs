//! Previews: components rendered in isolation, outside the application that uses them.
//!
//! A preview is written in Rust with [`preview!`], which returns a [`Preview`] to go on configuring with its setters, or generated from a `[preview]` block in `.rsx`. Its body reads its inputs from the [`PreviewCtx`] it is handed: [`PreviewCtx::arg`] for a value handed to a prop at build, [`PreviewCtx::signal`] for one the tree reads as it runs. Each arg's type decides its control through [`PreviewArg`], refined by the prop of the same name.
//!
//! Everything here exists only under the `previews` feature, and items a crate declares for previews are gated with [`crate::__previews!`]. A crate lists every preview it carries from one fn at its root, `telar_all_previews`, which is how an application reaches a library's previews by name, through `[telar.previews] include` in its `telar.toml`.
//!
//! `app!` and `rsx_modules!` define one that lists the crate's `.rsx` previews. A crate that also writes previews in Rust defines its own beside the macro call, which takes that one's place, and lists the `.rsx` ones in it through `telar_rsx_previews`:
//!
//! ```ignore
//! telar::__previews! {
//!     mod previews;
//!
//!     pub fn telar_all_previews() -> Vec<telar::preview::Preview> {
//!         [telar_rsx_previews(), previews::previews()].concat()
//!     }
//! }
//! ```
//!
//! A crate with no `.rsx` and no macro call lists its Rust previews alone.
//!
//! A preview's play, [`PreviewEntry::play`], drives it as a person would through a [`Play`], and [`a11y::check`] reports what would keep someone from using it.
//!
//! Under `preview-snapshots`, the `snapshot` module compares each preview and each cell of its matrix against the picture and the draw text kept for it.
//!
//! What mounts previews and gives them controls is in [`host`].

#[doc(hidden)]
#[path = "probe.rs"]
pub mod __probe;
pub mod a11y;
mod actions;
mod arg;
mod args;
mod authoring;
mod control;
mod entry;
mod frame;
mod globals;
pub mod host;
mod matrix;
mod mount;
mod play;
mod props;
mod query;
#[cfg(feature = "preview-snapshots")]
pub mod snapshot;
mod value;

#[cfg(not(target_os = "android"))]
mod app;
#[cfg(not(target_os = "android"))]
mod runner;

pub use actions::{ActionCall, ActionLog, IntoAction, PreviewActions};
pub use arg::PreviewArg;
pub use args::{ArgBinding, ArgSpec};
pub use authoring::IntoPreviewRoot;
pub use control::{ControlKind, NumberControl};
pub use entry::{
    BuildFn, Decorator, Layout, PreviewCtx, PreviewEntry, PreviewEnv, PreviewSurface, SourceSpan,
};
pub use frame::{Frame, FrameAccess};
pub use globals::Globals;
pub use matrix::{
    Axis, CellGlobals, MATRIX_CELL_CAP, Matrices, Matrix, MatrixCell, MatrixError, NamedMatrix,
    ViewportPreset, control_size_word, direction_word, parse_control_size, parse_direction,
    parse_size,
};
pub use play::{Play, PlayError, PlayFn, PlayResult, PlayStep};
pub use props::{HasPropsSchema, PropDefault, PropField, PropsSchema};
pub use query::{Query, by_role, by_text};
pub use telar_macros::PreviewArg;
pub use value::{ArgValue, ArgValueParseError};

#[doc(inline)]
pub use crate::__preview_entry as preview;
#[doc(hidden)]
pub use matrix::{__install_matrices, __install_matrices_if_unset};
#[doc(hidden)]
pub use telar_macros::{__preview_file_of, __preview_names, __preview_source};

/// A preview, as [`preview!`] returns it and its setters configure it: another name for [`PreviewEntry`], so `vec![preview!(…).title(…), …]` is already the `Vec<Preview>` a crate lists.
pub type Preview = PreviewEntry;
