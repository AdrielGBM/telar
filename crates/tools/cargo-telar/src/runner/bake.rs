//! `cargo telar bake` and the single point that runs it before every subcommand that goes on to compile.
//!
//! Only the workspace half lives here — which members to bake, in what order, and which `telar` the project resolves. Baking one package is `telar-baker`'s, because `telar-analyzer` bakes too and neither binary can depend on the other.

use std::path::Path;

use telar_baker::{BakeProblem, BakeReport, CatalogReport, Origin, ProblemKind, Problems};

use super::config::find_package_dir;

/// Bakes every workspace member's `.rsx` asset references and translation catalogs, answering whether every member baked without an error.
///
/// Called once from [`super::run`], before dispatching to any subcommand that goes on to invoke `cargo`, and again from each of `watch.rs`'s two loops, which spawn their own `cargo` per rebuild. **Every place in this binary that spawns `cargo` is a build route**, and a build route that has not baked compiles against a stale artifact — or, since the macro checks hashes, fails. A new one either calls this first or sits downstream of a call that did.
///
/// An error is printed here and left to the caller to act on: a one-off command exits on it, while a watch loop goes on to the build, whose `compile_error!` points at the `.rsx` line, and waits for the next edit.
///
/// Members bake after the members they are built with, and each is told every crate it is built with, so its icon notice merges the records those crates' bakes leave.
pub(crate) fn bake_workspace() -> bool {
    let mut clean = true;
    let dir = find_package_dir(&[]);
    let workspace_root = telar_project::find_workspace_root(&dir).unwrap_or_else(|| dir.clone());
    let metadata = telar_project::cargo_metadata(&workspace_root);
    // This binary's own version is the right fallback only because every crate here shares the workspace version; for any other project a failed resolve means cargo itself is unusable, and the build is about to say so.
    let telar_version = metadata
        .as_ref()
        .and_then(telar_project::telar_version_in)
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    super::config::warn_if_foreign_version(&telar_version);
    let producer = format!("cargo-telar {}", env!("CARGO_PKG_VERSION"));
    let (graph, graph_problem) = match metadata
        .as_ref()
        .map(telar_baker::DependencyGraph::from_metadata)
        .transpose()
    {
        Ok(graph) => (graph, None),
        Err(problem) => {
            eprintln!("[cargo-telar] error: {}", render(&problem, &workspace_root));
            clean = false;
            (None, Some(problem))
        }
    };
    let members = telar_project::workspace_member_dirs(&workspace_root);
    let members = match &graph {
        Some(graph) => graph.bake_order(members),
        None => members,
    };
    for member in members {
        let name = member
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("package");
        // Both bakes below read it and would each report it, so it is reported once here instead.
        match member_manifest(&member, graph_problem.as_ref()) {
            MemberManifest::Loaded => {}
            MemberManifest::Broken(problem) => {
                eprintln!("[cargo-telar] error: {name}: {}", render(&problem, &member));
                clean = false;
                continue;
            }
            MemberManifest::ReportedByGraph => {
                clean = false;
                continue;
            }
        }
        let dependencies = graph
            .as_ref()
            .and_then(|graph| graph.icon_dependencies(&member));
        if let Some(report) = telar_baker::bake_package_with(
            &member,
            &producer,
            &telar_version,
            dependencies.as_deref(),
        ) {
            for line in asset_lines(&report, name, &member) {
                eprintln!("{line}");
            }
            clean &= !report.problems.has_errors();
        }
        match telar_baker::bake_catalog(&member, &producer, &telar_version) {
            Ok(Some(report)) => {
                for line in catalog_lines(&report, name, &member) {
                    eprintln!("{line}");
                }
                clean &= !report.problems.has_errors();
            }
            Ok(None) => {}
            Err(e) => {
                eprintln!("[cargo-telar] error: {name}: {e}");
                clean = false;
            }
        }
    }
    clean
}

enum MemberManifest {
    Loaded,
    Broken(BakeProblem),
    /// The dependency graph could not be built over this same file, and said so already.
    ReportedByGraph,
}

fn member_manifest(member: &Path, graph_problem: Option<&BakeProblem>) -> MemberManifest {
    let Err(error) = telar_project::TelarManifest::load(member) else {
        return MemberManifest::Loaded;
    };
    let problem = BakeProblem::new(
        ProblemKind::Manifest(error),
        member.join(telar_project::MANIFEST_FILENAME),
    );
    match graph_problem {
        Some(reported) if same_file(&reported.file, &problem.file) => {
            MemberManifest::ReportedByGraph
        }
        _ => MemberManifest::Broken(problem),
    }
}

// The graph names a crate by the directory `cargo metadata` gives, a member by its workspace glob, so the two spellings can differ.
fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn asset_lines(report: &BakeReport, name: &str, package_dir: &Path) -> Vec<String> {
    let baked = report.changed.then(|| format!("{} asset(s)", report.baked));
    report_lines(&report.problems, baked, name, package_dir)
}

fn catalog_lines(report: &CatalogReport, name: &str, package_dir: &Path) -> Vec<String> {
    let baked = report
        .changed
        .then(|| format!("{} translation key(s)", report.keys));
    report_lines(&report.problems, baked, name, package_dir)
}

/// What `cargo telar bake` prints for one bake of a member: its warnings, what it baked when that changed, then its errors.
fn report_lines(
    problems: &Problems,
    baked: Option<String>,
    name: &str,
    package_dir: &Path,
) -> Vec<String> {
    let warnings = problems
        .warnings()
        .map(|problem| format!("[cargo-telar] warning: {}", render(problem, package_dir)));
    let baked = baked.map(|baked| format!("[cargo-telar] Baked {baked} for {name}"));
    let errors = problems.errors().map(|problem| {
        format!(
            "[cargo-telar] error: {name}: {}",
            render(problem, package_dir)
        )
    });
    warnings.chain(baked).chain(errors).collect()
}

pub(crate) fn render(problem: &BakeProblem, package_dir: &Path) -> String {
    match problem.origin() {
        Some(Origin::Line(line)) => {
            let file = problem
                .file
                .strip_prefix(package_dir)
                .unwrap_or(&problem.file);
            format!("{}:{line}: {problem}", file.display())
        }
        Some(Origin::Asset(_)) | None => problem.to_string(),
    }
}

#[cfg(test)]
#[path = "bake.test.rs"]
mod tests;
