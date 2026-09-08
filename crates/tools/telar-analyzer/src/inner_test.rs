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

#[tokio::test]
async fn completion_answers_from_the_overlay_rather_than_from_disk() {
    let workspace = Workspace::create("overlay");
    let lib_rs = workspace.lib_rs();
    let (tx, _editor) = tokio::sync::mpsc::unbounded_channel();
    let inner = Inner::start(
        &workspace.0,
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
