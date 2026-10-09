use telar::preview::a11y::Severity;
use telar::preview::{Matrices, Play, PreviewEntry};

/// Every way the catalogue mounts `entry`: as written, and in each cell of its matrix.
fn mounts(entry: &PreviewEntry, matrices: &Matrices) -> Vec<(String, Play)> {
    let mount = |id: String, play: Result<Play, _>| {
        play.map(|play| (id.clone(), play))
            .unwrap_or_else(|error| panic!("`{id}` does not mount: {error}"))
    };
    let mut mounted = vec![mount(entry.id.to_string(), Play::mount(entry))];
    let cells = entry
        .matrix
        .map(|matrix| matrix.cells(entry, matrices))
        .transpose()
        .unwrap_or_else(|error| panic!("`{}` has no matrix: {error}", entry.id))
        .unwrap_or_default();
    for cell in cells {
        let ctx = cell
            .ctx(entry)
            .unwrap_or_else(|error| panic!("`{}` has no canvas: {error}", cell.id));
        let play = Play::mount_with(entry, ctx, Play::DEFAULT_VIEWPORT);
        mounted.push(mount(cell.id, play));
    }
    mounted
}

#[test]
fn the_themes_matrix_spans_both_modes() {
    crate::theme::register_modes();
    assert_eq!(Matrices::installed().modes(), ["light", "dark"]);
}

#[test]
fn no_preview_breaks_an_accessibility_rule_that_is_an_error_in_either_mode() {
    crate::theme::register_modes();
    telar::set_theme(crate::theme::AppTheme::light());
    let matrices = Matrices::installed();
    let mut broken = Vec::new();
    for entry in crate::__telar_app_previews() {
        for (id, play) in mounts(&entry.locale("en"), &matrices) {
            let errors: Vec<String> = play
                .check_a11y()
                .at_least(Severity::Error)
                .map(ToString::to_string)
                .collect();
            if !errors.is_empty() {
                broken.push(format!("{id}:\n  {}", errors.join("\n  ")));
            }
        }
    }
    assert!(broken.is_empty(), "{}", broken.join("\n"));
}
