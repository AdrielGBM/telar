use telar::testing::lay_out;
use telar::{
    AccessNode, BODY_TEXT_CONTRAST, ColorScheme, ComponentList, DrawCommand, Role, ThemeTokens,
    focus, register_mode, relayout_if_dirty, set_mode, set_mode_scheme,
};

use super::*;

fn mounted(props: TextFieldPropsBuilder) -> ComponentList {
    crate::test_support::fresh_layout_runtime();
    focus::clear();
    let item = text_field(props.build()).unwrap();
    lay_out(item.layout_node(), 400.0, 400.0);
    let tree = ComponentList::new(item);
    settle(&tree);
    tree
}

fn settle(tree: &ComponentList) {
    relayout_if_dirty();
    let _ = tree.commands();
}

fn read(tree: &ComponentList) -> Vec<AccessNode> {
    telar::inspect::snapshot(&tree.commands())
}

fn the_field(tree: &ComponentList) -> AccessNode {
    read(tree)
        .into_iter()
        .find(|node| node.role == Role::TextInput)
        .expect("the field is read")
}

fn said(text: &str) -> Option<String> {
    Some(text.to_string())
}

fn ink_of(tree: &ComponentList, text: &str) -> Color {
    tree.commands()
        .iter()
        .find_map(|command| match command {
            DrawCommand::Text {
                text: drawn, style, ..
            } if drawn.as_ref() == text => Some(style.color.solid_color()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("`{text}` is drawn"))
}

/// A field whose content fails its checks says so to a reader along with why, and shows why under the box.
#[test]
fn a_field_with_an_error_reports_invalid_and_the_message_as_its_description() {
    let tree = mounted(
        TextFieldProps::props()
            .label("Email")
            .error(said("Enter an address with an @")),
    );
    let field = the_field(&tree);
    assert!(field.invalid);
    assert_eq!(
        field.description.as_deref(),
        Some("Enter an address with an @")
    );
    assert!(telar::testing::find_text(
        &tree,
        "Enter an address with an @"
    ));
}

/// The error a form works out as the person types comes and goes without the field being rebuilt, and while there is none the hint is what goes with the field.
#[test]
fn an_error_that_clears_leaves_the_hint_as_the_description() {
    let error = signal(said("Too short"));
    let tree = mounted(
        TextFieldProps::props()
            .label("Password")
            .hint(said("At least 8 characters"))
            .error(error),
    );
    let field = the_field(&tree);
    assert!(field.invalid);
    assert_eq!(field.description.as_deref(), Some("Too short"));
    assert!(!telar::testing::find_text(&tree, "At least 8 characters"));

    error.set(None);
    settle(&tree);
    let field = the_field(&tree);
    assert!(!field.invalid, "no message is nothing wrong");
    assert_eq!(field.description.as_deref(), Some("At least 8 characters"));
    assert!(telar::testing::find_text(&tree, "At least 8 characters"));
}

/// A field that must be filled in is announced so, and the mark that shows it to the eye is not read out as a word of its caption.
#[test]
fn a_required_field_reports_required_and_its_mark_is_not_read() {
    let tree = mounted(TextFieldProps::props().label("Name").required(true));
    assert!(the_field(&tree).required);
    assert!(telar::testing::find_text(&tree, REQUIRED_MARK_DRAWN));
    assert!(
        read(&tree).iter().all(|node| !node.name.contains('*')),
        "nothing a reader hears says the mark"
    );

    let plain = mounted(TextFieldProps::props().label("Name"));
    let field = the_field(&plain);
    assert!(!field.required && !field.invalid);
    assert_eq!(field.description, None);
}

const REQUIRED_MARK_DRAWN: &str = "*";

/// Help is what a reader hears after the field, and says nothing of it being wrong.
#[test]
fn a_hint_alone_describes_the_field_without_making_it_invalid() {
    let tree = mounted(
        TextFieldProps::props()
            .label("Handle")
            .hint(said("Shown on your profile")),
    );
    let field = the_field(&tree);
    assert!(!field.invalid);
    assert_eq!(field.description.as_deref(), Some("Shown on your profile"));
}

#[derive(Clone)]
struct Plain;

impl ThemeTokens for Plain {}

#[derive(Clone)]
struct PaleError;

impl ThemeTokens for PaleError {
    fn error(&self) -> Color {
        Color::rgba(1.0, 0.75, 0.78, 1.0)
    }
}

/// The message and the required mark are read at 4.5:1 on the surface in either mode, even where the theme's error colour alone would not be.
#[test]
fn the_error_and_its_mark_read_at_body_contrast_in_either_mode() {
    for scheme in [ColorScheme::Light, ColorScheme::Dark] {
        for pale in [false, true] {
            match pale {
                true => telar::set_theme(PaleError),
                false => telar::set_theme(Plain),
            }
            let id = format!("{scheme:?}");
            register_mode(id.clone(), || {});
            set_mode_scheme(id.clone(), scheme);
            set_mode(id);
            let tree = mounted(
                TextFieldProps::props()
                    .label("Email")
                    .required(true)
                    .error(said("Enter an address with an @")),
            );
            let surface = telar::use_theme_tokens().surface();
            for drawn in ["Enter an address with an @", REQUIRED_MARK_DRAWN] {
                let ratio = surface.contrast_ratio(ink_of(&tree, drawn));
                assert!(
                    ratio >= BODY_TEXT_CONTRAST,
                    "{scheme:?}, pale {pale}: `{drawn}` reads at {ratio:.2}:1"
                );
            }
        }
    }
}

/// The box itself shows which field the message is about, in the same colour as the message.
#[test]
fn the_frame_of_a_field_with_an_error_takes_the_error_colour() {
    let frames = |tree: &ComponentList| -> Vec<Color> {
        tree.commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Rect { style, .. } => {
                    style.border.map(|border| border.paint.solid_color())
                }
                _ => None,
            })
            .collect()
    };
    let wrong = mounted(TextFieldProps::props().error(said("Required")));
    assert!(frames(&wrong).contains(&shared::error_ink()));
    let fine = mounted(TextFieldProps::props().error(None));
    assert!(!frames(&fine).contains(&shared::error_ink()));
}
