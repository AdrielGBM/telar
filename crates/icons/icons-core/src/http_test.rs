use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use super::*;

const SET: &str = r#"{"prefix":"mdi","width":24,"height":24,"icons":{"home":{"body":"<path d=\"M1 1h2v2H1z\"/>"},"account":{"body":"<circle cx=\"12\" cy=\"12\" r=\"4\"/>"}},"not_found":["nothing"]}"#;
const COLLECTIONS: &str = r#"{"mdi":{"name":"Material Design Icons","license":{"title":"Apache 2.0","spdx":"Apache-2.0"},"category":"Material"}}"#;

fn response(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn answer(path: &str) -> String {
    if path.starts_with("/mdi.json?icons=") {
        response("200 OK", SET)
    } else if path == "/collections?prefixes=mdi" {
        response("200 OK", COLLECTIONS)
    } else {
        response("404 Not Found", "404")
    }
}

struct Api {
    base: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl Api {
    fn start() -> Self {
        Self::serving(answer)
    }

    fn failing_once(path: &'static str, first: String) -> Self {
        let first = Mutex::new(Some(first));
        Self::serving(move |requested| match requested.starts_with(path) {
            true => first
                .lock()
                .unwrap()
                .take()
                .unwrap_or_else(|| answer(requested)),
            false => answer(requested),
        })
    }

    fn serving(respond: impl Fn(&str) -> String + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let base = format!(
            "http://127.0.0.1:{}/",
            listener.local_addr().unwrap().port()
        );
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut buf = [0u8; 2048];
                let read = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..read]).to_string();
                let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
                let reply = respond(&path);
                seen.lock().unwrap().push(path);
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        Self { base, requests }
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }

    fn collection_requests(&self) -> usize {
        self.requests()
            .iter()
            .filter(|path| path.starts_with("/collections"))
            .count()
    }
}

fn id(text: &str) -> IconId {
    IconId::parse(text).unwrap()
}

#[test]
fn a_set_is_asked_for_once_with_every_name_wanted() {
    let api = Api::start();
    let provider = HttpProvider::new(&api.base);
    let answers = provider.icons(&[id("mdi:home"), id("mdi:account"), id("mdi:nothing")]);
    let home = answers[0].as_ref().unwrap().as_ref().unwrap();
    assert!(home.svg.contains("viewBox=\"0 0 24 24\""), "{}", home.svg);
    assert!(answers[1].as_ref().unwrap().is_some());
    assert!(answers[2].as_ref().unwrap().is_none());
    let icon_requests: Vec<String> = api
        .requests()
        .into_iter()
        .filter(|path| path.starts_with("/mdi.json"))
        .collect();
    assert_eq!(
        icon_requests,
        vec!["/mdi.json?icons=account,home,nothing".to_string()]
    );
}

#[test]
fn the_licence_comes_from_the_collections_endpoint() {
    let api = Api::start();
    let icon = HttpProvider::new(&api.base)
        .icon(&id("mdi:home"))
        .unwrap()
        .unwrap();
    let set = icon.set.unwrap();
    assert_eq!(set.license.unwrap().spdx.as_deref(), Some("Apache-2.0"));
    assert!(icon.origin.starts_with(api.base.trim_end_matches('/')));
}

#[test]
fn a_set_the_provider_does_not_have_is_none() {
    let api = Api::start();
    assert!(
        HttpProvider::new(&api.base)
            .icon(&id("tabler:home"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_licence_the_provider_does_not_have_is_unknown_and_cached() {
    let api = Api::serving(|path| match path.starts_with("/collections") {
        true => response("404 Not Found", "404"),
        false => answer(path),
    });
    let provider = HttpProvider::new(&api.base);
    let first = provider.icon(&id("mdi:home")).unwrap().unwrap();
    let second = provider.icon(&id("mdi:home")).unwrap().unwrap();
    assert!(first.set.is_none());
    assert!(second.set.is_none());
    assert_eq!(api.collection_requests(), 1);
}

#[test]
fn a_licence_lookup_that_fails_with_a_server_error_is_an_error_and_is_retried() {
    let api = Api::failing_once(
        "/collections",
        response("500 Internal Server Error", "boom"),
    );
    let provider = HttpProvider::new(&api.base);
    let error = provider.icon(&id("mdi:home")).unwrap_err();
    assert!(matches!(error, IconError::Fetch { .. }), "{error}");
    let icon = provider.icon(&id("mdi:home")).unwrap().unwrap();
    assert!(icon.set.is_some());
    assert_eq!(api.collection_requests(), 2);
}

#[test]
fn a_licence_lookup_that_is_not_json_is_an_error_and_is_retried() {
    let api = Api::failing_once("/collections", response("200 OK", "not json"));
    let provider = HttpProvider::new(&api.base);
    let error = provider.icon(&id("mdi:home")).unwrap_err();
    assert!(matches!(error, IconError::Fetch { .. }), "{error}");
    let icon = provider.icon(&id("mdi:home")).unwrap().unwrap();
    assert!(icon.set.is_some());
    assert_eq!(api.collection_requests(), 2);
}

#[test]
fn a_set_that_fails_with_a_server_error_is_an_error_and_is_retried() {
    let api = Api::failing_once("/mdi.json", response("503 Service Unavailable", "busy"));
    let provider = HttpProvider::new(&api.base);
    let error = provider.icon(&id("mdi:home")).unwrap_err();
    assert!(matches!(error, IconError::Fetch { .. }), "{error}");
    assert!(provider.icon(&id("mdi:home")).unwrap().is_some());
}

#[test]
fn a_provider_that_is_not_there_is_an_error() {
    let unused = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", unused.local_addr().unwrap().port());
    drop(unused);
    let error = HttpProvider::new(base).icon(&id("mdi:home")).unwrap_err();
    assert!(matches!(error, IconError::Fetch { .. }), "{error}");
}
