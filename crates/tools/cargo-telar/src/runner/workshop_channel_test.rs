use super::*;
use telar_project::protocol::{workshop_channel_file, workshop_server_file};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cargo-telar-workshop-channel-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn free_port() -> u16 {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
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
fn a_running_session_takes_each_request_and_says_so() {
    let dir = scratch("accepted");
    let file = workshop_channel_file(&dir, "demo");
    let channel = WorkshopChannel::open(file.clone()).expect("a loopback port");
    assert!(goto(&file, "/preview/demo--card--a"));
    assert_eq!(
        channel.take(None).as_deref(),
        Some("goto:/preview/demo--card--a")
    );
    assert!(focus(&file));
    assert_eq!(channel.take(None).as_deref(), Some("focus"));
    assert_eq!(channel.take(None), None, "each request is taken once");
}

#[test]
fn a_request_waits_for_the_workshop_and_the_latest_location_wins() {
    let dir = scratch("waiting");
    let file = workshop_channel_file(&dir, "demo");
    let channel = WorkshopChannel::open(file.clone()).expect("a loopback port");
    assert!(goto(&file, "/preview/demo--card--a"));
    assert!(goto(&file, "/preview/demo--card--b"));
    assert!(focus(&file));
    assert_eq!(
        channel.take(None).as_deref(),
        Some("goto:/preview/demo--card--b")
    );
}

#[test]
fn a_focus_does_not_replace_a_location_still_waiting() {
    assert_eq!(
        supersede(Some("goto:/preview/a".to_string()), "focus".to_string()).as_deref(),
        Some("goto:/preview/a")
    );
    assert_eq!(
        supersede(Some("focus".to_string()), "goto:/preview/a".to_string()).as_deref(),
        Some("goto:/preview/a")
    );
    assert_eq!(
        supersede(None, "focus".to_string()).as_deref(),
        Some("focus")
    );
}

#[test]
fn a_session_that_stopped_taking_requests_is_answered_as_not_running() {
    let dir = scratch("stopped");
    let file = workshop_channel_file(&dir, "demo");
    let channel = WorkshopChannel::open(file.clone()).expect("a loopback port");
    let port = std::fs::read_to_string(&file).unwrap();
    drop(channel);
    assert!(!file.exists(), "closing the channel removes its file");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, port).unwrap();
    assert!(!goto(&file, "/preview/x"));
}

#[test]
fn a_line_that_is_no_request_is_refused() {
    let dir = scratch("refused");
    let file = workshop_channel_file(&dir, "demo");
    let channel = WorkshopChannel::open(file.clone()).expect("a loopback port");
    assert!(!request(&file, "hot:/target/debug/libdemo.so"));
    assert_eq!(channel.take(None), None);
}

#[test]
fn no_file_means_no_session() {
    let dir = scratch("absent");
    assert!(!goto(&workshop_channel_file(&dir, "demo"), "/preview/x"));
}

#[test]
fn a_file_left_by_a_session_that_is_gone_is_removed() {
    let dir = scratch("stale");
    let file = workshop_channel_file(&dir, "demo");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, format!("{}\n", free_port())).unwrap();
    assert!(!focus(&file));
    assert!(!file.exists());
}

/// Answers every connection the way the development server answers its build counter.
fn serve_build_counter() -> u16 {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut request = [0; 512];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 1\r\n\r\n1");
        }
    });
    port
}

#[test]
fn a_served_workshop_is_found_at_the_address_its_session_named() {
    let dir = scratch("serving");
    let file = workshop_server_file(&dir, "demo");
    let address = format!("http://localhost:{}/", serve_build_counter());
    announce_server(&file, &address);
    assert_eq!(serving(&file), Some(address));
}

#[test]
fn an_address_nothing_serves_any_more_is_forgotten() {
    let dir = scratch("unserved");
    let file = workshop_server_file(&dir, "demo");
    announce_server(&file, &format!("http://localhost:{}/", free_port()));
    assert_eq!(serving(&file), None);
    assert!(!file.exists());
}

#[test]
fn starting_to_serve_at_an_address_forgets_whoever_named_it_before() {
    let dir = scratch("announce");
    let earlier = workshop_server_file(&dir, "gallery");
    let elsewhere = workshop_server_file(&dir, "other");
    std::fs::create_dir_all(earlier.parent().unwrap()).unwrap();
    std::fs::write(&earlier, "http://localhost:8080/\n").unwrap();
    std::fs::write(&elsewhere, "http://localhost:8081/\n").unwrap();

    let file = workshop_server_file(&dir, "catalogue");
    announce_server(&file, "http://localhost:8080/");
    assert!(!earlier.exists());
    assert!(elsewhere.exists());
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "http://localhost:8080/\n"
    );
}

#[test]
fn a_session_that_is_starting_announces_its_process_and_serves_nothing_yet() {
    let dir = scratch("starting");
    let file = workshop_server_file(&dir, "demo");
    announce_starting(&file);
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        format!("starting {}\n", std::process::id())
    );
    assert_eq!(serving(&file), None);
    assert!(file.exists());
}

#[test]
fn serving_replaces_the_starting_marker_with_the_address() {
    let dir = scratch("starting-then-serving");
    let file = workshop_server_file(&dir, "demo");
    announce_starting(&file);
    let address = format!("http://localhost:{}/", serve_build_counter());
    announce_server(&file, &address);
    assert_eq!(serving(&file), Some(address));
}

#[test]
fn a_session_that_gives_up_removes_its_server_file() {
    let dir = scratch("retired");
    let file = workshop_server_file(&dir, "demo");
    announce_starting(&file);
    retire_server(&file);
    assert!(!file.exists());
}
