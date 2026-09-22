# Renderer Performance — Benchmarking & Profiling Runbook

How the hardware renderer's frame cost is measured on **desktop** and **Android**, with which tools, and
how to reproduce it. Temporary benchmark scaffolding (an in-app driver scene, a couple of throwaway
criterion benches) is not kept in the tree; this document records *how* it was run so it can be
re-created when renderer performance is revisited.

---

## What we measure and why

A frame's cost splits into **CPU** (flatten the UI tree into `DrawCommand`s, clone them to the render
thread, walk them to build GPU instance buffers) and **GPU / present** (submit the command buffer, wait
on the swapchain). The durable instrumentation attributes each frame to these phases so an optimization
can be aimed at the real bottleneck.

Phase legend (emitted by `renderer_core::perf`, in µs, rolling avg over 60 frames):

| phase | thread | what |
| ------- | -------- | ------ |
| `build` | UI | `tree.commands()` flatten + `dev.on_frame` |
| `clone` | UI | the per-frame `Vec<DrawCommand>` clone handed to the render thread |
| `interpret` | render | `analyze_frame` (dirty/scroll detection) + `interpret_commands` |
| `gpu` | render | segment build + pass execution + `queue.submit` + `present` (superset of `present`) |
| `present` | render | `output.present()` alone — isolates the mobile swapchain/vsync block |
| `frame` | render | whole `render_frame` |
| `damage` | render | count of frames in the window that took the F1 damage-tracking path |

`gpu − present` is the CPU-side command-buffer build + submit; a large `present` means the render thread
is stalling on the swapchain (common on mobile FIFO present).

---

## Tools

- **cargo + [criterion]** — micro-benchmarks of pure CPU functions (headless) and of a full headless
  hardware frame (`render_frame`, needs a GPU adapter).
- **`renderer_core::perf`** — the durable in-app instrumentation, gated on the `TELAR_PERF` env var. Zero
  cost when off (one relaxed-atomic load). Dumps one `tracing` line per ~second at 60 fps →
  **stdout on desktop, logcat on Android**.
