use super::*;

#[test]
fn migrate_reports_nothing_to_migrate_and_returns() {
    run_migrate_cmd(MigrateArgs {
        check: true,
        paths: Vec::new(),
    });
    assert!(NOTHING_TO_MIGRATE.contains("no migration"));
}
