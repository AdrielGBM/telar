use super::{families_without_sfnt_face, font_asset_tokens, stale_cli_message};

/// The failure this catches was silent: an installed `cargo-telar` older than the `telar` it drives asks for a hot-reload build the way it used to — an environment variable — the macro builds a plain application instead, and the only thing said about it is that the app never connected to a channel it was never compiled to open.
#[test]
fn an_older_cli_asking_the_old_way_is_named() {
    let message = stale_cli_message(true).expect("this test binary has no `hot-reload` feature");

    assert!(
        message.contains("cargo install cargo-telar"),
        "the message has to name what fixes it: {message}"
    );
    assert!(
        message.contains(env!("CARGO_PKG_VERSION")),
        "and which version to install: {message}"
    );
}

/// A current CLI names the feature and sets no such variable, which is every build that is not the mismatch above.
#[test]
fn a_build_that_never_asked_is_left_alone() {
    assert_eq!(stale_cli_message(false), None);
}

/// A declared face reaches a native shaper embedded and answering to what the manifest said, weight range and axes included.
#[test]
fn a_declared_face_is_embedded_as_the_manifest_declared_it() {
    let font = telar_project::FontDeclaration {
        family: "Display".to_string(),
        src: "assets/Display.ttf".to_string(),
        weight: None,
        style: telar_project::FontStyleDeclaration::Italic,
        axes: [("wght".to_string(), [100.0, 900.0])].into_iter().collect(),
        display: telar_project::FontDisplay::Swap,
        size_adjust: None,
    };
    let tokens = font_asset_tokens(&font, std::path::Path::new("/pkg/assets/Display.ttf"))
        .to_string()
        .replace(' ', "");
    assert!(
        tokens.contains("include_bytes!(\"/pkg/assets/Display.ttf\")"),
        "{tokens}"
    );
    assert!(tokens.contains(".named(\"Display\")"), "{tokens}");
    assert!(
        tokens.contains("FontWeight::range(100u16,900u16)"),
        "{tokens}"
    );
    assert!(tokens.contains("FontStyle::Italic"), "{tokens}");
    assert!(
        tokens.contains("FontAxis::new([119u8,103u8,104u8,116u8],100f32,900f32)"),
        "{tokens}"
    );
}

fn declaration(family: &str, src: &str) -> telar_project::FontDeclaration {
    telar_project::FontDeclaration {
        family: family.to_string(),
        src: src.to_string(),
        weight: None,
        style: telar_project::FontStyleDeclaration::Normal,
        axes: Default::default(),
        display: Default::default(),
        size_adjust: None,
    }
}

#[test]
fn a_family_shipped_only_as_woff_is_reported_once_for_a_native_shaper() {
    let fonts = [
        declaration("Web", "a.woff2"),
        declaration("Web", "b.woff"),
        declaration("Both", "c.woff2"),
        declaration("Both", "c.ttf"),
        declaration("Native", "d.otf"),
    ];

    let reported: Vec<_> = families_without_sfnt_face(&fonts).collect();

    assert_eq!(reported, ["Web"]);
}

