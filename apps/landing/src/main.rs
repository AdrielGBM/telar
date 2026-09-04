//! Binary entry point.

fn main() {
    tracing_subscriber::fmt::init();
    landing::run();
}
