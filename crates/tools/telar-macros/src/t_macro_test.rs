use super::{library_namespace, lookup};

fn package(name: &str, manifest: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("telar_t_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(telar_project::MANIFEST_FILENAME), manifest).unwrap();
    root
}

#[test]
fn an_application_looks_its_strings_up_plainly() {
    let root = package("app", "[telar]\nbackend = \"auto\"\n");
    let namespace = library_namespace(&root, "my-app");
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(namespace, None);
    let call = lookup(None, "greeting", &[]).to_string().replace(' ', "");
    assert!(
        call.contains("::telar::i18n::translate(&crate::__rsx_i18n::CATALOG,\"greeting\""),
        "{call}"
    );
}

/// The namespace is the package as a crate name, which is how an application's catalog keys a plugin's strings — `telar_components.close` — however the package spells its hyphens.
#[test]
fn a_library_looks_its_strings_up_under_its_crate_name() {
    let root = package("library", "[telar]\nlibrary = true\n");
    let namespace = library_namespace(&root, "my-kit");
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(namespace.as_deref(), Some("my_kit"));

    let name: syn::Ident = syn::parse_quote!(name);
    let value: syn::Expr = syn::parse_quote!(who);
    let call = lookup(namespace.as_deref(), "hello", &[(name, value)])
        .to_string()
        .replace(' ', "");
    assert!(
        call.contains(
            "::telar::i18n::translate_with_override(\"my_kit\",&crate::__rsx_i18n::CATALOG,\"hello\",&[(\"name\",__rsx_t_arg_0.as_str())])"
        ),
        "{call}"
    );
}
