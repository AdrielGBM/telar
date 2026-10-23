//! The subcommands: dev, check, fmt, preview, test, build, package, publish and migrate.

use std::path::Path;
use std::process::Command;

use clap::Parser;
use telar_project::protocol::{
    PREVIEW_COMPONENT_VAR, PREVIEW_ID_VAR, PREVIEW_LIST_VAR, workshop_channel_file,
    workshop_server_file,
};

mod android;
mod bake;
mod check;
mod cli;
mod config;
mod diagnostics;
mod doctor;
mod fmt;
mod interrupt;
mod junit;
mod library;
mod migrate;
mod new;
mod package;
mod scratch;
mod test_cmd;
mod transpile;
mod watch;
mod web_dev;
mod workshop_channel;
mod workshop_export;

use android::build_android_package;
use bake::bake_workspace;
use check::run_check_cmd;
use cli::{
    BuildArgs, BuildFormat, Cli, CommonArgs, DevArgs, DevtoolsArg, HotArgs, PreviewArgs, Target,
    TelarCommand, WebRenderer,
};
use config::{TelarSection, load_config, resolve_package};
use doctor::run_doctor_cmd;
use fmt::run_fmt_cmd;
use library::{run_package_cmd, run_publish_cmd};
use migrate::run_migrate_cmd;
use new::{run_init_cmd, run_new_cmd};
use package::{build_appimage, build_deb, build_desktop_dir, build_dmg, build_nsis, build_web};
use test_cmd::run_test_cmd;
use transpile::transpile_workspace;
use watch::{HotLoopOpts, HotMode, run_hot_loop, with_web_features};
use web_dev::run_web_dev;
use workshop_channel::{ServedElsewhere, ServerClaim, WorkshopChannel};
use workshop_export::run_workshop_export;

/// Dispatches a `cargo telar` invocation to its subcommand.
pub fn run(args: Vec<String>) {
    let cli = Cli::parse_from(std::iter::once("cargo-telar".to_string()).chain(args));
    let command = cli.command.unwrap_or_else(default_dev_command);
    let workshop = match &command {
        TelarCommand::Preview(args) => match hold_workshop(args) {
            Some(workshop) => workshop,
            None => return,
        },
        _ => HeldWorkshop::default(),
    };
    // The one place `cargo telar` prepares a build, rather than at each of the nine sites that spawn `cargo`; the ones excluded here compile nothing or prepare for themselves. Bake first: a `src:"…"` transpiles against the artifact the bake writes.
    if !matches!(
        command,
        TelarCommand::New(_)
            | TelarCommand::Init(_)
            | TelarCommand::Doctor
            | TelarCommand::Fmt(_)
            | TelarCommand::Migrate(_)
            | TelarCommand::Bake
            | TelarCommand::Transpile
            | TelarCommand::Package(_)
            | TelarCommand::Publish(_)
    ) {
        if !bake_workspace() {
            drop(workshop);
            std::process::exit(1);
        }
        transpile_workspace();
    }
    match command {
        TelarCommand::New(args) => run_new_cmd(args),
        TelarCommand::Init(args) => run_init_cmd(args),
        TelarCommand::Dev(args) => run_dev_cmd(args),
        TelarCommand::Preview(args) => run_preview_cmd(args, workshop),
        TelarCommand::Build(args) => run_build_cmd(args),
        TelarCommand::Test(args) => run_test_cmd(args),
        TelarCommand::Check(args) => run_check_cmd(args),
        TelarCommand::Bake => {
            if !bake_workspace() {
                std::process::exit(1);
            }
        }
        TelarCommand::Transpile => {
            let baked = bake_workspace();
            if !transpile_workspace() || !baked {
                std::process::exit(1);
            }
        }
        TelarCommand::Package(args) => run_package_cmd(args),
        TelarCommand::Publish(args) => run_publish_cmd(args),
        TelarCommand::Doctor => run_doctor_cmd(),
        TelarCommand::Fmt(args) => run_fmt_cmd(args),
        TelarCommand::Migrate(args) => run_migrate_cmd(args),
    }
}

