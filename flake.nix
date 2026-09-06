{
  description = "Telar — a modular Rust UI framework";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, rust-overlay, ... }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
            config.android_sdk.accept_license = true;
            config.allowUnfree = true;
          };
          androidComposition = pkgs.androidenv.composeAndroidPackages {
            buildToolsVersions = [ "35.0.0" ];
            platformVersions = [ "35" "36" ];
            includeNDK = true;
            ndkVersions = [ "27.2.12479018" ];
          };
          androidSdk = androidComposition.androidsdk;
          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            targets = [ "aarch64-linux-android" "wasm32-unknown-unknown" ];
            extensions = [ "rust-src" "rust-analyzer" "rustfmt" "clippy" ];
          };
          # In buildInputs so pkg-config finds their .pc files at build time, and on LD_LIBRARY_PATH for the loaders winit and wgpu dlopen() at runtime.
          desktopDeps = [
            pkgs.wayland
            pkgs.libxkbcommon
            pkgs.vulkan-loader
            pkgs.libglvnd
            pkgs.libx11
            pkgs.libxcursor
            pkgs.libxi
            pkgs.libxrandr
          ];
        in
        {
          default = pkgs.mkShell {
            packages = [
              rustToolchain
              pkgs.mold
              pkgs.cargo-apk
              androidSdk
              pkgs.jdk17
              pkgs.nodejs
              pkgs.pnpm
              pkgs.pkg-config
              # Must match the pinned `wasm-bindgen` crate version, or the generated glue is rejected.
              pkgs.wasm-bindgen-cli_0_2_127
              pkgs.binaryen
              # What `wasm-bindgen-test-runner` drives: the browser tests are the only place Taffy is held to
              # what CSS actually lays out, and without a driver `cargo test --target wasm32-unknown-unknown`
              # has nowhere to run the module.
              pkgs.chromedriver
              pkgs.ungoogled-chromium
            ];
            buildInputs = desktopDeps;
            ANDROID_HOME = "${androidSdk}/libexec/android-sdk";
            ANDROID_NDK_ROOT = "${androidSdk}/libexec/android-sdk/ndk/27.2.12479018";
            RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
            # Host-target-scoped rather than RUSTFLAGS so the aarch64-linux-android build keeps the NDK's own linker.
            "CARGO_TARGET_${pkgs.stdenv.hostPlatform.rust.cargoEnvVarTarget}_RUSTFLAGS" = "-C link-arg=-fuse-ld=mold";
            CHROMEDRIVER = "${pkgs.chromedriver}/bin/chromedriver";
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath desktopDeps;
          };
        });
    };
}
