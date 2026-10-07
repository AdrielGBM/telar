//! Packaging for the browser: the wasm bundle, the page that loads it, and every other file the site serves.

use std::path::{Path, PathBuf};
use std::process::Command;

use telar_project::{FontDeclaration, PageLocation, WebHost, WebSection};

use super::{dist_dir, tool_missing};
use crate::runner::cli::{Target, WebRenderer};
use crate::runner::config::{
    TelarSection, WEB_PROFILE_TOML, find_package_dir, has_web_profile, resolve_package,
    split_android_flag,
};

mod assets;
mod cloudflare;
mod fonts;
mod images;
mod media;
mod negotiation;
mod page;
mod prerender;
mod site;

use assets::{Assets, MANIFEST_FILE};
use page::{Bootstrap, DEFAULT_TEMPLATE, Page};
use site::{PageMeta, SITEMAP_FILE, Site, SitePages};

pub(crate) use media::{compressible, media_type};

/// The name `wasm-bindgen` gives its output, and what the generated page imports.
const BUNDLE: &str = "app";

/// The page every build writes at the output root.
const PAGE_FILE: &str = "index.html";

/// The target a browser module is built for, and what `wasm-bindgen` reads its output from.
pub(crate) const WASM_TARGET: &str = "wasm32-unknown-unknown";

/// The cargo profile a release web build uses, and the directory cargo then writes it to.
const WEB_PROFILE: &str = "web";