// No subcommand behaves like `cargo telar dev` with default flags.
fn default_dev_command() -> TelarCommand {
    TelarCommand::Dev(DevArgs {
        hot: HotArgs {
            common: CommonArgs {
                package: None,
                features: None,
                target: None,
                backend: None,
                renderer: None,
                cargo_args: vec![],
            },
            release: false,
            no_hot_reload: false,
            port: None,
        },
        devtools: None,
    })
}

/// What every compiling subcommand works out before it does anything of its own: the cargo invocation, the resolved configuration, and whether this run ends up in the terminal.
///
/// It was written out four times. `cargo telar test` is the one that legitimately does not want it — it drops `--backend` and `--renderer` on purpose, because the value is read through `option_env!` and setting it would change the build fingerprint for nothing — so it keeps its own preamble and says why.
struct BuildPlan {
    cargo_args: Vec<String>,
    config: TelarSection,
    target: Target,
    renderer: Option<WebRenderer>,
    /// Whether the application will run in the terminal this command was launched from, which only `--target tui` makes true.
    terminal: bool,
}

impl BuildPlan {
    /// `select_frontend` is what names the feature the target needs and points the binary at it, so it runs for every command rather than the two that remembered.
    fn new(common: CommonArgs, release: bool) -> Self {
        let CommonArgs {
            package,
            features,
            target,
            backend,
            renderer,
            cargo_args: extra,
        } = common;
        let mut cargo_args = build_cargo_args(&package, release, &features);
        cargo_args.extend(extra);
        if matches!(target, Some(Target::Android)) {
            cargo_args.push("--android".to_string());
        }
        let terminal = select_frontend(target, renderer, &mut cargo_args);
        let mut config = load_config(&cargo_args);
        if let Some(backend) = backend {
            config.backend = Some(backend.into());
        }
        Self {
            cargo_args,
            config,
            target: target.unwrap_or(Target::Desktop),
            renderer,
            terminal,
        }
    }
}

fn run_dev_cmd(args: DevArgs) {
    let DevArgs { hot, devtools } = args;
    let HotArgs {
        common,
        release,
        no_hot_reload,
        port,
    } = hot;
    let mut plan = BuildPlan::new(common, release);
    // CLI `--devtools off` overrides any config-file setting.
    if let Some(devtools) = devtools {
        plan.config.dev.devtools = Some(matches!(devtools, DevtoolsArg::On));
    }
    if plan.target == Target::Web {
        run_web_dev(
            with_web_features(&HotMode::Dev, plan.cargo_args),
            plan.config,
            port,
            plan.renderer,
            None,
            None,
        );
    }
    let terminal = plan.terminal;
    run_hot_loop(
        HotMode::Dev,
        HotLoopOpts {
            args: plan.cargo_args,
            app_args: Vec::new(),
            config: plan.config,
            // The hot-reload host opens a window of its own, so an app running in the terminal restarts on a change instead. Reloading in place is the only thing lost: the rebuild is the same one.
            no_hot_reload: no_hot_reload || terminal,
            workshop: None,
        },
    );
}

