# Build tuning

`cargo telar new` and `cargo telar init` write all of this into a project (`[profile.web]` only for a
`--target web` one). This page is for adding Telar to an existing workspace, and for the knobs worth
turning by hand afterwards.

## Profiles

Copy these into your **workspace root** `Cargo.toml` — Cargo ignores `[profile.*]` in a member crate.
`cargo telar build` always implies `--release`.

```toml
[profile.dev]
opt-level = 1
debug = "line-tables-only"

# Dependencies compile once and are not rebuilt as you edit, and they are not what you are stepping
# through: optimising them is paid for on the first build and buys a renderer and a layout engine that
# run at a usable speed in dev, and dropping their debug info takes 140 MB out of a cdylib that gets
# rewritten on every reload. Your own crates keep theirs, so panics in your code still name a line.
[profile.dev.package."*"]
opt-level = 3
debug = false

[profile.release]
opt-level = 3        # "s" or "z" for a smaller binary instead — matters most on Android
lto = "fat"          # "thin" keeps most of the win for a fraction of the build time
codegen-units = 1    # better codegen, no parallelism left in that stage
strip = "symbols"    # smaller binary; release backtraces lose function names
```

`debug` decides how much the link has to write: `false` (addresses only), `"line-tables-only"` (file and
line, no debugger) or `true` (full, debugger-ready). `"line-tables-only"` is worth keeping for your own
crates — a panic still names the line it came from, without carrying what only a debugger reads — and
`false` is worth setting for everything else, which is why the two blocks above differ. Measured on a real
app: the rebuild drops ~14 % and the `cdylib` goes from 154 MB to 15 MB, with panics in the app's own code
unchanged. Setting `debug = false` on `[profile.dev]` as well takes it to 0.7 MB and saves another 15 ms,
which is inside the noise and not worth the panic locations.

## `[profile.web]`, for a release web build

`cargo telar build --target web` builds with `--profile web`, not `--release`: a module crosses a network
before it runs a single instruction, so it is worth tuning for size rather than for cycles the way
`[profile.release]` is. Add this beside the profiles above, in the same **workspace root** `Cargo.toml`:

```toml
[profile.web]
inherits = "release"
opt-level = "z"
panic = "abort"
```

`panic = "abort"` is safe here: `wasm32-unknown-unknown` has no unwinding support on stable Rust, so a
panic already aborts the module regardless of the panic strategy a profile names — the `catch_unwind` calls
that give Telar's panic recovery its effect on desktop, TUI, Android and headless builds (see below) are
already inert in a browser build before this profile says anything. Naming `panic = "abort"` here does not
give anything up; it only lets codegen skip landing pads and unwind tables it would never use, which is
where part of the size drop from `opt-level = "z"` above comes from too.

