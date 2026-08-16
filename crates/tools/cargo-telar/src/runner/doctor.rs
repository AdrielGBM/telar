use std::process::Command;

use super::android::{android_sdk_root, installed_android_platforms, resolve_ndk_root};
use super::config::{backend_as_str, find_package_dir, load_config, manifest_has_telar};

struct Doctor {
    warnings: usize,
    errors: usize,
}

impl Doctor {
    fn new() -> Self {
        Self {
            warnings: 0,
            errors: 0,
        }
    }

    fn section(&self, title: &str) {
        println!("\n{title}");
    }

    fn info(&self, label: &str, detail: &str) {
        println!("  \u{2022} {label}: {detail}");
    }

    fn ok(&self, label: &str, detail: &str) {
        println!("  \u{2713} {label}: {detail}");
    }

    fn warn(&mut self, label: &str, detail: &str) {
        self.warnings += 1;
        println!("  \u{26a0} {label}: {detail}");
    }

    fn fail(&mut self, label: &str, detail: &str) {
        self.errors += 1;
        println!("  \u{2717} {label}: {detail}");
    }
}

fn command_first_line(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stream = if output.stdout.is_empty() {
        output.stderr
    } else {
        output.stdout
    };
    let text = String::from_utf8_lossy(&stream);
    Some(text.lines().next().unwrap_or("").trim().to_string())
}

/// Whether a linker faster than the default is installed. The name probed is the driver's, not the
/// package's — `-fuse-ld=lld` makes the C compiler look for `ld.lld` on PATH, so probing `lld` would report
/// one that the link would then fail to find.
fn fast_linker() -> Option<&'static str> {
    [("mold", "mold"), ("lld", "ld.lld")]
        .into_iter()
        .find(|(_, program)| command_first_line(program, &["--version"]).is_some())
        .map(|(flavor, _)| flavor)
}

fn rustup_has_target(target: &str) -> Option<bool> {
    let output = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Some(text.lines().any(|line| line.trim() == target))
}

pub(crate) fn run_doctor_cmd() -> ! {
    let mut doc = Doctor::new();
    println!("cargo telar doctor");

    doc.section("Toolchain");
    match command_first_line("cargo", &["--version"]) {
        Some(v) => doc.ok("cargo", &v),
        None => doc.fail("cargo", "not found on PATH"),
    }
    match command_first_line("rustc", &["--version"]) {
        Some(v) => doc.ok("rustc", &v),
        None => doc.fail("rustc", "not found on PATH"),
    }
    // Reported, never imposed: the linker a machine builds with belongs to its own cargo config, and swapping it here would be choosing for the whole workspace.
    if cfg!(target_os = "linux") {
        match fast_linker() {
            Some(found) => doc.info("fast linker installed", found),
            None => doc.info(
                "fast linker installed",
                "none — `mold` or `lld` shortens the relink in front of every hot reload (see \"Build profiles\" in the README)",
            ),
        }
    }

    doc.section("Project");
    let cwd = std::env::current_dir().unwrap_or_default();
    match telar_transpiler::find_workspace_root(&cwd) {
        Some(root) => doc.info("workspace root", &root.display().to_string()),
        None => doc.info("workspace root", "none (standalone package)"),
    }
    let config = load_config(&[]);
    let package_dir = find_package_dir(&[]);
    let telar_toml = package_dir.join("telar.toml");
    if telar_toml.exists() {
        doc.ok("telar.toml", &telar_toml.display().to_string());
    } else {
        doc.info("telar.toml", "not found (using defaults)");
    }
    let has_manifest_telar = manifest_has_telar(&package_dir);
    doc.info(
        "[package.metadata.telar]",
        if has_manifest_telar {
            "present"
        } else {
            "not set"
        },
    );
    doc.info(
        "config precedence",
        "CLI flags > telar.toml > [package.metadata.telar] > defaults",
    );

    doc.section("Desktop");
    let backend = config
        .backend
        .map(backend_as_str)
        .unwrap_or("auto (default)");
    doc.info("configured backend", backend);
    doc.info("software renderer", "always available");
    doc.info(
        "hardware renderer",
        "needs a working GPU/driver, verified at runtime",
    );

    doc.section("Packaging");
    doc.info("dir/apk", "built-in");
    // Only the current host's native-installer tools are relevant: rsx does not cross-compile.
    if cfg!(target_os = "linux") {
        match command_first_line("dpkg-deb", &["--version"]) {
            Some(v) => doc.ok("dpkg-deb", &v),
            None => doc.info("dpkg-deb", "not found (needed for --format deb)"),
        }
        match command_first_line("appimagetool", &["--version"]) {
            Some(v) => doc.ok("appimagetool", &v),
            None => doc.info("appimagetool", "not found (needed for --format appimage)"),
        }
    }
    if cfg!(target_os = "macos") {
        match command_first_line("hdiutil", &["info"]) {
            Some(_) => doc.ok("hdiutil", "available"),
            None => doc.info("hdiutil", "not found (needed for --format dmg)"),
        }
    }
    if cfg!(target_os = "windows") {
        match command_first_line("makensis", &["/VERSION"]) {
            Some(v) => doc.ok("makensis", &v),
            None => doc.info("makensis", "not found (needed for --format nsis)"),
        }
    }

    doc.section("Android (only needed for --target android)");
    match android_sdk_root() {
        Some(sdk) if sdk.exists() => {
            doc.ok("Android SDK", &sdk.display().to_string());
            let platforms = installed_android_platforms(&sdk);
            if platforms.is_empty() {
                doc.warn(
                    "installed SDK platforms",
                    "none found — `sdkmanager \"platforms;android-36\"`",
                );
            } else {
                let list = platforms
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                doc.info("installed SDK platforms", &list);
            }
        }
        Some(sdk) => doc.warn(
            "Android SDK",
            &format!("{} (path does not exist)", sdk.display()),
        ),
        None => doc.warn("Android SDK", "not set (ANDROID_HOME / ANDROID_SDK_ROOT)"),
    }
    match resolve_ndk_root() {
        Some(ndk) => doc.ok("Android NDK", &ndk),
        None => doc.warn(
            "Android NDK",
            "not found (ANDROID_NDK_ROOT or $ANDROID_HOME/ndk/*)",
        ),
    }
    match command_first_line("cargo", &["apk", "--version"]) {
        Some(v) => doc.ok("cargo-apk", &v),
        None => doc.warn("cargo-apk", "not found (`cargo install cargo-apk`)"),
    }
    match rustup_has_target("aarch64-linux-android") {
        Some(true) => doc.ok("rust target aarch64-linux-android", "installed"),
        Some(false) => doc.warn(
            "rust target aarch64-linux-android",
            "missing (`rustup target add aarch64-linux-android`)",
        ),
        None => doc.warn(
            "rust target aarch64-linux-android",
            "rustup not found, cannot verify",
        ),
    }
    match command_first_line("adb", &["--version"]) {
        Some(v) => doc.ok("adb", &v),
        None => doc.warn("adb", "not found (Android platform-tools)"),
    }

    println!();
    if doc.errors > 0 {
        println!(
            "\u{2717} {} error(s), {} warning(s)",
            doc.errors, doc.warnings
        );
        std::process::exit(1);
    } else if doc.warnings > 0 {
        println!("\u{26a0} {} warning(s)", doc.warnings);
        std::process::exit(0);
    }
    println!("\u{2713} everything looks good");
    std::process::exit(0);
}
