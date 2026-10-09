use super::*;

/// Read from the `cargo-telar` package itself, which declares no `[features]` at all — the shape of a project that has named no frontend of its own, and the one case where a target has to be reached through the dependency.
#[test]
fn a_package_that_declares_no_frontend_reaches_one_through_telar() {
    assert_eq!(
        frontend_args(Target::Tui, None, &[]),
        vec!["--features", "telar/tui"]
    );
    assert_eq!(
        frontend_args(Target::Desktop, None, &[]),
        vec!["--features", "telar/desktop"]
    );
}

fn preview_args(argv: &[&str]) -> Result<PreviewArgs, clap::Error> {
    let cli = Cli::try_parse_from(
        ["cargo-telar", "preview"]
            .iter()
            .chain(argv)
            .map(|arg| arg.to_string()),
    )?;
    match cli.command {
        Some(TelarCommand::Preview(args)) => Ok(args),
        _ => panic!("not a preview command"),
    }
}

#[test]
fn a_preview_is_named_by_its_id_or_a_copied_link() {
    let args = preview_args(&["telar_components--button--primary"]).unwrap();
    assert_eq!(
        args.preview.as_deref(),
        Some("telar_components--button--primary")
    );
    let args = preview_args(&["/preview/demo--card--a?args=label:%22Hi%22", "-p", "demo"]).unwrap();
    assert_eq!(
        args.preview.as_deref(),
        Some("/preview/demo--card--a?args=label:%22Hi%22")
    );
    assert_eq!(args.hot.common.package.as_deref(), Some("demo"));
}

#[test]
fn a_named_preview_cannot_be_listed_or_rendered() {
    assert!(preview_args(&["demo--card--a", "--list"]).is_err());
    assert!(preview_args(&["demo--card--a", "--png", "out"]).is_err());
    assert!(preview_args(&["--component", "Button"]).is_ok());
}
