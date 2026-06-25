//! The `rsx-analyzer` language-server binary, shipped by the `cargo-rsx` package
//! so `cargo install cargo-rsx` puts it on PATH for the editor extension.

fn main() {
    rsx_analyzer::run();
}
