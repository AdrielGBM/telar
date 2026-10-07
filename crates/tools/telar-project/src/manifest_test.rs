use super::*;

fn package(name: &str, manifest: Option<&str>) -> PathBuf {
    let root = std::env::temp_dir().join(format!("telar_manifest_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    if let Some(content) = manifest {
        std::fs::write(root.join(MANIFEST_FILENAME), content).unwrap();
    }
    root
}

#[test]
fn a_package_with_no_manifest_gets_the_defaults() {
    let root = package("absent", None);
    let manifest = TelarManifest::load(&root).expect("no file is not an error");
    assert_eq!(manifest, TelarManifest::default());
    assert_eq!(manifest.telar.assets_root(&root), root.join("assets"));
}

#[test]
fn every_key_the_schema_names_round_trips() {
    let root = package(
        "full",
        Some(
            r#"
[telar]
backend = "software"
assets = "art"
theme = "app::Theme"

[telar.dev]
devtools = false

[telar.dev.window]
title = "dev"
width = 1024

[telar.i18n]
root = "strings"
scan = "lang"
default = "es"
"#,
        ),
    );
    let telar = TelarManifest::load(&root)
        .expect("a full manifest parses")
        .telar;
    assert_eq!(telar.backend, Some(RendererBackend::Software));
    assert_eq!(telar.assets_root(&root), root.join("art"));
    assert_eq!(telar.theme.as_deref(), Some("app::Theme"));
    assert_eq!(telar.dev.devtools, Some(false));
    assert_eq!(telar.dev.window.as_ref().unwrap().width, Some(1024));
    assert_eq!(telar.locales_root(&root), Some(root.join("strings")));
    assert_eq!(telar.catalog_scan_dir(), "lang");
    assert_eq!(telar.default_locale().as_deref(), Some("es"));
}

/// The whole reason this file exists. Every reader treated an unrecognised key as absent, so a setting that was off and a setting that was misspelled looked identical — and `cargo-telar`'s own comment records the release where that shipped.
#[test]
fn a_misspelled_key_is_an_error_that_names_it() {
    for (label, manifest) in [
        ("top level", "[telar]\nbackends = \"software\"\n"),
        ("dev", "[telar.dev]\ndevtool = true\n"),
        ("window", "[telar.dev.window]\nwith = 800\n"),
        ("i18n", "[telar.i18n]\nscann = \"lang\"\n"),
        ("web", "[telar.web]\ntemplat = \"page.html\"\n"),
        ("table", "[telarr]\nbackend = \"software\"\n"),
    ] {
        let root = package(&format!("typo_{}", label.replace(' ', "_")), Some(manifest));
        let error = TelarManifest::load(&root)
            .expect_err(&format!("a typo in the {label} table has to be an error"));
        let text = error.to_string();
        let key = manifest
            .lines()
            .find(|line| line.contains('='))
            .and_then(|line| line.split('=').next())
            .map(str::trim)
            .unwrap_or_default();
        let named = text.contains(key) || text.contains("telarr");
        assert!(
            named,
            "the {label} error should name the key it refused: {text}"
        );
    }
}

/// A value of the right shape but the wrong word is caught too, so `backend = "gpu"` is not silently `auto`.
#[test]
fn an_unknown_backend_is_refused_rather_than_defaulted() {
    let root = package("backend", Some("[telar]\nbackend = \"gpu\"\n"));
    assert!(TelarManifest::load(&root).is_err());
}

/// The pre-`[telar.i18n]` spellings still answer, so a project written against the old keys keeps working.
#[test]
fn the_older_i18n_spellings_still_answer() {
    let root = package(
        "legacy",
        Some("[telar]\nlocales = \"strings\"\ndefault_locale = \"fr\"\n"),
    );
    let telar = TelarManifest::load(&root)
        .expect("the old keys are still in the schema")
        .telar;
    assert_eq!(telar.locales_root(&root), Some(root.join("strings")));
    assert_eq!(telar.default_locale().as_deref(), Some("fr"));
}

/// `""` is how a project turns a discovery source off, which is different from leaving it at its default.
#[test]
fn an_empty_locales_root_disables_it() {
    let root = package("disabled", Some("[telar.i18n]\nroot = \"\"\n"));
    let telar = TelarManifest::load(&root).unwrap().telar;
    assert_eq!(telar.locales_root(&root), None);
}

/// A workspace declares once what its packages share. Every package answering the same way is the common case, and saying it in eight files is how the eight drift apart.
#[test]
fn a_package_inherits_the_workspace_manifest_key_by_key() {
    let root = std::env::temp_dir().join(format!("telar_inherit_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let package_root = root.join("crates/ui");
    std::fs::create_dir_all(&package_root).unwrap();
    std::fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
    std::fs::write(
        root.join(MANIFEST_FILENAME),
        "[telar]\nbackend = \"software\"\ntheme = \"config::Nord\"\n",
    )
    .unwrap();
    std::fs::write(
        package_root.join(MANIFEST_FILENAME),
        "[telar]\ntheme = \"ui::Own\"\n",
    )
    .unwrap();

    let telar = TelarManifest::load(&package_root)
        .expect("both manifests parse")
        .telar;
    assert_eq!(telar.backend, Some(RendererBackend::Software), "inherited");
    assert_eq!(telar.theme.as_deref(), Some("ui::Own"), "overridden");

    std::fs::remove_file(package_root.join(MANIFEST_FILENAME)).unwrap();
    let telar = TelarManifest::load(&package_root)
        .expect("a package with no manifest of its own")
        .telar;
    assert_eq!(telar.theme.as_deref(), Some("config::Nord"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_project_without_a_web_table_gets_the_web_defaults() {
    let root = package("web_defaults", Some("[telar]\nbackend = \"auto\"\n"));
    let web = TelarManifest::load(&root).unwrap().telar.web;
    assert_eq!(
        web.template_path(&root),
        (root.join("web/index.html"), false)
    );
    assert_eq!(web.public_dir(&root), (root.join("web/public"), false));
}

#[test]
fn the_web_table_names_its_template_and_public_directory() {
    let root = package(
        "web_named",
        Some("[telar.web]\ntemplate = \"site/page.html\"\npublic = \"static\"\n"),
    );
    let web = TelarManifest::load(&root).unwrap().telar.web;
    assert_eq!(
        web.template_path(&root),
        (root.join("site/page.html"), true)
    );
    assert_eq!(web.public_dir(&root), (root.join("static"), true));
}

#[test]
fn the_site_keys_are_inherited_key_by_key() {
    let root = std::env::temp_dir().join(format!("telar_inherit_web_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let package_root = root.join("apps/site");
    std::fs::create_dir_all(&package_root).unwrap();
    std::fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
    std::fs::write(
        root.join(MANIFEST_FILENAME),
        "[telar.web]\norigin = \"https://example.com\"\nhost = \"cloudflare-pages\"\ndescription = \"site.description\"\n",
    )
    .unwrap();
    std::fs::write(
        package_root.join(MANIFEST_FILENAME),
        "[telar.web]\nog_image = \"og.png\"\ntheme_color = \"#000\"\ndescription = \"app.description\"\n",
    )
    .unwrap();

    let web = TelarManifest::load(&package_root)
        .expect("an image of the site is fine once the workspace names the origin")
        .telar
        .web;
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(web.origin(), Some("https://example.com"));
    assert_eq!(web.host(), crate::WebHost::CloudflarePages);
    assert_eq!(web.description.as_deref(), Some("app.description"));
    assert_eq!(
        web.og_image,
        Some(crate::OgImage::Shared("og.png".to_string()))
    );
    assert_eq!(web.base_path(), "/");
}

#[test]
fn a_web_table_that_cannot_describe_a_site_is_an_error_naming_the_key() {
    let root = package(
        "web_invalid",
        Some("[telar.web]\norigin = \"example.com\"\n"),
    );
    let error = TelarManifest::load(&root).unwrap_err().to_string();
    assert!(error.contains("origin"), "{error}");
}

#[test]
fn a_package_naming_any_face_replaces_the_workspace_set_whole() {
    let root = std::env::temp_dir().join(format!("telar_inherit_fonts_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let package_root = root.join("crates/ui");
    std::fs::create_dir_all(&package_root).unwrap();
    std::fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
    let face = |family: &str| format!("[[telar.fonts]]\nfamily = \"{family}\"\nsrc = \"f.ttf\"\n");
    std::fs::write(
        root.join(MANIFEST_FILENAME),
        format!("{}{}", face("Shared"), face("Mono")),
    )
    .unwrap();
    std::fs::write(package_root.join(MANIFEST_FILENAME), "[telar]\n").unwrap();
    let inherited = TelarManifest::load(&package_root).unwrap().telar.fonts;
    assert_eq!(inherited.len(), 2);

    std::fs::write(package_root.join(MANIFEST_FILENAME), face("Own")).unwrap();
    let own = TelarManifest::load(&package_root).unwrap().telar.fonts;
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(own.len(), 1);
    assert_eq!(own[0].family, "Own");
}

#[test]
fn a_prerender_table_names_the_reader_pages_are_written_for() {
    let root = package(
        "web_prerender",
        Some(
            "[telar.web.prerender]\nwidth = 390\ncolor_scheme = \"dark\"\nlocales = [\"es-CL\"]\n",
        ),
    );
    let prerender = TelarManifest::load(&root).unwrap().telar.web.prerender;
    assert_eq!(
        prerender.surface(),
        crate::Surface {
            width: 390,
            height: crate::DEFAULT_SURFACE.1
        }
    );
    let preferences = prerender.preferences();
    assert_eq!(preferences.color_scheme.as_deref(), Some("dark"));
    assert_eq!(preferences.reduced_motion, None);
    assert_eq!(preferences.locales, ["es-CL"]);
}

#[test]
fn the_prelude_is_read_normalised_and_in_order() {
    let root = package(
        "prelude",
        Some("[telar]\nprelude = [\"telar-components\", \"my_plugin::prelude\"]\n"),
    );
    let paths: Vec<String> = crate::resolve_prelude(&root)
        .expect("a valid prelude")
        .iter()
        .map(|entry| entry.path().to_string())
        .collect();
    assert_eq!(paths, ["telar_components", "my_plugin::prelude"]);
}

#[test]
fn a_package_that_declares_no_prelude_has_an_empty_one() {
    let root = package("no_prelude", Some("[telar]\nbackend = \"auto\"\n"));
    assert!(crate::resolve_prelude(&root).unwrap().is_empty());
    assert!(
        crate::resolve_prelude(&package("no_manifest", None))
            .unwrap()
            .is_empty()
    );
}

/// The error is on the `telar.toml` value, with its line, rather than on the generated `use` every `.rsx` of the package would otherwise fail on.
#[test]
fn a_prelude_entry_that_is_not_a_path_is_an_error_on_the_value() {
    let root = package(
        "bad_prelude",
        Some("[telar]\nbackend = \"auto\"\nprelude = [\"telar-components\", \"not a path\"]\n"),
    );
    let error = TelarManifest::load(&root).unwrap_err().to_string();
    assert!(error.contains(MANIFEST_FILENAME), "{error}");
    assert!(error.contains("line 3"), "{error}");
    assert!(
        error.contains("\"not a path\" is not a Rust path"),
        "{error}"
    );
    assert!(crate::resolve_prelude(&root).is_err());
}

#[test]
fn a_prelude_naming_one_crate_twice_is_an_error() {
    let root = package(
        "twice_prelude",
        Some("[telar]\nprelude = [\"telar-components\", \"telar_components\"]\n"),
    );
    let error = TelarManifest::load(&root).unwrap_err().to_string();
    assert!(error.contains("more than once"), "{error}");
}

/// A workspace may share a prelude, and a package that does not depend on one of its crates has to be able to say so: an empty list is a declaration, not an absence.
#[test]
fn a_package_inherits_the_workspace_prelude_unless_it_declares_its_own() {
    let root = std::env::temp_dir().join(format!("telar_inherit_prelude_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let package_root = root.join("apps/site");
    std::fs::create_dir_all(&package_root).unwrap();
    std::fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
    std::fs::write(
        root.join(MANIFEST_FILENAME),
        "[telar]\nprelude = [\"telar-components\"]\n",
    )
    .unwrap();
    let declared = |manifest: &str| {
        std::fs::write(package_root.join(MANIFEST_FILENAME), manifest).unwrap();
        crate::resolve_prelude(&package_root)
            .unwrap()
            .iter()
            .map(|entry| entry.path().to_string())
            .collect::<Vec<_>>()
    };

    let inherited = declared("[telar]\nbackend = \"auto\"\n");
    let own = declared("[telar]\nprelude = [\"my-plugin\"]\n");
    let none = declared("[telar]\nprelude = []\n");
    let files = TelarManifest::files(&package_root);
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(inherited, ["telar_components"]);
    assert_eq!(own, ["my_plugin"]);
    assert!(none.is_empty());
    assert_eq!(
        files,
        [
            package_root.join(MANIFEST_FILENAME),
            root.join(MANIFEST_FILENAME)
        ]
    );
}
