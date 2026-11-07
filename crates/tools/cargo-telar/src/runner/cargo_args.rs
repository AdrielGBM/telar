//! What `cargo telar` reads back out of the cargo arguments it forwards: only which package they select.
//!
//! Everything else in them is cargo's to interpret, and reaches cargo verbatim. A flag read here as well would be a second interpretation that drifts from cargo's whenever it is spelled another way.

/// The package `args` select, in any spelling cargo accepts: `-p x`, `-px`, `-p=x`, `--package x` or `--package=x`. Nothing after a `--` is cargo's, so nothing there is read.
pub(crate) fn selected_package(args: &[String]) -> Option<&str> {
    let mut args = args.iter().take_while(|arg| *arg != "--");
    while let Some(arg) = args.next() {
        if arg == "-p" || arg == "--package" {
            return args.next().map(String::as_str);
        }
        if let Some(name) = arg.strip_prefix("--package=") {
            return Some(name);
        }
        if let Some(name) = arg.strip_prefix("-p") {
            return Some(name.strip_prefix('=').unwrap_or(name));
        }
    }
    None
}

#[cfg(test)]
#[path = "cargo_args.test.rs"]
mod tests;
