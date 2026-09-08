//! The language server's entry point.

fn main() {
    // rust-analyzer's extension validates a `rust-analyzer.server.path` binary by running it with `--version` and requiring exit 0, and refuses to start it otherwise.
    if std::env::args()
        .skip(1)
        .any(|arg| arg == "--version" || arg == "-V")
    {
        println!("telar-analyzer {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    telar_analyzer::run();
}
