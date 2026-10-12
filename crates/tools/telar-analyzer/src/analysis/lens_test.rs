use std::path::PathBuf;

use super::*;
use crate::analysis::preview_target::Workshop;
use telar_parser::parse;

fn host(component: &str) -> PreviewHost {
    PreviewHost {
        package_root: PathBuf::from("/work/app"),
        crate_name: "my_app".to_string(),
        component: component.to_string(),
        workshops: vec![Workshop {
            package_dir: "/work/app".to_string(),
            channel: "/work/.telar/workshop/my-app.channel".to_string(),
            server: "/work/.telar/workshop/my-app.server".to_string(),
        }],
    }
}

fn command(lens: &CodeLens) -> (&str, &str, &serde_json::Value) {
    let command = lens.command.as_ref().unwrap();
    let [argument] = command.arguments.as_deref().unwrap() else {
        panic!("one argument: the preview's target");
    };
    (command.title.as_str(), command.command.as_str(), argument)
}

#[test]
fn opens_each_preview_in_the_workshop_and_the_panel_by_its_id() {
    let src =
        "[view]\ncol\n\n[preview \"Default\"]\ncol\n\n[preview \"Landing — full page\"]\nbox\n";
    let lenses = code_lenses(&parse(src).unwrap(), &host("card"));
    assert_eq!(lenses.len(), 4);
    assert_eq!(lenses[0].range.start.line, 3);
    assert_eq!(lenses[1].range.start.line, 3);
    assert_eq!(lenses[2].range.start.line, 6);
    let target = serde_json::json!({
        "id": "my_app--card--default",
        "packageDir": "/work/app",
        "location": "/preview/my_app--card--default",
        "workshops": [{
            "packageDir": "/work/app",
            "channel": "/work/.telar/workshop/my-app.channel",
            "server": "/work/.telar/workshop/my-app.server",
        }],
    });
    assert_eq!(
        command(&lenses[0]),
        ("▶ Open in workshop", OPEN_IN_WORKSHOP, &target)
    );
    assert_eq!(
        command(&lenses[1]),
        ("▶ Show in panel", SHOW_IN_PANEL, &target)
    );
    assert_eq!(
        command(&lenses[2]).2["id"],
        "my_app--card--landing-full-page"
    );
}

#[test]
fn a_play_zone_gets_a_lens_that_runs_it() {
    let src = "[view]\ncol\n\n[preview \"Bound\" args(n:1)]\ncol\n\n[play]\ncanvas.expect_text(\"1\")?;\n";
    let lenses = code_lenses(&parse(src).unwrap(), &host("counter"));
    assert_eq!(lenses.len(), 3);
    let (title, name, target) = command(&lenses[2]);
    assert_eq!((title, name), ("▶ Run play", RUN_PLAY));
    assert_eq!(target["id"], "my_app--counter--bound");
    assert_eq!(lenses[2].range.start.line, 6, "on the [play] header");
}

/// The transpiler refuses these an id, so a lens would open a preview that does not exist.
#[test]
fn a_preview_with_no_id_of_its_own_gets_no_lens() {
    let src =
        "[view]\ncol\n\n[preview \"A b\"]\ncol\n\n[preview \"a-B\"]\ncol\n\n[preview \"—\"]\ncol\n";
    let lenses = code_lenses(&parse(src).unwrap(), &host("card"));
    assert_eq!(lenses.len(), 2);
    assert_eq!(command(&lenses[0]).2["id"], "my_app--card--a-b");
}
