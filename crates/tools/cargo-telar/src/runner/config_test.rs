use super::*;

#[test]
fn telar_toml_overrides_manifest_field_by_field() {
    let manifest = TelarSection {
        backend: Some(RendererBackend::Software),
        dev: DevSection {
            window: Some(WindowSection {
                width: Some(800),
                height: Some(600),
                fullscreen: Some("disabled".to_string()),
                ..Default::default()
            }),
            devtools: Some(true),
        },
        ..Default::default()
    };
    let file = TelarSection {
        backend: Some(RendererBackend::Hardware),
        dev: DevSection {
            window: Some(WindowSection {
                width: Some(1024),
                position: Some("10,20".to_string()),
                ..Default::default()
            }),
            devtools: None,
        },
        ..Default::default()
    };

    let merged = merge_config(manifest, file);
    assert!(
        matches!(merged.backend, Some(RendererBackend::Hardware)),
        "telar.toml wins over the manifest"
    );
    assert_eq!(merged.dev.devtools, Some(true)); // omitted in telar.toml → manifest value survives
    let window = merged.dev.window.unwrap();
    assert_eq!(window.width, Some(1024)); // telar.toml wins
    assert_eq!(window.height, Some(600)); // falls back to manifest
    assert_eq!(window.position.as_deref(), Some("10,20")); // only in telar.toml
    assert_eq!(window.fullscreen.as_deref(), Some("disabled")); // only in manifest
}

#[test]
fn manifest_used_when_telar_toml_absent() {
    let manifest = TelarSection {
        backend: Some(RendererBackend::Software),
        ..Default::default()
    };
    let merged = merge_config(manifest, TelarSection::default());
    assert!(
        matches!(merged.backend, Some(RendererBackend::Software)),
        "with no telar.toml the manifest is the whole answer"
    );
    assert_eq!(merged.dev, DevSection::default());
}
