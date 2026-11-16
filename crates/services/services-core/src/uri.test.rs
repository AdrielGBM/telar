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

fn installed_recorder() -> Arc<Recorder> {
    let recorder = Arc::new(Recorder::default());
    set_uri_opener(recorder.clone());
    recorder
}

#[test]
fn opening_goes_through_the_installed_backend_and_beside_is_declined_by_default() {
    test_process::OwnProcess::new(
        module_path!(),
        "opening_goes_through_the_installed_backend_and_beside_is_declined_by_default",
    )
    .run(|| {
        let recorder = installed_recorder();
        assert!(open_uri(
            &Uri::parse("mailto:someone@example.com").unwrap(),
            &UriSchemes::default()
        ));
        assert!(!open_beside(
            &RouteAddress::parse("/projects/telar").unwrap()
        ));
        assert_eq!(
            *recorder.opened.lock().unwrap(),
            vec!["mailto:someone@example.com".to_string()]
        );
    });
}

#[test]
fn a_uri_opens_only_under_schemes_that_allow_it_whichever_thread_asks() {
    test_process::OwnProcess::new(
        module_path!(),
        "a_uri_opens_only_under_schemes_that_allow_it_whichever_thread_asks",
    )
    .run(|| {
        let recorder = installed_recorder();
        let app = UriSchemes::default().allowing("telar-opener-app").unwrap();
        let opened_elsewhere = std::thread::spawn({
            let app = app.clone();
            move || {
                let uri = Uri::parse("telar-opener-app://a").unwrap();
                (open_uri(&uri, &UriSchemes::default()), open_uri(&uri, &app))
            }
        })
        .join()
        .unwrap();
        assert_eq!(opened_elsewhere, (false, true));
        assert!(!open_uri(&Uri::parse("telar-other-app://b").unwrap(), &app));
        assert_eq!(
            *recorder.opened.lock().unwrap(),
            vec!["telar-opener-app://a".to_string()]
        );
    });
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

    fn launched(script: &str) -> std::process::Child {
        std::process::Command::new("sh")
            .args(["-c", script])
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("start sh")
    }

    #[test]
    fn a_launcher_that_fails_is_reported_with_its_status_and_stderr() {
        test_process::OwnProcess::new(
            module_path!(),
            "a_launcher_that_fails_is_reported_with_its_status_and_stderr",
        )
        .run(|| {
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
        });
    }
}
