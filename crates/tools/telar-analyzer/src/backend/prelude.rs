//! The components `[telar] prelude` crates export, offered as tags.
//!
//! Only rust-analyzer knows what a crate exports through its globs and re-exports, and asking it takes longer than a keystroke can wait, so the answer is kept per package and worked out in the background: a completion that finds nothing kept answers without it, flagged incomplete so the editor asks again.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use telar_project::PreludeEntry;

use crate::analysis::completions::{PreludeComponent, component_items};

use super::Backend;

/// How long an answer that left a prelude entry unanswered is kept before rust-analyzer is asked again. It is still loading the workspace, or the entry names a crate the package cannot reach, and neither is worth a probe per keystroke.
const RETRY_UNSETTLED: Duration = Duration::from_secs(10);

/// What the components were worked out against. A package whose prelude or lockfile changes asks again.
#[derive(Clone, PartialEq, Eq)]
struct Key {
    prelude: Vec<PreludeEntry>,
    lockfile: Option<SystemTime>,
}

struct Known {
    key: Key,
    components: Vec<PreludeComponent>,
    /// Whether every entry was answered, so this is the whole list rather than what was reachable so far.
    settled: bool,
    at: Instant,
}

/// The prelude components each package's tags can name, keyed by package root.
#[derive(Default)]
pub(crate) struct PreludeCache {
    known: HashMap<PathBuf, Known>,
    probing: HashSet<PathBuf>,
    /// Bumped by [`Self::clear`], so a probe that started before a change cannot store an answer from before it.
    generation: u64,
}

impl PreludeCache {
    /// Forgets every answer. A crate's exports change with its source and its manifest, and a prelude crate can be any crate the workspace reaches, so any such change is reason enough.
    pub(crate) fn clear(&mut self) {
        self.known.clear();
        self.generation += 1;
    }
}

impl Backend {
    /// The prelude components the package around `rsx_path` exports, and whether that list may still grow. Never waits on rust-analyzer: a package with nothing kept starts a probe in the background and answers with what it has.
    pub(crate) fn prelude_components(
        &self,
        rsx_path: &Path,
        source: &str,
        theme: Option<&str>,
    ) -> (Vec<PreludeComponent>, bool) {
        let Some(root) = crate::build_sync::crate_root(rsx_path) else {
            return (Vec::new(), false);
        };
        let prelude = telar_project::resolve_prelude(&root).unwrap_or_default();
        if prelude.is_empty() {
            return (Vec::new(), false);
        }
        let key = Key {
            prelude,
            lockfile: lockfile_stamp(&root),
        };

        let Ok(mut cache) = self.prelude_cache.lock() else {
            return (Vec::new(), false);
        };
        let kept = cache.known.get(&root).filter(|known| known.key == key);
        let components = kept
            .map(|known| known.components.clone())
            .unwrap_or_default();
        let fresh = kept.is_some_and(|known| known.settled || known.at.elapsed() < RETRY_UNSETTLED);
        let settled = kept.is_some_and(|known| known.settled);
        if fresh || cache.probing.contains(&root) {
            return (components, !settled);
        }

        let Some(analyzer) = self.analyzer.ready() else {
            return (components, false);
        };
        let Some(target) = crate::build_sync::generated_target(rsx_path, source, theme) else {
            return (components, false);
        };
        cache.probing.insert(root.clone());
        let generation = cache.generation;
        drop(cache);

        let cache = self.prelude_cache.clone();
        tokio::spawn(async move {
            let paths: Vec<String> = key
                .prelude
                .iter()
                .map(|entry| entry.path().to_owned())
                .collect();
            let answers = analyzer
                .path_exports(&target.path, &target.code, &paths, component_items)
                .await;
            let settled = answers.iter().all(Option::is_some);
            let components = paths
                .iter()
                .zip(answers)
                .flat_map(|(path, items)| {
                    items
                        .unwrap_or_default()
                        .into_iter()
                        .map(move |item| PreludeComponent::from_item(path, item))
                })
                .collect();
            remember(&cache, generation, root, key, components, settled);
        });
        (components, true)
    }
}

fn remember(
    cache: &Arc<Mutex<PreludeCache>>,
    generation: u64,
    root: PathBuf,
    key: Key,
    components: Vec<PreludeComponent>,
    settled: bool,
) {
    let Ok(mut cache) = cache.lock() else {
        return;
    };
    cache.probing.remove(&root);
    if cache.generation != generation {
        return;
    }
    cache.known.insert(
        root,
        Known {
            key,
            components,
            settled,
            at: Instant::now(),
        },
    );
}

/// When the workspace's lockfile last changed, which is when a prelude crate's version or features may have.
fn lockfile_stamp(package_root: &Path) -> Option<SystemTime> {
    let workspace_root = telar_project::find_workspace_root(package_root)
        .unwrap_or_else(|| package_root.to_path_buf());
    std::fs::metadata(workspace_root.join("Cargo.lock"))
        .and_then(|meta| meta.modified())
        .ok()
}
