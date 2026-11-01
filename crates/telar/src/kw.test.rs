use super::*;
use crate::Color;

#[test]
fn a_word_is_its_axis_s_value() {
    assert_eq!(small, ControlSize::Small);
    assert_eq!(ghost, Variant::Ghost);
    assert_eq!(danger, Tone::Danger);
    assert_eq!(ControlSize::WORDS, ["small", "regular", "large"]);
    assert_eq!(Tone::Warning.word(), Some("warning"));
    assert_eq!(Tone::from(Color::BLACK).word(), None);
    assert_eq!(Variant::from_word("outline"), Some(Variant::Outline));
    assert_eq!(Variant::from_word("Outline"), None);
    assert_eq!(<Tone as Word>::AXIS, "tone");
}

#[cfg(feature = "previews")]
#[test]
fn the_word_is_the_arg_text_and_nothing_else_is() {
    use crate::preview::{ArgValue, ControlKind, PreviewArg};

    let read = |text: &str| text.parse::<ArgValue>().unwrap();
    assert_eq!(ControlSize::Large.to_arg_value().to_string(), "large");
    assert_eq!(
        ControlSize::from_arg_value(&read("small")),
        Some(ControlSize::Small)
    );
    assert_eq!(ControlSize::from_arg_value(&read("Small")), None);
    assert_eq!(Variant::Soft.to_arg_value().to_string(), "soft");
    assert_eq!(Tone::Info.to_arg_value().to_string(), "info");
    assert_eq!(
        Tone::from_arg_value(&read("#ff8800")),
        Some(Tone::from(Color::from_hex("#ff8800").unwrap()))
    );
    assert_eq!(Tone::from_arg_value(&read("Info")), None);
    assert!(matches!(
        ControlSize::CONTROL,
        ControlKind::Choice { variants } if variants == ["small", "regular", "large"]
    ));
}
