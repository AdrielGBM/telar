# Telar for Visual Studio Code

Language support for `.rsx` files — the markup Telar components are written in.

The extension talks to `telar-analyzer`, a language server that embeds rust-analyzer in-process. Your `[logic]` blocks are analyzed against the real crate graph as you type, not one `cargo check` behind.

## Features

- **Diagnostics** — syntax and semantic errors on the `.rsx` line that caused them, including `cargo check` errors mapped back from the generated Rust
- **Completion** — elements, attributes, signals, style classes and colors, triggered on `@`, `$`, `.`, `:` and inside strings
- **Hover and go to definition** — across `.rsx` files and into the Rust that backs them
- **Rename and find references** — component tags and signals, across the workspace
- **Formatting** — whole document or selection
- **Inlay hints, code lenses, code actions, document links and folding ranges**
- **Semantic highlighting** and color swatches for theme values
- **Workspace symbols** — components and `@classes` across the project

## Requirements

A Rust toolchain on `PATH`. The server discovers the sysroot through `rustc` and reads the crate graph with `cargo metadata`, so both need to be reachable from the editor's environment.

## Settings

| Setting            | Description                                                                                                                                                |
| ------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `telar.serverPath` | Path to a `telar-analyzer` binary. Takes precedence over everything else — point it at a local `cargo build --release` while working on the server itself. |

## Commands

| Command                    | Description                                |
| -------------------------- | ------------------------------------------ |
| `Telar: Preview component` | Renders the component under the cursor     |
| `Telar: Show server log`   | Opens the language server's output channel |

## How the server is found

1. `telar.serverPath`, if set
2. The `telar-analyzer` bundled in this extension under `server/`
3. `telar-analyzer` on `PATH`
4. `~/.cargo/bin/telar-analyzer`

Platform builds ship the matching binary in `server/`, so no manual install is needed.

### NixOS

The bundled binary is a generic Linux executable, and its ELF interpreter does not exist on NixOS — running it fails with a misleading `ENOENT`. The extension detects NixOS and patches a private copy through `nix-build` before starting it, caching the result until the extension version changes. A binary that already resolves inside `/nix/store` is left alone.

## License

MIT OR Apache-2.0
