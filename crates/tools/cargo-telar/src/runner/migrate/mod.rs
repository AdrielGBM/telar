//! `cargo telar migrate` — the home of the migrations a release needs, none of which exists yet.

use super::cli::MigrateArgs;

pub(crate) const NOTHING_TO_MIGRATE: &str = "[cargo-telar] there is no migration between the versions this cargo-telar knows; `cargo telar migrate` will carry the migrations of future releases that need one";

pub(crate) fn run_migrate_cmd(_args: MigrateArgs) {
    println!("{NOTHING_TO_MIGRATE}");
}

#[cfg(test)]
#[path = "migrate_test.rs"]
mod tests;
