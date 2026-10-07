//! [`IconName`]: what the `name` prop of [`icon`](crate::icon) holds.

use std::sync::Arc;

use telar::{Memo, Reactive, ReadSignal, RwSignal, SvgData};

/// An icon, as the `icon` tag is given it: an id baked ahead of time, or one left to resolve as the application runs.
///
/// Written in `.rsx` as `name:"mdi:home"`. Where `[telar.icons]` bakes, the transpiler hands the prop the pair the artifact holds for that literal, `("mdi:home", Arc<SvgData>)`, and it becomes [`Baked`](Self::Baked). Anywhere else — a signal, an expression, a package that bakes nothing — it is the id as written, [`Named`](Self::Named), for the runtime source to resolve.
#[derive(Clone)]
pub enum IconName {
    Baked { id: &'static str, svg: Arc<SvgData> },
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

impl From<(&'static str, Arc<SvgData>)> for IconName {
    fn from((id, svg): (&'static str, Arc<SvgData>)) -> Self {
        Self::Baked { id, svg }
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