fn package(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("telar_macros_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    root
}

fn wire(root: &std::path::Path) -> Result<Vec<super::WiredFile>, proc_macro2::TokenStream> {
    wire_with(root, &[])
}

fn wire_with(
    root: &std::path::Path,
    prelude: &[telar_project::PreludeEntry],
) -> Result<Vec<super::WiredFile>, proc_macro2::TokenStream> {
    wire_as(root, prelude, false)
}

fn wire_as(
    root: &std::path::Path,
    prelude: &[telar_project::PreludeEntry],
    library_dependency: bool,
) -> Result<Vec<super::WiredFile>, proc_macro2::TokenStream> {
    let telar = telar_project::TelarManifest::load(root).unwrap().telar;
    let assets = telar_project::AssetContext::load(root, &telar, "0.0.0");
    let flavour = telar_project::BuildFlavour::Plain;
    let package = super::Package {
        dir: root.to_path_buf(),
        name: "kit".to_string(),
        wiring: super::Wiring {
            flavour,
            library_dependency,
        },
    };
    super::wire_sources(
        &package,
        &root.join("src"),
        &telar_project::generated_dir(root, flavour),
        None,
        prelude,
        &assets,
    )
}

/// The macro has no transpiler, so the reason the CLI could not produce the artifact is the only thing that can tell the author what is wrong with their markup — "re-run the command" is a loop when the command is what failed.
#[test]
fn a_recorded_transpile_error_is_what_the_build_reports() {
    let root = package("reports");
    std::fs::write(root.join("src/home.rsx"), "[view]\ntext \"oops\n").unwrap();
    let failure = telar_project::BuildFailure {
        source: "home.rsx".to_string(),
        hash: telar_project::content_hash(b"[view]\ntext \"oops\n"),
        message: "src/home.rsx:2: unterminated string literal".to_string(),
    };
    telar_project::write_build_failure(&root, telar_project::BuildFlavour::Plain, &failure)
        .unwrap();

    let message = wire(&root).err().unwrap().to_string();
    assert!(
        message.contains("src/home.rsx:2: unterminated string literal"),
        "{message}"
    );
    assert!(!message.contains("no longer answers"), "{message}");

    let _ = std::fs::remove_dir_all(&root);
}

/// Once the author has edited the file the record names, it may be describing an error that is gone, so the generic advice is true again.
#[test]
fn a_record_for_an_edited_source_is_not_reported() {
    let root = package("edited");
    std::fs::write(root.join("src/home.rsx"), "[view]\ntext \"fixed\"\n").unwrap();
    let failure = telar_project::BuildFailure {
        source: "home.rsx".to_string(),
        hash: telar_project::content_hash(b"[view]\ntext \"oops\n"),
        message: "src/home.rsx:2: unterminated string literal".to_string(),
    };
    telar_project::write_build_failure(&root, telar_project::BuildFlavour::Plain, &failure)
        .unwrap();

    let message = wire(&root).err().unwrap().to_string();
    assert!(!message.contains("unterminated"), "{message}");

    let _ = std::fs::remove_dir_all(&root);
}

fn transpiled_under(name: &str, prelude: &[&str]) -> std::path::PathBuf {
    transpiled(name, prelude, false)
}

/// An artifact exactly as `cargo telar transpile` would leave it for one `home.rsx`, recorded under `prelude`, declaring `[preview]` blocks or not.
fn transpiled(name: &str, prelude: &[&str], previews: bool) -> std::path::PathBuf {
    let root = package(name);
    let source = "[view]\ntext \"a\"\n";
    let output = "// generated\n";
    std::fs::write(root.join("src/home.rsx"), source).unwrap();
    let flavour = telar_project::BuildFlavour::Plain;
    let generated = telar_project::generated_dir(&root, flavour);
    std::fs::create_dir_all(&generated).unwrap();
    std::fs::write(generated.join("home.rs"), output).unwrap();
    let index = telar_project::BuildIndex {
        format: telar_project::BUILD_ARTIFACT_FORMAT,
        producer: "test".to_string(),
        telar_version: env!("CARGO_PKG_VERSION").to_string(),
        theme: None,
        prelude: prelude.iter().map(|path| path.to_string()).collect(),
        uses_assets: false,
        entries: vec![telar_project::BuildEntry {
            source: "home.rsx".to_string(),
            hash: telar_project::content_hash(source.as_bytes()),
            output_hash: telar_project::content_hash(output.as_bytes()),
            previews,
        }],
    };
    telar_project::write_build_index(&root, flavour, &index).unwrap();
    root
}

fn entries(declared: &[&str]) -> Vec<telar_project::PreludeEntry> {
    declared
        .iter()
        .map(|entry| telar_project::PreludeEntry::parse(entry).unwrap())
        .collect()
}

#[test]
fn an_artifact_transpiled_under_the_declared_prelude_is_wired() {
    let root = transpiled_under("prelude_agrees", &["telar_components"]);
    let wired = wire_with(&root, &entries(&["telar-components"]));
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(wired.map(|files| files.len()).ok(), Some(1));
}

/// Nothing under `src/` changed, so the message has to point at `telar.toml` and at the command that brings the artifact back in line with it.
#[test]
fn an_artifact_transpiled_under_another_prelude_is_refused_naming_both() {
    let one: &[&str] = &["telar_components"];
    let two: &[&str] = &["telar_components", "other"];
    for (label, recorded, declared) in [
        ("added", one, two),
        ("removed", one, &[][..]),
        ("reordered", two, &["other", "telar-components"][..]),
    ] {
        let root = transpiled_under(&format!("prelude_{label}"), recorded);
        let message = wire_with(&root, &entries(declared))
            .err()
            .unwrap_or_else(|| panic!("a prelude {label} since the transpile is refused"))
            .to_string();
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            message.contains("telar.toml declares `prelude"),
            "{message}"
        );
        assert!(
            message.contains("was transpiled with `prelude"),
            "{message}"
        );
        assert!(message.contains("cargo telar transpile"), "{message}");
    }
}

