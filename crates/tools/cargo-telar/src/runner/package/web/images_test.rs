use super::*;

#[test]
fn only_the_local_crates_the_package_is_built_from_ship_pictures() {
    let metadata = serde_json::json!({
        "packages": [
            {"id": "app", "name": "app", "source": null, "manifest_path": "/w/app/Cargo.toml"},
            {"id": "ui", "name": "ui", "source": null, "manifest_path": "/w/ui/Cargo.toml"},
            {"id": "other", "name": "other", "source": null, "manifest_path": "/w/other/Cargo.toml"},
            {"id": "serde", "name": "serde", "source": "registry+https://github.com/rust-lang/crates.io-index", "manifest_path": "/r/serde/Cargo.toml"},
        ],
        "resolve": {"nodes": [
            {"id": "app", "deps": [{"pkg": "ui"}, {"pkg": "serde"}]},
            {"id": "ui", "deps": [{"pkg": "serde"}]},
            {"id": "other", "deps": []},
            {"id": "serde", "deps": []},
        ]},
    });
    assert_eq!(
        local_crates_in(&metadata, "app").unwrap(),
        vec![PathBuf::from("/w/app"), PathBuf::from("/w/ui")]
    );
    assert!(local_crates_in(&metadata, "missing").is_err());
}
