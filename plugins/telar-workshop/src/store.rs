//! What the workshop remembers between runs: the selection, the pane sizes and the canvas toolbar's settings, each under a `workshop.*` key of the preference store.
//!
//! On the desktop the workshop installs a store of its own, which keeps those keys in `<workspace>/.telar/workshop/state` and every other key in memory: a preview's own preferences last the run and no longer. Edited args are never stored; they live in memory, and in a copied link.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;
use telar::{
    FileStore, MemoryStore, PreferenceStore, RwSignal, Timer, effect, hot_signal, run_after,
};

/// The keys the workshop's state is stored under. Renaming one forgets what it held.
pub(crate) mod keys {
    pub(crate) const SELECTION: &str = "workshop.selection";
    pub(crate) const VIEW: &str = "workshop.view";
    pub(crate) const SIDEBAR_WIDTH: &str = "workshop.sidebar.width";
    pub(crate) const SIDEBAR_COLLAPSED: &str = "workshop.sidebar.collapsed";
    pub(crate) const PANEL_SIZE: &str = "workshop.panel.size";
    pub(crate) const PANEL_COLLAPSED: &str = "workshop.panel.collapsed";
    pub(crate) const PANEL_POSITION: &str = "workshop.panel.position";
    pub(crate) const CANVAS_SETTINGS: &str = "workshop.canvas.settings";
}

const PREFIX: &str = "workshop.";

/// How long the state rests before a change to it is written.
pub(crate) const WRITE_DELAY: Duration = Duration::from_millis(500);

/// The store a desktop workshop installs: `workshop.*` keys in a file, every other key in memory.
pub(crate) struct WorkshopStore {
    file: FileStore,
    memory: MemoryStore,
}

impl WorkshopStore {
    /// The store kept in `file`, with what it already holds.
    pub(crate) fn open(file: impl Into<PathBuf>) -> Self {
        Self {
            file: FileStore::open(file),
            memory: MemoryStore::default(),
        }
    }

    fn holding(&self, key: &str) -> &dyn PreferenceStore {
        match key.starts_with(PREFIX) {
            true => &self.file,
            false => &self.memory,
        }
    }
}

impl PreferenceStore for WorkshopStore {
    fn get(&self, key: &str) -> Option<String> {
        self.holding(key).get(key)
    }

    fn set(&self, key: &str, value: Option<&str>) {
        self.holding(key).set(key, value);
    }
}

/// Where the workshop of the workspace at `workspace` keeps its state.
pub(crate) fn state_file(workspace: &Path) -> PathBuf {
    workspace.join(".telar").join("workshop").join("state")
}

/// Installs [`WorkshopStore`] for the workspace `cargo telar preview` runs from, and answers where the workshop keeps its state: there, or nowhere for a build started some other way, which keeps the store it has and remembers nothing between runs.
pub(crate) fn install() -> Keeper {
    match telar::preview::host::workspace_dir() {
        Some(workspace) => {
            let store: Arc<dyn PreferenceStore> =
                Arc::new(WorkshopStore::open(state_file(&workspace)));
            telar::set_preference_store(Arc::clone(&store));
            Keeper::in_store(store)
        }
        None => Keeper::none(),
    }
}

/// Where a workshop's state is read from as it opens and written to as it changes: the store [`install`] installed, or nothing.
///
/// A change is written once the state has rested for [`WRITE_DELAY`], so a splitter dragged across the window writes the file once rather than on every step; what is still waiting is written when the last handle on it goes, or on [`flush`](Self::flush).
#[derive(Clone)]
pub(crate) struct Keeper(Option<Rc<Kept>>);

struct Kept {
    store: Arc<dyn PreferenceStore>,
    waiting: RefCell<BTreeMap<&'static str, Option<String>>>,
    timer: RefCell<Option<Timer>>,
}

impl Keeper {
    /// Keeps nothing between runs.
    pub(crate) fn none() -> Self {
        Self(None)
    }

    /// Keeps the state in `store`.
    pub(crate) fn in_store(store: Arc<dyn PreferenceStore>) -> Self {
        Self(Some(Rc::new(Kept {
            store,
            waiting: RefCell::default(),
            timer: RefCell::default(),
        })))
    }

    /// A `@workshop/…` hot signal that is also kept under `key`: a hot reload restores it first, then what was kept, then `default`, and every change is kept again.
    pub(crate) fn kept<T>(&self, hot_key: &str, key: &'static str, default: T) -> RwSignal<T>
    where
        T: Clone + Serialize + DeserializeOwned + 'static,
    {
        let signal = hot_signal(hot_key, self.stored(key).unwrap_or(default));
        self.keep(signal, key);
        signal
    }

    /// What is kept under `key`, when it reads as a `T`, counting what is still waiting to be written.
    pub(crate) fn stored<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let kept = self.0.as_ref()?;
        let text = match kept.waiting.borrow().get(key) {
            Some(waiting) => waiting.clone(),
            None => kept.store.get(key),
        }?;
        serde_json::from_str(&text).ok()
    }

    /// Keeps `signal` under `key` now and whenever it changes, for as long as the owner this is called under lives.
    pub(crate) fn keep<T: Serialize + 'static>(&self, signal: RwSignal<T>, key: &'static str) {
        let Some(kept) = self.0.clone() else {
            return;
        };
        effect(move || {
            let text = signal.with(|value| serde_json::to_string(value).ok());
            kept.wait(key, text);
        });
    }

    /// Writes what is waiting now.
    pub(crate) fn flush(&self) {
        if let Some(kept) = &self.0 {
            kept.flush();
        }
    }
}

impl Kept {
    fn wait(self: &Rc<Self>, key: &'static str, text: Option<String>) {
        self.waiting.borrow_mut().insert(key, text);
        let kept = Rc::downgrade(self);
        let timer = run_after(WRITE_DELAY, move || {
            if let Some(kept) = kept.upgrade() {
                kept.flush();
            }
        });
        self.timer.replace(Some(timer));
    }

    fn flush(&self) {
        self.timer.take();
        let waiting = std::mem::take(&mut *self.waiting.borrow_mut());
        for (key, text) in waiting {
            self.store.set(key, text.as_deref());
        }
    }
}

impl Drop for Kept {
    fn drop(&mut self) {
        self.flush();
    }
}

#[cfg(test)]
#[path = "store_test.rs"]
mod tests;