fn run_preview_cmd(args: PreviewArgs, workshop: HeldWorkshop) {
    let PreviewArgs {
        hot,
        preview,
        component,
        list,
        png,
        build,
        base,
    } = args;
    let HotArgs {
        common,
        release,
        no_hot_reload,
        port,
    } = hot;
    // Makes the generated entrypoint print one preview id per line and exit instead of opening a window.
    if list {
        run_preview_once(&common, release, "previews", PREVIEW_LIST_VAR, "1");
    }
    // The only answer for a shell with no display, and what a golden-image run compares.
    if let Some(dir) = &png {
        run_preview_once(
            &common,
            release,
            "preview-headless",
            "TELAR_PREVIEW_PNG",
            &dir.display().to_string(),
        );
    }
    if let Some(dir) = &build {
        run_workshop_export(common, dir, base.as_deref());
    }
    let plan = BuildPlan::new(common, release);
    if plan.target == Target::Web {
        if component.is_some() {
            eprintln!(
                "[cargo-telar] note: a page is started by its address alone, which carries no `--component`; search the workshop's sidebar for it instead."
            );
        }
        let opening = preview.as_deref().map(workshop_channel::location_of);
        run_web_dev(
            with_web_features(&HotMode::Preview, plan.cargo_args),
            plan.config,
            port,
            plan.renderer,
            opening.as_deref(),
            workshop.page,
        );
    }
    // The preview host process inherits our env, and reads both of these as it starts.
    if let Some(component) = &component {
        // SAFETY: single-threaded at this point (set before any threads/spawns are created).
        unsafe { std::env::set_var(PREVIEW_COMPONENT_VAR, component) };
    }
    let mut app_args = Vec::new();
    match preview {
        Some(id) if workshop_channel::is_preview_id(&id) => {
            // SAFETY: as above.
            unsafe { std::env::set_var(PREVIEW_ID_VAR, id) };
        }
        // A link carries args and a view as well as a preview, which only the workshop reads, from the location the app is started at.
        Some(link) => app_args = vec!["--location".to_string(), link],
        None => {}
    }
    // A window draws the previews itself, so the plan's `renderer`, which picks how a page draws, goes unread here.
    run_hot_loop(
        HotMode::Preview,
        HotLoopOpts {
            args: plan.cargo_args,
            app_args,
            config: plan.config,
            no_hot_reload,
            workshop: workshop.window,
        },
    );
}

/// What a `cargo telar preview` holds from before it builds anything until it ends, so a later one finds the workshop it runs rather than starting a second: the channel a window takes requests on, or the claim on the page's server file.
#[derive(Default)]
struct HeldWorkshop {
    window: Option<WorkshopChannel>,
    page: Option<ServerClaim>,
}

/// What this `cargo telar preview` holds for the workshop it is about to run, or `None` when one already runs for the package and the request was handed to it. Asked before anything is built: a workshop that is already running needs nothing rebuilt.
fn hold_workshop(args: &PreviewArgs) -> Option<HeldWorkshop> {
    if args.list || args.png.is_some() || args.build.is_some() {
        return Some(HeldWorkshop::default());
    }
    let common = &args.hot.common;
    let resolved = resolve_package(&build_cargo_args(&common.package, false, &None));
    let (root, package) = (&resolved.workspace_root, resolved.name());
    if common.target == Some(Target::Web) {
        return match ServerClaim::claim(&workshop_server_file(root, &package)) {
            Ok(claim) => Some(HeldWorkshop {
                page: Some(claim),
                ..HeldWorkshop::default()
            }),
            Err(session) => {
                show_served_workshop(&session, args.preview.as_deref());
                None
            }
        };
    }
    let file = workshop_channel_file(root, &package);
    if open_in_running_window(&file, args) {
        return None;
    }
    // Only a hot-reload session in a window hands requests on, and only a cdylib reloads.
    let takes_requests = matches!(common.target, None | Some(Target::Desktop))
        && !args.hot.no_hot_reload
        && resolved.produces_cdylib;
    Some(HeldWorkshop {
        window: takes_requests
            .then(|| WorkshopChannel::open(file))
            .flatten(),
        ..HeldWorkshop::default()
    })
}

/// Hands the request to the window a running `cargo telar preview` already shows for the package through the channel at `file`, and answers whether there was one: moved to the preview `args` names, or brought forward when it names none.
fn open_in_running_window(file: &Path, args: &PreviewArgs) -> bool {
    // A `--component` narrows the list a workshop starts with, which one already running cannot be asked to do.
    if args.preview.is_none() && args.component.is_some() {
        return false;
    }
    match &args.preview {
        Some(target) => {
            let opened = workshop_channel::goto(file, &workshop_channel::location_of(target));
            if opened {
                eprintln!("[cargo-telar] Opened {target} in the running workshop.");
            }
            opened
        }
        None => {
            let focused = workshop_channel::focus(file);
            if focused {
                eprintln!("[cargo-telar] Brought the running workshop forward.");
            }
            focused
        }
    }
}

