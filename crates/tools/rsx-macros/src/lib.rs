use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::quote;
use std::path::{Path, PathBuf};
use syn::{
    Token,
    parse::{Parse, ParseStream, Result as ParseResult},
};

struct AppInput {
    setup: syn::Block,
    config: syn::Expr,
    app_expr: syn::Expr,
}

impl Parse for AppInput {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        let setup = input.parse::<syn::Block>()?;
        input.parse::<Token![,]>()?;
        let config = input.parse::<syn::Expr>()?;
        input.parse::<Token![,]>()?;
        let app_expr = input.parse::<syn::Expr>()?;
        let _ = input.parse::<Token![,]>();
        Ok(AppInput {
            setup,
            config,
            app_expr,
        })
    }
}

fn rsx_to_snake_case(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c {
            '-' | ' ' => Some('_'),
            '_' => Some('_'),
            c if c.is_ascii_alphanumeric() => Some(c.to_ascii_lowercase()),
            _ => None,
        })
        .collect()
}

fn preview_const_ident(file_stem: &str) -> Ident {
    let name = format!(
        "{}_PREVIEW_ENTRIES",
        rsx_to_snake_case(file_stem).to_ascii_uppercase()
    );
    Ident::new(&name, Span::call_site())
}

fn find_rsx_files(dir: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                result.extend(find_rsx_files(&path));
            } else if path.extension().map(|e| e == "rsx").unwrap_or(false) {
                result.push(path);
            }
        }
    }
    result.sort();
    result
}

#[proc_macro]
pub fn app(input: TokenStream) -> TokenStream {
    let AppInput {
        setup,
        config,
        app_expr,
    } = match syn::parse::<AppInput>(input) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error().into(),
    };

    let manifest_dir = match std::env::var("CARGO_MANIFEST_DIR") {
        Ok(d) => PathBuf::from(d),
        Err(_) => {
            return quote! { compile_error!("CARGO_MANIFEST_DIR not set") }.into();
        }
    };

    let generated_dir = manifest_dir.join(".rsx");
    if let Err(e) = std::fs::create_dir_all(&generated_dir) {
        let msg = format!("Failed to create .rsx/: {e}");
        return quote! { compile_error!(#msg) }.into();
    }

    let src_dir = manifest_dir.join("src");
    let rsx_files = find_rsx_files(&src_dir);

    let mut include_stmts = TokenStream2::new();
    let mut rerun_stmts = TokenStream2::new();
    let mut preview_const_idents: Vec<Ident> = Vec::new();

    for rsx_file in &rsx_files {
        let source = match std::fs::read_to_string(rsx_file) {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("Failed to read {}: {e}", rsx_file.display());
                return quote! { compile_error!(#msg) }.into();
            }
        };

        let stem = rsx_file
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let result = match rsx_transpiler::transpile_source(&source, &stem) {
            Ok(r) => r,
            Err(e) => {
                let msg = format!("Failed to transpile {}: {e}", rsx_file.display());
                return quote! { compile_error!(#msg) }.into();
            }
        };

        let out_path = generated_dir.join(format!("{stem}.rs"));

        // Only write when content changed to avoid spurious recompilation.
        let needs_write = std::fs::read_to_string(&out_path)
            .map(|existing| existing != result.rust_code)
            .unwrap_or(true);
        if needs_write {
            if let Err(e) = std::fs::write(&out_path, &result.rust_code) {
                let msg = format!("Failed to write {}: {e}", out_path.display());
                return quote! { compile_error!(#msg) }.into();
            }
        }

        let out_path_str = out_path.to_string_lossy().to_string();
        include_stmts.extend(quote! { include!(#out_path_str); });

        let rsx_path_str = rsx_file.to_string_lossy().to_string();
        rerun_stmts.extend(quote! { const _: &str = include_str!(#rsx_path_str); });

        if !result.preview_names.is_empty() {
            preview_const_idents.push(preview_const_ident(&stem));
        }
    }

    let preview_fn = quote! {
        pub fn rsx_all_preview_entries() -> ::std::vec::Vec<::rsx::PreviewEntry> {
            let mut entries = ::std::vec::Vec::new();
            #( entries.extend_from_slice(#preview_const_idents); )*
            entries
        }
    };

    // Detected at macro expansion time: cargo-rsx sets this env var for hot reload builds.
    let is_hot_reload = std::env::var("RSX_HOT_RELOAD_BUILD").is_ok();

    let desktop_run = if is_hot_reload {
        quote! {
            #[cfg(not(target_os = "android"))]
            pub fn run() {
                if let (::std::result::Result::Ok(lib_path), ::std::result::Result::Ok(socket_path)) = (
                    ::std::env::var("RSX_HOT_LIB"),
                    ::std::env::var("RSX_HOT_SOCKET"),
                ) {
                    #setup
                    ::rsx::run_hot_reload_host(
                        &lib_path,
                        &socket_path,
                        ::rsx::AppConfig::from(#config),
                        env!("CARGO_PKG_NAME"),
                    );
                    return;
                }
                #setup
                if ::std::env::var("RSX_PREVIEW").is_ok() {
                    if ::rsx::try_run_preview(rsx_all_preview_entries()) {
                        return;
                    }
                }
                ::rsx::run_app_with_name(
                    ::rsx::AppConfig::from(#config),
                    #app_expr,
                    env!("CARGO_PKG_NAME"),
                )
            }
        }
    } else {
        quote! {
            #[cfg(not(target_os = "android"))]
            pub fn run() {
                #setup
                if ::std::env::var("RSX_PREVIEW").is_ok() {
                    if ::rsx::try_run_preview(rsx_all_preview_entries()) {
                        return;
                    }
                }
                ::rsx::run_app_with_name(
                    ::rsx::AppConfig::from(#config),
                    #app_expr,
                    env!("CARGO_PKG_NAME"),
                )
            }
        }
    };

    // Only emitted when cargo-rsx sets RSX_HOT_RELOAD_BUILD so dlopen can find the app factory.
    // The preview branch is gated on cfg(rsx_preview) — a custom cfg flag that cargo-rsx injects
    // via RUSTFLAGS (--cfg=rsx_preview). Using a RUSTFLAG (not an env var) is important because
    // Cargo uses RUSTFLAGS in its compilation fingerprint, so changing between dev and preview
    // always forces a recompile of the crate. An env-var-only approach would silently reuse the
    // cached dylib without the preview branch.
    let hot_export = if is_hot_reload {
        quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_create_app() -> ::std::boxed::Box<dyn ::rsx::App> {
                #setup
                #[cfg(rsx_preview)]
                {
                    return ::rsx::make_hot_preview_app(rsx_all_preview_entries());
                }
                ::std::boxed::Box::new(#app_expr)
            }
        }
    } else {
        quote! {}
    };

    let android_run = quote! {
        #[cfg(target_os = "android")]
        #[unsafe(no_mangle)]
        fn android_main(android_app: ::rsx::AndroidApp) {
            #setup
            ::rsx::run_android_app_with_name(
                ::rsx::AppConfig::from(#config),
                #app_expr,
                env!("CARGO_PKG_NAME"),
                android_app,
            );
        }
    };

    quote! {
        #rerun_stmts
        #include_stmts
        #preview_fn
        #desktop_run
        #android_run
        #hot_export
    }
    .into()
}
