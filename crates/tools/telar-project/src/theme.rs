//! Which theme type a package's `.rsx` resolves `use_theme` against, as the project *declares* it.
//!
//! The build has never had to look: `app!(MyTheme, …)` hands the macro the path directly. Everything *else* that transpiles the same file — the editor's live mirror, the golden harness, a CLI-side pass — has no macro invocation to read, and each answered on its own. The editor read `[telar] theme` from `telar.toml`, which the applications in this repository do not set, so it mirrored every themed component as `Theme::<>` while the build compiled `Theme::<SandboxTheme>`: two writers of one file, disagreeing, with the last one to run winning.
//!
//! The other half of the answer — reading the invocation out of the source when the key is absent — needs a Rust parser, so it lives with the transpiler as `resolve_theme_type`.

use std::path::Path;

/// The theme type declared in `[telar] theme`, if the package sets one.
///
/// This is the *declaration*, not the resolution: `telar_transpiler::resolve_theme_type` falls back to the source when it is absent, and `app!` compares against this one to refuse a `telar.toml` that says something the code does not.
pub fn theme_type_in_config(package_dir: &Path) -> Option<String> {
    let declared = crate::read_rsx_section(package_dir)?
        .get("theme")?
        .as_str()?
        .to_string();
    Some(normalize_theme_path(&declared))
}

/// A theme path as generated code has to spell it: rendering a token stream puts spaces around `::`, and a turbofish carrying them does not parse.
pub fn normalize_theme_path(rendered: &str) -> String {
    rendered.replace(" :: ", "::").trim().to_string()
}
