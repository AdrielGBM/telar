//! Resolving the theme type a package's components are transpiled against, when `telar.toml` does not declare one.
//!
//! The declaration itself is [`telar_project::theme`]; this is the fallback that reads it out of the `app!` invocation, which needs a Rust parser and so cannot live beside the config key.

use std::path::{Path, PathBuf};

use telar_project::{normalize_theme_path, theme_type_in_config};

/// The theme type a package's components are transpiled against: `[telar] theme` when set, otherwise the first argument of the `app!` / `rsx_modules!` invocation that places the package's `.rsx`.
pub fn resolve_theme_type(package_dir: &Path) -> Option<String> {
    theme_type_in_config(package_dir).or_else(|| theme_type_in_source(package_dir))
}

/// The theme named by the macro invocation that places this package's `.rsx` files.
///
/// Only the files that are allowed to hold one are read: the crate roots, and the `mod.rs` of a directory that places its own `.rsx` — which is the whole of where the framework lets `app!` / `rsx_modules!` be written. A full-tree parse would be the same answer at the cost of re-reading every `.rs` in the package on each editor keystroke.
fn theme_type_in_source(package_dir: &Path) -> Option<String> {
    invocation_files(&package_dir.join("src"))
        .into_iter()
        .find_map(|path| theme_type_in_file(&path))
}

fn invocation_files(src_dir: &Path) -> Vec<PathBuf> {
    let mut files = vec![src_dir.join("lib.rs"), src_dir.join("main.rs")];
    files.extend(
        telar_project::collect_files_by_ext(src_dir, "rs", &|name| {
            name != "target" && name != ".telar"
        })
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|name| name == "mod.rs")),
    );
    files.retain(|path| path.is_file());
    files
}

fn theme_type_in_file(path: &Path) -> Option<String> {
    let source = std::fs::read_to_string(path).ok()?;
    // Parsing is the expensive half and most files hold no invocation at all; the substring is what keeps this off the parser for all but the one file that does.
    if !source.contains("app!") && !source.contains("rsx_modules!") {
        return None;
    }
    let file = syn::parse_file(&source).ok()?;
    theme_type_in_items(&file.items)
}

fn theme_type_in_items(items: &[syn::Item]) -> Option<String> {
    items.iter().find_map(|item| match item {
        syn::Item::Macro(item) if places_rsx(&item.mac.path) => theme_argument(&item.mac.tokens),
        // An invocation inside an inline `mod` block is still this package's, and the file holding it is one of the few this walk reads.
        syn::Item::Mod(item) => item
            .content
            .as_ref()
            .and_then(|(_, items)| theme_type_in_items(items)),
        _ => None,
    })
}

/// Whether the macro being invoked is one of the two that transpile a package, however it was reached — `telar::app!`, `app!`, `crate::macros::app!`.
fn places_rsx(path: &syn::Path) -> bool {
    path.segments
        .last()
        .is_some_and(|last| last.ident == "app" || last.ident == "rsx_modules")
}

/// The leading path argument, which both macros take as their theme. `rsx_modules!()` passes none, and `app!` without one does not compile — so `None` here means the package has no theme, not that it could not be read.
fn theme_argument(tokens: &proc_macro2::TokenStream) -> Option<String> {
    let head: proc_macro2::TokenStream = tokens
        .clone()
        .into_iter()
        .take_while(
            |token| !matches!(token, proc_macro2::TokenTree::Punct(p) if p.as_char() == ','),
        )
        .collect();
    if head.is_empty() {
        return None;
    }
    // Parsed to refuse anything that is not a path, rendered from the tokens: `syn` and the `app!` macro spell one the same way, and going through the parse would only be a second spelling to keep in step.
    syn::parse2::<syn::Path>(head.clone()).ok()?;
    Some(normalize_theme_path(&head.to_string()))
}

#[cfg(test)]
#[path = "theme_test.rs"]
mod tests;
