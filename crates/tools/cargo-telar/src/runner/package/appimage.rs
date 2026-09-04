//! Packaging a Linux AppImage.

use std::process::Command;

use super::super::config::TelarConfig;
use super::{
    create_dir_or_exit, desktop_entry_file, dist_dir, run_bundler_tool, run_release_build,
    set_executable, stage_binary, write_or_exit,
};

// Minimal valid 1x1 transparent PNG; appimagetool requires an icon, and rsx apps carry their own assets so a real icon is unnecessary.
const MINIMAL_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x64, 0xf8, 0xcf, 0x50,
    0x0f, 0x00, 0x03, 0x86, 0x01, 0x80, 0x5a, 0x34, 0x7d, 0x6b, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

fn apprun_script(name: &str) -> String {
    format!(
        "#!/bin/sh\nHERE=\"$(dirname \"$(readlink -f \"$0\")\")\"\nexec \"$HERE/usr/bin/{name}\" \"$@\"\n"
    )
}

pub(crate) fn build_appimage(cargo_args: Vec<String>, config: TelarConfig) -> ! {
    let (bin_path, resolved) = run_release_build(cargo_args, config);
    let package_name = resolved.name();
    // appimagetool wants the host arch label (x86_64/aarch64), which is exactly what Rust reports.
    let arch = std::env::consts::ARCH;

    let dist_dir = dist_dir(&resolved.workspace_root);
    let appdir = dist_dir.join(format!("{package_name}.AppDir"));
    // Clear any previous AppDir so stale files never leak into the image.
    let _ = std::fs::remove_dir_all(&appdir);
    let bin_dir = appdir.join("usr").join("bin");
    create_dir_or_exit(&bin_dir);

    let apprun = appdir.join("AppRun");
    write_or_exit(&apprun, apprun_script(&package_name));
    set_executable(&apprun);
    write_or_exit(
        &appdir.join(format!("{package_name}.desktop")),
        desktop_entry_file(&package_name, true),
    );
    write_or_exit(&appdir.join(format!("{package_name}.png")), MINIMAL_PNG);
    stage_binary(&bin_path, &bin_dir.join(&package_name));

    let appimage_path = dist_dir.join(format!("{package_name}-{arch}.AppImage"));
    // The AppImage bundles only the binary (rsx embeds its assets) and relies on the host's wayland/vulkan libraries at runtime.
    let mut cmd = Command::new("appimagetool");
    cmd.arg(&appdir).arg(&appimage_path).env("ARCH", arch);
    run_bundler_tool(
        &mut cmd,
        "AppImage",
        &appimage_path,
        Some(
            "[cargo-telar] `appimagetool` is required for --format appimage but was not found on PATH. Get it from https://github.com/AppImage/appimagetool/releases (on NixOS run it via `nix shell nixpkgs#appimage-run`).",
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apprun_execs_the_bundled_binary() {
        let script = apprun_script("myapp");
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(script.contains("exec \"$HERE/usr/bin/myapp\" \"$@\""));
    }
}