`cargo telar new`/`init --target web` write this for you. A project that added `web` to an existing
workspace by hand and skipped it gets a clear diagnostic rather than cargo's own `error: profile 'web' is
not defined` — `cargo telar doctor` reports `[profile.web]` as missing (and names the file to fix), and
`cargo telar build --target web` refuses before invoking cargo, quoting the block above.

## Lock what `cargo telar` turns on

`dev`, `preview`, `check` and `test` turn some of `telar`'s features on from the command line — `telar/dev`
brings in `telar-devtools`, for one. Cargo.lock only pins what a feature of the workspace reaches, so name
them once under `[features]` in your app's `Cargo.toml`:

```toml
[features]
tooling = ["telar/dev", "telar/hot-reload", "telar/previews", "telar/preview", "telar/preview-headless"]
```

No build turns `tooling` on; it is there for the lockfile. Without it those dependencies are resolved against
whatever copy of the crates.io index the machine last fetched, so after a `telar` upgrade `cargo telar dev`
can fail with `failed to select a version for the requirement telar-devtools` until something refreshes that
copy. `cargo telar` warns when no member of the workspace names them.

## Do not set `panic = "abort"` on `[profile.dev]` or `[profile.release]`

Telar recovers from two kinds of panic on desktop, TUI, Android and headless builds, and both need
unwinding: a widget handler, effect or render that panics unmounts *only that surface* and leaves the rest
of the application running; and a wgpu validation error or lost device — a transient swapchain mismatch
while a compositor resizes a just-opened window is the common case — is caught on the render thread, which
drops that one frame and recovers on the next. Under `abort` both are process death.

This is what `[profile.dev]` and `[profile.release]` above rely on: Cargo's default panic strategy is
`unwind`, and neither profile overrides it. `[profile.web]` above is the one place `panic = "abort"`
belongs, and not really an exception — see why in that section.

If binary size is the goal, `opt-level = "z"` and `strip` give you more of it with nothing load-bearing
attached.

## Web size budget

CI's `web-size-budget` job builds `apps/landing` for `web-dom` with `--profile web` and fails if the
brotli-compressed wasm module grows more than 10% over the baseline recorded in
`.github/size-budget/landing-web-dom.json`. Brotli gates rather than the raw module or the gzip copy,
because that is the encoding a modern static host (Cloudflare Pages included) actually serves — it is
what a visitor's download is sized by.

The job builds with the `wasm-bindgen` version pinned in `Cargo.lock` and the `binaryen` version the
flake's devShell provides (both pinned again in the job itself, independent of whatever a runner or the
Nix channel resolves on a given day), because both change the module's exact byte count without touching
a line of source.

**To reproduce the measurement locally**, inside `nix develop`:

```sh
cargo run -q -p cargo-telar -- transpile
cargo run -q -p cargo-telar -- build --target web --renderer dom -p landing -- --release
ls -la target/telar-dist/web/$(jq -r '."app_bg.wasm"' target/telar-dist/web/asset-manifest.json)*
```

or run the CI check itself: `.github/scripts/check-web-size.sh`.

**To raise the baseline** — a deliberate change, never done to silence a failing job without first asking
whether the growth is justified:

1. Reproduce the measurement above and read the new `raw`, `brotli` and `gzip` byte counts off the three
   module files (the module is `app_bg-<hash>.wasm`, named in `target/telar-dist/web/asset-manifest.json`).
2. Update `baseline.raw_bytes`, `baseline.brotli_bytes` and `baseline.gzip_bytes` in
   `.github/size-budget/landing-web-dom.json` to those values, and `recorded_at` to today's date.
3. Commit the file on its own, with a message naming what grew and why (a new dependency, a new feature
   shipped to the landing page, and so on) — never bundled into the change that caused the growth, so the
   size move is visible in `git log` on its own.

Lowering the baseline (after shrinking the build) is the same procedure and is always welcome.

## Faster rebuilds

A hot reload is rustc on the crate you edited plus a full relink of the `cdylib`, and only the first half
gets cheaper the smaller your edit is. On `apps/sandbox` (155 MB `cdylib`) the rebuild is ~2.0 s, of which
~0.86 s is the link.

The default linker on Linux (GNU `ld`) works single-threaded; `mold` and `lld` parallelise it and take that
link to ~0.70 s — about 8 % of the rebuild. Install one (`apt install mold`, `dnf install mold`,
`pacman -S mold`, …) and point Cargo at it in `<your-project>/.cargo/config.toml`:

```toml
[target.x86_64-unknown-linux-gnu]
rustflags = ["-C", "link-arg=-fuse-ld=mold"]
```

Scope it to the host triple rather than to `build.rustflags`, or an Android build will try to link with the
host's linker instead of the NDK's. `lld` works the same way with `-fuse-ld=lld`, and needs `ld.lld` on
`PATH` — the driver's name, not the package's. macOS has shipped a parallel linker of its own since Xcode 15
and needs none of this; MSVC takes no `-fuse-ld=` at all. `cargo telar doctor` reports whether one is
installed; it never selects or installs anything.

## Where the build happens

`svg src:"…"` and `img src:"…"` become Rust source at build time, and **`cargo telar` produces it, not the
compiler**. The baker — usvg, resvg, `image`, about 66 crates — lives in the CLI, so it compiles once per
machine instead of once per project: no build of yours carries it, on any target, whether or not you bake
an asset. That is the largest single cut available to a cold build, and it needs no feature to claim.

The same holds for the `.rsx` itself. The parser and the code generator — about ten thousand lines — live
behind the CLI too, so `cargo telar transpile` writes `.telar/build/` and an index beside it, and the macro
wires what it finds there instead of producing it. Every source and every generated file is re-hashed
against that index before a line of it is used, so an artifact that has fallen behind is never quietly
compiled.

All of it lands under `.telar/`, and `cargo telar check` / `dev` / `build` / `test` / `preview` refresh it
before they invoke cargo. A plain `cargo build` does not, and fails with a message naming the command that
would. Add it to CI ahead of whatever compiles your app.

There is no second path inside the compiler: the macro carries no transpiler, so there is nothing there to
fall back to. A project that will not install the CLI writes the artifact itself from a `build.rs` — `telar-transpiler`
as a build-dependency, one call to `transpile_package`, one to `write_build_index`, and a
`cargo:rerun-if-changed` per `.rsx`, which is a re-run trigger a proc macro cannot have. See
[`build_index`](https://docs.rs/telar-transpiler/latest/telar_transpiler/fn.build_index.html) for the whole
of it.
