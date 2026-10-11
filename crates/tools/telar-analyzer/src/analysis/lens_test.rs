use std::path::PathBuf;

use super::*;
use telar_parser::parse;

fn host(component: &str) -> PreviewHost {
    PreviewHost {
        package_root: PathBuf::from("/work/app"),
        crate_name: "my_app".to_string(),
        component: component.to_string(),
        workshop_channel: Some(PathBuf::from("/work/.telar/workshop/my-app.channel")),
    }
}

fn command(lens: &CodeLens) -> (&str, &str, PreviewTarget) {
    let command = lens.command.as_ref().unwrap();
    let [argument] = command.arguments.as_deref().unwrap() else {
        panic!("one argument: the preview's target");
    };
    let target = PreviewTarget {
        id: argument["id"].as_str().unwrap().to_string(),
        package_dir: argument["packageDir"].as_str().unwrap().to_string(),
        location: argument["location"].as_str().unwrap().to_string(),
        channel: argument["channel"].as_str().map(str::to_string),
    };
    (command.title.as_str(), command.command.as_str(), target)
}

#[test]
fn opens_each_preview_in_the_workshop_by_its_id() {
    let src =
        "[view]\ncol\n\n[preview \"Default\"]\ncol\n\n[preview \"Landing — full page\"]\nbox\n";
    let lenses = code_lenses(&parse(src).unwrap(), &host("card"));
    assert_eq!(lenses.len(), 2);
    assert_eq!(lenses[0].range.start.line, 3);
    assert_eq!(lenses[1].range.start.line, 6);
    assert_eq!(
        command(&lenses[0]),
        (
            "▶ Open in workshop",
            OPEN_IN_WORKSHOP,
            PreviewTarget {
                id: "my_app--card--default".to_string(),
                package_dir: "/work/app".to_string(),
                location: "/preview/my_app--card--default".to_string(),
                channel: Some("/work/.telar/workshop/my-app.channel".to_string()),
            }
        )
    );
    assert_eq!(command(&lenses[1]).2.id, "my_app--card--landing-full-page");
}

#[test]
fn a_play_zone_gets_a_lens_that_runs_it() {
    let src = "[view]\ncol\n\n[preview \"Bound\" args(n:1)]\ncol\n\n[play]\ncanvas.expect_text(\"1\")?;\n";
    let lenses = code_lenses(&parse(src).unwrap(), &host("counter"));
    assert_eq!(lenses.len(), 2);
    let (title, name, target) = command(&lenses[1]);
    assert_eq!((title, name), ("▶ Run play", RUN_PLAY));
    assert_eq!(target.id, "my_app--counter--bound");
    assert_eq!(lenses[1].range.start.line, 6, "on the [play] header");
}

/// The transpiler refuses these an id, so a lens would open a preview that does not exist.
#[test]
fn a_preview_with_no_id_of_its_own_gets_no_lens() {
    let src =
        "[view]\ncol\n\n[preview \"A b\"]\ncol\n\n[preview \"a-B\"]\ncol\n\n[preview \"—\"]\ncol\n";
    let lenses = code_lenses(&parse(src).unwrap(), &host("card"));
    assert_eq!(lenses.len(), 1);
    assert_eq!(command(&lenses[0]).2.id, "my_app--card--a-b");
}
