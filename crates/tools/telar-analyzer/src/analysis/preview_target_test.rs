use super::*;
use telar_parser::parse;

fn host(component: &str) -> PreviewHost {
    PreviewHost {
        package_root: PathBuf::from("/work/app"),
        crate_name: "my_app".to_string(),
        component: component.to_string(),
        workshop_channel: Some(PathBuf::from("/work/.telar/workshop/my-app.channel")),
    }
}

fn id_at(src: &str, line: u32) -> Option<String> {
    preview_at(&parse(src).unwrap(), src, line, &host("card")).map(|target| target.id)
}

const FILE: &str = "\
[view]
col

[preview \"Default\"]
col
  text \"a\"

[play]
canvas.expect_text(\"a\")?;

[preview \"Dark mode\" dark]
box

[style]
@pad
  padding:4
";

#[test]
fn the_cursor_names_the_preview_whose_zone_holds_it() {
    assert_eq!(
        id_at(FILE, 3).as_deref(),
        Some("my_app--card--default"),
        "on the header"
    );
    assert_eq!(
        id_at(FILE, 5).as_deref(),
        Some("my_app--card--default"),
        "in the body"
    );
    assert_eq!(
        id_at(FILE, 6).as_deref(),
        Some("my_app--card--default"),
        "on a blank line before [play]"
    );
    assert_eq!(
        id_at(FILE, 8).as_deref(),
        Some("my_app--card--default"),
        "in its [play]"
    );
    assert_eq!(id_at(FILE, 10).as_deref(), Some("my_app--card--dark-mode"));
    assert_eq!(id_at(FILE, 11).as_deref(), Some("my_app--card--dark-mode"));
}

#[test]
fn outside_every_preview_there_is_none() {
    assert_eq!(id_at(FILE, 0), None, "in [view]");
    assert_eq!(id_at(FILE, 1), None);
    assert_eq!(id_at(FILE, 13), None, "a section after a preview ends it");
    assert_eq!(id_at(FILE, 15), None);
}

#[test]
fn the_previews_prose_is_no_preview() {
    let src = "[view]\ncol\n\n[previews \"Inputs/Card\"]\nProse.\n\n[preview \"A\"]\ncol\n";
    assert_eq!(id_at(src, 3), None);
    assert_eq!(id_at(src, 4), None);
    assert_eq!(id_at(src, 7).as_deref(), Some("my_app--card--a"));
}

#[test]
fn a_preview_with_no_id_of_its_own_names_none() {
    let src = "[view]\ncol\n\n[preview \"A b\"]\ncol\n\n[preview \"a-B\"]\ncol\n";
    assert_eq!(id_at(src, 4).as_deref(), Some("my_app--card--a-b"));
    assert_eq!(id_at(src, 7), None, "slugs to the id the first one took");
}

#[test]
fn the_target_carries_where_to_run_and_what_to_send() {
    let src = "[view]\ncol\n\n[preview \"Wide — RTL\"]\ncol\n";
    assert_eq!(
        preview_at(&parse(src).unwrap(), src, 4, &host("card")),
        Some(PreviewTarget {
            id: "my_app--card--wide-rtl".to_string(),
            package_dir: "/work/app".to_string(),
            location: "/preview/my_app--card--wide-rtl".to_string(),
            channel: Some("/work/.telar/workshop/my-app.channel".to_string()),
        })
    );
}

#[test]
fn a_previews_file_previews_the_component_it_is_named_after() {
    let root = std::env::temp_dir().join(format!("telar-preview-target-{}", std::process::id()));
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
    assert_eq!(
        host.workshop_channel,
        Some(root.join(".telar/workshop/rsx-fixture.channel")),
        "named by the package, as `cargo telar preview` names it"
    );

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
