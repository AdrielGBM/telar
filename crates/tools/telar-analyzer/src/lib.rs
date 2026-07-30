//! Language server for `.rsx` files.

mod analysis;
mod backend;
mod build_sync;
mod format;
mod index;
mod position;
mod project;
mod ra;
mod rpc;
mod server;
mod store;
mod text;
mod uri;

/// Runs the language server on stdio until the client disconnects.
///
/// An LSP server is IO-bound (stdio over a single connection), so a single-threaded runtime is enough and keeps `rt-multi-thread` out of the tree.
pub fn run() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build the telar-analyzer runtime");
    runtime.block_on(server::run());
}
