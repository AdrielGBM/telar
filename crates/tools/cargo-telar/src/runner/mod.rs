use std::process::Command;

use clap::Parser;

mod android;
mod check;
mod cli;
mod config;
mod diagnostics;
mod doctor;
mod fmt;
mod migrate;
mod package;
mod watch;
mod web_dev;

use android::build_android_package;
use check::run_check_cmd;
use cli::{
    BuildArgs, BuildFormat, Cli, CommonArgs, DevArgs, DevtoolsArg, HotArgs, PreviewArgs, Target,
    TelarCommand, TestArgs,
};
use config::load_config;
use doctor::run_doctor_cmd;
use fmt::run_fmt_cmd;
use migrate::run_migrate_cmd;
use package::{build_appimage, build_deb, build_desktop_dir, build_dmg, build_nsis, build_web};
use watch::{HotLoopOpts, HotMode, run_hot_loop};
use web_dev::run_web_dev;

pub fn run(args: Vec<String>) {
    let cli = Cli::parse_from(std::iter::once("cargo-telar".to_string()).chain(args));
    match cli.command.unwrap_or_else(default_dev_command) {
        TelarCommand::Dev(args) => run_dev_cmd(args),
        TelarCommand::Preview(args) => run_preview_cmd(args),
        TelarCommand::Build(args) => run_build_cmd(args),
        TelarCommand::Test(args) => run_test_cmd(args),
        TelarCommand::Check(args) => run_check_cmd(args),
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
                target: Target::Desktop,
                backend: None,
                cargo_args: vec![],
            },
            release: false,
            no_hot_reload: false,
        },
        devtools: None,
    })
}

fn run_dev_cmd(args: DevArgs) {
    let DevArgs { hot, devtools } = args;
    let HotArgs {
        common,
        release,
        no_hot_reload,
    } = hot;
    let CommonArgs {
        package,
        features,
        target,
        backend,
        cargo_args: extra,
    } = common;
    let mut cargo_args = build_cargo_args(&package, release, &features);
    cargo_args.extend(extra);
    if matches!(target, Target::Android) {
        cargo_args.push("--android".to_string());
    }
    let terminal = select_frontend(target, &mut cargo_args);
    let mut config = load_config(&cargo_args);
    if let Some(backend) = backend {
        config.backend = Some(backend);
    }
    // CLI `--devtools off` overrides any config-file setting.
    if let Some(devtools) = devtools {
        config.dev.get_or_insert_with(Default::default).devtools =
            Some(matches!(devtools, DevtoolsArg::On));
    }
    if target == Target::Web {
        run_web_dev(cargo_args, config, WEB_DEV_PORT);
    }
    run_hot_loop(
        HotMode::Dev,
        HotLoopOpts {
            args: cargo_args,
            config,
            // The hot-reload host opens a window of its own, so an app running in the terminal restarts on
            // a change instead. Reloading in place is the only thing lost: the rebuild is the same one.
            no_hot_reload: no_hot_reload || terminal,
        },
    );
}

fn run_preview_cmd(args: PreviewArgs) {
    let PreviewArgs {
        hot,
        component,
        list,
    } = args;
    // The preview host process inherits our env; it filters PreviewEntries by this when set.
    if let Some(component) = &component {
        // SAFETY: single-threaded at this point (set before any threads/spawns are created).
        unsafe { std::env::set_var("TELAR_PREVIEW_COMPONENT", component) };
    }
    let HotArgs {
        common,
        release,
        no_hot_reload,
    } = hot;
    if list {
        // TELAR_PREVIEW_LIST makes the generated entrypoint print "component\tpreview" lines and exit instead of opening a window.
        let mut cargo_args = vec!["run".to_string()];
        cargo_args.extend(build_cargo_args(&common.package, release, &common.features));
        cargo_args.extend(common.cargo_args);
        let status = Command::new("cargo")
            .args(&cargo_args)
            .env("TELAR_PREVIEW_LIST", "1")
            .status()
            .expect("[cargo-telar] failed to invoke cargo");
        std::process::exit(status.code().unwrap_or(1));
    }
    let CommonArgs {
        package,
        features,
        target,
        backend,
        cargo_args: extra,
    } = common;
    let mut cargo_args = build_cargo_args(&package, release, &features);
    cargo_args.extend(extra);
    if matches!(target, Target::Android) {
        cargo_args.push("--android".to_string());
    }
    let mut config = load_config(&cargo_args);
    if let Some(backend) = backend {
        config.backend = Some(backend);
    }
    run_hot_loop(
        HotMode::Preview,
        HotLoopOpts {
            args: cargo_args,
            config,
            no_hot_reload,
        },
    );
}

