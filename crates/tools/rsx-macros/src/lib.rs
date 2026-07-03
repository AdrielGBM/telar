use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::{ToTokens, quote};
use std::path::PathBuf;

mod app_input;
use app_input::{AppInput, preview_const_ident};

#[proc_macro]
pub fn app(input: TokenStream) -> TokenStream {
    let AppInput {
        theme_type,
        setup,
        config,
        app_expr,
    } = match syn::parse::<AppInput>(input) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error().into(),
    };

    // The transpiler has no runtime access to the theme type, so it is passed as a source string; to_string inserts spaces around `::` so we collapse them for a valid turbofish.
    let theme_type_str = theme_type
        .to_token_stream()
        .to_string()
        .replace(" :: ", "::");

    let manifest_dir = match std::env::var("CARGO_MANIFEST_DIR") {
        Ok(d) => PathBuf::from(d),
        Err(_) => {
            return quote! { compile_error!("CARGO_MANIFEST_DIR not set") }.into();
        }
    };

    let generated_dir = manifest_dir.join(".rsx").join("build");
    if let Err(e) = std::fs::create_dir_all(&generated_dir) {
        let msg = format!("Failed to create .rsx/build/: {e}");
        return quote! { compile_error!(#msg) }.into();
    }

    let src_dir = manifest_dir.join("src");
    let rsx_files = rsx_transpiler::find_rsx_files(&src_dir);

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

        let stem = rsx_transpiler::relative_stem(rsx_file, &src_dir);

        let result = match rsx_transpiler::transpile_source_with_theme(
            &source,
            &stem,
            Some(theme_type_str.as_str()),
            rsx_file.parent(),
        ) {
            Ok(r) => r,
            Err(rsx_transpiler::TranspileError::Parse(ref pe)) => {
                let msg = format!("{}:{}: {}", rsx_file.display(), pe.line, pe.message);
                return quote! { compile_error!(#msg) }.into();
            }
            Err(e) => {
                let msg = format!("Failed to transpile {}: {e}", rsx_file.display());
                return quote! { compile_error!(#msg) }.into();
            }
        };

        // Mirror the source tree under .rsx/build/ so files in different directories never collide. find_rsx_files only yields paths under src_dir, so None is unreachable here.
        let Some(rel_out) = rsx_transpiler::relative_output_path(rsx_file, &src_dir) else {
            continue;
        };
        let out_path = generated_dir.join(rel_out);
        if let Some(parent) = out_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                let msg = format!("Failed to create {}: {e}", parent.display());
                return quote! { compile_error!(#msg) }.into();
            }
        }

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

        // Persist the per-line source map next to the build file so the editor extension can map
        // rust-analyzer's diagnostics on the generated Rust back onto the original `.rsx` lines.
        let map_path = out_path.with_extension("rs.map");
        let map_json = rsx_transpiler::source_map_to_json(&result.source_map);
        let map_stale = std::fs::read_to_string(&map_path)
            .map(|existing| existing != map_json)
            .unwrap_or(true);
        if map_stale {
            let _ = std::fs::write(&map_path, &map_json);
        }

        // Wire each generated file as a real `#[path] mod` (not `include!`) so rust-analyzer treats it
        // as a first-class module and offers completion inside it; `pub use` keeps the component fns,
        // preview consts and `Props` types reachable by bare name, exactly as `include!` did.
        let out_path_str = out_path.to_string_lossy().to_string();
        let mod_ident = Ident::new(
            &format!("__rsx_mod_{}", rsx_transpiler::naming::to_snake_case(&stem)),
            Span::call_site(),
        );
        include_stmts.extend(quote! {
            #[path = #out_path_str]
            mod #mod_ident;
            #[allow(unused_imports)]
            pub use #mod_ident::*;
        });

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

    // Detected at macro expansion time: cargo-rsx sets these env vars.
    let is_hot_reload = std::env::var("RSX_HOT_RELOAD_BUILD").is_ok();
    let is_preview = std::env::var("RSX_PREVIEW_BUILD").is_ok();

    let run_tail = quote! {
        #setup
        if ::std::env::var("RSX_PREVIEW_LIST").is_ok() {
            for entry in rsx_all_preview_entries() {
                ::std::println!("{}\t{}", entry.component_name, entry.preview_name);
            }
            ::std::process::exit(0);
        }
        if ::std::env::var("RSX_TEST").is_ok() {
            ::rsx::try_run_test(rsx_all_preview_entries(), ::rsx::AppConfig::from(#config));
        }
        if ::std::env::var("RSX_PREVIEW").is_ok() {
            if ::rsx::try_run_preview(rsx_all_preview_entries(), ::rsx::AppConfig::from(#config)) {
                return;
            }
        }
        ::rsx::run_app_with_name(
            ::rsx::AppConfig::from(#config),
            #app_expr,
            env!("CARGO_PKG_NAME"),
        )
    };

    let hot_reload_prefix = if is_hot_reload {
        quote! {
            if let (::std::result::Result::Ok(lib_path), ::std::result::Result::Ok(hot_port)) = (
                ::std::env::var("RSX_HOT_LIB"),
                ::std::env::var("RSX_HOT_PORT"),
            ) {
                #setup
                ::rsx::run_hot_reload_host(
                    &lib_path,
                    &hot_port,
                    ::rsx::AppConfig::from(#config),
                    env!("CARGO_PKG_NAME"),
                );
                return;
            }
        }
    } else {
        quote! {}
    };

    let desktop_run = quote! {
        #[cfg(not(target_os = "android"))]
        pub fn run() {
            #hot_reload_prefix
            #run_tail
        }
    };

    // Only emitted under RSX_HOT_RELOAD_BUILD so dlopen can find the factory; RSX_PREVIEW_BUILD lets the macro branch here without leaking a custom cfg into generated output (--cfg=rsx_preview in RUSTFLAGS is only for cache-busting recompilation when switching modes).
    let hot_export = if is_hot_reload {
        let body: TokenStream2 = if is_preview {
            quote! {
                return ::rsx::make_hot_preview_app(rsx_all_preview_entries());
            }
        } else {
            quote! {
                return ::std::boxed::Box::new(#app_expr);
            }
        };
        quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_create_app() -> ::std::boxed::Box<dyn ::rsx::App> {
                #setup
                #body
            }
        }
    } else {
        quote! {}
    };

    // Cleanup function exported for hot reload: called before dlclose to clean up TLS in the dylib.
    let hot_cleanup = if is_hot_reload {
        quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_cleanup() {
                // Drop in-flight animations alongside the signals they target so none outlive this dylib's reset runtime.
                ::rsx::motion::reset();
                ::rsx::reset_runtime();
            }
        }
    } else {
        quote! {}
    };

    // State-preservation symbols: the host snapshots the outgoing dylib's hot signals and restores them into the incoming one (see rsx::hot_state).
    let hot_state_symbols = if is_hot_reload {
        quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_snapshot() -> ::std::string::String {
                ::rsx::hot_snapshot_json()
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_restore(blob: &str) {
                ::rsx::hot_restore_json(blob);
            }
        }
    } else {
        quote! {}
    };

    // Motion-tick symbols: host and dylib link separate copies of motion-core, each with its own registry; the `Animated` values live in the dylib's, so the host must call across this boundary instead of ticking its own (empty) copy.
    let hot_motion_symbols = if is_hot_reload {
        quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_motion_tick(now: ::std::time::Instant) {
                ::rsx::motion::tick(now);
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_motion_active() -> bool {
                ::rsx::motion::has_active()
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
        #hot_cleanup
        #hot_state_symbols
        #hot_motion_symbols
    }
    .into()
}
