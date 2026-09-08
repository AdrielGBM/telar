use super::*;

use std::time::Instant;

const ON_DISK: &str = "\
pub struct Greeter {
    pub name: String,
}

impl Greeter {
    pub fn greet_loudly(&self) -> String {
        self.name.to_uppercase()
    }
}

pub fn use_it(g: &Greeter) -> String {
    g.name.clone()
}
";

/// The same file cut back to `g.`. Never written to disk, so a completion that resolves `Greeter`'s methods here can only have come from the overlay.
const IN_FLIGHT: &str = "\
pub struct Greeter {
    pub name: String,
}

impl Greeter {
    pub fn greet_loudly(&self) -> String {
        self.name.to_uppercase()
    }
}

pub fn use_it(g: &Greeter) -> String {
    g.
}
";

const CURSOR_LINE: u32 = 11;
const CURSOR_CHAR: u32 = 6;

struct Workspace(PathBuf);

impl Workspace {
    fn create(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("telar-inner-{}-{name}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"subject\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("src").join("lib.rs"), ON_DISK).unwrap();
        Self(dir)
    }

    fn lib_rs(&self) -> PathBuf {
        self.0.join("src").join("lib.rs")
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

fn labels(result: &Value) -> Vec<String> {
    result
        .get("items")
        .and_then(Value::as_array)
        .or_else(|| result.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i.get("label").and_then(Value::as_str).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// rust-analyzer reads the client's capabilities into types of its own, and one field it cannot parse costs the whole object — silently, because the parse falls back on the default. What that costs is not the `.rsx` side: it is `.rs` losing progress reporting, the refresh notifications that reach the editor once the workspace finishes loading, and every completion edit a client only gets when it said it could take one.
#[test]
fn rust_analyzer_can_read_the_capabilities_we_hand_it() {
    let root = AbsPathBuf::assert_utf8(std::env::temp_dir().join("telar-capabilities"));
    let caps = serde_json::from_value(client_capabilities(&Value::Null))
        .expect("rust-analyzer could not read the capabilities we send it");
    let config = Config::new(root.clone(), caps, vec![root], None);
    assert!(
        config.caps().work_done_progress(),
        "the capabilities parsed but say the editor cannot show progress"
    );
}

/// The editor's own capabilities are what `.rs` is served against, so they have to survive the trip — a hardcoded set of our own would silently cap every feature rust-analyzer offers its client at whatever we thought to list.
#[test]
fn the_editors_capabilities_reach_rust_analyzer() {
    let editor = json!({
        "textDocument": { "hover": { "contentFormat": ["markdown"] } },
        "experimental": { "serverStatusNotification": true },
    });
    let sent = client_capabilities(&editor);
    assert_eq!(
        sent.pointer("/experimental/serverStatusNotification"),
        Some(&json!(true)),
        "an extension the editor advertised was dropped on the way"
    );
    let caps = serde_json::from_value(sent)
        .expect("rust-analyzer could not read the editor's capabilities");
    let root = AbsPathBuf::assert_utf8(std::env::temp_dir().join("telar-capabilities-editor"));
    let config = Config::new(root.clone(), caps, vec![root], None);
    assert!(config.caps().work_done_progress());
}

#[tokio::test]
async fn completion_answers_from_the_overlay_rather_than_from_disk() {
    let workspace = Workspace::create("overlay");
    let lib_rs = workspace.lib_rs();
    let (tx, _editor) = tokio::sync::mpsc::unbounded_channel();
    let inner = Inner::start(
        &workspace.0,
        Value::Null,
        Value::Null,
        crate::rpc::OutgoingSender::new(tx),
    )
    .expect("rust-analyzer failed to start");

    inner.sync(&lib_rs, IN_FLIGHT);

    // Analysis requests go unanswered until the crate graph is up, so the wait is a retry rather than a longer timeout.
    let deadline = Instant::now() + Duration::from_secs(180);
    let found = loop {
        let result = inner
            .request(
                "textDocument/completion",
                json!({
                    "textDocument": { "uri": uri_for(&lib_rs) },
                    "position": { "line": CURSOR_LINE, "character": CURSOR_CHAR },
                }),
                Duration::from_secs(20),
            )
            .await
            .unwrap_or(Value::Null);
        let labels = labels(&result);
        if labels.iter().any(|l| l == "greet_loudly") {
            break labels;
        }
        assert!(
            Instant::now() < deadline,
            "no completion for the overlay; last labels: {labels:?}"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    };

    assert!(
        found.iter().any(|l| l == "name"),
        "expected the struct's field alongside its method: {found:?}"
    );
    assert!(
        std::fs::read_to_string(&lib_rs)
            .unwrap()
            .contains("g.name.clone()"),
        "the test must never write the in-flight text to disk"
    );
}
