use platform_core::{ArgumentLocation, FixedLocation, HistoryStep};

use super::*;

fn at(path: &str) -> Location {
    LocationFormat::root().parse(path).unwrap()
}

fn push_to(history: &[&str]) -> HistoryUpdate {
    HistoryUpdate {
        step: HistoryStep::Push(1),
        history: history.iter().map(|p| at(p)).collect(),
    }
}

#[test]
fn a_remembered_history_is_saved_as_the_app_moves_and_reopened_without_a_location() {
    let mut prefs = UserPrefs::default();
    let mut first = LocationBinding::remembered(Box::new(ArgumentLocation::default()));
    assert!(first.open(&prefs).is_empty());
    assert!(first.apply(&push_to(&["/", "/es/projects"]), &mut prefs));
    assert_eq!(prefs.history, ["/", "/es/projects"]);
    assert!(
        !first.apply(&push_to(&["/", "/es/projects"]), &mut prefs),
        "an unchanged history is not written again"
    );

    let mut next_run = LocationBinding::remembered(Box::new(ArgumentLocation::default()));
    assert_eq!(next_run.open(&prefs), [at("/"), at("/es/projects")]);
}

#[test]
fn a_location_on_the_command_line_wins_over_the_remembered_one() {
    let prefs = UserPrefs {
        history: vec!["/old".into()],
        ..UserPrefs::default()
    };
    let mut binding =
        LocationBinding::remembered(Box::new(ArgumentLocation::from_args(["--location", "/es"])));
    assert_eq!(binding.open(&prefs), [at("/es")]);
}

#[test]
fn a_history_the_platform_keeps_is_never_written_to_prefs() {
    let mut prefs = UserPrefs {
        history: vec!["/old".into()],
        ..UserPrefs::default()
    };
    let mut binding = LocationBinding::new(Box::new(FixedLocation::default()));
    assert!(
        binding.open(&prefs).is_empty(),
        "only a remembering binding reopens prefs"
    );
    assert!(!binding.apply(&push_to(&["/", "/a"]), &mut prefs));
    assert!(!binding.follow(vec![at("/")], &mut prefs));
    assert_eq!(prefs.history, ["/old"]);
    assert_eq!(binding.open(&prefs), [at("/")]);
}

#[test]
fn the_opening_history_is_asked_for_once() {
    let mut binding = LocationBinding::new(Box::new(FixedLocation::new([at("/start")])));
    let prefs = UserPrefs::default();
    assert_eq!(binding.open(&prefs), [at("/start")]);
    binding.follow(vec![at("/start"), at("/later")], &mut UserPrefs::default());
    assert_eq!(
        binding.open(&prefs),
        [at("/start"), at("/later")],
        "a rebuild reopens where the app is, not where it started"
    );
}
