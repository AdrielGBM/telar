use super::*;

#[test]
fn a_named_tone_compares_by_its_name_and_a_colour_by_what_it_reads() {
    assert_eq!(Tone::Danger, Tone::Danger);
    assert_ne!(Tone::Danger, Tone::Warning);
    assert_eq!(Tone::from(Color::BLACK), Tone::from(Color::BLACK));
    assert_ne!(Tone::from(Color::BLACK), Tone::Neutral);
    assert_eq!(format!("{:?}", Tone::Info), "Info");
}

#[test]
fn a_smaller_control_steps_its_text_less_than_its_box() {
    assert!(ControlSize::Small.scale() < ControlSize::Small.text_scale());
    assert!(ControlSize::Large.scale() > ControlSize::Large.text_scale());
    assert_eq!(ControlSize::default(), ControlSize::Regular);
}