#[test]
fn a_source_edited_under_the_same_prelude_gets_the_generic_message() {
    let root = transpiled_under("prelude_same_edited", &["telar_components"]);
    std::fs::write(root.join("src/home.rsx"), "[view]\ntext \"b\"\n").unwrap();
    let message = wire_with(&root, &entries(&["telar_components"]))
        .err()
        .unwrap()
        .to_string();
    let _ = std::fs::remove_dir_all(&root);
    assert!(message.contains("no longer answers"), "{message}");
    assert!(!message.contains("prelude"), "{message}");
}

use telar_project::BuildFlavour;

#[test]
fn a_library_compiled_for_another_package_is_plain_whatever_was_asked() {
    for requested in BuildFlavour::ALL {
        let wiring = super::Wiring::decide(true, false, requested);
        assert_eq!(wiring.flavour, BuildFlavour::Plain, "{requested:?}");
        assert!(wiring.library_dependency);
    }
}

/// Selected on the command line, a library is being developed — its signals hot — so it gets what the build asked for, like an application.
#[test]
fn a_selected_library_and_any_application_get_the_flavour_asked_for() {
    for requested in BuildFlavour::ALL {
        for (library, primary) in [(true, true), (false, true), (false, false)] {
            let wiring = super::Wiring::decide(library, primary, requested);
            assert_eq!(
                wiring.flavour, requested,
                "library {library}, primary {primary}"
            );
            assert!(!wiring.library_dependency);
        }
    }
}

/// A library dependency's artifact ships with it, so a version mismatch is fixed by the dependency's version, never by a command the consumer cannot run on a registry copy.
#[test]
fn a_library_artifact_for_another_telar_names_both_versions() {
    let root = transpiled_under("library_version", &[]);
    let mut index = telar_project::read_build_index(&root, BuildFlavour::Plain).unwrap();
    index.telar_version = "0.0.1-other".to_string();
    telar_project::write_build_index(&root, BuildFlavour::Plain, &index).unwrap();

    let as_dependency = wire_as(&root, &[], true).err().unwrap().to_string();
    let as_package = wire_as(&root, &[], false).err().unwrap().to_string();
    let _ = std::fs::remove_dir_all(&root);

    assert!(
        as_dependency.contains("`kit` was transpiled for telar 0.0.1-other"),
        "{as_dependency}"
    );
    assert!(
        as_dependency.contains(super::ARTIFACT_TELAR_VERSION),
        "{as_dependency}"
    );
    assert!(!as_dependency.contains("Re-run"), "{as_dependency}");
    assert!(
        as_package.contains("transpiled for telar 0.0.1-other"),
        "{as_package}"
    );
    assert!(as_package.contains("cargo telar transpile"), "{as_package}");
}

/// What the handshake compares against is this macro's own version, which is the consumer's: an artifact recorded for it is wired with no `cargo metadata` asked from the package's directory.
#[test]
fn a_library_artifact_for_this_telar_is_wired() {
    let root = transpiled_under("library_current", &[]);
    let wired = wire_as(&root, &[], true);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(wired.map(|files| files.len()).ok(), Some(1));
}

#[test]
fn a_library_without_an_artifact_is_told_who_produces_it() {
    let root = package("library_absent");
    std::fs::write(root.join("src/home.rsx"), "[view]\ntext \"a\"\n").unwrap();
    let message = wire_as(&root, &[], true).err().unwrap().to_string();
    let _ = std::fs::remove_dir_all(&root);
    assert!(message.contains("`kit` is a telar library"), "{message}");
    assert!(message.contains("Whoever packages `kit`"), "{message}");
    assert!(!message.contains("cargo install"), "{message}");
}

