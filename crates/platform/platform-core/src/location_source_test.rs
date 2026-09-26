use super::*;

fn at(path: &str) -> Location {
    LocationFormat::root().parse(path).unwrap()
}

fn step(from: &[&str], to: &[&str]) -> Option<HistoryStep> {
    let from: Vec<Location> = from.iter().map(|p| at(p)).collect();
    let to: Vec<Location> = to.iter().map(|p| at(p)).collect();
    HistoryUpdate::between(&from, &to).map(|update| {
        assert_eq!(
            update.history, to,
            "the update carries the history it leads to"
        );
        update.step
    })
}

#[test]
fn the_same_history_is_no_move() {
    assert_eq!(step(&["/", "/a"], &["/", "/a"]), None);
    assert_eq!(step(&[], &[]), None);
}

#[test]
fn growing_on_top_is_a_push_of_every_new_entry() {
    assert_eq!(step(&["/"], &["/", "/a"]), Some(HistoryStep::Push(1)));
    assert_eq!(step(&["/"], &["/", "/a", "/b"]), Some(HistoryStep::Push(2)));
}

#[test]
fn shrinking_from_the_top_is_a_back() {
    assert_eq!(step(&["/", "/a", "/b"], &["/"]), Some(HistoryStep::Back(2)));
}

#[test]
fn anything_else_rewrites_the_top() {
    assert_eq!(step(&["/", "/a"], &["/", "/b"]), Some(HistoryStep::Replace));
    assert_eq!(
        step(&["/", "/a", "/b"], &["/c"]),
        Some(HistoryStep::Replace)
    );
    assert_eq!(
        step(&[], &["/"]),
        Some(HistoryStep::Replace),
        "a platform that named nothing is given the app's root in place, not a second entry"
    );
    assert_eq!(step(&["/a"], &[]), Some(HistoryStep::Replace));
}

#[derive(Default)]
struct Log(Vec<String>);

impl LocationSource for Log {
    fn initial(&mut self) -> Vec<Location> {
        Vec::new()
    }

    fn push(&mut self, history: &[Location]) {
        self.0.push(format!("push {}", history.len()));
    }

    fn replace(&mut self, history: &[Location]) {
        self.0.push(format!("replace {}", history.len()));
    }

    fn back(&mut self, count: usize, history: &[Location]) {
        self.0.push(format!("back {count} to {}", history.len()));
    }
}

#[test]
fn a_push_of_several_entries_reaches_the_source_one_at_a_time() {
    let mut log = Log::default();
    log.apply(&HistoryUpdate {
        step: HistoryStep::Push(2),
        history: vec![at("/"), at("/a"), at("/b")],
    });
    log.apply(&HistoryUpdate {
        step: HistoryStep::Back(1),
        history: vec![at("/"), at("/a")],
    });
    log.apply(&HistoryUpdate {
        step: HistoryStep::Replace,
        history: vec![at("/"), at("/c")],
    });
    assert_eq!(log.0, ["push 2", "push 3", "back 1 to 2", "replace 2"]);
}

#[test]
fn the_location_flag_takes_its_value_either_way_it_is_written() {
    let spaced = ArgumentLocation::from_args(["--verbose", "--location", "/es/projects#telar"]);
    let joined = ArgumentLocation::from_args(["--location=/es/projects#telar"]);
    let expected = Location::from_segments(["es", "projects"]).with_fragment("telar");
    assert_eq!(spaced.location(), Some(&expected));
    assert_eq!(joined.location(), Some(&expected));
}

#[test]
fn a_uri_on_the_command_line_opens_its_path() {
    let deep_link = ArgumentLocation::from_args(["--location", "https://example.com/es"]);
    assert_eq!(deep_link.location(), Some(&Location::from_segments(["es"])));
}

#[test]
fn free_arguments_are_never_read_as_an_address() {
    assert_eq!(
        ArgumentLocation::from_args(["/home/user/notes.txt"]).location(),
        None
    );
    assert_eq!(
        ArgumentLocation::from_args(["--", "--location", "/es"]).location(),
        None
    );
    assert_eq!(
        ArgumentLocation::from_args(["--locations=/es"]).location(),
        None
    );
    assert_eq!(ArgumentLocation::from_args(["--location"]).location(), None);
}

#[test]
fn an_argument_opens_on_its_one_location_and_ignores_moves() {
    let mut source = ArgumentLocation::from_args(["--location", "/es"]);
    assert_eq!(source.initial(), [at("/es")]);
    source.push(&[at("/es"), at("/es/a")]);
    assert_eq!(source.initial(), [at("/es")]);
    assert!(ArgumentLocation::default().initial().is_empty());
}

#[test]
fn a_fixed_location_records_every_move_it_is_told_about() {
    let sink = HistorySink::default();
    let mut source = FixedLocation::new([at("/"), at("/es")]).recording_into(sink.clone());
    assert_eq!(source.initial(), [at("/"), at("/es")]);
    assert_eq!(*sink.lock().unwrap(), [at("/"), at("/es")]);
    source.apply(&HistoryUpdate {
        step: HistoryStep::Push(1),
        history: vec![at("/"), at("/es"), at("/es/a")],
    });
    assert_eq!(sink.lock().unwrap().len(), 3);
    source.back(2, &[at("/")]);
    assert_eq!(*sink.lock().unwrap(), [at("/")]);
}
