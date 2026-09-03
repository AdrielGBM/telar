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
      system = "x86_64-linux";
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
      # In buildInputs so pkg-config finds their .pc files at build time, and on LD_LIBRARY_PATH for the
      # loaders winit and wgpu dlopen() at runtime.
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
      devShells.${system}.default = pkgs.mkShell {
        packages = [
          rustToolchain
          pkgs.mold
          pkgs.cargo-apk
          androidSdk
          pkgs.jdk17
          pkgs.nodejs
          pkgs.pnpm
          pkgs.pkg-config
          # Must match the `wasm-bindgen` version pinned in Cargo.toml, or the generated glue is rejected.
          pkgs.wasm-bindgen-cli
          pkgs.binaryen
        ];
        buildInputs = desktopDeps;
        ANDROID_HOME = "${androidSdk}/libexec/android-sdk";
        ANDROID_NDK_ROOT = "${androidSdk}/libexec/android-sdk/ndk/27.2.12479018";
        RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
        # Host-target-scoped rather than RUSTFLAGS so the aarch64-linux-android build keeps the NDK's own linker.
        CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS = "-C link-arg=-fuse-ld=mold";
        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath desktopDeps;
      };
    };
}