/// A library is transpiled against no theme type, so an invocation naming one is told why rather than compared against an artifact that records none.
#[test]
fn a_library_naming_a_theme_type_is_refused() {
    let root = package("library_named_theme");
    std::fs::write(root.join("telar.toml"), "[telar]\nlibrary = true\n").unwrap();
    let telar = telar_project::TelarManifest::load(&root).unwrap().telar;
    let named = super::check_theme_agrees(&telar, Some("crate::KitTheme"));
    let unnamed = super::check_theme_agrees(&telar, None);
    let _ = std::fs::remove_dir_all(&root);

    let message = named.err().unwrap().to_string();
    assert!(message.contains("`[telar] library`"), "{message}");
    assert!(message.contains("crate::KitTheme"), "{message}");
    assert!(message.contains("ThemeTokens"), "{message}");
    assert!(unnamed.is_ok());
}

/// The key an application declares is still compared, and a library has none for the comparison to find.
#[test]
fn only_an_application_has_a_declared_theme_to_agree_with() {
    let app = package("app_declared_theme");
    std::fs::write(
        app.join("telar.toml"),
        "[telar]\ntheme = \"crate::AppTheme\"\n",
    )
    .unwrap();
    let app_telar = telar_project::TelarManifest::load(&app).unwrap().telar;
    let agrees = super::check_theme_agrees(&app_telar, Some("crate::AppTheme"));
    let disagrees = super::check_theme_agrees(&app_telar, Some("crate::Other"));
    let library = package("library_declared_theme");
    std::fs::write(library.join("telar.toml"), "[telar]\nlibrary = true\n").unwrap();
    let library_theme = telar_project::theme_type_in_config(
        &telar_project::TelarManifest::load(&library).unwrap().telar,
    );
    let _ = std::fs::remove_dir_all(&app);
    let _ = std::fs::remove_dir_all(&library);

    assert!(agrees.is_ok());
    assert!(disagrees.is_err());
    assert_eq!(library_theme, None);
}

fn files_under(dir: &std::path::Path) -> Vec<(std::path::PathBuf, std::time::SystemTime)> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(files_under(&path));
        }
        found.push((
            path.clone(),
            std::fs::metadata(&path).unwrap().modified().unwrap(),
        ));
    }
    found.sort();
    found
}

fn set_read_only(dir: &std::path::Path, read_only: bool) {
    for (path, _) in files_under(dir)
        .into_iter()
        .chain([(dir.to_path_buf(), std::time::SystemTime::UNIX_EPOCH)])
    {
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(read_only);
        std::fs::set_permissions(&path, permissions).unwrap();
    }
}

/// A library unpacked from a registry: transpiled and given its module tree elsewhere, then wired from a directory nothing may write to, in a build whose features asked for hot reload and previews.
#[test]
fn a_library_dependency_is_wired_from_a_read_only_package_without_writing() {
    let root = transpiled_under("library_read_only", &[]);
    std::fs::write(root.join("src/lib.rs"), "telar::rsx_modules!();\n").unwrap();
    std::fs::write(
        root.join(telar_project::MANIFEST_FILENAME),
        "[telar]\nlibrary = true\n",
    )
    .unwrap();
    telar_project::ModuleTree::discover(
        &root.join("src"),
        &telar_project::generated_dir(&root, BuildFlavour::Plain),
        BuildFlavour::Plain.site_file_name(),
    )
    .write()
    .unwrap();
    let before = files_under(&root);
    set_read_only(&root, true);

    let as_dependency = super::wire_package(
        root.clone(),
        "kit".into(),
        false,
        BuildFlavour::Hot,
        super::Invocation::Modules,
        None,
    );
    let selected = super::wire_package(
        root.clone(),
        "kit".into(),
        true,
        BuildFlavour::Hot,
        super::Invocation::Modules,
        None,
    );

    let after = files_under(&root);
    set_read_only(&root, false);
    let _ = std::fs::remove_dir_all(&root);

    let output = as_dependency.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(output.flavour, BuildFlavour::Plain);
    assert!(
        output
            .include_stmts
            .to_string()
            .contains("\".telar/modules.rs\""),
        "{}",
        output.include_stmts
    );
    assert_eq!(before, after, "nothing under the package was written");
    let refused = selected
        .err()
        .expect("selected, it asks for the hot artifact it was never given")
        .to_string();
    assert!(refused.contains("no transpiled `.rsx`"), "{refused}");
}

