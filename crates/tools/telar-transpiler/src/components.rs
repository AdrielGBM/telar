//! The cross-file component registry: the pre-pass every `.rsx` transpile needs before it can emit a call
//! into another file.
//!
//! One implementation for both callers. The build (`app!`) and the editor's live mirror must agree on which
//! file a component name resolves to, or IntelliSense describes a different component than the one the
//! compiler will call.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::codegen::{ComponentRegistry, external_component_sigs, scan_component_sig};
use crate::discovery::{find_rsx_files, relative_stem};
use crate::naming::to_snake_case;

/// The registry plus what the caller has to do about how it was built.
pub struct ProjectComponents {
    /// Every component's signature (its `Props` shape and whether it takes a slot), keyed by both the
    /// path-flattened stem and the bare basename.
    pub registry: ComponentRegistry,
    /// The borrowed `.rsx` files that fed it. Their signatures are baked into this crate's call sites, so a
    /// build has to re-run when one of them changes.
    pub borrowed: Vec<PathBuf>,
    /// Two of this crate's own files claiming one bare basename. The registry is still usable — whoever
    /// claimed the name keeps it — so the build turns this into a `compile_error!` while the editor ignores
    /// it and lets IntelliSense degrade instead of going dark.
    pub collision: Option<String>,
}

/// Scans the built-in catalogue, then `borrowed_dirs`, then `src_dir`, each layer overriding the last, so a
/// local component wins its name against a borrowed one and against the catalogue.
///
/// `overrides` supplies the source for a path instead of reading it from disk — the editor passes the live
/// buffer of the file being typed in, so unsaved edits to a component's props take effect immediately.
pub fn build_component_registry(
    src_dir: &Path,
    borrowed_dirs: &[PathBuf],
    overrides: &[(&Path, &str)],
) -> ProjectComponents {
    let read = |path: &Path| -> Option<String> {
        match overrides.iter().find(|(p, _)| *p == path) {
            Some((_, source)) => Some((*source).to_string()),
            None => std::fs::read_to_string(path).ok(),
        }
    };

    let mut registry = ComponentRegistry::new();
    for (name, sig) in external_component_sigs() {
        registry.insert(name.to_string(), sig);
    }

    // The crates this one borrows components from (`[telar] components` in telar.toml). Signatures only —
    // each of those files is compiled by the crate that owns it.
    let mut borrowed = Vec::new();
    for dir in borrowed_dirs {
        for rsx_file in find_rsx_files(dir) {
            let Some(source) = read(&rsx_file) else {
                continue;
            };
            let sig = scan_component_sig(&source);
            registry.insert(to_snake_case(&relative_stem(&rsx_file, dir)), sig.clone());
            if let Some(base) = rsx_file.file_stem().and_then(|s| s.to_str()) {
                registry.entry(to_snake_case(base)).or_insert(sig);
            }
            borrowed.push(rsx_file);
        }
    }

    // Which of *this crate's own* files claimed each bare basename, so a second claimant is named rather than
    // dropped. Borrowed components are not tracked here: a local component outranks one from another crate.
    let mut short_names: HashMap<String, PathBuf> = HashMap::new();
    let mut collision = None;
    for rsx_file in find_rsx_files(src_dir) {
        let Some(source) = read(&rsx_file) else {
            continue;
        };
        let sig = scan_component_sig(&source);
        registry.insert(
            to_snake_case(&relative_stem(&rsx_file, src_dir)),
            sig.clone(),
        );
        let Some(base) = rsx_file.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        // Two files with the same basename in different directories both want the short name, and only the
        // first in walk order got it — silently, so a call meant for one resolved to the other's signature
        // and failed somewhere else entirely. Whoever claimed it keeps it; the collision is now named.
        match short_names.entry(to_snake_case(base)) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(rsx_file.clone());
                registry.insert(to_snake_case(base), sig);
            }
            std::collections::hash_map::Entry::Occupied(taken) => {
                collision.get_or_insert_with(|| {
                    format!(
                        "two components share the short name `{base}`: {} and {}. Call either by its full path-flattened name, or rename one.",
                        taken.get().display(),
                        rsx_file.display()
                    )
                });
            }
        }
    }

    ProjectComponents {
        registry,
        borrowed,
        collision,
    }
}
