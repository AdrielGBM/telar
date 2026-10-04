//! Hot-reload state preservation: a dylib-local registry of serializable signals. The host asks the outgoing dylib for a JSON snapshot (via `_rsx_hot_snapshot`), hands it to the incoming dylib (via `_rsx_hot_restore`), and `hot_signal` consumes the restored values as components remount. A prerendered page carries the same snapshot, and the browser that takes it over restores it the same way.

use std::cell::RefCell;
use std::collections::HashMap;
use std::mem::ManuallyDrop;

use reactive_core::RwSignal;
use serde::Serialize;
use serde::de::DeserializeOwned;

// Readable after the component unmounts because the signal is created detached, not because this closure holds anything: a `Copy` handle pins nothing.
#[cfg(any(feature = "dev", feature = "prerender"))]
type HotReader = Box<dyn Fn() -> Option<String>>;

// `ManuallyDrop` keeps these slots trivially destructible: a TLS destructor registered from the dylib would make `dlclose` unsafe. The maps leak per reload, which is fine on a dev-only path.
thread_local! {
    #[cfg(any(feature = "dev", feature = "prerender"))]
    static REGISTRY: ManuallyDrop<RefCell<HashMap<String, HotReader>>> =
        ManuallyDrop::new(RefCell::new(HashMap::new()));
    // Carried over from the previous dylib or the prerendered page being taken over, consumed by `hot_signal` on mount.
    static PENDING: ManuallyDrop<RefCell<HashMap<String, String>>> =
        ManuallyDrop::new(RefCell::new(HashMap::new()));
}

/// As [`reactive_core::signal`], but keyed: the value is captured in the hot-reload snapshot and restored across dylib swaps in `cargo telar dev`. If two live instances share a key, the last one mounted wins the snapshot and both restore to the same value.
pub fn hot_signal<T>(key: &str, init: T) -> RwSignal<T>
where
    T: Clone + Serialize + DeserializeOwned + 'static,
{
    let restored = PENDING
        .with(|p| p.borrow_mut().remove(key))
        .and_then(|raw| serde_json::from_str::<T>(&raw).ok());
    // Detached, so the signal outlives the component that made it. The refcount used to do that, but a `Copy` handle pins nothing, so a snapshot taken after unmount read a freed signal.
    let sig = reactive_core::detached(|| reactive_core::signal(restored.unwrap_or(init)));
    // A page being taken over only reads values back; snapshotting is for the two builds that write them.
    #[cfg(any(feature = "dev", feature = "prerender"))]
    REGISTRY.with(|r| {
        r.borrow_mut().insert(
            key.to_string(),
            Box::new(move || serde_json::to_string(&sig.peek()).ok()),
        );
    });
    sig
}

// Unlike a hot signal, which is consumed passively when its component remounts, this lives in another crate's thread-local and is set during `setup` — which the incoming dylib re-runs before restore — so it must be captured and actively pushed back. Keys are `@telar/`-namespaced to avoid colliding with a user's `[logic]` signal key.
const THEME_MODE_KEY: &str = "@telar/theme.mode";
const SCHEME_PREFERENCE_KEY: &str = "@telar/theme.scheme";
const REDUCED_MOTION_OVERRIDE_KEY: &str = "@telar/motion.reduced";

/// Serializes every registered hot signal into a JSON map. Runs inside the outgoing dylib via its `_rsx_hot_snapshot` export, while the old tree (and thus its signals) is still alive.
#[cfg(any(feature = "dev", feature = "prerender"))]
pub fn hot_snapshot_json() -> String {
    let mut map: HashMap<String, String> = REGISTRY.with(|r| {
        r.borrow()
            .iter()
            .filter_map(|(key, read)| read().map(|value| (key.clone(), value)))
            .collect()
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
#[macro_export]
macro_rules! hot_signal_auto {
    ($key:expr, $init:expr) => {{
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