/// Says where the page another `cargo telar preview --target web` serves for the package is, and where it opens `preview`, or that it is still starting. A page is not moved, since the browser showing it holds its address.
fn show_served_workshop(session: &ServedElsewhere, preview: Option<&str>) {
    let address = match session {
        ServedElsewhere::Serving(address) => address,
        ServedElsewhere::Starting { pid } => {
            let process = pid.map_or(String::new(), |pid| format!(" (process {pid})"));
            eprintln!(
                "[cargo-telar] The workshop is already starting in another session{process}, which prints its address once it serves."
            );
            return;
        }
    };
    eprintln!("[cargo-telar] The workshop is already served at {address}");
    if let Some(target) = preview {
        eprintln!(
            "[cargo-telar] Open {}",
            web_dev::address_under(address, &workshop_channel::location_of(target))
        );
    }
}

/// Runs the app binary once with `var` set, under the `telar` feature that makes it answer, and exits with its code.
///
/// The feature is the half that was missing. These entry points are reached through an environment variable the generated `run()` reads, and that `run()` only reads one in a build carrying previews — so without naming a feature here the binary starts the application instead, which is what `--png` would have done for as long as the feature behind it existed.
fn run_preview_once(
    common: &CommonArgs,
    release: bool,
    feature: &str,
    var: &str,
    value: &str,
) -> ! {
    let status = preview_once_command(common, release, feature)
        .env(var, value)
        .status()
        .expect("[cargo-telar] failed to invoke cargo");
    std::process::exit(status.code().unwrap_or(1));
}

fn preview_once_command(common: &CommonArgs, release: bool, feature: &str) -> Command {
    let mut cargo_args = vec!["run".to_string()];
    cargo_args.extend(build_cargo_args(&common.package, release, &common.features));
    cargo_args.extend(common.cargo_args.clone());
    // A host binary either way, so a browser target names nothing here; the rest keep the terminal listing and the PNG render off the frontend the project does not build.
    if let Some(target) = common.target.filter(|target| *target != Target::Web) {
        let selected = frontend_args(target, common.renderer, &cargo_args);
        cargo_args.extend(selected);
    }
    let tooling = format!("telar/{feature}");
    config::warn_if_tooling_unlocked(&cargo_args, &[&tooling]);
    cargo_args.push("--features".to_string());
    cargo_args.push(tooling);
    let mut command = Command::new("cargo");
    command.args(&cargo_args);
    command
}

fn build_format_name(format: &BuildFormat) -> &'static str {
    match format {
        BuildFormat::Appimage => "appimage",
        BuildFormat::Deb => "deb",
        BuildFormat::Dmg => "dmg",
        BuildFormat::Nsis => "nsis",
        BuildFormat::Apk => "apk",
        BuildFormat::Dir => "dir",
    }
}

