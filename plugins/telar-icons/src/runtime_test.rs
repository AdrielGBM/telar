//! Runtime mode end to end: an id the bake never saw, resolved through an installed [`RuntimeIcons`] while the tree is up, drawn once it lands and empty until then. The provider is a loopback server, so no request leaves the machine.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use telar::{
    AvailableSpace, Children, Color, ComponentList, Declared, DrawCommand, LayoutItem, LayoutStyle,
    Paint, compute_layout, declare, drain_tasks, new_container, reset_layout_runtime,
};
use telar_icons::{IconProps, RuntimeIcons, SvgDir, icon};

const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="currentColor" d="M2 2h20v20H2z"/></svg>"#;
const LOGO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="#1877f2" d="M2 2h20v20H2z"/></svg>"##;

/// The text colour the icon is laid out under, which a tinted icon takes.
const INK: Color = Color::RED;

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
    declare(
        root,
        Declared {
            color: Some(Paint::Solid(INK)),
            ..Declared::default()
        },
    );
    compute_layout(
        root,
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    ComponentList::new(item)
}

fn fills(tree: &ComponentList) -> Vec<Paint> {
    tree.commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Path { style, .. } => style.fill,
            _ => None,
        })
        .collect()
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

/// A folder of SVGs, `<root>/<set>/<name>.svg`, unique to `name` and this process.
fn svg_dir(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("telar_icons_runtime_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, contents) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    root
}

fn clean(root: &Path) {
    RuntimeIcons::uninstall();
    let _ = std::fs::remove_dir_all(root);
}

/// A loopback provider answering every request with `svg`, and the paths it was asked for.
fn serve(svg: &'static str) -> (String, Arc<Mutex<Vec<String>>>) {
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
                "HTTP/1.1 200 OK\r\nContent-Type: image/svg+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{svg}",
                svg.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (base, asked)
}

#[test]
fn an_unbaked_id_resolves_through_an_installed_source() {
    reset_layout_runtime();
    let root = svg_dir("source", &[("app/logo.svg", SVG)]);
    RuntimeIcons::source(SvgDir::new(&root)).install();

    let tree = laid_out("app:logo");
    assert!(settles(&tree), "the icon never arrived");
    assert_eq!(fills(&tree), vec![Paint::Solid(INK)]);

    clean(&root);
}

#[test]
fn a_provider_is_asked_for_set_and_name() {
    reset_layout_runtime();
    let (base, asked) = serve(SVG);
    RuntimeIcons::provider(&format!("{base}/"))
        .without_cache()
        .install();

    let tree = laid_out("mdi:home");
    assert!(settles(&tree), "the icon never arrived");
    assert_eq!(asked.lock().unwrap().as_slice(), ["/mdi/home.svg"]);
    assert_eq!(fills(&tree), vec![Paint::Solid(INK)]);

    RuntimeIcons::uninstall();
}

#[test]
fn a_provider_icon_in_fixed_colours_keeps_them() {
    reset_layout_runtime();
    let (base, _) = serve(LOGO);
    RuntimeIcons::provider(&base).without_cache().install();

    let tree = laid_out("logos:acme");
    assert!(settles(&tree), "the icon never arrived");
    assert_ne!(fills(&tree), vec![Paint::Solid(INK)]);

    RuntimeIcons::uninstall();
}

#[test]
fn a_bare_name_is_read_in_the_default_set() {
    reset_layout_runtime();
    let (base, asked) = serve(SVG);
    RuntimeIcons::provider(&base)
        .with_default_set("mdi")
        .without_cache()
        .install();

    let tree = laid_out("home");
    assert!(settles(&tree), "the icon never arrived");
    assert_eq!(asked.lock().unwrap().as_slice(), ["/mdi/home.svg"]);

    RuntimeIcons::uninstall();
}

#[test]
fn a_bare_name_without_a_default_set_stays_empty() {
    reset_layout_runtime();
    let root = svg_dir("bare", &[("app/logo.svg", SVG)]);
    RuntimeIcons::source(SvgDir::new(&root)).install();

    let tree = laid_out("logo");
    drain_tasks();
    assert!(!draws_a_path(&tree));

    clean(&root);
}

#[test]
fn a_default_set_outside_iconify_naming_is_ignored() {
    reset_layout_runtime();
    let root = svg_dir("bad_default", &[("app/logo.svg", SVG)]);
    RuntimeIcons::source(SvgDir::new(&root))
        .with_default_set("App")
        .install();

    let tree = laid_out("logo");
    drain_tasks();
    assert!(!draws_a_path(&tree));

    clean(&root);
}

#[test]
fn a_palette_sets_one_colour_logo_keeps_its_colour_at_run_time() {
    reset_layout_runtime();
    let root = svg_dir(
        "palette",
        &[
            (
                "brand/info.json",
                r#"{ "name": "Brands", "palette": true }"#,
            ),
            ("brand/acme.svg", SVG),
            ("mono/info.json", r#"{ "name": "Mono", "palette": false }"#),
            ("mono/dot.svg", LOGO),
        ],
    );
    RuntimeIcons::source(SvgDir::new(&root)).install();

    let brand = laid_out("brand:acme");
    assert!(settles(&brand), "the brand icon never arrived");
    assert_ne!(
        fills(&brand),
        vec![Paint::Solid(INK)],
        "a palette set is not tinted, even drawn in currentColor"
    );

    let mono = laid_out("mono:dot");
    assert!(settles(&mono), "the monochrome icon never arrived");
    assert_eq!(
        fills(&mono),
        vec![Paint::Solid(INK)],
        "a set declaring `palette: false` is tinted whatever its markup"
    );

    clean(&root);
}

#[test]
fn without_a_source_an_unbaked_id_stays_empty() {
    reset_layout_runtime();
    RuntimeIcons::uninstall();
    let tree = laid_out("mdi:home");
    drain_tasks();
    assert!(!draws_a_path(&tree));
}