/// The macro no longer writes the module tree, so a Rust module added since the CLI ran is reported, not silently left undeclared.
#[test]
fn a_module_tree_older_than_the_sources_is_refused() {
    let root = transpiled_under("stale_tree", &[]);
    std::fs::write(root.join("src/lib.rs"), "telar::rsx_modules!();\n").unwrap();
    let generated = telar_project::generated_dir(&root, BuildFlavour::Plain);
    let discover = || {
        telar_project::ModuleTree::discover(
            &root.join("src"),
            &generated,
            BuildFlavour::Plain.site_file_name(),
        )
    };
    let package = super::Package {
        dir: root.clone(),
        name: "kit".into(),
        wiring: super::Wiring::decide(false, true, BuildFlavour::Plain),
    };
    let missing = super::check_module_tree(&package, &discover())
        .err()
        .unwrap()
        .to_string();
    discover().write().unwrap();
    let current = super::check_module_tree(&package, &discover());
    std::fs::write(root.join("src/extra.rs"), "").unwrap();
    let stale = super::check_module_tree(&package, &discover())
        .err()
        .unwrap()
        .to_string();
    let _ = std::fs::remove_dir_all(&root);

    assert!(missing.contains("has no module tree"), "{missing}");
    assert!(current.is_ok());
    assert!(stale.contains("no longer matches `src/`"), "{stale}");
    assert!(stale.contains("cargo telar transpile"), "{stale}");
}

/// A package with no `.rsx` whose catalog the CLI baked with `keys`, wired for `flavour`: what `rsx_modules!` sees in a crate that only translates.
fn with_baked_catalog(
    name: &str,
    telar_toml: &str,
    flavour: telar_project::BuildFlavour,
    keys: &[&str],
) -> std::path::PathBuf {
    let root = package(name);
    std::fs::write(root.join("src/lib.rs"), "telar::rsx_modules!();\n").unwrap();
    std::fs::write(root.join(telar_project::MANIFEST_FILENAME), telar_toml).unwrap();
    let telar_dir = root.join(".telar");
    std::fs::create_dir_all(&telar_dir).unwrap();
    let index = telar_project::CatalogIndex {
        format: telar_project::CATALOG_ARTIFACT_FORMAT,
        producer: "test".to_string(),
        telar_version: env!("CARGO_PKG_VERSION").to_string(),
        entries: keys
            .iter()
            .map(|key| telar_project::CatalogEntry {
                key: key.to_string(),
                args: Vec::new(),
            })
            .collect(),
        sources: Vec::new(),
    };
    std::fs::write(
        telar_dir.join(telar_project::CATALOG_INDEX_FILENAME),
        index.to_json(),
    )
    .unwrap();
    std::fs::write(
        telar_dir.join(telar_project::CATALOG_SOURCE_FILENAME),
        "// generated\n",
    )
    .unwrap();
    telar_project::ModuleTree::discover(
        &root.join("src"),
        &telar_project::generated_dir(&root, flavour),
        flavour.site_file_name(),
    )
    .write()
    .unwrap();
    root
}

/// The function the expansion's load-time constructor installs its catalog with, or `None` when it emits no constructor.
fn catalog_installer(
    root: &std::path::Path,
    flavour: telar_project::BuildFlavour,
    invocation: super::Invocation,
) -> Option<&'static str> {
    let output = super::wire_package(
        root.to_path_buf(),
        "kit".into(),
        true,
        flavour,
        invocation,
        None,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let wired = output.include_stmts.to_string();
    ["set_catalog_if_unset", "set_catalog"]
        .into_iter()
        .find(|installer| wired.contains(&format!("{installer} (& self :: __rsx_i18n :: CATALOG)")))
}

/// The hot flavours are the dylib a reload swaps in: it carries its own constructor, so every load installs the catalog it holds into its own copy of the runtime.
#[test]
fn an_application_catalog_is_installed_as_its_binary_loads_in_every_flavour() {
    use telar_project::BuildFlavour;
    for flavour in BuildFlavour::ALL {
        let root = with_baked_catalog(
            &format!("installs_{flavour:?}"),
            "[telar]\n",
            flavour,
            &["greeting"],
        );
        let app = catalog_installer(&root, flavour, super::Invocation::App);
        let modules = catalog_installer(&root, flavour, super::Invocation::Modules);
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(app, Some("set_catalog"), "{flavour:?}");
        assert_eq!(modules, Some("set_catalog_if_unset"), "{flavour:?}");
    }
}