fn run_build_cmd(args: BuildArgs) -> ! {
    let BuildArgs {
        mut common,
        format,
        prerender,
    } = args;

    // All desktop formats reject `--target android`, and `--format apk` implies Android. Resolved before the plan is built, because the format can move the target and the plan is what the target decides.
    match &format {
        Some(
            fmt @ (BuildFormat::Deb | BuildFormat::Appimage | BuildFormat::Dmg | BuildFormat::Nsis),
        ) if common.target == Some(Target::Android) => {
            eprintln!(
                "[cargo-telar] `--format {}` is desktop-only; drop `--target android` (use `--format apk` for Android).",
                build_format_name(fmt)
            );
            std::process::exit(2);
        }
        Some(BuildFormat::Apk) => common.target = Some(Target::Android),
        Some(BuildFormat::Dir) if common.target == Some(Target::Android) => {
            eprintln!(
                "[cargo-telar] `--format dir` is desktop-only; use `--target android` (or `--format apk`) for Android."
            );
            std::process::exit(2);
        }
        _ => {}
    }
    if common.target == Some(Target::Web) && format.is_some() {
        eprintln!(
            "[cargo-telar] `--format` is for native installers; a web build is a directory of files."
        );
        std::process::exit(2);
    }
    // Build always implies --release.
    let plan = BuildPlan::new(common, true);
    if prerender && plan.target != Target::Web {
        eprintln!(
            "[cargo-telar] `--prerender` writes the pages of a site; build with `--target web`."
        );
        std::process::exit(2);
    }
    if plan.target == Target::Web {
        build_web(plan.cargo_args, plan.config, true, plan.renderer, prerender);
    }
    if plan.target == Target::Android {
        build_android_package(plan.cargo_args, plan.config)
    } else {
        match format {
            Some(BuildFormat::Deb) => build_deb(plan.cargo_args, plan.config),
            Some(BuildFormat::Appimage) => build_appimage(plan.cargo_args, plan.config),
            Some(BuildFormat::Dmg) => build_dmg(plan.cargo_args, plan.config),
            Some(BuildFormat::Nsis) => build_nsis(plan.cargo_args, plan.config),
            _ => build_desktop_dir(plan.cargo_args, plan.config),
        }
    }
}

fn build_cargo_args(
    package: &Option<String>,
    release: bool,
    features: &Option<String>,
) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(pkg) = package {
        args.push("-p".to_string());
        args.push(pkg.clone());
    }
    if release {
        args.push("--release".to_string());
    }
    if let Some(features) = features {
        args.push("--features".to_string());
        args.push(features.clone());
    }
    args
}

/// The flags that build exactly the frontend `target` names, for the invocation `cargo_args` is becoming.
///
/// A package that declares a feature for the target gets it named on its own, with `--no-default-features` and the rest of its defaults re-named alongside: `default` is where a project says which frontend it builds, so asking for another one has to turn that one off. Adding it on top instead is what made `--target tui` compile a desktop stack beside the terminal one — build time nobody asked for on a desktop, and the whole of the failure under Termux, where `platform-desktop` is `cfg`'d out of the graph and the window it pays for cannot exist.
///
/// A package that declares no such feature reaches the frontend through `telar/` and keeps its defaults, so any project builds for a target without first declaring one.
fn frontend_args(
    target: Target,
    renderer: Option<WebRenderer>,
    cargo_args: &[String],
) -> Vec<String> {
    let mut args = Vec::new();
    resolve_package(cargo_args)
        .frontend_feature(target.feature(renderer))
        .push_to(&mut args);
    args
}

/// Turns on the frontend a `--target` named and tells the app to start on it, returning whether the app will run in this terminal.
///
/// No target is a project left to say it in its manifest, which is where it says everything else about the build: nothing is named here, and the terminal is still answered for, because a package whose `default` is the terminal and no window runs in one whether or not anybody passed a flag.
///
/// The environment variable is what picks between the frontends a build ends up with, which is only still a question in the case where the package declared no feature for the target: there it kept a windowed default and the terminal rides along beside it.
fn select_frontend(
    target: Option<Target>,
    renderer: Option<WebRenderer>,
    cargo_args: &mut Vec<String>,
) -> bool {
    // A browser build names its own frontend in `build_web_bundle`, which is also where the wasm target and the profile that go with it are decided.
    if let Some(target) = target.filter(|target| *target != Target::Web) {
        let selected = frontend_args(target, renderer, cargo_args);
        cargo_args.extend(selected);
    }
    let terminal = match target {
        Some(target) => target == Target::Tui,
        None => resolve_package(cargo_args).defaults_to_terminal(),
    };
    if terminal {
        // SAFETY: single-threaded at this point — set before any child is spawned or any thread started.
        unsafe { std::env::set_var("TELAR_TARGET", "tui") };
    }
    terminal
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;
