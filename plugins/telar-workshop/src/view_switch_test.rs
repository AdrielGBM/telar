use telar::focus;
use telar::preview::{PreviewCtx, PreviewEntry};
use telar::testing::{centre, mount, named, press, release, route};
use telar::{
    AccessNode, App, AppRuntime, ComponentList, Container, Event, LayoutStyle, LocalApp, NamedKey,
    Role, box_item, reset_layout_runtime,
};

use super::*;
use crate::test_support::{Shell, settle};

fn blank(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(Container::new(LayoutStyle::new(), Vec::new())?))
}

struct Workshop {
    runtime: LocalApp<Shell>,
    tree: ComponentList,
}

impl Workshop {
    fn open() -> Self {
        reset_layout_runtime();
        focus::clear();
        let entries = vec![PreviewEntry::new(
            "button--primary",
            "button",
            "Primary",
            blank,
        )];
        let state = WorkshopState::new(entries.into(), &Default::default());
        let runtime = LocalApp(Shell { state });
        let tree = mount(runtime.0.root(), 1200, 800);
        let workshop = Self { runtime, tree };
        settle(&workshop.tree);
        workshop
    }

    fn view(&self) -> ViewMode {
        self.runtime.0.state.view().get()
    }

    fn send(&mut self, event: Event) {
        route(&mut self.tree, &event);
        settle(&self.tree);
    }

    fn tab(&self, name: &str) -> AccessNode {
        self.runtime
            .access_snapshot(&self.tree.commands())
            .into_iter()
            .find(|node| node.role == Role::Tab && node.name == name)
            .unwrap_or_else(|| panic!("no tab named {name}"))
    }

    fn click(&mut self, name: &str) {
        let rect = self.tab(name).rect;
        let (x, y) = centre(rect);
        self.send(press(x, y));
        self.send(release(x, y));
    }
}

#[test]
fn the_top_bar_switches_between_the_canvas_and_the_docs() {
    let mut workshop = Workshop::open();
    assert_eq!(workshop.tab("Canvas").toggled, Some(true));
    assert_eq!(workshop.tab("Docs").toggled, Some(false));
    workshop.click("Docs");
    assert_eq!(workshop.view(), ViewMode::Docs);
    assert_eq!(workshop.tab("Docs").toggled, Some(true));
    workshop.click("Canvas");
    assert_eq!(workshop.view(), ViewMode::Canvas);
}

#[test]
fn the_matrix_is_not_offered_before_it_lands() {
    let workshop = Workshop::open();
    let tabs: Vec<_> = workshop
        .runtime
        .access_snapshot(&workshop.tree.commands())
        .into_iter()
        .filter(|node| node.role == Role::Tab)
        .map(|node| node.name)
        .collect();
    assert!(tabs.contains(&"Canvas".to_string()) && tabs.contains(&"Docs".to_string()));
    assert!(!tabs.contains(&"Matrix".to_string()));
}

#[test]
fn the_arrows_move_along_the_views_while_one_has_focus() {
    let mut workshop = Workshop::open();
    workshop.click("Canvas");
    workshop.send(named(NamedKey::ArrowRight));
    assert_eq!(workshop.view(), ViewMode::Docs);
    workshop.send(named(NamedKey::ArrowRight));
    assert_eq!(workshop.view(), ViewMode::Canvas, "wrapping at the end");
    workshop.send(named(NamedKey::End));
    assert_eq!(workshop.view(), ViewMode::Docs);
    workshop.send(named(NamedKey::Home));
    assert_eq!(workshop.view(), ViewMode::Canvas);
}