/// A library's strings are what the application overrides, so its catalog must never take the application's place.
#[test]
fn a_library_catalog_is_never_installed() {
    let flavour = telar_project::BuildFlavour::Plain;
    let root = with_baked_catalog(
        "library_catalog",
        "[telar]\nlibrary = true\n",
        flavour,
        &["greeting"],
    );
    let installer = catalog_installer(&root, flavour, super::Invocation::Modules);
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(installer, None);
}

/// The CLI bakes an empty catalog for an application with no `locales/`, and there is nothing in it to install.
#[test]
fn an_empty_catalog_is_not_installed() {
    let flavour = telar_project::BuildFlavour::Plain;
    let root = with_baked_catalog("empty_catalog", "[telar]\n", flavour, &[]);
    let installer = catalog_installer(&root, flavour, super::Invocation::App);
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(installer, None);
}

const ICON_NOTICE: &str = "Icons in kit\n\nmdi — Material Design Icons\n";

/// The text the expansion compiles into the binary, or `None` when it emits no installer.
fn icon_notice_installer(
    root: &std::path::Path,
    flavour: telar_project::BuildFlavour,
    invocation: super::Invocation,
) -> Option<String> {
    let output = super::wire_package(
        root.to_path_buf(),
        "kit".into(),
        true,
        flavour,
        invocation,
        None,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let wired = output.include_stmts.to_string();
    let rerun = output.rerun_stmts.to_string();
    let notice = telar_project::icon_notice_file(root)?;
    let path = notice.to_string_lossy().to_string();
    let installed = wired.contains(&format!(
        ":: telar :: __install_icon_licenses (include_str ! ({path:?}))"
    ));
    assert_eq!(
        installed,
        rerun.contains(&format!("include_str ! ({path:?})"))
    );
    installed.then(|| std::fs::read_to_string(notice).unwrap())
}

fn write_icon_notice(root: &std::path::Path) {
    std::fs::write(
        root.join(".telar")
            .join(telar_project::ICONS_NOTICE_FILENAME),
        ICON_NOTICE,
    )
    .unwrap();
}

/// Every flavour, the hot ones included: the dylib a reload swaps in carries its own constructor and installs the notice into its own copy of the facade.
#[test]
fn an_application_compiles_its_icon_notice_in_and_installs_it_as_its_binary_loads() {
    use telar_project::BuildFlavour;
    for flavour in BuildFlavour::ALL {
        let root = with_baked_catalog(
            &format!("icon_notice_{flavour:?}"),
            "[telar]\n",
            flavour,
            &[],
        );
        write_icon_notice(&root);
        let app = icon_notice_installer(&root, flavour, super::Invocation::App);
        let modules = icon_notice_installer(&root, flavour, super::Invocation::Modules);
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(app.as_deref(), Some(ICON_NOTICE), "{flavour:?}");
        assert_eq!(modules.as_deref(), Some(ICON_NOTICE), "{flavour:?}");
    }
}

/// A bake that found no icons removes the notice, and an application without one installs nothing, so `licenses()` answers `None`.
#[test]
fn an_application_that_baked_no_icons_installs_no_notice() {
    let flavour = telar_project::BuildFlavour::Plain;
    let root = with_baked_catalog("no_icon_notice", "[telar]\n", flavour, &[]);
    let wired = super::wire_package(
        root.clone(),
        "kit".into(),
        true,
        flavour,
        super::Invocation::App,
        None,
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .include_stmts
    .to_string();
    let _ = std::fs::remove_dir_all(&root);

    assert!(!wired.contains("__install_icon_licenses"), "{wired}");
}

/// The application's notice lists the library's icons already, and a library installing its own would replace it.
#[test]
fn a_library_never_installs_an_icon_notice() {
    let flavour = telar_project::BuildFlavour::Plain;
    let root = with_baked_catalog(
        "library_icon_notice",
        "[telar]\nlibrary = true\n",
        flavour,
        &[],
    );
    write_icon_notice(&root);
    let installer = icon_notice_installer(&root, flavour, super::Invocation::Modules);
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(installer, None);
}

fn spaced(tokens: proc_macro2::TokenStream) -> String {
    tokens.to_string()
}

/// A crate with Rust previews defines its own `telar_all_previews` beside the macro call, which only an item reached through a glob lets it shadow.
#[test]
fn the_rsx_table_is_a_default_a_crate_can_shadow() {
    let consts = [quote::quote! { crate::card::CARD_PREVIEW_ENTRIES }];
    let table = spaced(super::previews_table(&consts));
    assert!(table.starts_with(":: telar :: __previews !"), "{table}");
    assert!(table.contains("pub use __telar_previews :: * ;"), "{table}");
    assert!(
        table.contains("pub (crate) fn telar_rsx_previews ()"),
        "{table}"
    );
    assert!(
        table.contains("crate :: card :: CARD_PREVIEW_ENTRIES"),
        "{table}"
    );
    assert!(table.contains("pub fn telar_all_previews ()"), "{table}");
}

/// A library compiled as a dependency is wired from its Plain artifact whatever the build asked for, and that artifact carries its previews behind the gate, so its table names them for the application that lists it.
#[test]
fn a_library_dependency_lists_its_rsx_previews() {
    let root = transpiled("dependency_previews", &[], true);
    std::fs::write(root.join("src/lib.rs"), "telar::rsx_modules!();\n").unwrap();
    std::fs::write(
        root.join(telar_project::MANIFEST_FILENAME),
        "[telar]\nlibrary = true\n",
    )
    .unwrap();
    telar_project::ModuleTree::discover(
        &root.join("src"),
        &telar_project::generated_dir(&root, BuildFlavour::Plain),
        BuildFlavour::Plain.site_file_name(),
    )
    .write()
    .unwrap();

    let output = super::wire_package(
        root.clone(),
        "kit".into(),
        false,
        BuildFlavour::Hot,
        super::Invocation::Modules,
        None,
    );
    let _ = std::fs::remove_dir_all(&root);

    let output = output.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(output.flavour, BuildFlavour::Plain);
    let table = spaced(super::previews_table(&output.preview_const_idents));
    assert!(
        table.contains("crate :: home :: HOME_PREVIEW_ENTRIES"),
        "{table}"
    );
}

#[test]
fn a_library_tests_its_previews_from_a_unit_test_at_the_call_site() {
    let fonts = quote::quote! { ::std::vec::Vec::new() };
    let test = spaced(super::library_previews_test(fonts));
    assert!(test.starts_with(":: telar :: __previews !"), "{test}");
    assert!(
        test.contains("# [cfg (test)] # [test] fn telar_previews ()"),
        "{test}"
    );
    assert!(
        test.contains(":: telar :: preview :: test_run :: __test_library (telar_all_previews () , :: telar :: AppConfig :: default () . with_fonts (:: std :: vec :: Vec :: new ())"),
        "{test}"
    );
}

#[test]
fn an_application_lists_its_previews_then_each_included_crates() {
    let includes = ["telar_components", "my_plugin"]
        .map(|name| proc_macro2::Ident::new(name, proc_macro2::Span::call_site()));
    let collected = spaced(super::app_previews_fn(&includes));
    assert!(
        collected.contains("[telar_all_previews () , :: telar_components :: telar_all_previews () , :: my_plugin :: telar_all_previews ()] . concat ()"),
        "{collected}"
    );
}

/// The workshop is named only under its own feature, and only for a package that declares it, so a package without it never sees a feature cfg it does not define.
#[test]
fn the_preview_shell_is_the_workshop_only_under_its_feature() {
    let fallback = spaced(super::preview_shell(None));
    assert_eq!(fallback, ":: telar :: preview :: host :: PreviewApp :: new");

    let krate = proc_macro2::Ident::new("telar_workshop", proc_macro2::Span::call_site());
    let shell = spaced(super::preview_shell(Some(&krate)));
    assert!(
        shell.contains("# [cfg (feature = \"telar-workshop\")] let shell = | entries | :: telar_workshop :: WorkshopApp :: new (entries) . project_name (:: core :: env ! (\"CARGO_PKG_NAME\")) ;"),
        "{shell}"
    );
    assert!(
        shell.contains("# [cfg (not (feature = \"telar-workshop\"))] let shell = :: telar :: preview :: host :: PreviewApp :: new ;"),
        "{shell}"
    );
}
