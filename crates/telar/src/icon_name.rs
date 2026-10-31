//! [`IconName`]: the one type every prop naming an icon takes, whichever crate draws it, and [`icon!`](crate::icon!), which names a baked one from Rust.

use std::sync::Arc;

use reactive_core::{Memo, Reactive, ReadSignal, RwSignal};
use renderer_assets::SvgData;

/// An icon, as a prop that names one is given it: an id baked ahead of time, or one left to resolve as the application runs.
///
/// It is the same type in every build, so a prop's type never depends on which crate draws it: `telar-icons` draws the artwork, and a widget built without it can still read the id, such as to draw a stand-in glyph.
///
/// Written in `.rsx` as `name:"mdi:home"` or `icon:"mdi:home"`. Where `[telar.icons]` bakes, the transpiler hands the prop the triple the artifact holds for that literal, `("mdi:home", Arc<SvgData>, monochrome)`, and it becomes [`Baked`](Self::Baked); a bare name written where `[telar.icons] default_set` is configured arrives already read in that set. Anywhere else — a signal, an expression, a package that bakes nothing — it is the id as written, [`Named`](Self::Named), for a runtime source to resolve.
#[derive(Clone)]
pub enum IconName {
    /// An icon the bake resolved ahead of time, so drawing it needs no icon set at run time.
    Baked {
        /// The Iconify id the bake resolved, such as `mdi:home`.
        id: &'static str,
        /// The artwork the bake stored for that id.
        svg: Arc<SvgData>,
        /// Whether the icon takes the colour around it, decided when it was baked from its set's `palette` and its markup.
        monochrome: bool,
    },
    /// An id a runtime source resolves as the application runs, because the markup gave it a signal or an expression.
    Named(Reactive<String>),
}

impl IconName {
    /// The id as it reads now: the baked one, or the current value of a named one, which subscribes the caller to it.
    pub fn id(&self) -> String {
        match self {
            Self::Baked { id, .. } => (*id).to_string(),
            Self::Named(id) => id.get(),
        }
    }
}

impl From<(&'static str, Arc<SvgData>, bool)> for IconName {
    fn from((id, svg, monochrome): (&'static str, Arc<SvgData>, bool)) -> Self {
        Self::Baked {
            id,
            svg,
            monochrome,
        }
    }
}

impl From<&'static str> for IconName {
    fn from(id: &'static str) -> Self {
        Self::Named(Reactive::Const(id.to_string()))
    }
}

impl From<String> for IconName {
    fn from(id: String) -> Self {
        Self::Named(Reactive::Const(id))
    }
}

impl From<Reactive<String>> for IconName {
    fn from(id: Reactive<String>) -> Self {
        Self::Named(id)
    }
}

impl From<RwSignal<String>> for IconName {
    fn from(id: RwSignal<String>) -> Self {
        Self::Named(id.into())
    }
}

impl From<ReadSignal<String>> for IconName {
    fn from(id: ReadSignal<String>) -> Self {
        Self::Named(id.into())
    }
}

impl From<Memo<String>> for IconName {
    fn from(id: Memo<String>) -> Self {
        Self::Named(id.into())
    }
}

/// An icon baked into the calling crate, named from Rust: `icon!("lucide:search")` is the [`IconName`] the bake resolved for that id.
///
/// `cargo telar bake` finds every `icon!("…")` in the crate's `.rs` files, and in the Rust its `.rsx` writes, and bakes those ids as it bakes an `icon name:"…"`, through the sources the crate's `[telar.icons]` names. So a widget library written in Rust ships the icons it names in its own artifact, credited in the notice of every application built with it, and a typo is a compile error on the literal rather than an empty box at run time. Call it as `icon!` or by a path ending in it, such as `telar::icon!` or `telar_icons::icon!`; under another name the bake does not see it.
///
/// ```ignore
/// let search = telar::icon!("lucide:search");
/// ```
#[macro_export]
macro_rules! icon {
    ($id:literal) => {
        $crate::IconName::from($crate::__baked_id!(icon, $id, $crate))
    };
}

#[cfg(test)]
#[path = "icon_name_test.rs"]
mod tests;
