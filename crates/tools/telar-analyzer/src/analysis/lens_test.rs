use super::*;
use telar_parser::parse;

fn host(component: &str) -> PreviewHost {
    PreviewHost {
        package_root: PathBuf::from("/work/app"),
        crate_name: "my_app".to_string(),
        component: component.to_string(),
    }
}

fn command(lens: &CodeLens) -> (&str, &str, Vec<String>) {
    let command = lens.command.as_ref().unwrap();
    let arguments = command
        .arguments
        .as_ref()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_string())
        .collect();
    (command.title.as_str(), command.command.as_str(), arguments)
}

#[test]
fn opens_each_preview_in_the_workshop_by_its_id() {
    let src =
        "[view]\ncol\n\n[preview \"Default\"]\ncol\n\n[preview \"Landing — full page\"]\nbox\n";
    let lenses = code_lenses(&parse(src).unwrap(), &host("card"));
    assert_eq!(lenses.len(), 2);
    assert_eq!(lenses[0].range.start.line, 3);
    assert_eq!(lenses[1].range.start.line, 6);
    assert_eq!(
        command(&lenses[0]),
        (
            "▶ Open in workshop",
            OPEN_IN_WORKSHOP,
            vec!["/work/app".to_string(), "my_app--card--default".to_string()]
        )
    );
    assert_eq!(command(&lenses[1]).2[1], "my_app--card--landing-full-page");
}

#[test]
fn a_play_zone_gets_a_lens_that_runs_it() {
    let src = "[view]\ncol\n\n[preview \"Bound\" args(n:1)]\ncol\n\n[play]\ncanvas.expect_text(\"1\")?;\n";
    let lenses = code_lenses(&parse(src).unwrap(), &host("counter"));
    assert_eq!(lenses.len(), 2);
    let (title, name, arguments) = command(&lenses[1]);
    assert_eq!((title, name), ("▶ Run play", RUN_PLAY));
    assert_eq!(arguments[1], "my_app--counter--bound");
    assert_eq!(lenses[1].range.start.line, 6, "on the [play] header");
}

/// The transpiler refuses these an id, so a lens would open a preview that does not exist.
#[test]
fn a_preview_with_no_id_of_its_own_gets_no_lens() {
    let src =
        "[view]\ncol\n\n[preview \"A b\"]\ncol\n\n[preview \"a-B\"]\ncol\n\n[preview \"—\"]\ncol\n";
    let lenses = code_lenses(&parse(src).unwrap(), &host("card"));
    assert_eq!(lenses.len(), 1);
    assert_eq!(command(&lenses[0]).2[1], "my_app--card--a-b");
}

#[test]
fn a_previews_file_previews_the_component_it_is_named_after() {
    let root = std::env::temp_dir().join(format!("telar-lens-{}", std::process::id()));
    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"rsx-fixture\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let file = src_dir.join("tally.previews.rsx");

    let host = PreviewHost::discover(&file).unwrap();
    assert_eq!(host.crate_name, "rsx_fixture");
    assert_eq!(host.component, "tally");

    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"rsx-fixture\"\nversion = \"0.1.0\"\n\n[lib]\nname = \"fixture_lib\"\n",
    )
    .unwrap();
    assert_eq!(
        PreviewHost::discover(&file).unwrap().crate_name,
        "fixture_lib",
        "`CARGO_CRATE_NAME` is the library's name"
    );
    let _ = std::fs::remove_dir_all(&root);
}
