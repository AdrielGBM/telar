use super::*;

fn selected(args: &[&str]) -> Option<String> {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    selected_package(&args).map(str::to_string)
}

#[test]
fn every_spelling_cargo_accepts_selects_the_package() {
    for spelling in [
        &["-p", "sandbox"][..],
        &["-psandbox"],
        &["-p=sandbox"],
        &["--package", "sandbox"],
        &["--package=sandbox"],
    ] {
        assert_eq!(
            selected(spelling).as_deref(),
            Some("sandbox"),
            "{spelling:?}"
        );
    }
}

#[test]
fn the_package_is_found_among_other_flags() {
    assert_eq!(
        selected(&[
            "--release",
            "--features",
            "a,b",
            "--package=sandbox",
            "--all-features"
        ])
        .as_deref(),
        Some("sandbox")
    );
}

#[test]
fn no_package_flag_selects_none() {
    assert_eq!(
        selected(&["--release", "--profile", "web", "--features=p"]),
        None
    );
}

#[test]
fn arguments_after_a_double_dash_belong_to_the_app() {
    assert_eq!(selected(&["--release", "--", "-p", "sandbox"]), None);
}
