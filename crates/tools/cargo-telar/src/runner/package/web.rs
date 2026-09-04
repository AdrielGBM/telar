//! Packaging for the browser: the wasm bundle and the page that loads it.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{dist_dir, tool_missing};
use crate::runner::cli::WebRenderer;
use crate::runner::config::{TelarConfig, resolve_package, split_android_flag};

/// The name `wasm-bindgen` gives its output, and what the generated page imports.
const BUNDLE: &str = "app";

/// Builds the app for the browser: a wasm module, the JavaScript that instantiates it, and a page that starts it.
///
/// Three tools rather than one, because that is what the toolchain is: cargo produces a wasm module whose imports are `wasm-bindgen`'s ABI, `wasm-bindgen` writes the JavaScript that satisfies them, and `wasm-opt` shrinks the result. The first two are required; the third is skipped with a note if it is not installed.
pub(crate) fn build_web(
    cargo_args: Vec<String>,
    config: TelarConfig,
    release: bool,
    renderer: Option<WebRenderer>,
) -> ! {
    let out = match build_web_bundle(cargo_args, config, release, renderer) {
        Ok(out) => out,
        Err(e) => {
            eprintln!("[cargo-telar] {e}");
            std::process::exit(1);
        }
    };
    eprintln!("[cargo-telar] Packaged web build at {}", out.display());
    std::process::exit(0);
}

/// The same build, as a function that returns rather than exits — what `dev --target web` rebuilds with.
pub(crate) fn build_web_bundle(
    cargo_args: Vec<String>,
    _config: TelarConfig,
    release: bool,
    renderer: Option<WebRenderer>,
) -> Result<PathBuf, String> {
    let (_android, rest) = split_android_flag(cargo_args);

    let mut build_args = vec![
        "build".to_string(),
        "--target".to_string(),
        "wasm32-unknown-unknown".to_string(),
        // The browser loads a module, not an executable: the `[lib]` target is what carries the app.
        "--lib".to_string(),
    ];
    build_args.extend(rest.iter().filter(|a| *a != "--release").cloned());
    if release {
        build_args.push("--release".to_string());
    }
    // Reached through `telar/` rather than a feature of the app's own, so any project builds for the web without first declaring one.
    build_args.push("--features".to_string());
    build_args.push("telar/web".to_string());

    eprintln!("[cargo-telar] Building the wasm module...");
    let status = Command::new("cargo")
        .args(&build_args)
        .status()
        .map_err(|e| format!("failed to invoke cargo: {e}"))?;
    if !status.success() {
        return Err("the wasm build failed".to_string());
    }

    let resolved = resolve_package(&rest);
    let profile = if release { "release" } else { "debug" };
    let module = resolved
        .workspace_root
        .join("target/wasm32-unknown-unknown")
        .join(profile)
        .join(format!("{}.wasm", resolved.name().replace('-', "_")));
    if !module.exists() {
        return Err(format!(
            "the build produced no wasm module at {}. Does this package have a `[lib]` target?",
            module.display()
        ));
    }

    let out = dist_dir(&resolved.workspace_root).join("web");
    std::fs::create_dir_all(&out)
        .map_err(|e| format!("could not create {}: {e}", out.display()))?;

    run_wasm_bindgen(&module, &out)?;
    optimise(&out.join(format!("{BUNDLE}_bg.wasm")), release);
    write_page(&out, &resolved.name(), renderer)?;
    Ok(out)
}

fn run_wasm_bindgen(module: &Path, out: &Path) -> Result<(), String> {
    let status = Command::new("wasm-bindgen")
        .arg("--target")
        .arg("web")
        .arg("--no-typescript")
        .arg("--out-dir")
        .arg(out)
        .arg("--out-name")
        .arg(BUNDLE)
        .arg(module)
        .status()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                tool_missing(
                    "wasm-bindgen",
                    "cargo install wasm-bindgen-cli --version <the version in your Cargo.lock>",
                )
            } else {
                format!("failed to invoke wasm-bindgen: {e}")
            }
        })?;
    if !status.success() {
        return Err("wasm-bindgen failed".to_string());
    }
    Ok(())
}

/// Shrinks the module in place. Optional: a build that skips it works, it is just larger.
fn optimise(module: &Path, release: bool) {
    if !release {
        return;
    }
    let tmp = module.with_extension("opt.wasm");
    let status = Command::new("wasm-opt")
        .args([
            "-Oz",
            "--enable-bulk-memory",
            "--enable-nontrapping-float-to-int",
        ])
        .arg("-o")
        .arg(&tmp)
        .arg(module)
        .status();
    match status {
        Ok(status) if status.success() => {
            let _ = std::fs::rename(&tmp, module);
        }
        Ok(_) => {
            let _ = std::fs::remove_file(&tmp);
            eprintln!("[cargo-telar] wasm-opt failed; shipping the unoptimised module.");
        }
        Err(_) => eprintln!(
            "[cargo-telar] wasm-opt is not installed, so the module is shipped unoptimised (install `binaryen` for a smaller one)."
        ),
    }
}

/// Writes the page that starts the app, unless the project brought its own.
///
/// A project that wants control puts a `web/index.html` beside its manifest, and this leaves it alone: the generated one is a starting point, not a thing to fight.
fn write_page(out: &Path, app_name: &str, renderer: Option<WebRenderer>) -> Result<(), String> {
    let page = out.join("index.html");
    let provided = Path::new("web").join("index.html");
    if provided.exists() {
        return std::fs::copy(&provided, &page)
            .map(|_| ())
            .map_err(|e| format!("could not copy {}: {e}", provided.display()));
    }
    std::fs::write(&page, default_page(app_name, renderer))
        .map_err(|e| format!("could not write {}: {e}", page.display()))
}

fn default_page(app_name: &str, renderer: Option<WebRenderer>) -> String {
    // Stamped on the element rather than compiled in, so the same bundle can be loaded either way — and a `?telar-renderer=` on the URL still wins over it.
    let choice = renderer
        .map(|r| format!(" data-telar-renderer=\"{}\"", r.as_str()))
        .unwrap_or_default();
    format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
    <title>{app_name}</title>
    <style>
      /* What the page is before the app has drawn anything, and what shows through an overscroll once it
         has. Without it both are white on a system asking for dark. */
      html {{ color-scheme: light dark; }}
      html, body {{ margin: 0; height: 100%; overflow: hidden; }}
      #telar-root {{ width: 100vw; height: 100vh; }}
    </style>
  </head>
  <body>
    <div id="telar-root"{choice}></div>
    <script type="module">
      import init from './{BUNDLE}.js';
      // `init` instantiates the module and returns its exports; `telar_start` is the entry `telar::app!` generates for this target.
      const wasm = await init();
      wasm.telar_start();
    </script>
  </body>
</html>
"#
    )
}
