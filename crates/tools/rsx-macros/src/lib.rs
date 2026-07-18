use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::{ToTokens, quote};
use std::path::PathBuf;

mod app_input;
mod t_macro;
use app_input::{AppInput, preview_const_ident};

/// Translates a catalog key to a `String`, substituting named arguments: `t!("battery.remaining", time = t)`.
///
/// The key and its arguments are validated against the on-disk `locales/` catalog at compile time (an unknown
/// key or wrong argument is a `compile_error!`). At runtime it reads the active locale reactively, so calling it
/// inside a widget's content closure makes that widget re-render on a language switch.
#[proc_macro]
pub fn t(input: TokenStream) -> TokenStream {
    match syn::parse::<t_macro::TInput>(input) {
        Ok(parsed) => t_macro::expand(parsed).into(),
        Err(e) => e.to_compile_error().into(),
    }
}

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

    let TranspileOutput {
        include_stmts,
        rerun_stmts,
        preview_const_idents,
    } = match transpile_project(Some(theme_type_str.as_str())) {
        Ok(o) => o,
        Err(err) => return err.into(),
    };

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
            // Batch the dylib's reactive runtime around event dispatch: host and dylib link separate reactive-core copies, so the host must open/close the batch on the app's own runtime across this boundary — otherwise a handler's signal write flushes mid-dispatch and a segment loses its subscriptions while its widget is borrowed.
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_begin_batch() {
                ::rsx::begin_batch();
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_end_batch() {
                ::rsx::end_batch();
            }
            // Relayout the dylib's own layout runtime: the layout tree (taffy nodes) lives in the dylib's
            // thread-local runtime, so the host must drive relayout across this boundary for a reactive
            // list change to be laid out — its own copy is empty.
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_relayout() {
                ::rsx::relayout_if_dirty();
            }
            // Consult the dylib's own overlay registry: `overlay` widgets register in this dylib's
            // thread-local, so the host must route pointer events to overlays (modal priority / background
            // blocking) across this boundary — its own copy is empty.
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_dispatch_overlays(event: &::rsx::Event) -> bool {
                ::rsx::dispatch_overlays(event)
            }
            // Write the OS light/dark preference into the dylib's own theme runtime (where `follow_system`'s
            // effect lives), across the same boundary the host cannot reach directly.
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_set_system_dark(dark: bool) {
                ::rsx::set_system_dark(dark);
            }
            // Drain the dylib's own window-command queue: a title bar's `on_press` pushes into this dylib's
            // thread-local, so the host must drain it across this boundary to apply drag/minimize/maximize/
            // close — its own copy is empty.
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_drain_window_commands()
            -> ::std::vec::Vec<::rsx::WindowCommand> {
                ::rsx::take_window_commands()
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

struct TranspileOutput {
    include_stmts: TokenStream2,
    rerun_stmts: TokenStream2,
    preview_const_idents: Vec<Ident>,
}

// Transpiles every `.rsx` file under `src/` into `.rsx/build/`, wiring each as a `#[path] mod` and aliasing
// nested components to their basenames; also emits `include_str!` rerun triggers and (via `auto_modules`)
// declares the hand-written `.rs` module tree. Shared by `app!` (which then adds the runner) and
// `rsx_modules!` (transpilation only). `theme_type_str` types the transpiler's `use_theme` calls; pass `None`
// when no theme is in scope. `Err` carries a `compile_error!` token stream for the caller to emit.
fn transpile_project(theme_type_str: Option<&str>) -> Result<TranspileOutput, TokenStream2> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .map_err(|_| quote! { compile_error!("CARGO_MANIFEST_DIR not set") })?;

    let generated_dir = manifest_dir.join(".rsx").join("build");
    if let Err(e) = std::fs::create_dir_all(&generated_dir) {
        let msg = format!("Failed to create .rsx/build/: {e}");
        return Err(quote! { compile_error!(#msg) });
    }

    let src_dir = manifest_dir.join("src");
    let rsx_files = rsx_transpiler::find_rsx_files(&src_dir);
    // Baked `src:"..."` asset paths resolve against one project asset root (default `./assets`), not each
    // `.rsx`'s own directory — see `[rsx] assets` in rsx.toml.
    let assets_root = rsx_transpiler::assets_root(&manifest_dir);

    // Pre-pass: collect every component's signature (its Props shape + whether it takes a slot) so each file's
    // transpile can emit calls to other components correctly — optional props and the slot arg both need the
    // callee's shape, which lives in another file. Keyed by both the path-flattened stem and the bare basename.
    let mut registry = rsx_transpiler::ComponentRegistry::new();
    // Seed the built-in component catalogue first so a local `.rsx` of the same name still overrides it.
    for (name, sig) in rsx_transpiler::external_component_sigs() {
        registry.insert(name.to_string(), sig);
    }
    for rsx_file in &rsx_files {
        let Ok(source) = std::fs::read_to_string(rsx_file) else {
            continue;
        };
        let sig = rsx_transpiler::scan_component_sig(&source);
        let stem = rsx_transpiler::relative_stem(rsx_file, &src_dir);
        registry.insert(rsx_transpiler::naming::to_snake_case(&stem), sig.clone());
        if let Some(base) = rsx_file.file_stem().and_then(|s| s.to_str()) {
            registry
                .entry(rsx_transpiler::naming::to_snake_case(base))
                .or_insert(sig);
        }
    }

    let mut include_stmts = TokenStream2::new();
    let mut rerun_stmts = TokenStream2::new();
    let mut preview_const_idents: Vec<Ident> = Vec::new();

    for rsx_file in &rsx_files {
        let source = match std::fs::read_to_string(rsx_file) {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("Failed to read {}: {e}", rsx_file.display());
                return Err(quote! { compile_error!(#msg) });
            }
        };

        let stem = rsx_transpiler::relative_stem(rsx_file, &src_dir);

        let result = match rsx_transpiler::transpile_source_full(
            &source,
            &stem,
            theme_type_str,
            Some(assets_root.as_path()),
            Some(&registry),
        ) {
            Ok(r) => r,
            Err(rsx_transpiler::TranspileError::Parse(ref pe)) => {
                let msg = format!("{}:{}: {}", rsx_file.display(), pe.line, pe.message);
                return Err(quote! { compile_error!(#msg) });
            }
            Err(e) => {
                let msg = format!("Failed to transpile {}: {e}", rsx_file.display());
                return Err(quote! { compile_error!(#msg) });
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
                return Err(quote! { compile_error!(#msg) });
            }
        }

        // Only write when content changed to avoid spurious recompilation.
        let needs_write = std::fs::read_to_string(&out_path)
            .map(|existing| existing != result.rust_code)
            .unwrap_or(true);
        if needs_write {
            if let Err(e) = std::fs::write(&out_path, &result.rust_code) {
                let msg = format!("Failed to write {}: {e}", out_path.display());
                return Err(quote! { compile_error!(#msg) });
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

        // Wire each generated file as a real `#[path] mod` (not `include!`) so rust-analyzer treats it as a
        // first-class module and offers completion inside it; `pub use` keeps the component fns, preview consts
        // and `Props` types reachable by bare name, exactly as `include!` did.
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

        // Let a nested component be referenced in markup by its bare file name, not its path-flattened name:
        // alias the path-derived fn (and Props type) to the basename at crate root. Skipped for files directly
        // under src/ (basename already equals the full name).
        let base_name = rsx_file
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let base_fn = rsx_transpiler::naming::to_snake_case(&base_name);
        let full_fn = rsx_transpiler::naming::to_snake_case(&stem);
        if !base_fn.is_empty() && base_fn != full_fn {
            let full_fn_ident = Ident::new(&full_fn, Span::call_site());
            let base_fn_ident = Ident::new(&base_fn, Span::call_site());
            include_stmts.extend(quote! {
                #[allow(unused_imports)]
                pub use #mod_ident::#full_fn_ident as #base_fn_ident;
            });
            if result.has_props {
                let full_props = Ident::new(
                    &(rsx_transpiler::naming::to_pascal_case(&full_fn) + "Props"),
                    Span::call_site(),
                );
                let base_props = Ident::new(
                    &(rsx_transpiler::naming::to_pascal_case(&base_fn) + "Props"),
                    Span::call_site(),
                );
                include_stmts.extend(quote! {
                    #[allow(unused_imports)]
                    pub use #mod_ident::#full_props as #base_props;
                });
            }
        }

        let rsx_path_str = rsx_file.to_string_lossy().to_string();
        rerun_stmts.extend(quote! { const _: &str = include_str!(#rsx_path_str); });

        if !result.preview_names.is_empty() {
            preview_const_idents.push(preview_const_ident(&stem));
        }
    }

    // Opt-in via `[rsx] auto_modules = true` in rsx.toml: declare the hand-written `.rs` modules by walking the
    // source tree, so an app needs no `mod.rs`/`mod` statements for them — mirroring how `.rsx` files are wired.
    let rsx_toml = manifest_dir.join("rsx.toml");
    if rsx_toml.exists() {
        // Re-run the macro when rsx.toml changes (e.g. toggling auto_modules), like the `.rsx` sources.
        let rsx_toml_str = rsx_toml.to_string_lossy().to_string();
        rerun_stmts.extend(quote! { const _: &str = include_str!(#rsx_toml_str); });
    }
    if rsx_transpiler::auto_modules_enabled(&manifest_dir) {
        // The discovered tree is split across real generated files (one per directory) so every module is a
        // file-based `#[path] mod`; see `discover_rust_modules` for why inline `mod` blocks break rust-analyzer.
        let modtree_dir = generated_dir.join("__modules");
        if let Err(e) = std::fs::create_dir_all(&modtree_dir) {
            let msg = format!("Failed to create .rsx/build/__modules/: {e}");
            return Err(quote! { compile_error!(#msg) });
        }
        let modules_src = match rsx_transpiler::discover_rust_modules(&src_dir, &modtree_dir) {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("Failed to write the auto-discovered module tree: {e}");
                return Err(quote! { compile_error!(#msg) });
            }
        };
        match modules_src.parse::<TokenStream2>() {
            Ok(tokens) => include_stmts.extend(tokens),
            Err(e) => {
                let msg = format!("Failed to emit auto-discovered modules: {e}");
                return Err(quote! { compile_error!(#msg) });
            }
        }
    }

    // Bake the i18n catalog when a `locales/` directory exists: parse every `locales/<tag>.toml` into one
    // generated module wired at the crate root, so `t!`/markup call sites reference `crate::__rsx_i18n::CATALOG`.
    // Inert (nothing generated) when there is no catalog, mirroring how svg baking only fires for `svg` elements.
    match rsx_transpiler::parse_catalog(&manifest_dir) {
        Ok(Some(catalog)) => {
            let src = rsx_transpiler::bake_catalog_to_source(&catalog);
            let out_path = generated_dir.join("__i18n.rs");
            let needs_write = std::fs::read_to_string(&out_path)
                .map(|existing| existing != src)
                .unwrap_or(true);
            if needs_write && let Err(e) = std::fs::write(&out_path, &src) {
                let msg = format!("Failed to write {}: {e}", out_path.display());
                return Err(quote! { compile_error!(#msg) });
            }
            let out_path_str = out_path.to_string_lossy().to_string();
            let mod_ident = Ident::new(rsx_transpiler::I18N_MODULE, Span::call_site());
            include_stmts.extend(quote! {
                #[path = #out_path_str]
                #[allow(dead_code)]
                pub mod #mod_ident;
            });
            // Re-bake when any locale file changes, like a `.rsx` edit.
            for file in rsx_transpiler::catalog_files(&manifest_dir) {
                let path_str = file.to_string_lossy().to_string();
                rerun_stmts.extend(quote! { const _: &str = include_str!(#path_str); });
            }
        }
        Ok(None) => {}
        Err(msg) => return Err(quote! { compile_error!(#msg) }),
    }

    Ok(TranspileOutput {
        include_stmts,
        rerun_stmts,
        preview_const_idents,
    })
}

/// Transpile every `.rsx` file under `src/` and declare the module tree — what `app!` does, minus the winit
/// runner. Use this in a crate that drives rsx through a **custom** `Platform` (e.g. a Wayland layer-shell
/// backend) instead of the built-in desktop runner: invoke `rsx::rsx_modules!()` at the crate root, then build
/// your own `App` from the transpiled components and run it via `rsx::run_with_platform` /
/// `rsx::run_multi_with_platform`. Pass a theme type — `rsx_modules!(MyTheme)` — if your `.rsx` calls
/// `use_theme`; otherwise `rsx_modules!()`.
#[proc_macro]
pub fn rsx_modules(input: TokenStream) -> TokenStream {
    let theme_type_str = if input.is_empty() {
        None
    } else {
        match syn::parse::<syn::Path>(input) {
            Ok(path) => Some(path.to_token_stream().to_string().replace(" :: ", "::")),
            Err(e) => return e.to_compile_error().into(),
        }
    };
    let TranspileOutput {
        include_stmts,
        rerun_stmts,
        preview_const_idents,
    } = match transpile_project(theme_type_str.as_deref()) {
        Ok(o) => o,
        Err(err) => return err.into(),
    };
    let preview_fn = quote! {
        pub fn rsx_all_preview_entries() -> ::std::vec::Vec<::rsx::PreviewEntry> {
            let mut entries = ::std::vec::Vec::new();
            #( entries.extend_from_slice(#preview_const_idents); )*
            entries
        }
    };
    quote! {
        #rerun_stmts
        #include_stmts
        #preview_fn
    }
    .into()
}