/// Builds the app for the browser: a wasm module, the JavaScript that instantiates it, and a page that starts it.
///
/// Three tools rather than one, because that is what the toolchain is: cargo produces a wasm module whose imports are `wasm-bindgen`'s ABI, `wasm-bindgen` writes the JavaScript that satisfies them, and `wasm-opt` shrinks the result. The first two are required; the third is skipped with a note if it is not installed.
pub(crate) fn build_web(
    cargo_args: Vec<String>,
    config: TelarSection,
    release: bool,
    renderer: Option<WebRenderer>,
    prerender: bool,
) -> ! {
    let out = match build_web_bundle(cargo_args, config, release, renderer, prerender) {
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
    config: TelarSection,
    release: bool,
    renderer: Option<WebRenderer>,
    prerender: bool,
) -> Result<PathBuf, String> {
    let (_android, rest) = split_android_flag(cargo_args);
    let resolved = resolve_package(&rest);
    let package_root = find_package_dir(&rest);

    // Checked before cargo ever runs: `--profile web` below would otherwise fail with cargo's own "profile `web` is not defined" error, which names neither the file to fix nor what to put in it.
    if release && !has_web_profile(&resolved.workspace_root) {
        return Err(format!(
            "this project's Cargo.toml has no `[profile.web]`, which a release web build needs (`cargo telar build --target web` passes `--profile web`). Add this to {}:\n\n{WEB_PROFILE_TOML}",
            resolved.workspace_root.join("Cargo.toml").display(),
        ));
    }

    let mut build_args = vec![
        "build".to_string(),
        "--target".to_string(),
        WASM_TARGET.to_string(),
        // The browser loads a module, not an executable: the `[lib]` target is what carries the app.
        "--lib".to_string(),
    ];
    build_args.extend(rest.iter().filter(|a| *a != "--release").cloned());
    if release {
        // Not `--release`: a module travels a network before it runs, and `[profile.web]` is release tuned for that rather than for a machine that already has the code.
        build_args.push("--profile".to_string());
        build_args.push(WEB_PROFILE.to_string());
    }
    // The renderers a window needs are not target-gated, so a default left on brings wgpu and a glyph shaper into a page that calls neither.
    let mut frontend = Vec::new();
    resolved
        .frontend_feature(Target::Web.feature(renderer))
        .push_to(&mut frontend);
    build_args.extend(frontend.iter().cloned());

    eprintln!("[cargo-telar] Building the wasm module...");
    let status = Command::new("cargo")
        .args(&build_args)
        .status()
        .map_err(|e| format!("failed to invoke cargo: {e}"))?;
    if !status.success() {
        return Err("the wasm build failed".to_string());
    }

    let profile = if release { WEB_PROFILE } else { "debug" };
    let module = telar_project::find_target_dir(&resolved.workspace_root)
        .join(WASM_TARGET)
        .join(profile)
        .join(format!("{}.wasm", resolved.name().replace('-', "_")));
    if !module.exists() {
        return Err(format!(
            "the build produced no wasm module at {}. Does this package have a `[lib]` target?",
            module.display()
        ));
    }

    // Assembled beside the output and swapped in whole, so the output only ever holds one build: every name in it is either this build's or gone, and a server reading it mid-build sees the last build rather than half of this one.
    let dist = dist_dir(&resolved.workspace_root);
    let out = dist.join("web");
    let staging = dist.join("web.staging");
    recreate_dir(&staging)?;

    let locale = config.default_locale().unwrap_or_else(|| "en".to_string());
    let site = Site::new(&config.web, &package_root, &resolved.name())?;
    let host = config.web.host();
    run_wasm_bindgen(&module, &staging)?;
    optimise(&staging.join(format!("{BUNDLE}_bg.wasm")), release);
    let (page, template) = assemble(
        &staging,
        &package_root,
        &config.web,
        &config.fonts,
        &Shell {
            app_name: &resolved.name(),
            renderer,
            locale: &locale,
            site: &site,
            reserved: &reserved_paths(prerender, host),
        },
    )?;
    let pages = if prerender {
        let binary =
            prerender::build_host_binary(&rest, &frontend, &package_root.join("Cargo.toml"))?;
        let pages = prerender::write_pages(
            &staging,
            &page,
            &template,
            &binary,
            &package_root,
            &config.web.prerender,
            &site,
        )?;
        eprintln!(
            "[cargo-telar] Prerendered {} page(s) and a 404 page",
            pages.pages.len()
        );
        write_sitemap(&staging, &site, &pages)?;
        pages
    } else {
        SitePages::root_only()
    };
    images::ship_images(
        &staging,
        &images::local_crates(&resolved.workspace_root, &resolved.name())?,
    )?;
    ship_icon_notice(&staging, &package_root)?;
    match host {
        WebHost::Static if release => assets::precompress(&staging)?,
        WebHost::Static => {}
        WebHost::CloudflarePages => {
            cloudflare::write_host_files(&staging, &site, &pages)?;
            for (file, len) in cloudflare::oversized(&staging)? {
                eprintln!(
                    "[cargo-telar] warning: {file} is {:.1} MiB, and Cloudflare Pages serves no file over {} MiB",
                    len as f64 / (1024.0 * 1024.0),
                    cloudflare::MAX_FILE_BYTES / (1024 * 1024)
                );
            }
        }
    }
    publish(&staging, &out)?;
    Ok(out)
}

/// The paths a build writes itself, which a public file of the same name would collide with.
fn reserved_paths(prerender: bool, host: WebHost) -> Vec<&'static str> {
    let mut reserved = vec![PAGE_FILE, MANIFEST_FILE, telar_baker::ICONS_NOTICE_FILENAME];
    if prerender {
        reserved.extend([prerender::NOT_FOUND_FILE, SITEMAP_FILE]);
    }
    if host == WebHost::CloudflarePages {
        reserved.extend([cloudflare::WORKER_FILE, cloudflare::ROUTES_FILE]);
    }
    reserved
}

fn write_sitemap(out: &Path, site: &Site, pages: &SitePages) -> Result<(), String> {
    match site.sitemap(pages) {
        Some(sitemap) => assets::write_new(&out.join(SITEMAP_FILE), sitemap.as_bytes()),
        None => {
            eprintln!(
                "[cargo-telar] note: `[telar.web] origin` is not set, so the pages carry no canonical or alternate-language links and no sitemap is written"
            );
            Ok(())
        }
    }
}

/// What the page around the app is built from, beyond the project's own files.
struct Shell<'a> {
    app_name: &'a str,
    renderer: Option<WebRenderer>,
    /// The locale the plain page is written in.
    locale: &'a str,
    site: &'a Site,
    /// The paths of the output a public file may not take.
    reserved: &'a [&'a str],
}

