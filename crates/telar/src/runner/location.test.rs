use platform_core::{ArgumentLocation, FixedLocation, HistoryStep};

use super::*;
use crate::user_preferences::{store_history, stored_history};
use test_process::OwnProcess;

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
fn a_remembered_history_is_stored_as_the_app_moves_and_reopened_without_a_location() {
    OwnProcess::new(
        module_path!(),
        "a_remembered_history_is_stored_as_the_app_moves_and_reopened_without_a_location",
    )
    .run(|| {
        let mut first = LocationBinding::remembered(Box::new(ArgumentLocation::default()));
        assert!(first.open().is_empty());
        first.apply(&push_to(&["/", "/es/projects"]));
        assert_eq!(stored_history(), ["/", "/es/projects"]);

        let mut next_run = LocationBinding::remembered(Box::new(ArgumentLocation::default()));
        assert_eq!(next_run.open(), [at("/"), at("/es/projects")]);
    });
}

#[test]
fn a_location_on_the_command_line_wins_over_the_remembered_one() {
    OwnProcess::new(
        module_path!(),
        "a_location_on_the_command_line_wins_over_the_remembered_one",
    )
    .run(|| {
        store_history(&["/old".to_owned()]);
        let mut binding = LocationBinding::remembered(Box::new(ArgumentLocation::from_args([
            "--location",
            "/es",
        ])));
        assert_eq!(binding.open(), [at("/es")]);
    });
}

#[test]
fn a_history_the_platform_keeps_is_never_written_to_the_store() {
    OwnProcess::new(
        module_path!(),
        "a_history_the_platform_keeps_is_never_written_to_the_store",
    )
    .run(|| {
        store_history(&["/old".to_owned()]);
        let mut binding = LocationBinding::new(Box::new(FixedLocation::default()));
        assert!(
            binding.open().is_empty(),
            "only a remembering binding reopens the store"
        );
        binding.apply(&push_to(&["/", "/a"]));
        binding.follow(vec![at("/")]);
        assert_eq!(stored_history(), ["/old"]);
        assert_eq!(binding.open(), [at("/")]);
    });
}

#[test]
fn the_opening_history_is_asked_for_once() {
    let mut binding = LocationBinding::new(Box::new(FixedLocation::new([at("/start")])));
    assert_eq!(binding.open(), [at("/start")]);
    binding.follow(vec![at("/start"), at("/later")]);
    assert_eq!(
        binding.open(),
        [at("/start"), at("/later")],
        "a rebuild reopens where the app is, not where it started"
    );
}

#[test]
fn a_link_opens_on_top_of_where_the_app_is() {
    let mut binding = LocationBinding::new(Box::new(FixedLocation::new([at("/preview/a")])));
    binding.open();
    assert_eq!(
        binding.linking(at("/preview/b?args=n:2")),
        [at("/preview/a"), at("/preview/b?args=n:2")]
    );
}
