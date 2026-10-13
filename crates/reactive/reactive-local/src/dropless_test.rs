use std::path::{Path, PathBuf};

/// The thread-locals declared with `std::thread_local!` on purpose: the file, the slot, and why it may have a destructor.
const EXCEPTIONS: &[(&str, &str, &str)] = &[
    (
        "crates/reactive/reactive-local/src/dropless.rs",
        "$name",
        "it is `dropless_thread_local!` itself, handing std a slot it has already refused a destructor",
    ),
    (
        "crates/reactive/reactive-local/src/thread_exit.rs",
        "AT_EXIT",
        "it is the destructor that runs a thread's releases as the thread exits, and a copy told it is a guest never reaches it",
    ),
];

/// What an app's library is built from. The tools run beside an app and are never linked into one.
const LINKED_BY_APPS: &[&str] = &["crates", "plugins", "apps"];
const NEVER_LINKED: &[&str] = &["crates/tools"];

/// `dropless_thread_local!` refuses a destructor at compile time, but only where it is used: a `std::thread_local!` in any crate an app links bypasses it, and the first anyone hears of that is a hot-reloaded build that stays mapped after it is unloaded.
#[test]
fn every_thread_local_an_app_links_is_dropless() {
    let root = workspace_root();
    let mut found = Vec::new();
    for dir in LINKED_BY_APPS {
        collect_sources(&root, &root.join(dir), &mut found);
    }

    let mut offenders = Vec::new();
    let mut excepted = Vec::new();
    for path in found {
        let source = std::fs::read_to_string(root.join(&path)).expect("read a source file");
        for (line, slot) in std_thread_locals(&source) {
            if EXCEPTIONS
                .iter()
                .any(|(file, name, _)| *file == path && *name == slot)
            {
                excepted.push((path.clone(), slot));
            } else {
                offenders.push(format!("{path}:{line} `{slot}`"));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "declare these with `dropless_thread_local!` (`reactive_local::`, `reactive_core::` or `telar::`), holding what has a destructor in a `ManuallyDrop`:\n{}",
        offenders.join("\n")
    );
    for (file, name, _) in EXCEPTIONS {
        assert!(
            excepted
                .iter()
                .any(|(path, slot)| path == file && slot == name),
            "`{name}` in {file} is excepted but no longer declared there: drop the exception"
        );
    }
}

/// Every slot `std::thread_local!` declares in `source`, by the line it is on, written bare or by any path: no crate exports another macro of that name.
fn std_thread_locals(source: &str) -> Vec<(usize, String)> {
    let code = source
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("//") {
                ""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut slots = Vec::new();
    for (at, invocation) in code.match_indices("thread_local!") {
        let named_otherwise = code[..at]
            .chars()
            .next_back()
            .is_some_and(|before| before.is_alphanumeric() || before == '_');
        if named_otherwise {
            continue;
        }
        let line = code[..at].matches('\n').count() + 1;
        let body = delimited(code[at + invocation.len()..].trim_start());
        slots.extend(declared_slots(body).map(|slot| (line, slot)));
    }
    slots
}

/// The tokens between `text`'s opening delimiter and the one that closes it.
fn delimited(text: &str) -> &str {
    let Some(open) = text.chars().next() else {
        return "";
    };
    let close = match open {
        '{' => '}',
        '(' => ')',
        '[' => ']',
        _ => return "",
    };
    let mut depth = 0usize;
    for (at, c) in text.char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return &text[1..at];
            }
        }
    }
    text
}

fn declared_slots(body: &str) -> impl Iterator<Item = String> + '_ {
    let keywords = body
        .match_indices("static ")
        .filter(|(at, _)| !body[..*at].ends_with('\''));
    keywords.map(move |(at, keyword)| {
        body[at + keyword.len()..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
            .collect()
    })
}

fn collect_sources(root: &Path, dir: &Path, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let relative = relative(root, &path);
        if name.starts_with('.') || ["target", "node_modules", "tests"].contains(&name.as_str()) {
            continue;
        }
        if path.is_dir() {
            if !NEVER_LINKED.contains(&relative.as_str()) {
                collect_sources(root, &path, found);
            }
        } else if (name.ends_with(".rs") || name.ends_with(".rsx")) && !name.ends_with("_test.rs") {
            found.push(relative);
        }
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .expect("under the workspace")
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn workspace_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let manifest =
        std::fs::read_to_string(root.join("Cargo.toml")).expect("the workspace manifest");
    assert!(
        manifest.contains("[workspace]"),
        "this crate sits at crates/reactive/reactive-local in its workspace"
    );
    root.canonicalize().expect("the workspace root")
}

#[test]
fn a_slot_is_found_however_the_std_macro_is_spelled() {
    let source = "std::thread_local! {\n    static A: u8 = 0;\n}\n::std::thread_local!(static B: u8 = 0);\nthread_local! { static C: u8 = 0; pub static D: u8 = 0; }\n// thread_local! { static E: u8 = 0; }\ncrate::dropless_thread_local! { static F: u8 = 0; }\nthread_local! { static G: &'static str = \"\"; }\n";
    let slots: Vec<_> = std_thread_locals(source)
        .into_iter()
        .map(|(_, slot)| slot)
        .collect();
    assert_eq!(slots, ["A", "B", "C", "D", "G"]);
}