/// The licence notice of the icons baked into the module, the application's and those of the crates it is built with, at the root of the site, where it is served beside what it describes.
fn ship_icon_notice(out: &Path, package_root: &Path) -> Result<(), String> {
    let Some(notice) = super::icon_notice(package_root) else {
        return Ok(());
    };
    std::fs::copy(&notice, out.join(telar_baker::ICONS_NOTICE_FILENAME))
        .map(|_| ())
        .map_err(|e| format!("could not copy {} into the site: {e}", notice.display()))
}

/// Everything after `wasm-bindgen`: the hashed bootstrap files, the public directory, the page and the manifest.
///
/// The module is hashed first and the glue after it is pointed at the module's new name, so the glue's own hash covers which module it loads: a new module is a new glue URL, and a page cached with the old one can never pair it with the new module.
fn assemble(
    out: &Path,
    package_root: &Path,
    web: &WebSection,
    fonts: &[FontDeclaration],
    shell: &Shell,
) -> Result<(Page, String), String> {
    let mut assets = Assets::new(out);
    let module = assets.adopt(&format!("{BUNDLE}_bg.wasm"))?;
    point_glue_at(&out.join(format!("{BUNDLE}.js")), &module)?;
    let glue = assets.adopt(&format!("{BUNDLE}.js"))?;

    let (public, named) = web.public_dir(package_root);
    if public.is_dir() {
        assets::copy_public(&public, out, shell.reserved)?;
    } else if named {
        return Err(format!(
            "`[telar.web] public` names {}, which is not a directory",
            public.display()
        ));
    }

    let mut page = Page::new(shell.app_name, Bootstrap { glue, module });
    page.lang = shell.locale.to_string();
    page.dir = layout_core::Direction::for_locale(shell.locale);
    page.renderer = shell.renderer;
    page.base = shell.site.base().to_string();
    page.meta = shell.site.head_tags(&PageMeta {
        title: shell.app_name,
        lang: shell.locale,
        location: Some(&PageLocation::root()),
        pages: &SitePages::root_only(),
    });
    fonts::declare_fonts(&mut page, &mut assets, package_root, fonts)?;
    let (template, origin) = read_template(web, package_root)?;
    let html = page
        .render(&template)
        .map_err(|e| format!("the page template ({origin}): {e}"))?;
    assets::write_new(&out.join(PAGE_FILE), html.as_bytes())?;
    assets.write_manifest()?;
    Ok((page, template))
}

/// The project's template, or the built-in page where the project has none at the default path. A path the project named that cannot be read is an error, never a quiet fallback.
fn read_template(web: &WebSection, package_root: &Path) -> Result<(String, String), String> {
    let (path, named) = web.template_path(package_root);
    match std::fs::read_to_string(&path) {
        Ok(template) => Ok((template, path.display().to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !named => Ok((
            DEFAULT_TEMPLATE.to_string(),
            "the built-in page".to_string(),
        )),
        Err(e) => Err(format!(
            "could not read the page template {}: {e}",
            path.display()
        )),
    }
}

/// Rewrites the glue's reference to the module `wasm-bindgen` named, so it fetches the hashed file instead.
fn point_glue_at(glue: &Path, module: &str) -> Result<(), String> {
    let plain = format!("'{BUNDLE}_bg.wasm'");
    let source = std::fs::read_to_string(glue)
        .map_err(|e| format!("could not read {}: {e}", glue.display()))?;
    let patched = source.replace(&plain, &format!("'{module}'"));
    if patched == source {
        return Err(format!(
            "the generated glue does not name {plain}, so the hashed module would never be fetched"
        ));
    }
    std::fs::write(glue, patched).map_err(|e| format!("could not write {}: {e}", glue.display()))
}

fn recreate_dir(dir: &Path) -> Result<(), String> {
    if dir.exists() {
        std::fs::remove_dir_all(dir)
            .map_err(|e| format!("could not clear {}: {e}", dir.display()))?;
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))
}

fn publish(staging: &Path, out: &Path) -> Result<(), String> {
    if out.exists() {
        std::fs::remove_dir_all(out)
            .map_err(|e| format!("could not replace {}: {e}", out.display()))?;
    }
    std::fs::rename(staging, out).map_err(|e| {
        format!(
            "could not move {} to {}: {e}",
            staging.display(),
            out.display()
        )
    })
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

#[cfg(test)]
#[path = "web_test.rs"]
mod tests;
