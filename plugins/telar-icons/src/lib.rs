//! Iconify icons for Telar: an `icon` tag that draws `set:name` ids, baked into the application at build time by default and resolved as it runs only when it asks.
//!
//! Icons are a format here, not a service. An id is an [Iconify](https://iconify.design) name — `mdi:home`, `lucide:settings`, `app:logo` — and it resolves against what the application points at: Iconify JSON sets on disk, a folder of its own SVGs, or an Iconify-compatible provider it names. Nothing reaches `api.iconify.design` unless that is the provider configured.
//!
//! ```toml
//! # Cargo.toml
//! telar-icons = "0.2.1"
//!
//! # telar.toml
//! [telar]
//! prelude = ["telar_icons"]
//!
//! [telar.icons]
//! iconify = "node_modules"   # `npm i -D @iconify-json/mdi`, or `@iconify/json` for every set
//! svg = "icons"              # icons/app/logo.svg is `app:logo`
//! ```
//!
//! ```rsx
//! [view]
//! row gap:8 align:center
//!     icon name:"mdi:home"
//!     text "Home"
//! icon name:"mdi:alert" size:24 color:$theme.error label:"Unsaved changes"
//! ```
//!
//! # The tag
//!
//! [`icon`] draws one icon in a square box of `size` px, the theme's `icon_size` when unset. A monochrome icon takes the colour of the text around it from the cascade, so it follows a `color:` on its row, a theme switch and a hover state like the text beside it; `color:` sets one instead. Which icons are monochrome is Iconify's answer, decided when the icon is resolved: every icon of a set whose `info` declares `palette: false`, none of a set that declares `palette: true` — so a brand logo drawn in one colour keeps it — and, for a set that declares neither, such as a folder of your own SVGs, an icon that paints in `currentColor`. Draw your own monochrome artwork in `currentColor`. A palette icon keeps its own colours unless `color:` is given, which draws it as a silhouette: the renderer tints flat, every paint of the artwork in the one colour, so it never maps only the `currentColor` parts. It is decoration to a screen reader, skipped, unless `label:` names it, which makes it a picture by that name: label the icon that is the only thing saying something, such as a button with no text.
//!
//! # Baked by default
//!
//! `cargo telar bake`, which every `cargo telar` build command runs first, finds every `icon name:"…"` the package's `.rsx` writes literally, resolves those ids and nothing else through the sources `[telar.icons]` names, and bakes them into the package's artifact as `SvgData`, the way it bakes a `svg src:"…"`. The application carries the icons it draws, needs no network and no icon set at run time, and a set of seven thousand icons costs the three it uses. An id the bake cannot resolve fails the bake, naming the `.rsx` line and every source it looked in.
//!
//! An id names its set, `mdi:home`, unless `[telar.icons] default_set` names the set a bare name is read in: with `default_set = "mdi"`, `icon name:"home"` is `mdi:home`, baked and recorded under that id. A bare name with no default set fails the bake and the build on its `.rsx` line. In `mode = "runtime"` nothing is baked, but a literal bare name is still read in the default set at build time, so it reaches the runtime spelled in full.
//!
//! Only a literal can be baked. A `name:` given a signal or an expression is an error while `[telar.icons]` bakes every id, pointing at the two ways out: choose between literal ids with `if` or `match`, or resolve the rest as the application runs.
//!
//! # Sources
//!
//! Every source is an [`IconSource`], the seam the bake and the runtime both resolve through; [`Sources`] asks several in order. `[telar.icons]` asks its three in this order, so an application redraws one icon of a set by dropping its own SVG under the set's name:
//!
//! - `svg = "icons"`: an [`SvgDir`], one file per icon, `icons/<set>/<name>.svg`. A set there is the application's own artwork unless it carries an `info.json` saying otherwise.
//! - `iconify = "node_modules"`: an [`IconifyDir`] of Iconify JSON sets, as `@iconify/json`, an `@iconify-json/<set>` package or a downloaded `<set>.json` lays them out.
//! - `provider = "https://icons.example.com"`: an Iconify-compatible API the bake fetches from, one request per set. Self-host one rather than lean on the public instance; a fetched icon is kept in `.telar/icons/` so a rebake does not ask again.
//!
//! # Configuration
//!
//! ```toml
//! [telar.icons]
//! mode = "baked"        # "baked" (default), "both" (literals baked, the rest at run time) or "runtime"
//! iconify = "node_modules"
//! svg = "icons"
//! provider = "https://icons.example.com"
//! default_set = "mdi"    # the set a bare name, `home`, is read in
//!
//! [telar.icons.licenses]
//! allow = ["CC-BY-4.0"] # SPDX ids, or set prefixes, accepted beyond the permissive ones
//! unlisted = "warn"     # or "fail"
//! ```
//!
//! # Licences
//!
//! Each Iconify set declares its licence. The bake records the set and licence of every icon it baked in `.telar/icons.json`, and writes the notice an application ships to `.telar/ICONS-LICENSES.txt`; every `cargo telar build` puts it where its format keeps third-party notices, from the root of a site to `/usr/share/doc/<name>/` in a `.deb`. The same text is compiled into the application, which reads it with `telar_icons::licenses()` to show it on a screen of its own, the way an Android, iOS or web build must since none can carry the file beside it. The notice also lists the icons of every `[telar] library` the application is built with, read from the `.telar/icons-library.json` each ships, and of every other local crate the workspace bakes, judged against the application's own policy. Public-domain and permissive sets (CC0, MIT, Apache-2.0, ISC, the BSDs) are accepted, since what they ask is that notice. An attribution (CC-BY), copyleft (GPL, MPL, OFL, CC-BY-SA), non-commercial or undeclared licence needs a decision: the bake warns about it, or fails with `unlisted = "fail"`, until `allow` names its SPDX id or the set's prefix. A set of brand logos, such as `simple-icons`, carries a trademark note in the notice: its licence covers the drawings, not the marks.
//!
//! # Runtime mode
//!
//! With the `runtime` feature, an id the bake did not answer for — every id in `mode = "runtime"`, the computed ones in `mode = "both"` — resolves as the application runs, through the `RuntimeIcons` it installs from `telar::app!`'s setup block: an Iconify-compatible provider over `telar-dynamic`'s HTTP transport, or any [`IconSource`]. The box is drawn empty at its full size until the icon lands. Weigh it before reaching for it:
//!
//! - **Availability.** An icon is only as available as the provider: one that is down, slow or gone leaves its box empty. The transport retries a transient failure; nothing replaces a provider that is not there.
//! - **Privacy.** Every request tells the provider which icon the application is drawing, from the user's address, which is a record of what they look at. Self-host the provider, and say so in the application's privacy notice when it is somebody else's.
//! - **Offline.** Without a network an icon not already cached draws nothing. A `DiskCache` keeps what arrived across restarts, so an icon seen once draws offline; baking is what makes every icon draw offline.
//! - **Web.** A browser build fetches from the page, so the site's Content Security Policy must allow the provider in `connect-src` (`connect-src 'self' https://icons.example.com`), and the provider must answer with CORS headers that let the site's origin read the response.
//!
//! An application that names its icons in a config file, as a desktop shell's panel does, is the case runtime mode is for: `mode = "both"` bakes every icon the markup writes and leaves the configured ones to `RuntimeIcons::provider`, with a `DiskCache` under the user's cache directory — the provider, the `set:name` ids, the retrying fetch, the disk cache and the tinted glyph a hand-rolled icon view assembles itself, behind one tag. Its bare names are read in the set given to `RuntimeIcons::with_default_set`, which should be the one `[telar.icons] default_set` names: the transpiler reads the bare names the `.rsx` writes literally in that set at build time, but `telar.toml` is not there to read as the application runs on every target, so an id that arrives at run time is read in the set handed to the runtime.
//!
//! **Keep this crate on the same version as `telar`.** The `SvgData` an icon is drawn from is the facade's own, so a mismatch resolves two copies of the kernel and a baked icon stops being the type the tag accepts — a type error naming one struct twice. It is the same lockstep `telar` and `telar-macros` already have, and for the same reason.

#![warn(rustdoc::broken_intra_doc_links)]

mod icon;
mod licenses;
mod name;
mod runtime;

pub use icon::{IconProps, icon};
pub use icons_core::{
    Author, Icon, IconError, IconId, IconSet, IconSource, IconifyDir, License, SetInfo,
    SourcedIcon, Sources, SvgDir,
};
pub use licenses::licenses;
pub use name::IconName;
#[cfg(feature = "runtime")]
pub use runtime::RuntimeIcons;
#[cfg(all(feature = "runtime", not(target_arch = "wasm32")))]
pub use telar_dynamic::DiskCache;
#[cfg(feature = "runtime")]
pub use telar_dynamic::MemoryCache;
