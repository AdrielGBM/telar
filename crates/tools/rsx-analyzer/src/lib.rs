//! Language server for `.rsx` files.
//!
//! Exposed as a library so the `cargo-rsx` package can ship the analyzer as both
//! the `rsx-analyzer` binary and the `cargo rsx lsp` subcommand, instead of
//! requiring a separate install.

mod analysis;
mod backend;
mod format;
mod logic_sync;
mod position;
mod project;
mod ra_client;
mod rpc;
mod semantic_tokens;
mod server;
mod store;
mod uri;

/// Runs the language server on stdio until the client disconnects.
///
/// An LSP server is IO-bound (stdio plus one rust-analyzer subprocess), so a
/// single-threaded runtime is enough and keeps `rt-multi-thread` out of the tree.
pub fn run() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build the rsx-analyzer runtime");
    runtime.block_on(server::run());
}
