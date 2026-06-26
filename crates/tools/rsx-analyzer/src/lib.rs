//! Language server for `.rsx` files.
//!
//! Exposed as a library so the `cargo-rsx` package can ship the analyzer as both
//! the `rsx-analyzer` binary and the `cargo rsx lsp` subcommand, instead of
//! requiring a separate install.

mod analysis;
mod backend;
mod build_sync;
mod format;
mod position;
mod project;
mod rpc;
mod server;
mod store;
mod uri;

/// Runs the language server on stdio until the client disconnects.
///
/// An LSP server is IO-bound (stdio over a single connection), so a
/// single-threaded runtime is enough and keeps `rt-multi-thread` out of the tree.
pub fn run() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build the rsx-analyzer runtime");
    runtime.block_on(server::run());
}
