use clap::Parser;

use super::Cli;

fn rejection(args: &[&str]) -> String {
    Cli::try_parse_from(std::iter::once("cargo-telar").chain(args.iter().copied()))
        .err()
        .expect("the value is not accepted")
        .to_string()
}

#[test]
fn an_unknown_build_format_is_rejected_listing_the_valid_ones() {
    let error = rejection(&["build", "--format", "zip"]);
    for format in ["appimage", "deb", "dmg", "nsis", "apk", "dir"] {
        assert!(error.contains(format), "{format} missing from: {error}");
    }
}

#[test]
fn an_unknown_backend_is_rejected_listing_the_valid_ones() {
    let error = rejection(&["dev", "--backend", "vulkan"]);
    for backend in ["auto", "hardware", "software"] {
        assert!(error.contains(backend), "{backend} missing from: {error}");
    }
}
