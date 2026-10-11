use super::*;

#[test]
fn the_channel_file_is_per_package_under_the_workshop_directory() {
    assert_eq!(
        workshop_channel_file(Path::new("/ws"), "catalogue"),
        PathBuf::from("/ws/.telar/workshop/catalogue.channel")
    );
}

#[test]
fn a_package_names_its_channel_by_its_package_name_under_its_workspace() {
    let root = std::env::temp_dir().join(format!("telar-protocol-{}", std::process::id()));
    let member = root.join("apps").join("demo");
    std::fs::create_dir_all(&member).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"apps/*\"]\n",
    )
    .unwrap();
    std::fs::write(
        member.join("Cargo.toml"),
        "[package]\nname = \"demo-app\"\nversion = \"0.1.0\"\n\n[lib]\nname = \"demo_lib\"\n",
    )
    .unwrap();

    assert_eq!(
        package_workshop_channel_file(&member),
        Some(root.join(".telar/workshop/demo-app.channel"))
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_package_outside_a_workspace_keeps_its_channel_under_itself() {
    let root = std::env::temp_dir().join(format!("telar-protocol-alone-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"solo\"\n").unwrap();

    assert_eq!(
        package_workshop_channel_file(&root),
        Some(root.join(".telar/workshop/solo.channel"))
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_id_opens_its_preview() {
    assert_eq!(
        preview_location("telar_components--button--primary"),
        "/preview/telar_components--button--primary"
    );
}

#[test]
fn an_id_with_characters_a_path_cannot_hold_is_encoded() {
    assert_eq!(preview_location("a b/c"), "/preview/a%20b%2Fc");
    assert_eq!(
        preview_location("app--card--ñ"),
        "/preview/app--card--%C3%B1"
    );
}
