use super::*;

struct Fixed(f32);

impl TextMetrics for Fixed {
    fn measure(
        &self,
        text: &str,
        _spans: Option<&[Span]>,
        _max_width: f32,
        _style: &TextStyle,
    ) -> (f32, f32) {
        (text.chars().count() as f32 * self.0, self.0)
    }
    fn ink_bounds(&self, _text: &str, _max_width: f32, _style: &TextStyle) -> (f32, f32) {
        (0.0, self.0)
    }
    fn line_height(&self, _font_size: f32) -> f32 {
        self.0
    }
}

// One test rather than three: the installed measurer is process-wide, so separate tests would race over it.
#[test]
fn a_frontends_own_metrics_survive_the_runtimes_default() {
    let style = TextStyle::new(14.0, crate::Color::BLACK);

    assert!(
        set_default_text_metrics(Fixed(10.0)),
        "the first default install takes"
    );
    assert_eq!(measure_text("ab", None, 1.0e6, &style).0, 20.0);

    assert!(
        !set_default_text_metrics(Fixed(1.0)),
        "a second default must not displace metrics already in force — that is the whole point of it"
    );
    assert_eq!(line_height(14.0), 10.0);

    set_text_metrics(Fixed(2.0));
    assert_eq!(
        line_height(14.0),
        2.0,
        "an explicit install replaces, so a frontend can change its mind"
    );
}
