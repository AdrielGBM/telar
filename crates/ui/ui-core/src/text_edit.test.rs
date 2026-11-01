use super::*;

#[test]
fn boundaries_step_over_whole_characters() {
    let text = "añ日";
    assert_eq!(next_boundary(text, 0), 1);
    assert_eq!(next_boundary(text, 1), 3);
    assert_eq!(next_boundary(text, 3), 6);
    assert_eq!(next_boundary(text, 6), 6);
    assert_eq!(prev_boundary(text, 6), 3);
    assert_eq!(prev_boundary(text, 3), 1);
    assert_eq!(prev_boundary(text, 0), 0);
    assert_eq!(floor_boundary(text, 2), 1);
    assert_eq!(floor_boundary(text, 99), 6);
}

#[test]
fn a_word_is_the_run_of_letters_a_press_lands_in() {
    let text = "dijo «mañana», y_se fue";
    let at = text.find("ñ").unwrap();
    assert_eq!(&text[word_at(text, at)], "mañana");
    assert_eq!(&text[word_at(text, text.find("y_se").unwrap())], "y_se");
    assert_eq!(&text[word_at(text, text.find(' ').unwrap())], " ");
    assert_eq!(&text[word_at(text, text.find('«').unwrap())], "«");
}

#[test]
fn a_press_past_the_end_of_a_line_takes_the_word_before_it() {
    let text = "uno dos\ntres";
    assert_eq!(&text[word_at(text, text.find('\n').unwrap())], "dos");
    assert_eq!(&text[word_at(text, text.len())], "tres");
    assert_eq!(word_at("a\n\nb", 2), 2..2);
    assert_eq!(word_at("", 0), 0..0);
}

#[test]
fn a_line_runs_between_the_breaks_around_it() {
    let text = "uno\ndos tres\ncuatro";
    assert_eq!(&text[line_at(text, text.find("tres").unwrap())], "dos tres");
    assert_eq!(&text[line_at(text, 0)], "uno");
    assert_eq!(&text[line_at(text, text.len())], "cuatro");
}

#[test]
fn presses_close_together_count_up_to_three_and_start_over() {
    let mut clicks = Clicks::default();
    assert_eq!(clicks.press(10.0, 10.0), 1);
    assert_eq!(clicks.press(11.0, 10.0), 2);
    assert_eq!(clicks.press(11.0, 11.0), 3);
    assert_eq!(clicks.press(11.0, 11.0), 1);
    assert_eq!(
        clicks.press(80.0, 11.0),
        1,
        "a press elsewhere is a new click"
    );
}

#[test]
fn presses_far_apart_in_time_are_single_clicks() {
    let mut clicks = Clicks::default();
    assert_eq!(clicks.press(0.0, 0.0), 1);
    std::thread::sleep(DOUBLE_CLICK + Duration::from_millis(30));
    assert_eq!(clicks.press(0.0, 0.0), 1);
}
