use super::super::interrupt::removes_on_interrupt;
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
fn an_open_channel_is_removed_on_an_interruption_and_a_closed_one_is_not() {
    let dir = scratch("channel-interrupt");
    let file = workshop_channel_file(&dir, "demo");
    let channel = WorkshopChannel::open(file.clone()).expect("a loopback port");
    assert!(removes_on_interrupt(&file));
    drop(channel);
    assert!(!removes_on_interrupt(&file));
}

#[test]
fn a_claim_marks_the_server_file_with_its_process_and_serves_nothing_yet() {
    let dir = scratch("starting");
    let file = workshop_server_file(&dir, "demo");
    let _claim = ServerClaim::claim(&file).expect("an unclaimed file");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        format!("starting {}\n", std::process::id())
    );
    assert!(removes_on_interrupt(&file));
}

#[test]
fn a_second_session_finds_the_first_still_starting() {
    let dir = scratch("second-while-starting");
    let file = workshop_server_file(&dir, "demo");
    let _first = ServerClaim::claim(&file).expect("an unclaimed file");
    assert_eq!(
        ServerClaim::claim(&file).err(),
        Some(ServedElsewhere::Starting {
            pid: Some(std::process::id())
        })
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        format!("starting {}\n", std::process::id()),
        "the second session leaves the file to the first"
    );
}

#[test]
fn a_second_session_finds_the_first_at_the_address_it_serves() {
    let dir = scratch("second-while-serving");
    let file = workshop_server_file(&dir, "demo");
    let first = ServerClaim::claim(&file).expect("an unclaimed file");
    let address = format!("http://localhost:{}/", serve_build_counter());
    first.serve(&address);
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        format!("{address}\n")
    );
    assert_eq!(
        ServerClaim::claim(&file).err(),
        Some(ServedElsewhere::Serving(address))
    );
}

#[test]
fn an_address_the_holder_has_not_overwritten_yet_reads_as_starting() {
    let dir = scratch("held-stale-address");
    let file = workshop_server_file(&dir, "demo");
    let first = ServerClaim::claim(&file).expect("an unclaimed file");
    std::fs::write(&file, format!("http://localhost:{}/\n", free_port())).unwrap();
    assert_eq!(
        ServerClaim::claim(&file).err(),
        Some(ServedElsewhere::Starting { pid: None })
    );
    drop(first);
}

#[test]
fn a_file_left_by_a_session_that_is_gone_is_claimed_over() {
    let dir = scratch("unserved");
    let file = workshop_server_file(&dir, "demo");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, format!("http://localhost:{}/\n", free_port())).unwrap();
    let _claim = ServerClaim::claim(&file).expect("no live session holds it");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        format!("starting {}\n", std::process::id())
    );
}

#[test]
fn serving_at_an_address_forgets_whoever_named_it_before() {
    let dir = scratch("announce");
    let earlier = workshop_server_file(&dir, "gallery");
    let elsewhere = workshop_server_file(&dir, "other");
    std::fs::create_dir_all(earlier.parent().unwrap()).unwrap();
    std::fs::write(&earlier, "http://localhost:8080/\n").unwrap();
    std::fs::write(&elsewhere, "http://localhost:8081/\n").unwrap();

    let file = workshop_server_file(&dir, "catalogue");
    let claim = ServerClaim::claim(&file).expect("an unclaimed file");
    claim.serve("http://localhost:8080/");
    assert!(!earlier.exists());
    assert!(elsewhere.exists());
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "http://localhost:8080/\n"
    );
}

#[test]
fn a_session_that_ends_removes_its_server_file_and_frees_the_workshop() {
    let dir = scratch("retired");
    let file = workshop_server_file(&dir, "demo");
    let claim = ServerClaim::claim(&file).expect("an unclaimed file");
    drop(claim);
    assert!(!file.exists());
    assert!(!removes_on_interrupt(&file));
    assert!(ServerClaim::claim(&file).is_ok());
}

const HELD_FILE_VAR: &str = "TELAR_TEST_HELD_SERVER_FILE";

/// Not a test of its own: the process [`an_interrupted_session_removes_its_server_file`] interrupts, which claims the file it is given and waits.
#[test]
#[ignore = "run by an_interrupted_session_removes_its_server_file"]
fn hold_a_server_file_until_interrupted() {
    let Some(file) = std::env::var_os(HELD_FILE_VAR) else {
        return;
    };
    let _claim = ServerClaim::claim(Path::new(&file)).expect("an unclaimed file");
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[cfg(unix)]
#[test]
fn an_interrupted_session_removes_its_server_file() {
    for signal in ["INT", "TERM", "HUP"] {
        let dir = scratch(&format!("interrupted-{signal}"));
        let file = workshop_server_file(&dir, "demo");
        let mut session = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "runner::workshop_channel::tests::hold_a_server_file_until_interrupted",
                "--ignored",
                "--test-threads=1",
            ])
            .env(HELD_FILE_VAR, &file)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let starting = format!("starting {}\n", session.id());
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while std::fs::read_to_string(&file).ok().as_deref() != Some(starting.as_str()) {
            assert!(
                std::time::Instant::now() < deadline,
                "the session never claimed its file"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            ServerClaim::claim(&file).err(),
            Some(ServedElsewhere::Starting {
                pid: Some(session.id())
            }),
            "another process finds the session running"
        );

        let sent = std::process::Command::new("kill")
            .args([format!("-{signal}"), session.id().to_string()])
            .status()
            .unwrap();
        assert!(sent.success());
        let status = session.wait().unwrap();
        assert_eq!(status.code(), Some(130), "SIG{signal} ends the session");
        assert!(!file.exists(), "SIG{signal} leaves no server file");
        assert!(
            ServerClaim::claim(&file).is_ok(),
            "SIG{signal} frees the workshop"
        );
    }
}
