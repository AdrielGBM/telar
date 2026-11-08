use super::*;

fn frame(body: &str) -> Vec<u8> {
    format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

async fn read_from(bytes: &[u8]) -> Result<Value, FrameError> {
    read_message(&mut BufReader::new(bytes)).await
}

#[tokio::test]
async fn a_written_message_reads_back() {
    let message = json!({ "jsonrpc": "2.0", "id": 1, "method": "shutdown" });
    let mut wire = Vec::new();
    write_message(&mut wire, &message).await.unwrap();

    assert_eq!(read_from(&wire).await.unwrap(), message);
}

#[tokio::test]
async fn frames_back_to_back_come_out_one_at_a_time_then_the_stream_closes() {
    let mut wire = frame(r#"{"n":1}"#);
    wire.extend(frame(r#"{"n":2}"#));
    wire.extend(frame(r#"{"n":3}"#));
    let mut reader = BufReader::new(wire.as_slice());

    for n in 1..=3 {
        assert_eq!(read_message(&mut reader).await.unwrap(), json!({ "n": n }));
    }
    assert!(matches!(
        read_message(&mut reader).await,
        Err(FrameError::Closed)
    ));
}

#[tokio::test]
async fn a_frame_delivered_in_small_pieces_is_read_whole() {
    let wire = frame(r#"{"method":"initialize","params":{"processId":7}}"#);
    let (mut client, server) = tokio::io::duplex(4);
    let writer = tokio::spawn(async move {
        for piece in wire.chunks(3) {
            client.write_all(piece).await.unwrap();
        }
    });

    let message = read_message(&mut BufReader::new(server)).await.unwrap();

    writer.await.unwrap();
    assert_eq!(message["params"]["processId"], 7);
}

#[tokio::test]
async fn other_headers_and_a_multibyte_body_are_read_by_byte_length() {
    let body = r#"{"text":"añadir ✓"}"#;
    let mut wire = format!(
        "Content-Type: application/vscode-jsonrpc; charset=utf-8\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    wire.extend(body.as_bytes());

    assert_eq!(read_from(&wire).await.unwrap()["text"], "añadir ✓");
}

#[tokio::test]
async fn a_header_without_content_length_is_named_as_the_cause() {
    let error = read_from(b"Content-Type: x\r\n\r\n{}").await.unwrap_err();

    assert!(matches!(error, FrameError::MissingContentLength), "{error}");
}

#[tokio::test]
async fn a_content_length_that_is_not_a_number_is_named_as_the_cause() {
    let error = read_from(b"Content-Length: lots\r\n\r\n{}")
        .await
        .unwrap_err();

    assert!(
        matches!(&error, FrameError::InvalidContentLength(value) if value == "lots"),
        "{error}"
    );
    let negative = read_from(b"Content-Length: -4\r\n\r\n{}")
        .await
        .unwrap_err();
    assert!(matches!(negative, FrameError::InvalidContentLength(_)));
}

#[tokio::test]
async fn a_content_length_past_the_limit_is_refused_before_any_allocation() {
    let wire = format!("Content-Length: {}\r\n\r\n", MAX_BODY_BYTES + 1);

    let error = read_from(wire.as_bytes()).await.unwrap_err();

    assert!(
        matches!(error, FrameError::TooLarge { length } if length == MAX_BODY_BYTES + 1),
        "{error}"
    );
}

#[tokio::test]
async fn a_body_that_is_not_json_is_named_as_the_cause() {
    let error = read_from(&frame("not json")).await.unwrap_err();

    assert!(matches!(error, FrameError::InvalidBody(_)), "{error}");
}

#[tokio::test]
async fn a_stream_that_ends_inside_a_frame_is_an_io_error_not_a_clean_close() {
    let truncated_body = read_from(b"Content-Length: 50\r\n\r\n{}")
        .await
        .unwrap_err();
    assert!(
        matches!(truncated_body, FrameError::Io(_)),
        "{truncated_body}"
    );

    let truncated_header = read_from(b"Content-Length: 2\r\n").await.unwrap_err();
    assert!(
        matches!(truncated_header, FrameError::Io(_)),
        "{truncated_header}"
    );
}

#[tokio::test]
async fn an_empty_stream_is_a_clean_close() {
    assert!(matches!(read_from(b"").await, Err(FrameError::Closed)));
}
