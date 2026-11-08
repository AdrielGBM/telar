use std::sync::Mutex;

use super::*;

#[derive(Default)]
struct Recorder {
    opened: Mutex<Vec<String>>,
}

impl UriOpener for Recorder {
    fn open(&self, uri: &Uri) -> bool {
        self.opened.lock().unwrap().push(uri.to_string());
        true
    }
}

#[test]
fn opening_goes_through_the_installed_backend_and_beside_is_declined_by_default() {
    let recorder = Arc::new(Recorder::default());
    set_uri_opener(recorder.clone());
    assert!(open_uri(&Uri::parse("mailto:someone@example.com").unwrap()));
    assert!(!open_beside(
        &RouteAddress::parse("/projects/telar").unwrap()
    ));
    assert_eq!(
        *recorder.opened.lock().unwrap(),
        vec!["mailto:someone@example.com".to_string()]
    );
}

#[cfg(all(unix, feature = "system-opener"))]
mod launcher_report {
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
        type Writer = Self;

        fn make_writer(&'a self) -> Self {
            self.clone()
        }
    }

    const CHILD: &str = "TELAR_URI_LAUNCHER_LOG_TEST_CHILD";
    const TEST: &str = "a_launcher_that_fails_is_reported_with_its_status_and_stderr";

    fn launched(script: &str) -> std::process::Child {
        std::process::Command::new("sh")
            .args(["-c", script])
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("start sh")
    }

    #[test]
    fn a_launcher_that_fails_is_reported_with_its_status_and_stderr() {
        if std::env::var_os(CHILD).is_none() {
            let module = module_path!();
            let module = module.split_once("::").map_or(module, |(_, path)| path);
            let name = format!("{module}::{TEST}");
            let output =
                std::process::Command::new(std::env::current_exe().expect("the test binary"))
                    .args([name.as_str(), "--exact", "--nocapture"])
                    .env(CHILD, "1")
                    .output()
                    .expect("start the test's own process");
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success() && stdout.contains("1 passed"),
                "{stdout}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let captured = Captured::default();
        tracing::subscriber::set_global_default(
            tracing_subscriber::fmt()
                .with_writer(captured.clone())
                .with_ansi(false)
                .finish(),
        )
        .expect("the process has no other subscriber");
        let uri = Uri::parse("https://example.com/").unwrap();

        reap(launched("exit 0"), &uri);
        assert!(captured.0.lock().unwrap().is_empty());

        reap(
            launched("echo no handler >&2; printf '\\033[31m' >&2; exit 3"),
            &uri,
        );
        let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
        assert!(log.contains("exit status: 3"), "{log}");
        assert!(log.contains("no handler"), "{log}");
        assert!(!log.contains('\u{1b}'), "{log:?}");
    }
}