- **`adb`** — install, launch, set flags, capture logcat, screenshots (from the Android SDK; provided by
  the parent-directory Nix flake's `androidSdk`).
- **`cargo-apk`** — builds/signs the Android APK from the sandbox `cdylib`. Also provided by the flake.
- The parent-directory **Nix flake** (`../flake.nix`, activated by direnv) provides the Rust toolchain
  with the `aarch64-linux-android` target, the Android SDK/NDK, `cargo-apk`, `adb`, and the desktop GPU
  loaders. All commands below assume that dev shell is active.

### Enabling flags

| flag | env var (desktop) | Android system property | effect |
| ------ | ------------------- | ------------------------- | -------- |
| perf timing | `TELAR_PERF=1` | `debug.telar.perf 1` | emit the per-phase `perf[…]` lines |
| F1 damage tracking | `TELAR_HW_DAMAGE=0` disables | `debug.telar.hw_damage {1,0}` | A/B the damage-tracking optimization |
| scroll-blit | `TELAR_HW_SCROLL_BLIT=0` disables | `debug.telar.scroll_blit {1,0}` | A/B the scroll-blit optimization |

**Why the Android properties exist:** an Android app process does not inherit the `adb shell`
environment, so the env vars above are unreachable on device. `run_android_app_with_name`
(`crates/telar/src/runner/android.rs`) bridges the `debug.telar.<k>` system properties (settable without
root via `adb shell setprop`) into the corresponding env vars at startup, before anything reads them.
To add a new flag, add a `(prop, var)` pair to `bridge_debug_props_to_env`.

---

## Desktop

### Criterion benchmarks

```sh
cargo bench -p renderer-core        # scale_commands (+ any core benches)
cargo bench -p renderer-hardware    # render_frame_hw/dense_ui — full HW frame, needs a GPU adapter
cargo bench -p ui-tree              # tree_flatten / scroll_tick
```

Results print to stdout and are saved under `target/criterion/`. The hardware bench builds a headless
renderer via `HardwareRenderer::new_headless(...)`; it skips gracefully if no GPU adapter is available.

### In-app per-phase timing

Run the app with `TELAR_PERF=1` and read the `perf[…]` lines from stdout. `RUST_LOG=telar_perf=info` keeps
the output to just the perf target. Use a release build for representative numbers:

```sh
cargo build --release -p sandbox
TELAR_PERF=1 RUST_LOG=telar_perf=info ./target/release/sandbox
# A/B an optimization by toggling its flag, e.g. the F1 damage path:
TELAR_PERF=1 TELAR_HW_DAMAGE=0 RUST_LOG=telar_perf=info ./target/release/sandbox
```

To capture a bounded sample non-interactively, wrap it in `timeout` (a GUI window opens):

```sh
TELAR_PERF=1 RUST_LOG=telar_perf=info timeout -k 2 12 ./target/release/sandbox 2>&1 | grep 'perf\['
```

You need a scene that produces steady frames to get stable numbers — see **Driver scenes** below.

### Pixel-correctness regression tests

The damage-tracking correctness is guarded by headless pixel tests that render a scene, apply a small
change, and assert the damage-tracked frame is byte-comparable to a full repaint:

```sh
cargo test -p renderer-hardware --test headless_smoke
```

`damage_prime_matches_full_repaint`, `damage_confines_translucent_layer_composite`, and
`damage_confines_opacity_layer` each fail loudly (measured ~54% pixel divergence) if the optimization
regresses. They run on the MSAA path (`msaa_samples > 1`); on a single-sample GPU F1 no-ops and they
pass trivially.

---

## Android

Device used for the baseline: Xiaomi `23129RA5FL`, Android 15, `arm64-v8a`, single-sample
(`msaa_samples == 1`) direct-to-surface path.

### Build & install the APK

The APK is built from the sandbox **`cdylib`** (`--lib`), not the desktop `bin` (whose `main.rs` is
gated off on Android). Release signing needs a keystore; the repo `.env` names one and its password.

```sh
CARGO_APK_RELEASE_KEYSTORE="$(pwd)/android-release.keystore" \
CARGO_APK_RELEASE_KEYSTORE_PASSWORD=android-release \
  cargo apk build --lib -p sandbox --release

adb install -r target/release/apk/sandbox.apk
```

(The project's `cargo telar build --format apk` wrapper does the same `cargo apk build --lib` plus the
keystore wiring from `.env`.) A plain `cargo build --target aarch64-linux-android` will fail at link
time — `cargo apk` is what wires the NDK linker.

### Set flags, launch, capture

```sh
adb shell setprop debug.telar.perf 1
adb shell setprop debug.telar.hw_damage 1        # 0 to A/B the pre-F1 full-repaint path
adb shell am force-stop com.example.telar.sandbox
adb logcat -c
adb shell am start -n com.example.telar.sandbox/android.app.NativeActivity
adb logcat | grep 'perf\['                     # tag is "rsx"; Ctrl-C or wrap in `timeout N`
```

The `perf[…]` lines carry `damage=N` — for a scene that should damage-track, `damage=60` confirms F1 is
firing every frame.

**Device gotchas:**

- A secure lock screen pauses the NativeActivity (you'll see `App Paused` right after `App Resumed` in
  logcat, and `isKeyguardShowing=true` in `adb shell dumpsys window`). Unlock the device manually; a
  secure credential can't be dismissed over adb.
- `adb shell svc power stayon true` keeps the screen on while on USB so it doesn't re-lock mid-run.
- Wall-clock fps ≈ 60 / (seconds between two `perf[60f]` timestamps).
- The GPU thermally throttles under sustained load; A/B back-to-back (toggle the prop, relaunch) so both
  arms share the same thermal state, and prefer the *lowest* (least-throttled) samples.

### Screenshots (visual correctness)

```sh
adb exec-out screencap -p > shot.png
```

Used to confirm no visual corruption on the single-sample path (which the desktop MSAA pixel tests don't
cover) — e.g. a translucent panel that renders as a clean uniform color rather than progressively
darkening (opacity accumulation) confirms the layer-composite confinement is correct.

### Cross-compiled CPU benches (optional, on-device numbers)

The pure-CPU criterion benches (renderer-core) can run on the device via `adb shell`. Cross-compile with
the NDK linker, push, and run:

```sh
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=\
"$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android33-clang"
cargo bench -p renderer-core --no-run --target aarch64-linux-android
# push the built bench binary (target/aarch64-linux-android/release/deps/<bench>-<hash>) and run:
adb push <bench-binary> /data/local/tmp/bench
adb shell 'cd /data/local/tmp && ./bench --bench'
```

The hardware `render_frame` bench needs a GPU surface and does **not** run this way — measure GPU cost
on device with the in-app `TELAR_PERF` instrumentation instead.

---

## Driver scenes (re-creating the removed benchmark harness)

Stable in-app numbers need a scene that redraws steadily without manual input. The removed
`apps/sandbox/src/bench.rs` booted directly into one of four scenes when `TELAR_BENCH` was set
(bridged from `debug.telar.bench` on Android). To re-create it: add a module that `App::root()` checks
first (`if let Some(bench) = build_bench_root() { return bench; }`), reading `std::env::var("TELAR_BENCH")`,
and build one of:

- **`heavy`** — ~800 static cells (`StyledContainer` + `Text`), nothing animating. Measures full-scene
  cost and the idle-blit path (a static scene stops producing render frames).
- **`dirty`** — the same ~800 cells but **one** cell's fill color + label driven by a looping
  `motion::Keyframes<f32>` so exactly one small widget changes every frame. This is the F1 scenario: a
  localized change over a large static scene.
- **`scroll`** — a few-hundred-row list whose scroll offset is driven by a ping-ponging
  `motion::Keyframes<f32>`, wrapped in a `RenderNode::Clip` + `transform_with`. Exercises the scroll path.
- **`layers`** — a large semi-transparent **rounded** panel (`Color::rgba(…, 0.5)` + non-zero radius →
  fill-layer expansion into an opacity layer) containing the one animated cell. A localized change over
  an opacity layer — exercises F1's layer-composite confinement.

The key property is that continuous motion goes through `motion::Keyframes` with `Repeat::Loop` /
`Repeat::PingPong`, which keeps `motion_has_active()` true so the runner keeps scheduling frames with no
user input.

---

## Interpreting results — what the effort found

- **Desktop** never present-blocks; its frames are ~1 ms (well under the 16.67 ms budget), so it is a
  pure CPU+GPU-work signal.
- **Android** (single-sample, direct-to-surface) full-repaint frames stall ~12 ms in `present` (FIFO
  swapchain) → ~52 fps. F1 damage tracking renders through a persistent offscreen instead, decoupling
  the render from the swapchain: `present` drops to ~0.4 ms and the device reaches ~60 fps. The
  remaining floor is the offscreen→swapchain full-screen copy, which scales with thermal state.
- Attributing CPU vs GPU/present up front (via the `present` split) is what revealed that Android was
  present-bound rather than CPU-bound — which is why the structural CPU-cache work (F3) was deferred as
  low-value and the effort focused on damage tracking + composite confinement instead.
