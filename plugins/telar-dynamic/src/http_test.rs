use super::*;

#[test]
fn the_template_substitutes_the_whole_id() {
    let transport = HttpTransport::new("https://api.iconify.design/{name}.svg");
    assert_eq!(
        transport.url_for("mdi/home").unwrap(),
        "https://api.iconify.design/mdi/home.svg"
    );
}

/// Zero attempts would mean never asking at all, which is not what anybody turning retries off means.
#[test]
fn retries_cannot_be_set_below_one_attempt() {
    let transport =
        HttpTransport::new("http://example.invalid/{name}").with_retry(0, Duration::ZERO);
    assert_eq!(transport.max_attempts, 1);
}

/// Both bodies count attempts through this, so the give-up point cannot drift between targets.
#[test]
fn the_last_attempt_is_the_one_that_gives_up() {
    let error = AssetError("boom".to_string());
    assert!(
        !give_up("x", 1, 3, &error),
        "there are attempts left, so this is not the end"
    );
    assert!(!give_up("x", 2, 3, &error), "nor is the one before last");
    assert!(
        give_up("x", 3, 3, &error),
        "the last attempt is the one that gives up"
    );
}

fn url(id: &str) -> Result<String, AssetError> {
    HttpTransport::new("https://h.test/{name}.svg").url_for(id)
}

#[test]
fn dot_segments_are_refused_wherever_they_appear() {
    for id in ["..", ".", "../x", "a/../b", "a/./b", "a/.."] {
        assert!(url(id).is_err(), "`{id}` must not reach the URL");
    }
}

#[test]
fn query_and_fragment_delimiters_are_escaped() {
    assert_eq!(url("a?x=1#f").unwrap(), "https://h.test/a%3Fx%3D1%23f.svg");
}

#[test]
fn each_segment_is_encoded_and_slashes_between_them_are_kept() {
    assert_eq!(
        url("my set/ho me%").unwrap(),
        "https://h.test/my%20set/ho%20me%25.svg"
    );
    assert_eq!(url("a\\..\\b").unwrap(), "https://h.test/a%5C..%5Cb.svg");
    assert_eq!(
        url("..hidden/a..b").unwrap(),
        "https://h.test/..hidden/a..b.svg"
    );
}

#[cfg(not(target_arch = "wasm32"))]
mod body_cap {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn serve(body: Vec<u8>, content_length: bool) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let header = if content_length {
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
            } else {
                "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_string()
            };
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&body);
        });
        format!("http://127.0.0.1:{port}/{{name}}")
    }

    fn transport(template: String, cap: u64) -> HttpTransport {
        HttpTransport::new(template)
            .with_retry(1, Duration::ZERO)
            .with_max_body_size(cap)
    }

    #[test]
    fn a_declared_length_over_the_cap_is_rejected() {
        let error = transport(serve(vec![0; 64], true), 16)
            .fetch("x")
            .unwrap_err();
        assert!(error.0.contains("exceeds 16 bytes"), "{error}");
    }

    #[test]
    fn a_stream_without_a_length_is_cut_off_at_the_cap() {
        let error = transport(serve(vec![0; 64], false), 16)
            .fetch("x")
            .unwrap_err();
        assert!(error.0.contains("exceeds 16 bytes"), "{error}");
    }

    #[test]
    fn a_body_within_the_cap_is_returned() {
        let bytes = transport(serve(vec![7; 16], true), 16).fetch("x").unwrap();
        assert_eq!(bytes, vec![7; 16]);
    }
}
