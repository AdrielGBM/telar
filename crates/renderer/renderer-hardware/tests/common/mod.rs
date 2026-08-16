//! Shared guard for the GPU suite.

// Each test binary compiles this module whole but uses only the helpers it needs.
#![allow(dead_code)]

/// Reports that a GPU test is skipping for want of an adapter — and fails instead when `TELAR_REQUIRE_GPU`
/// is set.
///
/// CI sets it on the leg that installs lavapipe, so a suite that quietly stopped covering the GPU reads as
/// red there rather than as fifteen passing tests that never ran. Everywhere else the absence is a real
/// answer and the test skips.
pub fn skip_without_gpu(what: &str, error: impl std::fmt::Debug) {
    assert!(
        !gpu_required(),
        "{what}: TELAR_REQUIRE_GPU is set, so an adapter was expected: {error:?}"
    );
    eprintln!("skipping {what}: no GPU adapter available: {error:?}");
}

/// An empty value counts as unset: a workflow that picks the variable per matrix leg still defines it as `""`
/// on the legs that do not want it, and reading that as "required" would fail every skip.
fn gpu_required() -> bool {
    std::env::var("TELAR_REQUIRE_GPU").is_ok_and(|v| !v.is_empty())
}
