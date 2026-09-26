use std::sync::Mutex;

use super::*;

#[derive(Default)]
struct Recorder {
    opened: Mutex<Vec<String>>,
}

impl UriOpener for Recorder {
    fn open(&self, uri: &str) -> bool {
        self.opened.lock().unwrap().push(uri.to_string());
        true
    }
}

#[test]
fn opening_goes_through_the_installed_backend_and_beside_is_declined_by_default() {
    let recorder = Arc::new(Recorder::default());
    set_uri_opener(recorder.clone());
    assert!(open_uri("mailto:someone@example.com"));
    assert!(!open_beside("/projects/telar"));
    assert_eq!(
        *recorder.opened.lock().unwrap(),
        vec!["mailto:someone@example.com".to_string()]
    );
}
