//! The proc macros: `app!` and `rsx_modules!`, which transpile a project's `.rsx` at build time, plus the `Props` and `ThemeTokens` derives and the `t!` catalogue lookup.

#![warn(rustdoc::broken_intra_doc_links)]

use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::{ToTokens, quote};
use std::path::{Path, PathBuf};

mod app_input;
mod component;
mod props;
mod t_macro;
mod theme_tokens;
use app_input::{AppInput, preview_const_ident};

/// Reads a function of named arguments as a tag: the arguments are the props, and the body is what the component builds.
///
/// ```ignore
/// #[telar::component]
/// pub fn glyph(rows: &'static [&'static str], #[props(into)] color: Reactive<Color>) -> Result<Box<dyn LayoutItem>, LayoutError> {
///     chrome::mark(rows, move || color.get())
/// }
/// ```
///
/// **For the widget a `[view]` cannot build itself** — one that owns a canvas, a register, a document — which reaches the markup as a component with named props, the shape the child position took when the `widget` escape went. Written out by hand that is a struct, a `derive`, a destructuring `let` and a signature nobody reads; the arguments already say all four, and saying them twice is how the two drift apart.
///
/// Each argument carries the same `#[props(…)]` attributes a field of the struct would, and its doc comment. An argument named `children` is bound to the children the call site nested instead of becoming a prop.
#[proc_macro_attribute]
pub fn component(_attr: TokenStream, item: TokenStream) -> TokenStream {
    match component::expand(item.into()) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Generates a typed builder for a component's props: `Props::props()`, one setter per prop, `.build()`.
///
/// **What it removes.** A call site used to need a table describing the callee — which props exist, which are optional, which take a closure — kept by hand and able to drift from the struct it described. With a builder the call site spells the prop names it was given and rustc answers everything else.
///
/// A prop with no attribute is **required**: its setter must be called or `.build()` does not exist, so forgetting one is a compile error rather than a default that looks like a value at runtime.
///
/// - `#[props(default)]` — omitting it yields `Default::default()`.
/// - `#[props(default = expr)]` — omitting it yields `expr`.
///
/// - `#[props(into)]` — the setter takes `impl Into<T>` instead of `T`, which is what lets a prop declared `Reactive<T>` accept a literal, a signal or a memo. Opt-in, because a generic parameter leaves a literal's type unconstrained: `.size(20.0)` would infer `f64` and ask for `f32: From<f64>`.
///
/// Forgetting a required prop is caught where it was forgotten:
///
/// ```compile_fail
/// use telar_macros::Props;
/// #[derive(Props)]
/// struct RowProps {
///     label: &'static str,
///     #[props(default)]
///     muted: bool,
/// }
/// // No `.label(…)`, so this builder still holds `RowPropsMissing` and has no `build`.
/// let _ = RowProps::props().muted(true).build();
/// ```
#[proc_macro_derive(Props, attributes(props))]
pub fn derive_props(input: TokenStream) -> TokenStream {
    let parsed = match syn::parse::<syn::DeriveInput>(input) {
        Ok(parsed) => parsed,
        Err(e) => return e.to_compile_error().into(),
    };
    match props::expand(parsed) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Implements `ThemeTokens` for a theme struct, mapping each token to the field of the same name.
///
/// A token whose built-in is a fixed value has to be answered — silence there is what puts a 4px radius next to bars the user configured to 10, on the same screen, with nothing failing. `radius_sm`/`_md`/`_lg` are exempt: they derive from `radius`, so a theme that moves the base takes the steps with it.
///
/// - `#[token(other)]` on a field: that field also answers `other`.
/// - `#[theme(token = expr)]` on the struct: an expression, which may read `self`.
/// - `#[theme(default(a, b))]` on the struct: keep the built-in, on purpose.
#[proc_macro_derive(ThemeTokens, attributes(token, theme))]
pub fn derive_theme_tokens(input: TokenStream) -> TokenStream {
    let parsed = match syn::parse::<syn::DeriveInput>(input) {
        Ok(parsed) => parsed,
        Err(e) => return e.to_compile_error().into(),
    };
    match theme_tokens::expand(parsed) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Translates a catalog key to a `String`, substituting named arguments: `t!("battery.remaining", time = t)`.
///
/// The key and its arguments are validated against the on-disk `locales/` catalog at compile time (an unknown key or wrong argument is a `compile_error!`). At runtime it reads the active locale reactively, so calling it inside a widget's content closure makes that widget re-render on a language switch.
#[proc_macro]
pub fn t(input: TokenStream) -> TokenStream {
    match syn::parse::<t_macro::TInput>(input) {
        Ok(parsed) => t_macro::expand(parsed).into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[proc_macro]
/// Transpiles the project's `.rsx`, wires the generated modules, and emits the runner entry point.
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

    // The transpiler has no runtime access to the theme type, so it is passed as a source string, spelled the one way everything that resolves a theme spells it.
    let theme_type_str =
        telar_project::normalize_theme_path(&theme_type.to_token_stream().to_string());

    let TranspileOutput {
        include_stmts,
        rerun_stmts,
        preview_const_idents,
    } = match transpile_project(Some(theme_type_str.as_str())) {
        Ok(o) => o,
        Err(err) => return err.into(),
    };

    // Decided at macro expansion time by the features cargo-telar names on the build, which cargo tracks — unlike the environment variables these were, which it does not.
    let is_hot_reload = build_flavour().is_hot();
    let is_preview = cfg!(feature = "preview");

    let preview_fn = preview_entries_fn(is_preview, &preview_const_idents);

    // The env-var dispatch lives in `telar::dev_entry` rather than here, so an app that wires its own runner (`rsx_modules!` plus a hand-written `run()`) gets the same dev loop this macro generates.
    //
    // Absent from a build that carries no previews, and that is the whole of what keeps the dev loop out of a shipped application: `dev_entry` reads four environment variables on every start and can be told to run the preview harness and exit, so a release binary that still called it answered `TELAR_TEST=1` from anyone who set it.
    let dev_entry_call = match is_preview {
        true => quote! {
            if ::telar::dev_entry(
                telar_all_preview_entries,
                ::telar::AppConfig::from(#config),
                || #setup,
            ) {
                return;
            }
        },
        false => quote! {},
    };
    let run_tail = quote! {
        #dev_entry_call
        #setup
        ::telar::run_app_with_name(
            ::telar::AppConfig::from(#config),
            #app_expr,
            env!("CARGO_PKG_NAME"),
        )
    };

    let hot_reload_prefix = if is_hot_reload {
        quote! {
            if let (::std::result::Result::Ok(lib_path), ::std::result::Result::Ok(hot_port)) = (
                ::std::env::var("TELAR_HOT_LIB"),
                ::std::env::var("TELAR_HOT_PORT"),
            ) {
                #setup
                ::telar::run_hot_reload_host(
                    &lib_path,
                    &hot_port,
                    ::telar::AppConfig::from(#config),
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

        // A browser has no `main` to reach: `wasm-bindgen`'s `init()` runs the module's constructors and returns its exports, and the page calls this by name. A raw export rather than `#[wasm_bindgen(start)]`, so an app needs no `wasm-bindgen` dependency of its own to be startable.
        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub extern "C" fn telar_start() {
            run()
        }
    };

    // Only under `telar/hot-reload`, so dlopen can find the factory.
    let hot_export = if is_hot_reload {
        let body: TokenStream2 = if is_preview {
            quote! {
                return ::std::boxed::Box::new(::telar::PreviewApp {
                    entries: telar_all_preview_entries(),
                });
            }
        } else {
            quote! {
                return ::std::boxed::Box::new(#app_expr);
            }
        };
        quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_create_app() -> ::std::boxed::Box<dyn ::telar::App> {
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
                ::telar::motion::reset();
                // Drop pending task callbacks too: they are code compiled into this dylib, so running — or even dropping — one after dlclose would jump into unmapped memory.
                ::telar::reset_tasks();
                ::telar::reset_runtime();
            }
        }
    } else {
        quote! {}
    };

    // State-preservation symbols: the host snapshots the outgoing dylib's hot signals and restores them into the incoming one (see telar::hot_state).
    let hot_state_symbols = if is_hot_reload {
        quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_snapshot() -> ::std::string::String {
                ::telar::hot_snapshot_json()
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_restore(blob: &str) {
                ::telar::hot_restore_json(blob);
            }
        }
    } else {
        quote! {}
    };

    // The dylib mounts and owns the segment tree, so its view effects are created in the same reactive runtime as the signals they read. Mounting on the host's side leaves every subscription unestablished, which is what the force-tick workaround exists to paper over.
    let hot_tree_symbols = if is_hot_reload {
        quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_tree_mount(
                app: &dyn ::telar::App,
            ) -> *mut ::telar::HotTree {
                ::telar::HotTree::mount(app)
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_tree_release(tree: *mut ::telar::HotTree) {
                unsafe { ::telar::HotTree::release(tree) }
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_tree_on_event(
                tree: *mut ::telar::HotTree,
                event: &::telar::Event,
            ) -> bool {
                unsafe { ::telar::HotTree::on_event(tree, event) }
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_tree_end_frame(tree: *mut ::telar::HotTree) {
                unsafe { ::telar::HotTree::end_frame(tree) }
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_tree_paint(
                tree: *mut ::telar::HotTree,
            ) -> ::std::vec::Vec<::telar::DrawCommand> {
                unsafe { ::telar::HotTree::paint(tree) }
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_tree_dirty(tree: *mut ::telar::HotTree) -> bool {
                unsafe { ::telar::HotTree::is_dirty(tree) }
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_tree_generation(
                tree: *mut ::telar::HotTree,
            ) -> u64 {
                unsafe { ::telar::HotTree::generation(tree) }
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_tree_walk(
                tree: *mut ::telar::HotTree,
            ) -> ::std::vec::Vec<::telar::SegmentNodeInfo> {
                unsafe { ::telar::HotTree::walk(tree) }
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
                ::telar::motion::tick(now);
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_motion_active() -> bool {
                ::telar::motion::has_active()
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_motion_continuous() -> bool {
                ::telar::motion::has_continuous()
            }
            // Host and dylib link separate reactive-core copies, so the host must open and close the batch on the app's own runtime across this boundary — otherwise a handler's write flushes mid-dispatch and a segment loses its subscriptions while its widget is borrowed.
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_begin_batch() {
                ::telar::begin_batch();
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_end_batch() {
                ::telar::end_batch();
            }
            // Relayout the dylib's own layout runtime: the layout tree (taffy nodes) lives in the dylib's thread-local runtime, so the host must drive relayout across this boundary for a reactive list change to be laid out — its own copy is empty.
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_relayout() {
                ::telar::relayout_if_dirty();
            }
            // Consult the dylib's own overlay registry: `overlay` widgets register in this dylib's thread-local, so the host must route pointer events to overlays (modal priority / background blocking) across this boundary — its own copy is empty.
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_dispatch_overlays(event: &::telar::Event) -> bool {
                ::telar::dispatch_overlays(event)
            }
            // Write the OS light/dark preference into the dylib's own theme runtime (where `follow_system`'s effect lives), across the same boundary the host cannot reach directly.
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_set_system_dark(dark: bool) {
                ::telar::set_system_dark(dark);
            }
            // Drain the dylib's own window-command queue: a title bar's `on_press` pushes into this dylib's thread-local, so the host must drain it across this boundary to apply drag/minimize/maximize/ close — its own copy is empty.
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_drain_window_commands()
            -> ::std::vec::Vec<::telar::WindowCommand> {
                ::telar::take_window_commands()
            }
            // Run the completions of tasks spawned inside this dylib: `spawn_task` registers its callback in this dylib's reactive-core thread-local, so the host must drain it across this boundary — its own copy is empty.
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_drain_tasks() {
                ::telar::drain_tasks();
            }
            // Give this dylib's reactive-core copy the loop wake, so a worker finishing in here runs a frame instead of waiting for the next input event.
            #[unsafe(no_mangle)]
            pub unsafe extern "Rust" fn _rsx_hot_install_task_waker(waker: ::telar::RedrawWaker) {
                ::telar::set_task_waker(move || waker.wake());
            }
        }
    } else {
        quote! {}
    };

    let android_run = quote! {
        #[cfg(target_os = "android")]
        #[unsafe(no_mangle)]
        fn android_main(android_app: ::telar::AndroidApp) {
            #setup
            ::telar::run_android_app_with_name(
                ::telar::AppConfig::from(#config),
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
        #hot_tree_symbols
        #hot_motion_symbols
    }
    .into()
}

/// Which shape `cargo telar` asked this build to be, as a feature.
///
/// It was an environment variable, and cargo does not track a proc macro's env reads — so the flavour needed something cargo *does* track to move the fingerprint by, and that was `--cfg=telar_hot_reload` pushed into `RUSTFLAGS`. Rustflags are hashed into every unit in the graph, so alternating `cargo telar check` and `cargo telar dev` recompiled all four hundred dependencies each way to change which directory this function names. A feature is tracked too, and reaches only the crates that enable it.
fn build_flavour() -> telar_project::BuildFlavour {
    telar_project::BuildFlavour::new(cfg!(feature = "hot-reload"), cfg!(feature = "preview"))
}

/// Refuses a build driven by a `cargo-telar` older than this `telar`.
///
/// The two halves agree on the build shape through a feature now. An older CLI asks for it by setting `TELAR_HOT_RELOAD_BUILD` and pushing `--cfg=telar_hot_reload` into `RUSTFLAGS`, and neither is read any more — so nothing fails: the project compiles, the window opens, and hot reload simply never happens, because the dylib exports no factory and the host branch was never generated. `cargo telar dev` then reports only that the app is not connected to its own channel, which describes the symptom and names nothing that could be done about it.
///
/// The variable is read here and nowhere else, and only ever to refuse. What a build *is* still comes from the feature alone, so this cannot reintroduce an output cargo does not track.
fn check_cli_is_current() -> Result<(), TokenStream2> {
    let Some(message) = stale_cli_message(std::env::var_os("TELAR_HOT_RELOAD_BUILD").is_some())
    else {
        return Ok(());
    };
    Err(quote! { compile_error!(#message); })
}

fn stale_cli_message(hot_requested_by_env: bool) -> Option<String> {
    if !hot_requested_by_env || cfg!(feature = "hot-reload") {
        return None;
    }
    Some(format!(
        "rsx: this build was started by a `cargo-telar` older than telar {v}, which asks for a hot-reload build through an environment variable this version no longer reads — so the build would come out with no hot reload in it and nothing would say so. Run: cargo install cargo-telar --version {v} --force",
        v = env!("CARGO_PKG_VERSION")
    ))
}

/// Refuses a `telar.toml` whose `[telar] theme` is not what the invocation names.
///
/// Nothing else in the build reads that key — the macro is handed the type directly — but everything that transpiles the same file *without* a macro to read does: the editor's live mirror and the golden harness both resolve the theme through `telar_transpiler::resolve_theme_type`, which answers with this key when it is set. Two answers means two different files generated from one source, and the last writer wins.
fn check_theme_agrees(package_dir: &Path, given: Option<&str>) -> Result<(), TokenStream2> {
    let Some(declared) = telar_project::theme_type_in_config(package_dir) else {
        return Ok(());
    };
    if given == Some(declared.as_str()) {
        return Ok(());
    }
    let named = match given {
        Some(given) => format!("names `{given}`"),
        None => "names none".to_string(),
    };
    let msg = format!(
        "rsx: telar.toml declares `theme = \"{declared}\"`, but this invocation {named}. The editor and the golden snapshots resolve the theme from telar.toml, so a difference here generates two different files from one source: pass `{declared}` here, or drop the key."
    );
    Err(quote! { compile_error!(#msg); })
}

struct TranspileOutput {
    include_stmts: TokenStream2,
    rerun_stmts: TokenStream2,
    preview_const_idents: Vec<TokenStream2>,
}

/// One `.rsx` this crate has to wire, however its Rust got there.
struct WiredFile {
    rsx_path: PathBuf,
    /// Where the generated Rust sits, relative to the generated directory — the module path it is declared under.
    rel_out: PathBuf,
    out_path: PathBuf,
    previews: bool,
}

/// The files to wire, taken from `cargo telar transpile`'s artifact when it still answers for the sources on disk, and produced here when it does not.
///
/// The artifact is never trusted on its word: [`telar_project::BuildIndex::answers_for`] re-hashes every `.rsx` before a line of it is used, so a source edited since the CLI last ran sends this straight to the fallback rather than compiling the output of the run before. Which makes a stale artifact slow, not wrong — the property that lets the fallback exist at all.
fn wire_sources(
    package_dir: &Path,
    src_dir: &Path,
    generated_dir: &Path,
    theme_type_str: Option<&str>,
    flavour: telar_project::BuildFlavour,
    assets: &telar_project::AssetContext,
) -> Result<Vec<WiredFile>, TokenStream2> {
    // A package with no markup needs nothing produced and nothing attested to, so it never reaches for an artifact — `rsx_modules!()` in a crate that only wires an i18n catalog is exactly this.
    if telar_project::find_rsx_files(src_dir).is_empty() {
        return Ok(Vec::new());
    }
    let artifact = telar_project::read_build_index(package_dir, flavour);
    if let Some(index) = &artifact
        && index.answers_for(
            src_dir,
            generated_dir,
            theme_type_str,
            env!("CARGO_PKG_VERSION"),
        )
        // Output that reaches into the baked asset module is only wirable while that module is: declared against an unusable artifact it would resolve to nothing, and rustc would report it against generated code instead of the `.rsx` line that named the asset — which is the whole thing `AssetContext`'s messages exist to prevent.
        && !(index.uses_assets && assets.module_file().is_none())
    {
        return Ok(telar_project::find_rsx_files(src_dir)
            .into_iter()
            .filter_map(|rsx_path| {
                let rel_out = telar_project::relative_output_path(&rsx_path, src_dir)?;
                let previews = index.entry_for(&rsx_path, src_dir)?.previews;
                Some(WiredFile {
                    out_path: generated_dir.join(&rel_out),
                    rsx_path,
                    rel_out,
                    previews,
                })
            })
            .collect());
    }

    // Nothing here can produce the Rust: this crate carries no transpiler, on purpose. Which of the two messages is not a guess — an artifact that is absent and one that no longer answers are different facts, and only the first can mean the CLI was never installed. What made an artifact stop answering (an edited `.rsx`, a rewritten output, another theme) it does not try to say: the command is the same for all of them.
    let msg = match artifact.is_some() {
        true => {
            "rsx: the transpiled `.rsx` for this package no longer answers for its sources.\n\
             Re-run `cargo telar transpile`, or build through `cargo telar dev`/`check`/`build`/`test`."
                .to_string()
        }
        false => format!(
            "rsx: this package has no transpiled `.rsx` — it is compiled by the CLI, not by rustc.\n\
             Run `cargo telar transpile`, or build through `cargo telar dev`/`check`/`build`/`test`.\n\
             No CLI? `cargo install cargo-telar --version {v}`",
            v = env!("CARGO_PKG_VERSION")
        ),
    };
    Err(quote! { compile_error!(#msg); })
}

/// The `src`-relative directory the macro was written in.
///
/// `Span::local_file` gives the path the compiler knows, which is relative to the *working* directory — the workspace root under cargo, not the package. So the path is re-rooted by its own `src` component rather than trusted whole: rooting it at the wrong `src` silently placed nothing, which reads as a crate that simply has no `.rsx` in it.
fn invocation_dir(file: &Path, src_dir: &Path) -> Option<PathBuf> {
    let dir = file.parent()?;
    if dir.starts_with(src_dir) {
        return Some(dir.to_path_buf());
    }
    let segments: Vec<_> = dir.components().collect();
    let at = segments
        .iter()
        .rposition(|c| c.as_os_str() == std::ffi::OsStr::new("src"))?;
    let below: PathBuf = segments[at + 1..].iter().collect();
    let resolved = src_dir.join(below);
    resolved.is_dir().then_some(resolved)
}

// Transpiles every `.rsx` under `src/` into the build directory, wiring each as a `#[path] mod` and aliasing nested components to their basenames; also emits `include_str!` rerun triggers and, under `auto_modules`, declares the hand-written `.rs` module tree. Shared by `app!`, which then adds the runner, and `rsx_modules!`, which transpiles only. `Err` carries a `compile_error!` stream to emit.
fn transpile_project(theme_type_str: Option<&str>) -> Result<TranspileOutput, TokenStream2> {
    check_cli_is_current()?;

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .map_err(|_| quote! { compile_error!("CARGO_MANIFEST_DIR not set"); })?;

    // A hot-reload build emits different code for the same `.rsx`, so it needs its own output dir: sharing one has the two flavours — and the analyzer's live mirror, which always writes the plain one — overwrite each other on every build, leaving each cargo unit permanently stale.
    let flavour = build_flavour();
    let generated_dir = telar_project::generated_dir(&manifest_dir, flavour);
    if let Err(e) = std::fs::create_dir_all(&generated_dir) {
        let msg = format!("Failed to create {}: {e}", generated_dir.display());
        return Err(quote! { compile_error!(#msg); });
    }

    let src_dir = manifest_dir.join("src");
    // This crate's own version, because it is the one whose generated code the artifact's `assets.rs` calls into. `telar` and `telar-macros` share the workspace version, but the handshake compares against whoever loads the module, not whoever wrote the check.
    let assets = telar_project::AssetContext::load(&manifest_dir, env!("CARGO_PKG_VERSION"));

    // Before anything reads a setting out of it: every reader below falls back to a default on a manifest it cannot parse, which is right for them and wrong as the only answer — a misspelled key would configure nothing and say nothing.
    if let Err(e) = telar_project::TelarManifest::load(&manifest_dir) {
        let msg = format!("rsx: {e}");
        return Err(quote! { compile_error!(#msg); });
    }

    check_theme_agrees(&manifest_dir, theme_type_str)?;

    let wired = wire_sources(
        &manifest_dir,
        &src_dir,
        &generated_dir,
        theme_type_str,
        flavour,
        &assets,
    )?;
    // Every path this run is answerable for under `generated_dir`, so a stale file left behind by a renamed or deleted `.rsx` (or a toggled-off `auto_modules`/i18n catalog) can be told apart from live output and pruned. The CLI writes output but never sweeps: it does not know about the module tree written below, and a sweep that knows half the directory deletes the other half.
    let mut written_files: std::collections::HashSet<PathBuf> =
        wired.iter().map(|file| file.out_path.clone()).collect();

    let mut include_stmts = TokenStream2::new();
    let mut rerun_stmts = TokenStream2::new();
    let mut preview_const_idents: Vec<TokenStream2> = Vec::new();

    for file in &wired {
        // A real `#[path] mod`, not `include!`, so rust-analyzer treats it as a first-class module and offers completion inside it. `pub use` keeps the component fns, preview consts and `Props` types reachable by bare name, exactly as `include!` did.
        let rsx_path_str = file.rsx_path.to_string_lossy().to_string();
        rerun_stmts.extend(quote! { const _: &str = include_str!(#rsx_path_str); });

        if file.previews {
            // The const lives inside the file's own module now, so it is named by its path rather than reached by a bare name the crate root used to re-export.
            let module: Vec<Ident> = file
                .rel_out
                .with_extension("")
                .components()
                .map(|c| Ident::new(&c.as_os_str().to_string_lossy(), Span::call_site()))
                .collect();
            let name = preview_const_ident(&telar_project::component_name(&file.rsx_path));
            preview_const_idents.push(quote! { crate::#(#module)::*::#name });
        }
    }

    // Opt-in via `[telar] auto_modules = true`: declares the hand-written `.rs` modules by walking the source tree, so an app needs no `mod` statements for them, mirroring how `.rsx` files are wired.
    //
    // Nothing tracks a borrowed component any more. Its signature used to be baked into this crate's call sites, so editing its `Props` elsewhere had to rebuild this crate or the call kept the old arity.
    let auto_modules = telar_project::auto_modules_enabled(&manifest_dir);
    // The compiler's own span, not proc-macro2's shim: only the real one carries a file. A crate may invoke the macro once per module owning `.rsx` files, and what differs is where the module tree is rooted and whether this run may sweep the generated directory.
    let invoked_in = proc_macro::Span::call_site()
        .local_file()
        .and_then(|file| invocation_dir(&file, &src_dir))
        .unwrap_or_else(|| src_dir.clone());
    let invoked_at_root = invoked_in == src_dir;

    let telar_toml = manifest_dir.join(telar_project::MANIFEST_FILENAME);
    if telar_toml.exists() {
        // Re-run the macro when telar.toml changes (e.g. toggling auto_modules), like the `.rsx` sources.
        let telar_toml_str = telar_toml.to_string_lossy().to_string();
        rerun_stmts.extend(quote! { const _: &str = include_str!(#telar_toml_str); });
    }
    // Always, not opt-in: a `.rsx` is a module where its file sits, so the tree placing it has to exist whatever `auto_modules` says. What the setting decides is whether hand-written `.rs` siblings are declared for you.
    //
    // Rooted where the macro was written, not at `src/`. A module declares its own children, so a `rsx_modules!()` in `app/editor/mod.rs` places that directory's files; declaring `pub mod app;` there would name an ancestor of the file doing the declaring, which rustc reads as a cycle.
    {
        // The discovered tree is split across real generated files (one per directory) so every module is a file-based `#[path] mod`; see `discover_rust_modules` for why inline `mod` blocks break rust-analyzer.
        let modtree_dir = generated_dir.join("__modules");
        if let Err(e) = std::fs::create_dir_all(&modtree_dir) {
            let msg = format!("Failed to create {}: {e}", modtree_dir.display());
            return Err(quote! { compile_error!(#msg); });
        }
        let (modules_src, modtree_written) = match telar_project::discover_rust_modules(
            &src_dir,
            &invoked_in,
            &modtree_dir,
            &generated_dir,
            auto_modules,
        ) {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("Failed to write the auto-discovered module tree: {e}");
                return Err(quote! { compile_error!(#msg); });
            }
        };
        written_files.extend(modtree_written);
        match modules_src.parse::<TokenStream2>() {
            Ok(tokens) => include_stmts.extend(tokens),
            Err(e) => {
                let msg = format!("Failed to emit auto-discovered modules: {e}");
                return Err(quote! { compile_error!(#msg); });
            }
        }
    }

    // The catalog the CLI baked, wired exactly like the asset module. The macro neither discovers locale files nor parses one: an empty artifact is a project with no translations, and a missing one is a build that has not run the baker, which is `t!`'s error to report and not this pass's.
    let catalog = telar_project::CatalogContext::load(&manifest_dir, env!("CARGO_PKG_VERSION"));
    if let Some(stale) = catalog.staleness() {
        return Err(quote! { compile_error!(#stale); });
    }
    if let Some(module_file) = catalog.module_file() {
        let path_str = module_file.to_string_lossy().to_string();
        let mod_ident = Ident::new(telar_project::I18N_MODULE, Span::call_site());
        include_stmts.extend(quote! {
            #[path = #path_str]
            #[allow(dead_code)]
            // A crate invokes this once per module owning `.rsx`, and each one loads this same file on purpose: `t!` resolves `crate::__rsx_i18n::CATALOG`, so the module has to exist wherever the catalog is baked.
            #[allow(clippy::duplicate_mod)]
            pub mod #mod_ident;
        });
    }
    if let Some(index_file) = catalog.index_file() {
        let path_str = index_file.to_string_lossy().to_string();
        rerun_stmts.extend(quote! { const _: &str = include_str!(#path_str); });
    }
    // Editing a translation re-expands every `t!` in the crate, which is what makes the staleness check fire instead of the build quietly keeping last week's wording.
    for file in catalog.tracked_files() {
        let path_str = file.to_string_lossy().to_string();
        rerun_stmts.extend(quote! { const _: &str = include_str!(#path_str); });
    }

    // The module every static `src:"…"` resolves into, wired like the i18n catalog. Declared only when the artifact is usable: against a stale or missing one the error belongs on the `.rsx` line naming the asset, which is where `AssetContext` puts it, not inside generated code nobody wrote.
    if let Some(module_file) = assets.module_file() {
        let path_str = module_file.to_string_lossy().to_string();
        let mod_ident = Ident::new(telar_project::ASSETS_MODULE, Span::call_site());
        include_stmts.extend(quote! {
            #[path = #path_str]
            #[allow(dead_code)]
            // A crate invokes this once per module owning `.rsx`, and each one loads this same file on purpose: a baked `src:"…"` resolves `crate::__rsx_assets::ASSET_…`, so the module has to exist wherever an asset is referenced. Which also means an invocation in a submodule declares it at `crate::that::module::__rsx_assets` and the reference does not resolve — a known limit, shared with `__rsx_i18n` below, and the reason both are invoked at the crate root.
            #[allow(clippy::duplicate_mod)]
            pub mod #mod_ident;
        });
    }

    // What makes cargo re-expand this macro when an asset or the index changes. A proc macro cannot emit `cargo:rerun-if-changed`, so without these an edited asset reaches nothing: the build kept compiling whatever was baked into the last expansion. `include_bytes!` for the assets themselves, since a PNG is not UTF-8.
    if let Some(index_file) = assets.index_file() {
        let path_str = index_file.to_string_lossy().to_string();
        rerun_stmts.extend(quote! { const _: &str = include_str!(#path_str); });
    }
    for asset_file in assets.tracked_files() {
        let path_str = asset_file.to_string_lossy().to_string();
        rerun_stmts.extend(quote! { const _: &[u8] = include_bytes!(#path_str); });
    }

    // Only reached once the whole project transpiled without error, so `written_files` is complete: anything else under the generated directory is what an earlier run wrote for a `.rsx` that is gone now.
    //
    // The root invocation only. A crate may have several, one per module owning `.rsx` files, and each knows only its own module tree, so a nested one sweeping the directory deletes what the root wrote.
    if invoked_at_root {
        telar_project::prune_stale_generated(&generated_dir, &written_files);
    }

    Ok(TranspileOutput {
        include_stmts,
        rerun_stmts,
        preview_const_idents,
    })
}

/// Transpile every `.rsx` file under `src/` and declare the module tree — what `app!` does, minus the winit runner. Use this in a crate that drives rsx through a **custom** `Platform` (e.g. a Wayland layer-shell backend) instead of the built-in desktop runner: invoke `telar::rsx_modules!()` at the crate root, then build your own `App` from the transpiled components and run it via `telar::run_with_platform` / `telar::run_multi_with_platform`. Pass a theme type — `rsx_modules!(MyTheme)` — if your `.rsx` calls `use_theme`; otherwise `rsx_modules!()`.
#[proc_macro]
pub fn rsx_modules(input: TokenStream) -> TokenStream {
    let theme_type_str = if input.is_empty() {
        None
    } else {
        match syn::parse::<syn::Path>(input) {
            Ok(path) => Some(telar_project::normalize_theme_path(
                &path.to_token_stream().to_string(),
            )),
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
    let preview_fn = preview_entries_fn(cfg!(feature = "preview"), &preview_const_idents);
    quote! {
        #rerun_stmts
        #include_stmts
        #preview_fn
    }
    .into()
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;

/// The table `telar::dev_entry` reads, or nothing at all.
///
/// Nothing at all is the normal case: a `[preview]` is emitted only for the flavours that ask for one (see `telar_project::BuildFlavour`), so in every other build there are no consts to name and no caller left to name them. Emitting an empty table instead would keep `telar::PreviewEntry` in the surface of a crate that has no previews, and keep the shape alive for the next thing that decides to call it.
fn preview_entries_fn(previews: bool, consts: &[TokenStream2]) -> TokenStream2 {
    if !previews {
        return quote! {};
    }
    quote! {
        pub fn telar_all_preview_entries() -> ::std::vec::Vec<::telar::PreviewEntry> {
            let mut entries = ::std::vec::Vec::new();
            #( entries.extend_from_slice(#consts); )*
            entries
        }
    }
}
