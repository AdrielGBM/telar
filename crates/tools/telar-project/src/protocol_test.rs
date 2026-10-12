use super::*;

#[test]
fn the_channel_file_is_per_package_under_the_workshop_directory() {
    assert_eq!(
        workshop_channel_file(Path::new("/ws"), "catalogue"),
        PathBuf::from("/ws/.telar/workshop/catalogue.channel")
    );
}

#[test]
fn a_package_names_its_workshop_files_by_its_package_name_under_its_workspace() {
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
        package_workshop_files(&member),
        Some(WorkshopFiles {
            channel: root.join(".telar/workshop/demo-app.channel"),
            server: root.join(".telar/workshop/demo-app.server"),
        })
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_package_outside_a_workspace_keeps_its_workshop_files_under_itself() {
    let root = std::env::temp_dir().join(format!("telar-protocol-alone-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"solo\"\n").unwrap();

    assert_eq!(
        package_workshop_files(&root).map(|files| files.channel),
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

fn listed(id: &str, title: &str) -> ListedPreview {
    ListedPreview {
        id: id.to_string(),
        title: title.to_string(),
    }
}

fn segments(pages: &[PageLocation]) -> Vec<Vec<&str>> {
    pages
        .iter()
        .map(|page| page.segments.iter().map(String::as_str).collect())
        .collect()
}

#[test]
fn each_preview_has_a_canvas_and_a_matrix_page_and_each_title_one_docs_page() {
    let pages = workshop_pages(&[
        listed("app--button--primary", "Forms/Button"),
        listed("app--button--ghost", "Forms/Button"),
        listed("app--card--plain", "Card"),
    ]);
    assert_eq!(
        segments(&pages),
        vec![
            vec!["preview", "app--button--primary"],
            vec!["matrix", "app--button--primary"],
            vec!["docs", "Forms", "Button"],
            vec!["preview", "app--button--ghost"],
            vec!["matrix", "app--button--ghost"],
            vec!["preview", "app--card--plain"],
            vec!["matrix", "app--card--plain"],
            vec!["docs", "Card"],
        ]
    );
    assert!(pages.iter().all(|page| page.locale.is_none()));
}

#[test]
fn a_docs_page_is_written_as_the_workshop_writes_its_title() {
    let pages = workshop_pages(&[listed("a", " Forms / Text field /"), listed("b", " / ")]);
    assert_eq!(
        segments(&pages),
        vec![
            vec!["preview", "a"],
            vec!["matrix", "a"],
            vec!["docs", "Forms", "Text field"],
            vec!["preview", "b"],
            vec!["matrix", "b"],
        ]
    );
}

#[test]
fn a_listing_reads_back_what_it_wrote() {
    let previews = vec![listed("app--card--plain", "Card")];
    let json = serde_json::to_string(&previews).unwrap();
    assert_eq!(json, r#"[{"id":"app--card--plain","title":"Card"}]"#);
    assert_eq!(
        serde_json::from_str::<Vec<ListedPreview>>(&json).unwrap(),
        previews
    );
}

#[test]
fn a_title_is_cut_into_trimmed_groups_and_has_one_canonical_form() {
    for (title, path) in [
        ("Forms / Button", vec!["Forms", "Button"]),
        ("Forms/Button", vec!["Forms", "Button"]),
        ("  Forms  /  Text field  ", vec!["Forms", "Text field"]),
        ("Formulários / Botão ñ", vec!["Formulários", "Botão ñ"]),
        ("100% / a?b#c", vec!["100%", "a?b#c"]),
        (" / ", vec![]),
    ] {
        assert_eq!(title_path(title).collect::<Vec<_>>(), path, "{title}");
        assert_eq!(canonical_title(title), path.join("/"), "{title}");
    }
}

#[test]
fn a_docs_page_keeps_spaces_and_non_ascii_inside_its_segments() {
    let pages = workshop_pages(&[listed("a", "Forms / Botão ñ / 100%")]);
    assert_eq!(
        segments(&pages)[2],
        vec!["docs", "Forms", "Botão ñ", "100%"]
    );
}

#[test]
fn a_booting_session_names_its_process_and_a_serving_one_its_address() {
    assert_eq!(starting_server_line(4242), "starting 4242");
    assert_eq!(
        parse_server_file("starting 4242\n"),
        Some(ServerFile::Starting { pid: 4242 })
    );
    assert_eq!(
        parse_server_file("http://localhost:8081/\n"),
        Some(ServerFile::Serving("http://localhost:8081/".to_string()))
    );
}

#[test]
fn a_server_file_that_says_nothing_usable_is_not_parsed() {
    assert_eq!(parse_server_file(""), None);
    assert_eq!(parse_server_file("\n"), None);
    assert_eq!(parse_server_file("starting soon"), None);
}
