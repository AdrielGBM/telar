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
    fn min_content(&self, text: &str, spans: Option<&[Span]>, style: &TextStyle) -> (f32, f32) {
        self.measure(text, spans, 0.0, style)
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

#[test]
fn invalidating_moves_the_generation_forward() {
    let before = text_metrics_generation();
    invalidate_text_metrics();
    assert!(text_metrics_generation() > before);
}

struct Proportional;

impl TextMetrics for Proportional {
    fn measure(
        &self,
        text: &str,
        _spans: Option<&[Span]>,
        _max_width: f32,
        style: &TextStyle,
    ) -> (f32, f32) {
        let advance = 0.6 * style.font_size + style.letter_spacing;
        let line = style.line_height.factor().unwrap_or(1.2);
        (
            text.chars().count() as f32 * advance,
            line * style.font_size,
        )
    }
    fn min_content(&self, text: &str, spans: Option<&[Span]>, style: &TextStyle) -> (f32, f32) {
        self.measure(text, spans, 0.0, style)
    }
    fn ink_bounds(&self, _text: &str, _max_width: f32, style: &TextStyle) -> (f32, f32) {
        (0.0, style.font_size)
    }
    fn line_height(&self, font_size: f32) -> f32 {
        1.2 * font_size
    }
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1.0e-3
}

#[test]
fn a_fitted_line_comes_to_the_width_with_its_tracking_scaled_or_held() {
    let in_em =
        |size: f32| TextStyle::new(size, crate::Color::BLACK).with_letter_spacing(-0.04 * size);
    let size = Proportional
        .fitted_size("ADRIEL", None, 336.0, None, &in_em)
        .expect("a line that grows with its size fits");
    assert!(close(size, 336.0 / (6.0 * 0.56)), "{size}");

    let in_px = |size: f32| TextStyle::new(size, crate::Color::BLACK).with_letter_spacing(2.0);
    let size = Proportional
        .fitted_size("ADRIEL", None, 336.0, None, &in_px)
        .expect("a line that grows with its size fits");
    assert!(
        close(
            Proportional.measure("ADRIEL", None, 1.0e6, &in_px(size)).0,
            336.0
        ),
        "tracking in pixels does not scale with the size, and the fit must not pretend it does: {size}"
    );
}

#[test]
fn a_maximum_height_caps_a_fitted_line() {
    let style_at = |size: f32| TextStyle::new(size, crate::Color::BLACK).with_line_height(0.84);
    let size = Proportional
        .fitted_size("AB", None, 600.0, Some(84.0), &style_at)
        .expect("fits");
    assert!(close(size, 100.0), "{size}");
    let size = Proportional
        .fitted_size("AB", None, 60.0, Some(84.0), &style_at)
        .expect("fits");
    assert!(
        close(size, 50.0),
        "a height that does not bind changes nothing: {size}"
    );
}

#[test]
fn a_line_whose_extent_does_not_grow_cannot_be_fitted() {
    let style_at = |size: f32| TextStyle::new(size, crate::Color::BLACK);
    assert_eq!(
        Fixed(8.0).fitted_size("ADRIEL", None, 300.0, None, &style_at),
        None,
        "a cell is a cell whatever the size"
    );
    assert_eq!(
        Proportional.fitted_size("", None, 300.0, None, &style_at),
        None
    );
}