fn run_test_cmd(args: TestArgs) -> ! {
    let TestArgs { common, release } = args;
    let CommonArgs {
        package,
        features,
        target,
        backend,
        cargo_args: extra,
    } = common;
    if matches!(target, Target::Android) {
        eprintln!(
            "[cargo-telar] `cargo telar test` renders on the host; --target android is not supported."
        );
        std::process::exit(2);
    }
    // Run the app binary in test mode: TELAR_TEST makes the generated entrypoint render every preview headlessly and exit non-zero on any failure, instead of opening a window.
    let mut cargo_args = vec!["run".to_string()];
    cargo_args.extend(build_cargo_args(&package, release, &features));
    cargo_args.extend(extra);
    // No TELAR_RENDERER_BACKEND: the test host never instantiates a renderer, and that value is read via option_env!, so setting it would change the build fingerprint and force a needless recompile.
    let _ = backend;
    eprintln!("[cargo-telar] Running component render tests...");
    let status = Command::new("cargo")
        .args(&cargo_args)
        .env("TELAR_TEST", "1")
        .status()
        .expect("[cargo-telar] failed to invoke cargo");
    std::process::exit(status.code().unwrap_or(1));
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
    let BuildArgs { common, format } = args;
    let CommonArgs {
        package,
        features,
        target,
        backend,
        cargo_args: extra,
    } = common;
    let mut android = matches!(target, Target::Android);
    let terminal = target == Target::Tui;

    if target == Target::Web {
        if format.is_some() {
            eprintln!(
                "[cargo-telar] `--format` is for native installers; a web build is a directory of files."
            );
            std::process::exit(2);
        }
        let mut cargo_args = build_cargo_args(&package, true, &features);
        cargo_args.extend(extra);
        build_web(cargo_args, load_config(&[]), true);
    }

    // All desktop formats reject `--target android`; `--format apk` implies Android. Host-OS gating (dmg → macOS, nsis → Windows) happens in each build fn since rsx does not cross-compile.
    match &format {
        Some(
            fmt @ (BuildFormat::Deb | BuildFormat::Appimage | BuildFormat::Dmg | BuildFormat::Nsis),
        ) if android => {
            eprintln!(
                "[cargo-telar] `--format {}` is desktop-only; drop `--target android` (use `--format apk` for Android).",
                build_format_name(fmt)
            );
            std::process::exit(2);
        }
        Some(BuildFormat::Apk) => android = true,
        Some(BuildFormat::Dir) if android => {
            eprintln!(
                "[cargo-telar] `--format dir` is desktop-only; use `--target android` (or `--format apk`) for Android."
            );
            std::process::exit(2);
        }
        _ => {}
    }

    // Build always implies --release.
    let mut cargo_args = build_cargo_args(&package, true, &features);
    cargo_args.extend(extra);
    if android {
        cargo_args.push("--android".to_string());
    }
    if terminal {
        select_frontend(target, &mut cargo_args);
    }
    let mut config = load_config(&cargo_args);
    if let Some(backend) = backend {
        config.backend = Some(backend);
    }

    if android {
        build_android_package(cargo_args, config)
    } else {
        match format {
            Some(BuildFormat::Deb) => build_deb(cargo_args, config),
            Some(BuildFormat::Appimage) => build_appimage(cargo_args, config),
            Some(BuildFormat::Dmg) => build_dmg(cargo_args, config),
            Some(BuildFormat::Nsis) => build_nsis(cargo_args, config),
            _ => build_desktop_dir(cargo_args, config),
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

/// Turns on the frontend `target` names and tells the app to start on it, returning whether the app will run
/// in this terminal.
///
/// The feature goes through `telar/` rather than a feature of the app's own, so any project reaches a
/// frontend without first declaring one; the environment variable is what picks between the frontends a
/// build ends up with, since a default build still has the windowed one compiled in.
fn select_frontend(target: Target, cargo_args: &mut Vec<String>) -> bool {
    if target != Target::Tui {
        return false;
    }
    cargo_args.push("--features".to_string());
    cargo_args.push("telar/tui".to_string());
    // SAFETY: single-threaded at this point — set before any child is spawned or any thread started.
    unsafe { std::env::set_var("TELAR_TARGET", "tui") };
    true
}

/// Where `cargo telar dev --target web` serves from. Fixed rather than chosen: a page reloaded by hand, a
/// bookmark and a second terminal all have to name the same address.
const WEB_DEV_PORT: u16 = 8080;
