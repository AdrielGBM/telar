//! The catalogue's own plays and accessibility checks, mounted the way the workshop mounts them.

use telar::preview::Play;
use telar::preview::a11y::Severity;

fn failures(check: impl Fn(&mut Play) -> Vec<String>) -> Vec<String> {
    telar_components::telar_all_previews()
        .into_iter()
        .filter_map(|entry| {
            let entry = entry.locale("en");
            let mut play = Play::mount(&entry)
                .unwrap_or_else(|error| panic!("`{}` does not mount: {error}", entry.id));
            let found = check(&mut play);
            (!found.is_empty()).then(|| format!("{}:\n  {}", entry.id, found.join("\n  ")))
        })
        .collect()
}

#[test]
fn every_play_passes() {
    let failed = failures(|play| match play.run() {
        Ok(()) => Vec::new(),
        Err(_) => play
            .steps()
            .iter()
            .map(|step| match &step.outcome {
                Ok(()) => format!("ok    {}", step.action),
                Err(error) => format!("FAIL  {}: {error}", step.action),
            })
            .collect(),
    });
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn no_preview_breaks_an_accessibility_rule_that_is_an_error() {
    let broken = failures(|play| {
        play.check_a11y()
            .at_least(Severity::Error)
            .map(ToString::to_string)
            .collect()
    });
    assert!(broken.is_empty(), "{}", broken.join("\n"));
}
