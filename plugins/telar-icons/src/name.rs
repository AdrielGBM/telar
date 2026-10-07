//! [`IconName`]: what the `name` prop of [`icon`](crate::icon) holds.

use std::sync::Arc;

use telar::{Memo, Reactive, ReadSignal, RwSignal, SvgData};

/// An icon, as the `icon` tag is given it: an id baked ahead of time, or one left to resolve as the application runs.
///
/// Written in `.rsx` as `name:"mdi:home"`. Where `[telar.icons]` bakes, the transpiler hands the prop the triple the artifact holds for that literal, `("mdi:home", Arc<SvgData>, monochrome)`, and it becomes [`Baked`](Self::Baked); a bare name written where `[telar.icons] default_set` is configured arrives already read in that set. Anywhere else — a signal, an expression, a package that bakes nothing — it is the id as written, [`Named`](Self::Named), for the runtime source to resolve.
#[derive(Clone)]
pub enum IconName {
    Baked {
        id: &'static str,
        svg: Arc<SvgData>,
        /// Whether the icon takes the colour around it, decided when it was baked from its set's `palette` and its markup.
        monochrome: bool,
    },
    Named(Reactive<String>),
}

impl IconName {
    /// The id as it reads now: the baked one, or the current value of a named one.
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
