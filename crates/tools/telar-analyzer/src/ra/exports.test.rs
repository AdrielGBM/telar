use super::*;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::analysis::completions::{PreludeComponent, component_items};

#[test]
fn each_probe_ends_on_the_path_it_asks_about() {
    let base = "pub fn host() {}";
    let paths = ["widgets".to_string(), "crate::ui".to_string()];
    let (text, offsets) = probe_text(base, &paths);

    assert!(text.starts_with("pub fn host() {}\n"), "{text}");
    assert!(text[..offsets[0]].ends_with("{ ::widgets::"), "{text}");
    assert!(
        text[..offsets[1]].ends_with("{ crate::ui::"),
        "a path that starts at `crate` keeps its own anchor: {text}"
    );
    assert_eq!(&text[offsets[1]..offsets[1] + 2], " }");
}

const WIDGETS: &str = "\
mod badge {
    /// A small count beside a label.
    pub fn badge(props: BadgeProps) -> u32 {
        props.size
    }

    pub struct BadgeProps {
        pub size: u32,
    }
}

pub mod card {
    pub fn card(_props: CardProps) {}

    pub struct CardProps;
}

pub use badge::*;
pub use card::{CardProps, card};

pub fn helper() {}

pub struct OrphanProps;
";

const HOST: &str = "pub fn host() -> u32 {\n    1\n}\n";

/// A workspace of two crates: `widgets`, shaped like a prelude crate — one component reached through a glob, one through a named re-export beside a module of the same name, and a function and a props type that are not components — and `app`, which depends on it.
struct Workspace(PathBuf);

impl Workspace {
    fn create() -> Self {
        let dir = std::env::temp_dir().join(format!("telar-exports-{}", std::process::id()));
        let write = |rel: &str, content: &str| {
            let path = dir.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        };
        write(
            "Cargo.toml",
            "[workspace]\nresolver = \"2\"\nmembers = [\"app\", \"widgets\"]\n",
        );
        write(
            "widgets/Cargo.toml",
            "[package]\nname = \"widgets\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        );
        write("widgets/src/lib.rs", WIDGETS);
        write(
            "app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[dependencies]\nwidgets = { path = \"../widgets\" }\n",
        );
        write("app/src/lib.rs", HOST);
        Self(dir)
    }

    fn host(&self) -> PathBuf {
        self.0.join("app").join("src").join("lib.rs")
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// The editor VS Code is: it resolves documentation and detail lazily, so the first answer comes without them and only a resolve fills them in.
fn lazily_resolving_editor() -> Value {
    json!({
        "textDocument": {
            "completion": {
                "completionItem": {
                    "resolveSupport": { "properties": ["documentation", "detail"] },
                },
            },
        },
    })
}

#[tokio::test]
async fn a_prelude_crates_components_come_from_rust_analyzer() {
    let workspace = Workspace::create();
    let host = workspace.host();
    let (tx, _editor) = tokio::sync::mpsc::unbounded_channel();
    let analyzer = Analyzer::start(
        &workspace.0,
        Value::Null,
        lazily_resolving_editor(),
        crate::rpc::OutgoingSender::new(tx),
    )
    .expect("rust-analyzer failed to start");
    analyzer.inner.sync(&host, HOST);

    let paths = ["widgets".to_string()];
    // Analysis requests go unanswered until the crate graph is up, so the wait is a retry rather than a longer timeout.
    let deadline = Instant::now() + Duration::from_secs(180);
    let components: Vec<PreludeComponent> = loop {
        let answer = analyzer
            .path_exports(&host, HOST, &paths, component_items)
            .await
            .pop()
            .flatten()
            .unwrap_or_default();
        if answer.iter().any(|item| item.label.starts_with("badge")) {
            break answer
                .into_iter()
                .map(|item| PreludeComponent::from_item("widgets", item))
                .collect();
        }
        assert!(
            Instant::now() < deadline,
            "rust-analyzer never listed `widgets`' components"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    };

    let mut names: Vec<&str> = components.iter().map(|c| c.name.as_str()).collect();
    names.sort();
    assert_eq!(names, ["badge", "card"]);

    let badge = components.iter().find(|c| c.name == "badge").unwrap();
    let docs = match &badge.documentation {
        Some(lsp_types::Documentation::String(text)) => text.clone(),
        Some(lsp_types::Documentation::MarkupContent(markup)) => markup.value.clone(),
        None => String::new(),
    };
    assert!(
        docs.contains("A small count beside a label."),
        "the doc comment is resolved: {docs:?}"
    );
    assert!(
        badge
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("BadgeProps")),
        "the signature is resolved: {:?}",
        badge.detail
    );

    assert_eq!(
        analyzer.inner.synced_text(&host).as_deref(),
        Some(HOST),
        "the probe is taken back once answered"
    );
}
