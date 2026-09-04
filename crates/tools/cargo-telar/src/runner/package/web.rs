//! Packaging for the browser: the wasm bundle and the page that loads it.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{dist_dir, tool_missing};
use crate::runner::cli::WebRenderer;
use crate::runner::config::{TelarConfig, resolve_package, split_android_flag};

/// The name `wasm-bindgen` gives its output, and what the generated page imports.
const BUNDLE: &str = "app";

/// The cargo profile a release web build uses, and the directory cargo then writes it to.
const WEB_PROFILE: &str = "web";

/// The `telar/` feature a build with this renderer needs.
///
/// `--renderer dom` is a build saying it will never draw pixels, and that is worth saying: the canvas renderer brings wgpu and a glyph shaper with it, and neither is reachable from a frame that becomes elements. Anything else keeps both renderers, because `auto` has to be able to choose between them at load time.
fn telar_feature(renderer: Option<WebRenderer>) -> &'static str {
    match renderer {
        Some(WebRenderer::Dom) => "web-dom",
        _ => "web",
    }
}

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
    let resolved = resolve_package(&rest);

    let mut build_args = vec![
        "build".to_string(),
        "--target".to_string(),
        "wasm32-unknown-unknown".to_string(),
        // The browser loads a module, not an executable: the `[lib]` target is what carries the app.
        "--lib".to_string(),
    ];
    build_args.extend(rest.iter().filter(|a| *a != "--release").cloned());
    if release {
        // Not `--release`: a module travels a network before it runs, and `[profile.web]` is release tuned for that rather than for a machine that already has the code.
        build_args.push("--profile".to_string());
        build_args.push(WEB_PROFILE.to_string());
    }
    let wanted = telar_feature(renderer);
    build_args.push("--features".to_string());
    match resolved.features.contains_key(wanted) {
        // A package that named this frontend itself is one whose `default` stands for another: the renderers a window needs are not target-gated, so a default left on brings wgpu and a glyph shaper into a page that calls neither.
        true => {
            build_args.push(format!("{}/{wanted}", resolved.name()));
            build_args.push("--no-default-features".to_string());
        }
        // Reached through `telar/` rather than a feature of the app's own, so any project builds for the web without first declaring one — and keeps its defaults, which are the only thing it has said.
        false => build_args.push(format!("telar/{wanted}")),
    }

    eprintln!("[cargo-telar] Building the wasm module...");
    let status = Command::new("cargo")
        .args(&build_args)
        .status()
        .map_err(|e| format!("failed to invoke cargo: {e}"))?;
    if !status.success() {
        return Err("the wasm build failed".to_string());
    }

    let profile = if release { WEB_PROFILE } else { "debug" };
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
    let wasm = fingerprint(&out, release)?;
    write_page(&out, &resolved.name(), renderer, &wasm)?;
    precompress(&out, release);
    Ok(out)
}

/// Below this a compressed copy saves nothing worth a second file on disk and a second lookup at request time.
const COMPRESS_FLOOR: u64 = 1024;

/// Writes a `.br` and a `.gz` beside every artifact big enough to be worth one.
///
/// Compression is the server's job and stays it — but the *level* is not something a server can choose freely. Brotli at its highest takes seconds over a module this size, so nothing compresses one per request: a server runs a fast level and ships a body a hundred kilobytes larger than it had to. Doing it once here is the only way anyone ever gets the small one.
///
/// nginx (`brotli_static`, `gzip_static`), Caddy (`precompressed`) and most static hosts serve these in place of the original when the request accepts the encoding. One that has never heard of them reads the original and ignores the rest, so this costs nothing but disk.
fn precompress(out: &Path, release: bool) {
    // Cleared before anything is written, and on a debug build instead of writing: a stale copy is worse than none, because a server that looks for one serves the last build's bytes under this build's URL and says nothing about it. A name carrying its own hash is safe from that; `app.js` and `index.html` are not.
    if let Ok(entries) = std::fs::read_dir(out) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".br") || name.ends_with(".gz") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    if !release {
        return;
    }
    let Ok(entries) = std::fs::read_dir(out) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.metadata().is_ok_and(|m| m.len() < COMPRESS_FLOOR) {
            continue;
        }
        let Ok(bytes) = std::fs::read(entry.path()) else {
            continue;
        };
        if let Some(encoded) = brotli(&bytes) {
            let _ = std::fs::write(out.join(format!("{name}.br")), encoded);
        }
        if let Some(encoded) = gzip(&bytes) {
            let _ = std::fs::write(out.join(format!("{name}.gz")), encoded);
        }
    }
}

fn brotli(bytes: &[u8]) -> Option<Vec<u8>> {
    // Quality 11 and a 4MB window: the slow end, which is the whole reason to do this ahead of time rather than per request.
    let params = brotli::enc::BrotliEncoderParams {
        quality: 11,
        lgwin: 22,
        ..Default::default()
    };
    let mut encoded = Vec::new();
    brotli::BrotliCompress(&mut std::io::Cursor::new(bytes), &mut encoded, &params).ok()?;
    Some(encoded)
}

