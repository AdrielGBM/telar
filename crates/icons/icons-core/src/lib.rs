//! Iconify icon sets for Telar: the `set:name` id, the Iconify JSON format, the [`IconSource`] seam an id resolves through, and the licence policy over what was resolved.
//!
//! Icons are a format here, not a service. An id is resolved against what the application points at — a directory of Iconify JSON sets ([`IconifyDir`]), a folder of its own SVGs ([`SvgDir`]), or an Iconify-compatible HTTP provider it names (`HttpProvider`, behind `http`) — and nothing reaches `api.iconify.design` unless that is the provider configured.
//!
//! Two consumers share it, which is why it is a crate of its own: `telar-baker` resolves the literal ids an application's `.rsx` names and bakes them into its artifact, and the `telar-icons` plugin draws them and, in runtime mode, resolves ids while the application runs. A source written once serves both.
//!
//! **Applications depend on `telar-icons`, not on this crate.** It re-exports everything here an application names.

#![warn(rustdoc::broken_intra_doc_links)]

mod error;
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
mod http;
mod iconify_dir;
mod id;
mod license;
mod set;
mod source;
mod svg_dir;

pub use error::IconError;
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
pub use http::HttpProvider;
pub use iconify_dir::IconifyDir;
pub use id::IconId;
pub use license::{
    LicenseClass, LicensePolicy, NoticeSet, OnUnlisted, Verdict, is_brand_set, notice,
};
pub use set::{Author, Icon, IconSet, License, SetInfo};
pub use source::{IconSource, SourcedIcon, Sources};
pub use svg_dir::SvgDir;
