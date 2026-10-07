use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use super::*;

const SET: &str = r#"{"prefix":"mdi","width":24,"height":24,"icons":{"home":{"body":"<path d=\"M1 1h2v2H1z\"/>"},"account":{"body":"<circle cx=\"12\" cy=\"12\" r=\"4\"/>"}},"not_found":["nothing"]}"#;
const COLLECTIONS: &str = r#"{"mdi":{"name":"Material Design Icons","license":{"title":"Apache 2.0","spdx":"Apache-2.0"},"category":"Material"}}"#;

/// An Iconify API on a loopback port, answering the two endpoints the provider calls and recording each request path.
struct Api {
    base: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl Api {
    fn start() -> Self {
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
                seen.lock().unwrap().push(path.clone());
                let body = if path.starts_with("/mdi.json?icons=") {
                    Some(SET)
                } else if path == "/collections?prefixes=mdi" {
                    Some(COLLECTIONS)
                } else {
                    None
                };
                let response = match body {
                    Some(body) => format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    ),
                    None => "HTTP/1.1 404 Not Found\r\nContent-Length: 3\r\nConnection: close\r\n\r\n404"
                        .to_string(),
                };
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self { base, requests }
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
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
fn a_provider_that_is_not_there_is_an_error() {
    let unused = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", unused.local_addr().unwrap().port());
    drop(unused);
    let error = HttpProvider::new(base).icon(&id("mdi:home")).unwrap_err();
    assert!(matches!(error, IconError::Fetch { .. }), "{error}");
}