fn gzip(bytes: &[u8]) -> Option<Vec<u8>> {
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(bytes).ok()?;
    encoder.finish().ok()
}

/// Renames the module after its own content, and points the glue at the new name. Returns the file name the page should preload.
///
/// A module served under a fixed name cannot be cached for longer than you are willing to wait for a deploy to be noticed: the URL is the only thing a browser has to tell one build from the next. Under a content-derived name it can be cached forever, because a new build is a new URL — which is what lets the largest file in the bundle cost nothing at all on a second visit.
///
/// Only the module is renamed. The glue keeps its name so a project that brought its own `web/index.html` — which reaches for `./app.js` — is not broken by a flag it never set, and at forty kilobytes against two megabytes it is not where the caching is won.
///
/// Debug builds keep the plain name: they are rebuilt every few seconds behind a server that already says `no-store`, and a fresh file name per rebuild would only litter the directory.
fn fingerprint(out: &Path, release: bool) -> Result<String, String> {
    let plain = format!("{BUNDLE}_bg.wasm");
    let module = out.join(&plain);
    let named = match release {
        true => {
            let bytes = std::fs::read(&module)
                .map_err(|e| format!("could not read {}: {e}", module.display()))?;
            Some(format!("{BUNDLE}-{}_bg.wasm", short_hash(&bytes)))
        }
        false => None,
    };

    // Every fingerprinted module but this build's, so a debug build after a release one does not leave two megabytes behind that nothing reaches. The suffix is stripped before comparing so a module's compressed copies go with it.
    if let Ok(entries) = std::fs::read_dir(out) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let base = name
                .strip_suffix(".br")
                .or_else(|| name.strip_suffix(".gz"))
                .unwrap_or(&name);
            if base.starts_with(&format!("{BUNDLE}-"))
                && base.ends_with("_bg.wasm")
                && Some(base) != named.as_deref()
            {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    let Some(named) = named else {
        return Ok(plain);
    };
    std::fs::rename(&module, out.join(&named))
        .map_err(|e| format!("could not rename {}: {e}", module.display()))?;

    let glue = out.join(format!("{BUNDLE}.js"));
    let source = std::fs::read_to_string(&glue)
        .map_err(|e| format!("could not read {}: {e}", glue.display()))?;
    let patched = source.replace(&format!("'{plain}'"), &format!("'{named}'"));
    if patched == source {
        return Err(format!(
            "the generated glue does not name `{plain}`, so the renamed module would never be fetched"
        ));
    }
    std::fs::write(&glue, patched)
        .map_err(|e| format!("could not write {}: {e}", glue.display()))?;
    Ok(named)
}

/// A short content hash, for telling one build's module from another's. FNV-1a rather than a cryptographic digest: nothing here is defended against a forged collision, and the question is only whether these bytes are the bytes the browser already has.
fn short_hash(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")[..12].to_string()
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
fn write_page(
    out: &Path,
    app_name: &str,
    renderer: Option<WebRenderer>,
    wasm: &str,
) -> Result<(), String> {
    let page = out.join("index.html");
    let provided = Path::new("web").join("index.html");
    if provided.exists() {
        return std::fs::copy(&provided, &page)
            .map(|_| ())
            .map_err(|e| format!("could not copy {}: {e}", provided.display()));
    }
    std::fs::write(&page, default_page(app_name, renderer, wasm))
        .map_err(|e| format!("could not write {}: {e}", page.display()))
}

fn default_page(app_name: &str, renderer: Option<WebRenderer>, wasm: &str) -> String {
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
    <meta name="description" content="{app_name}, a Telar application." />
    <!-- Both start downloading with the page rather than one after the other. The module is only reached
         through an import inside the glue, so without this the browser learns the wasm exists after it has
         fetched and parsed the glue — two round trips in series before a pixel, on the largest file here. -->
    <link rel="modulepreload" href="./{BUNDLE}.js" />
    <link rel="preload" href="./{wasm}" as="fetch" type="application/wasm" crossorigin />
    <style>
      /* What the page is before the app has drawn anything, and what shows through an overscroll once it
         has. Without it both are white on a system asking for dark. */
      html {{ color-scheme: light dark; }}
      html, body {{ margin: 0; height: 100%; overflow: hidden; }}
      /* `100%` rather than `100vw`, which is the viewport including the space a scrollbar takes and so a
         hair wider than the page on every desktop browser that reserves one. `dvh` rather than `vh` because
         a mobile browser's `vh` is the viewport with the URL bar retracted, which it is not on arrival. */
      #telar-root {{ width: 100%; height: 100dvh; }}
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
