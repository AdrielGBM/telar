//! `cargo telar preview --build <dir> --target web`: the workshop as a static site, the build `preview --target web` serves, written once with a page at the address of every preview, matrix and docs page, so any static host opens each of them directly.

use std::path::{Path, PathBuf};

use telar_project::protocol::{
    ListedPreview, PREVIEW_LIST_FILE_VAR, PREVIEW_LIST_VAR, workshop_pages,
};

use super::cli::{CommonArgs, Target};
use super::config::TelarSection;
use super::package::{SitePlan, WebBuild, build_web_site, check_site_dir};
use super::watch::{HotMode, with_web_features};
use super::{BuildPlan, preview_once_command};

pub(super) fn run_workshop_export(common: CommonArgs, out: &Path, base: Option<&str>) -> ! {
    match export(common, out, base) {
        Ok(out) => {
            eprintln!("[cargo-telar] Exported the workshop at {}", out.display());
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("[cargo-telar] {e}");
            std::process::exit(1);
        }
    }
}

fn export(mut common: CommonArgs, out: &Path, base: Option<&str>) -> Result<PathBuf, String> {
    common.target = Some(export_target(common.target)?);
    check_site_dir(out)?;
    let plan = BuildPlan::new(common.clone(), true);
    let config = with_base(plan.config, base)?;
    let previews = list_previews(&common)?;
    if previews.is_empty() {
        eprintln!(
            "[cargo-telar] note: the package lists no previews, so the site is the workshop's root page alone"
        );
    }
    build_web_site(
        with_web_features(&HotMode::Preview, plan.cargo_args),
        config,
        &WebBuild {
            release: true,
            renderer: plan.renderer,
            pages: SitePlan::Addresses(workshop_pages(&previews)),
            out: Some(out),
        },
    )
}

fn export_target(target: Option<Target>) -> Result<Target, String> {
    match target {
        None | Some(Target::Web) => Ok(Target::Web),
        Some(_) => Err(
            "`--build` exports the workshop as a site, which only a browser opens; use `--target web`"
                .to_string(),
        ),
    }
}

fn with_base(mut config: TelarSection, base: Option<&str>) -> Result<TelarSection, String> {
    let Some(base) = base else {
        return Ok(config);
    };
    config.web.base = Some(base.to_string());
    let problems = config.web.problems();
    if problems.is_empty() {
        return Ok(config);
    }
    Err(format!("`--base {base}`: {}", problems.join("; ")))
}

fn list_previews(common: &CommonArgs) -> Result<Vec<ListedPreview>, String> {
    let scratch = crate::runner::scratch::scratch_dir("preview-list")?;
    let file = scratch.path().join("previews.json");
    eprintln!("[cargo-telar] Listing the previews...");
    let status = preview_once_command(common, false, "previews")
        .env(PREVIEW_LIST_VAR, "1")
        .env(PREVIEW_LIST_FILE_VAR, &file)
        .status()
        .map_err(|e| format!("failed to invoke cargo: {e}"))?;
    if !status.success() {
        return Err("listing the previews failed".to_string());
    }
    let raw = std::fs::read(&file).map_err(|e| {
        format!(
            "the app wrote no preview listing to {} ({e}); does its `run()` go through `telar::app!` or call `telar::dev_entry`?",
            file.display()
        )
    })?;
    serde_json::from_slice(&raw).map_err(|e| format!("could not read the preview listing: {e}"))
}

#[cfg(test)]
#[path = "workshop_export_test.rs"]
mod tests;
