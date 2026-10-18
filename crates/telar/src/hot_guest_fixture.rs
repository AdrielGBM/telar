use std::path::{Path, PathBuf};
use std::process::Command;

/// Builds the fixture under `target_tmp`, in a target directory of its own: the `cargo test` running this holds the workspace's until it returns. Returns the library it left.
pub(crate) fn build_hot_guest(target_tmp: &Path) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = manifest_dir.join("tests/fixtures/hot-panic-guest");
    let target_dir = target_tmp.join("hot-panic");
    // Offline, the versions the workspace already fetched are the only ones the fixture can resolve to, and the same ones let it reuse what earlier runs compiled.
    std::fs::copy(
        manifest_dir.join("../../Cargo.lock"),
        fixture.join("Cargo.lock"),
    )
    .expect("copy Cargo.lock");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args(["build", "--lib", "--manifest-path"])
        .arg(fixture.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", &target_dir)
        .output()
        .expect("start cargo");
    assert!(
        output.status.success(),
        "the fixture did not build: {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    target_dir.join("debug").join(format!(
        "{}hot_panic_guest{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ))
}
