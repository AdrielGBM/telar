# Build tuning

`cargo telar new` writes all of this into a new project. This page is for adding Telar to an existing
workspace, and for the two knobs worth turning by hand afterwards.

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

## Do not set `panic = "abort"`

Telar recovers from two kinds of panic and both need unwinding: a widget handler, effect or render that
panics unmounts *only that surface* and leaves the rest of the application running; and a wgpu validation
error or lost device — a transient swapchain mismatch while a compositor resizes a just-opened window is the
common case — is caught on the render thread, which drops that one frame and recovers on the next. Under
`abort` both are process death.

If binary size is the goal, `opt-level = "z"` and `strip` give you more of it with nothing load-bearing
attached.

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

## Where assets are baked

`svg src:"…"` and `img src:"…"` become Rust source at build time, and **`cargo telar` produces it, not the
compiler**. The baker — usvg, resvg, `image`, about 66 crates — lives in the CLI, so it compiles once per
machine instead of once per project: no build of yours carries it, on any target, whether or not you bake
an asset. That is the largest single cut available to a cold build, and it needs no feature to claim.

The artifact lands in `.telar/assets.rs` and `.telar/assets.json`, and `cargo telar check` / `dev` /
`build` / `test` / `preview` refresh it before they invoke cargo. A plain `cargo build` does not: if the
artifact is missing or older than the asset, the macro fails with a message naming `cargo telar bake`
rather than drawing nothing at runtime. Add it to CI ahead of whatever compiles your app.
