use super::*;

use std::sync::mpsc;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cargo-telar-workshop-channel-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Serves one request on another thread the way the session's loop does, answering with `forward`.
fn serve(channel: WorkshopChannel, forward: bool) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for _ in 0..200 {
            let mut seen = false;
            channel.poll(|line| {
                tx.send(line.to_string()).ok();
                seen = true;
                forward
            });
            if seen {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    rx
}

#[test]
fn the_channel_file_is_per_package_under_the_workshop_directory() {
    assert_eq!(
        channel_file(Path::new("/ws"), "catalogue"),
        PathBuf::from("/ws/.telar/workshop/catalogue.channel")
    );
}

#[test]
fn an_id_opens_its_preview_and_a_link_opens_as_written() {
    assert_eq!(
        location_of("telar_components--button--primary"),
        "/preview/telar_components--button--primary"
    );
    assert_eq!(
        location_of("/preview/demo--card--a?args=label:%22Hi%22"),
        "/preview/demo--card--a?args=label:%22Hi%22"
    );
    assert!(is_preview_id("demo--card--a"));
    assert!(!is_preview_id("/docs/Inputs/Button"));
}

#[test]
fn an_id_with_characters_a_path_cannot_hold_is_encoded() {
    assert_eq!(location_of("a b/c"), "/preview/a%20b%2Fc");
}

#[test]
fn a_running_session_passes_the_location_on_and_says_so() {
    let dir = scratch("accepted");
    let file = channel_file(&dir, "demo");
    let channel = WorkshopChannel::open(file.clone()).expect("a loopback port");
    let received = serve(channel, true);
    assert!(send(&file, "/preview/demo--card--a"));
    assert_eq!(
        received.recv_timeout(Duration::from_secs(5)).unwrap(),
        "goto:/preview/demo--card--a"
    );
}

#[test]
fn a_session_that_could_not_pass_it_on_is_answered_as_not_running() {
    let dir = scratch("refused");
    let file = channel_file(&dir, "demo");
    let channel = WorkshopChannel::open(file.clone()).expect("a loopback port");
    let _received = serve(channel, false);
    assert!(!send(&file, "/preview/demo--card--a"));
}

#[test]
fn no_file_means_no_session() {
    let dir = scratch("absent");
    assert!(!send(&channel_file(&dir, "demo"), "/preview/x"));
}

#[test]
fn a_file_left_by_a_session_that_is_gone_is_removed() {
    let dir = scratch("stale");
    let file = channel_file(&dir, "demo");
    let channel = WorkshopChannel::open(file.clone()).expect("a loopback port");
    let port = std::fs::read_to_string(&file).unwrap();
    drop(channel);
    assert!(!file.exists(), "closing the channel removes its file");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, port).unwrap();
    assert!(!send(&file, "/preview/x"));
    assert!(!file.exists());
}
