use std::path::{Path, PathBuf};
use std::process::Command;

/// Builds the guest crate `tests/fixtures/<name>` under `target_tmp`, in a target directory of its own: the `cargo test` running this holds the workspace's until it returns. Returns the library it left.
pub(crate) fn build_guest_fixture(target_tmp: &Path, name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = manifest_dir.join("tests/fixtures").join(name);
    let target_dir = target_tmp.join(name);
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
        "the fixture {name} did not build: {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    target_dir.join("debug").join(format!(
        "{}{}{}",
        std::env::consts::DLL_PREFIX,
        name.replace('-', "_"),
        std::env::consts::DLL_SUFFIX
    ))
}
