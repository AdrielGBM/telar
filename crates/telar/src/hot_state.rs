//! Hot-reload state preservation: a dylib-local registry of serializable signals. The host asks the outgoing dylib for a JSON snapshot (via `_rsx_hot_snapshot`), hands it to the incoming dylib (via `_rsx_hot_restore`), and `hot_signal` consumes the restored values as components remount. A prerendered page carries the same snapshot, and the browser that takes it over restores it the same way.

use std::collections::HashMap;

use reactive_core::RwSignal;
use serde::Serialize;
use serde::de::DeserializeOwned;

#[cfg(any(feature = "dev", feature = "prerender"))]
type HotReader = Box<dyn Fn() -> Option<String>>;

#[cfg(any(feature = "dev", feature = "prerender"))]
#[derive(Default)]
struct Registry {
    next_instance: u64,
    live: HashMap<String, (u64, HotReader)>,
    kept: HashMap<String, (u64, String)>,
}

// The maps leak per reload, which is fine on a dev-only path.
reactive_core::dropless_thread_local! {
    #[cfg(any(feature = "dev", feature = "prerender"))]
    static REGISTRY: released Registry = Registry::default();
    // Carried over from the previous dylib or the prerendered page being taken over, consumed by `hot_signal` on mount.
    static PENDING: released HashMap<String, String> = HashMap::new();
}

/// As [`reactive_core::signal`], but keyed: the value is captured in the hot-reload snapshot and restored across dylib swaps in `cargo telar dev`. If two instances share a key, the last one mounted wins the snapshot and both restore to the same value. The signal belongs to the component that made it and is freed with it; its last value is kept for the snapshot when that owner is disposed.
pub fn hot_signal<T>(key: &str, init: T) -> RwSignal<T>
where
    T: Clone + Serialize + DeserializeOwned + 'static,
{
    let restored = PENDING
        .with(|p| p.borrow_mut().remove(key))
        .and_then(|raw| serde_json::from_str::<T>(&raw).ok());
    let sig = reactive_core::signal(restored.unwrap_or(init));
    // A page being taken over only reads values back; snapshotting is for the two builds that write them.
    #[cfg(any(feature = "dev", feature = "prerender"))]
    register(key, sig);
    sig
}

#[cfg(any(feature = "dev", feature = "prerender"))]
fn register<T: Serialize + Clone + 'static>(key: &str, sig: RwSignal<T>) {
    let instance = REGISTRY.with(|r| {
        let mut registry = r.borrow_mut();
        registry.next_instance += 1;
        let instance = registry.next_instance;
        registry.live.insert(
            key.to_string(),
            (
                instance,
                Box::new(move || sig.try_get().and_then(|v| serde_json::to_string(&v).ok())),
            ),
        );
        instance
    });
    let key = key.to_string();
    reactive_core::on_cleanup(move || {
        let Some(value) = sig.try_get().and_then(|v| serde_json::to_string(&v).ok()) else {
            return;
        };
        REGISTRY.with(|r| {
            let mut registry = r.borrow_mut();
            if registry
                .live
                .get(&key)
                .is_some_and(|(id, _)| *id == instance)
            {
                registry.live.remove(&key);
            }
            if registry.kept.get(&key).is_none_or(|(id, _)| *id < instance) {
                registry.kept.insert(key, (instance, value));
            }
        });
    });
}

// Unlike a hot signal, which is consumed passively when its component remounts, this lives in another crate's thread-local and is set during `setup` — which the incoming dylib re-runs before restore — so it must be captured and actively pushed back. Keys are `@telar/`-namespaced to avoid colliding with a user's `[logic]` signal key.
const THEME_MODE_KEY: &str = "@telar/theme.mode";
const SCHEME_PREFERENCE_KEY: &str = "@telar/theme.scheme";
const REDUCED_MOTION_OVERRIDE_KEY: &str = "@telar/motion.reduced";
const HIGH_CONTRAST_OVERRIDE_KEY: &str = "@telar/contrast.high";

/// Serializes every registered hot signal into a JSON map. Runs inside the outgoing dylib via its `_rsx_hot_snapshot` export, while the old tree (and thus its signals) is still alive.
#[cfg(any(feature = "dev", feature = "prerender"))]
pub fn hot_snapshot_json() -> String {
    let mut map: HashMap<String, String> = REGISTRY.with(|r| {
        let registry = r.borrow();
        let mut map: HashMap<String, String> = registry
            .kept
            .iter()
            .map(|(key, (_, value))| (key.clone(), value.clone()))
            .collect();
        map.extend(
            registry
                .live
                .iter()
                .filter_map(|(key, (_, read))| read().map(|value| (key.clone(), value))),
        );
        map
    });
    if let Some(mode) = theme_core::active_mode() {
        map.insert(THEME_MODE_KEY.to_string(), mode);
    }
    map.insert(
        SCHEME_PREFERENCE_KEY.to_string(),
        theme_core::scheme_preference().as_str().to_string(),
    );
    // Carried only while set: absent means the outgoing library followed the system, which an incoming one already does.
    if let Some(reduced) = preferences_core::reduced_motion_override() {
        map.insert(REDUCED_MOTION_OVERRIDE_KEY.to_string(), reduced.to_string());
    }
    if let Some(high) = preferences_core::high_contrast_override() {
        map.insert(HIGH_CONTRAST_OVERRIDE_KEY.to_string(), high.to_string());
    }
    serde_json::to_string(&map).unwrap_or_default()
}

