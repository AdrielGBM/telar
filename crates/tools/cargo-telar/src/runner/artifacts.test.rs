use super::*;

/// What `cargo build --message-format=json` printed for a package `my-app` whose manifest says `[lib] name = "other"`, `crate-type = ["cdylib", "rlib"]` and `[[bin]] name = "launcher"`, with the paths moved under `/work`.
const RECORDED: &str = r#"{"reason":"compiler-artifact","package_id":"path+file:///work/my-app#0.1.0","manifest_path":"/work/my-app/Cargo.toml","target":{"kind":["cdylib","rlib"],"crate_types":["cdylib","rlib"],"name":"other","src_path":"/work/my-app/src/lib.rs","edition":"2024","doc":true,"doctest":true,"test":true},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/work/my-app/target/debug/libother.so","/work/my-app/target/debug/libother.rlib"],"executable":null,"fresh":false}
{"reason":"compiler-artifact","package_id":"path+file:///work/my-app#0.1.0","manifest_path":"/work/my-app/Cargo.toml","target":{"kind":["bin"],"crate_types":["bin"],"name":"launcher","src_path":"/work/my-app/src/main.rs","edition":"2024","doc":true,"doctest":false,"test":true},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/work/my-app/target/debug/launcher"],"executable":"/work/my-app/target/debug/launcher","fresh":false}
{"reason":"build-finished","success":true}"#;

fn artifacts_of(messages: &str) -> Vec<Artifact> {
    diagnostics::collect(messages.as_bytes())
        .artifacts()
        .to_vec()
}

fn bin_message(manifest: &str, name: &str, executable: &str) -> String {
    serde_json::json!({
        "reason": "compiler-artifact",
        "manifest_path": manifest,
        "target": { "name": name, "kind": ["bin"] },
        "filenames": [executable],
        "executable": executable,
    })
    .to_string()
}

#[test]
fn the_dynamic_library_is_the_file_cargo_named_after_the_lib_name() {
    let artifacts = artifacts_of(RECORDED);
    assert_eq!(
        cdylib(&artifacts, Path::new("/work/my-app"), "so"),
        Some(PathBuf::from("/work/my-app/target/debug/libother.so"))
    );
}

#[test]
fn the_binary_is_the_executable_cargo_named_after_the_bin_name() {
    let artifacts = artifacts_of(RECORDED);
    assert_eq!(
        binary(&artifacts, Path::new("/work/my-app"), None),
        Ok(PathBuf::from("/work/my-app/target/debug/launcher"))
    );
}

#[test]
fn artifacts_of_another_package_are_not_the_selected_ones() {
    let artifacts = artifacts_of(RECORDED);
    assert_eq!(cdylib(&artifacts, Path::new("/work/elsewhere"), "so"), None);
    assert!(binary(&artifacts, Path::new("/work/elsewhere"), None).is_err());
}

#[test]
fn several_binaries_run_the_one_default_run_names() {
    let messages = [
        bin_message("/work/site/Cargo.toml", "tool", "/t/tool"),
        bin_message("/work/site/Cargo.toml", "site", "/t/site"),
    ]
    .join("\n");
    let artifacts = artifacts_of(&messages);
    assert_eq!(
        binary(&artifacts, Path::new("/work/site"), Some("site")),
        Ok(PathBuf::from("/t/site"))
    );
    let unnamed = binary(&artifacts, Path::new("/work/site"), None).unwrap_err();
    assert!(unnamed.contains("default-run"), "{unnamed}");
}

#[test]
fn messages_that_are_not_artifacts_report_none() {
    assert!(artifacts_of("not json\n{\"reason\":\"build-script-executed\"}").is_empty());
}

#[test]
fn json_messages_are_asked_for_unless_a_format_was_chosen() {
    let mut args = vec!["build".to_string()];
    with_json_messages(&mut args);
    assert_eq!(args, ["build", "--message-format=json"]);

    let mut chosen = vec![
        "build".to_string(),
        "--message-format=json-render-diagnostics".to_string(),
    ];
    with_json_messages(&mut chosen);
    assert_eq!(
        chosen,
        ["build", "--message-format=json-render-diagnostics"]
    );
}
