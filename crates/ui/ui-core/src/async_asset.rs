use std::sync::Arc;

use reactive_core::ReadSignal;
use renderer_assets::SvgData;

/// The lifecycle of an asynchronously-resolved asset. Kept inside a signal so a widget re-renders as the state advances from `Loading` to `Ready`/`Failed`.
#[derive(Clone, Debug)]
pub enum AssetState<T> {
    Loading,
    Ready(T),
    Failed,
}

impl<T> AssetState<T> {
    pub fn is_ready(&self) -> bool {
        matches!(self, AssetState::Ready(_))
    }

    pub fn as_ready(&self) -> Option<&T> {
        match self {
            AssetState::Ready(value) => Some(value),
            _ => None,
        }
    }

    pub fn into_ready(self) -> Option<T> {
        match self {
            AssetState::Ready(value) => Some(value),
            _ => None,
        }
    }
}

/// A transport-agnostic source of SVG assets addressed by string id. rsx owns the reactive contract — a signal that advances `Loading` → `Ready`/`Failed` and re-renders whoever read it — while an implementor supplies the bytes from disk, a bundle, or the network entirely outside this crate.
pub trait AssetSource {
    /// A reactive handle to the SVG named `id`. Reading it subscribes the caller and may start the load; the state advances as the asset resolves.
    fn svg(&self, id: &str) -> ReadSignal<AssetState<Arc<SvgData>>>;

    /// The resolved SVG, or `fallback()` while it is still loading or has failed. Reads reactively, so a widget calling this re-renders once the asset lands.
    fn svg_or(&self, id: &str, fallback: impl FnOnce() -> Arc<SvgData>) -> Arc<SvgData> {
        match self.svg(id).get() {
            AssetState::Ready(data) => data,
            _ => fallback(),
        }
    }
}