/// Loads a snapshot produced by the previous dylib. Runs inside the incoming dylib via its `_rsx_hot_restore` export, before the new tree mounts.
#[cfg(any(feature = "dev", feature = "prerender"))]
pub fn hot_restore_json(blob: &str) {
    if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(blob) {
        restore(map);
    }
}

/// Loads snapshotted values by key, before the tree that reads them mounts: the previous dylib's, or the ones a prerendered page was written with.
pub(crate) fn restore(map: impl IntoIterator<Item = (String, String)>) {
    let mut map: HashMap<String, String> = map.into_iter().collect();
    // Overrides the default mode this dylib's `setup` just selected, so the user's last selection survives the swap. Removed from the map so it never lingers in `PENDING`.
    if let Some(preference) = map
        .remove(SCHEME_PREFERENCE_KEY)
        .and_then(|word| theme_core::SchemePreference::parse(&word))
    {
        theme_core::set_scheme_preference(preference);
    }
    if let Some(reduced) = map
        .remove(REDUCED_MOTION_OVERRIDE_KEY)
        .and_then(|word| word.parse().ok())
    {
        preferences_core::set_reduced_motion_override(Some(reduced));
    }
    if let Some(high) = map
        .remove(HIGH_CONTRAST_OVERRIDE_KEY)
        .and_then(|word| word.parse().ok())
    {
        preferences_core::set_high_contrast_override(Some(high));
    }
    if let Some(mode) = map.remove(THEME_MODE_KEY) {
        theme_core::set_mode(mode);
    }
    PENDING.with(|p| p.borrow_mut().extend(map));
}

// Autoref specialization, so generated code can key every `signal()` without knowing whether `T` is serde-able: serializable types get `hot_signal`, everything else a plain signal.
#[doc(hidden)]
pub mod probe {
    use super::*;

    pub struct Probe<'a, T>(pub &'a T);
    pub struct SerdeTag;
    pub struct PlainTag;

    pub trait SignalValue {
        type Value;
    }
    impl<T> SignalValue for RwSignal<T> {
        type Value = T;
    }

    pub trait SerdeKind {
        fn kind(&self) -> SerdeTag;
    }
    // Receiver `&Probe` matches this impl directly, so it wins over the autoref fallback whenever the bounds hold.
    impl<'a, T: Clone + Serialize + DeserializeOwned + 'static> SerdeKind for Probe<'a, T> {
        fn kind(&self) -> SerdeTag {
            SerdeTag
        }
    }

    pub trait PlainKind {
        fn kind(&self) -> PlainTag;
    }
    impl<'a, T> PlainKind for &Probe<'a, T> {
        fn kind(&self) -> PlainTag {
            PlainTag
        }
    }

    impl SerdeTag {
        pub fn make<T: Clone + Serialize + DeserializeOwned + 'static>(
            self,
            key: &str,
            init: T,
        ) -> RwSignal<T> {
            hot_signal(key, init)
        }
    }
    impl PlainTag {
        pub fn make<T: 'static>(self, key: &str, init: T) -> RwSignal<T> {
            let _ = key;
            reactive_core::signal(init)
        }
    }
}

/// Creates a hot-preserved signal when the value type is serde-able, else a plain signal. Emitted by the rsx transpiler for `[logic]` signal bindings in hot-reload builds.
///
/// `type RwSignal<T>,` before the value fixes `T` before the probe looks at it; without it a value whose type is only settled later (`None` for an `Option<NotSerde>`) is probed as `Option<_>`, which picks serde and then fails its bounds.
#[macro_export]
macro_rules! hot_signal_auto {
    ($key:expr, type $signal:ty, $init:expr $(,)?) => {{
        #[allow(unused_imports)]
        use $crate::probe::{PlainKind as _, SerdeKind as _};
        let __rsx_hot_init: <$signal as $crate::probe::SignalValue>::Value = $init;
        (&$crate::probe::Probe(&__rsx_hot_init))
            .kind()
            .make($key, __rsx_hot_init)
    }};
    ($key:expr, $init:expr $(,)?) => {{
        #[allow(unused_imports)]
        use $crate::probe::{PlainKind as _, SerdeKind as _};
        let __rsx_hot_init = $init;
        (&$crate::probe::Probe(&__rsx_hot_init))
            .kind()
            .make($key, __rsx_hot_init)
    }};
}

#[cfg(test)]
#[path = "hot_state_test.rs"]
mod tests;
