//! Runtime mode end to end: an id the bake never saw, resolved through an installed [`RuntimeIcons`] while the tree is up, drawn once it lands and empty until then. The provider is a loopback server, so no request leaves the machine.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use telar::{
    AvailableSpace, Children, ComponentList, DrawCommand, LayoutItem, LayoutStyle, compute_layout,
    drain_tasks, new_container, reset_layout_runtime,
};
use telar_icons::{IconProps, RuntimeIcons, SvgDir, icon};

const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="currentColor" d="M2 2h20v20H2z"/></svg>"#;

fn laid_out(name: &'static str) -> ComponentList {
    let item = icon(
        IconProps::props().name(name).size(24.0).build(),
        Children::default(),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().width(100.0).height(100.0),
        &[item.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    ComponentList::new(item)
}

fn draws_a_path(tree: &ComponentList) -> bool {
    tree.commands()
        .iter()
        .any(|command| matches!(command, DrawCommand::Path { .. }))
}

/// Drains the task queue until `tree` draws its icon, or gives up after a few seconds.
fn settles(tree: &ComponentList) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        drain_tasks();
        if draws_a_path(tree) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn an_unbaked_id_resolves_through_an_installed_source() {
    reset_layout_runtime();
    let root = std::env::temp_dir().join(format!("telar_icons_runtime_{}", std::process::id()));
    std::fs::create_dir_all(root.join("app")).unwrap();
    std::fs::write(root.join("app/logo.svg"), SVG).unwrap();
    RuntimeIcons::source(SvgDir::new(PathBuf::from(&root))).install();

    let tree = laid_out("app:logo");
    assert!(settles(&tree), "the icon never arrived");

    RuntimeIcons::uninstall();
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_provider_is_asked_for_set_and_name() {
    reset_layout_runtime();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let asked = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen = Arc::clone(&asked);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut buf = [0u8; 1024];
            let read = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..read]).to_string();
            let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
            seen.lock().unwrap().push(path);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: image/svg+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{SVG}",
                SVG.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    RuntimeIcons::provider(&format!("{base}/"))
        .without_cache()
        .install();

    let tree = laid_out("mdi:home");
    assert!(settles(&tree), "the icon never arrived");
    assert_eq!(asked.lock().unwrap().as_slice(), ["/mdi/home.svg"]);

    RuntimeIcons::uninstall();
}

#[test]
fn without_a_source_an_unbaked_id_stays_empty() {
    reset_layout_runtime();
    RuntimeIcons::uninstall();
    let tree = laid_out("mdi:home");
    drain_tasks();
    assert!(!draws_a_path(&tree));
}
