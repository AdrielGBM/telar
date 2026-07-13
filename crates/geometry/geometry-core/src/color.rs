#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::rgba(r, g, b, 1.0)
    }

    pub fn from_rgb_u8(r: u8, g: u8, b: u8) -> Self {
        Self::rgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
    }

    pub fn from_hsl(h: f32, s: f32, l: f32) -> Self {
        Self::from_hsla(h, s, l, 1.0)
    }

    pub fn from_hsla(h: f32, s: f32, l: f32, a: f32) -> Self {
        let h = h.rem_euclid(360.0);
        let s = s.clamp(0.0, 1.0);
        let l = l.clamp(0.0, 1.0);
        let a = a.clamp(0.0, 1.0);
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = l - c / 2.0;
        let (r, g, b) = Self::hue_to_rgb(c, x, h);
        Self::rgba(r + m, g + m, b + m, a)
    }

    pub fn from_hsv(h: f32, s: f32, v: f32) -> Self {
        Self::from_hsva(h, s, v, 1.0)
    }

    pub fn from_hsva(h: f32, s: f32, v: f32, a: f32) -> Self {
        let h = h.rem_euclid(360.0);
        let s = s.clamp(0.0, 1.0);
        let v = v.clamp(0.0, 1.0);
        let a = a.clamp(0.0, 1.0);
        let c = v * s;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = v - c;
        let (r, g, b) = Self::hue_to_rgb(c, x, h);
        Self::rgba(r + m, g + m, b + m, a)
    }

    pub fn from_oklch(l: f32, c: f32, h: f32) -> Self {
        Self::from_oklcha(l, c, h, 1.0)
    }

    pub fn from_oklcha(l: f32, c: f32, h: f32, a: f32) -> Self {
        let l = l.clamp(0.0, 1.0);
        let c = c.max(0.0);
        let alpha = a.clamp(0.0, 1.0);
        let h_rad = h.to_radians();
        let lab_a = c * h_rad.cos();
        let lab_b = c * h_rad.sin();
        // Ottosson's OKLab -> linear sRGB constants (bottomless.com/oklab).
        let l_ = l + 0.3963377774 * lab_a + 0.2158037573 * lab_b;
        let m_ = l - 0.1055613458 * lab_a - 0.0638541728 * lab_b;
        let s_ = l - 0.0894841775 * lab_a - 1.2914855480 * lab_b;
        let l3 = l_ * l_ * l_;
        let m3 = m_ * m_ * m_;
        let s3 = s_ * s_ * s_;
        let r = 4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3;
        let g = -1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3;
        let b = -0.0041960863 * l3 - 0.7034186147 * m3 + 1.7076147010 * s3;
        Self::rgba(
            Self::linear_to_srgb(r),
            Self::linear_to_srgb(g),
            Self::linear_to_srgb(b),
            alpha,
        )
    }

    /// Exact inverse of [`Color::from_oklcha`]: sRGB -> linear -> OKLab -> LCh. Returns `(l, c, h, a)` with `h` in degrees `[0, 360)`.
    pub fn to_oklcha(self) -> (f32, f32, f32, f32) {
        let r = Self::srgb_to_linear(self.r);
        let g = Self::srgb_to_linear(self.g);
        let b = Self::srgb_to_linear(self.b);
        // Ottosson's linear sRGB -> OKLab constants; the exact inverse of the matrices in from_oklcha.
        let l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
        let m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
        let s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;
        let l_ = l.cbrt();
        let m_ = m.cbrt();
        let s_ = s.cbrt();
        let lightness = 0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_;
        let lab_a = 1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_;
        let lab_b = 0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_;
        let c = (lab_a * lab_a + lab_b * lab_b).sqrt();
        let h = lab_b.atan2(lab_a).to_degrees().rem_euclid(360.0);
        (lightness, c, h, self.a)
    }

    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        // Byte-slice indexing below assumes single-byte chars.
        if !hex.is_ascii() {
            return None;
        }
        let byte = |s: &str| u8::from_str_radix(s, 16).ok();
        let (r, g, b, a) = match hex.len() {
            3 => (
                byte(&hex[0..1])? * 17,
                byte(&hex[1..2])? * 17,
                byte(&hex[2..3])? * 17,
                255,
            ),
            4 => (
                byte(&hex[0..1])? * 17,
                byte(&hex[1..2])? * 17,
                byte(&hex[2..3])? * 17,
                byte(&hex[3..4])? * 17,
            ),
            6 => (byte(&hex[0..2])?, byte(&hex[2..4])?, byte(&hex[4..6])?, 255),
            8 => (
                byte(&hex[0..2])?,
                byte(&hex[2..4])?,
                byte(&hex[4..6])?,
                byte(&hex[6..8])?,
            ),
            _ => return None,
        };
        Some(Self::from_rgba_u8(r, g, b, a))
    }

    pub fn from_rgba_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::rgba(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        )
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    pub fn darken(self, factor: f32) -> Self {
        Self {
            r: (self.r * (1.0 - factor)).max(0.0),
            g: (self.g * (1.0 - factor)).max(0.0),
            b: (self.b * (1.0 - factor)).max(0.0),
            a: self.a,
        }
    }

    /// WCAG 2.x relative luminance in `[0, 1]` (0 = black, 1 = white): the linearized sRGB channels weighted
    /// by human luminance sensitivity. Alpha is ignored — compose over a background first if it matters.
    pub fn relative_luminance(self) -> f32 {
        let r = Self::srgb_to_linear(self.r);
        let g = Self::srgb_to_linear(self.g);
        let b = Self::srgb_to_linear(self.b);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    /// WCAG 2.x contrast ratio between two colors, from `1.0` (identical) to `21.0` (black vs white).
    /// Order-independent. Use it to pick a legible foreground: `>= 4.5` passes AA for body text, `>= 3.0`
    /// for large text.
    pub fn contrast_ratio(self, other: Color) -> f32 {
        let a = self.relative_luminance();
        let b = other.relative_luminance();
        let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// Picks the most legible foreground for this color (treated as a background): the candidate with the
    /// highest [`contrast_ratio`](Self::contrast_ratio) against `self`. Returns `self` if `candidates` is
    /// empty. This is the "auto-contrast" pick — e.g. a filled chip choosing text vs base over its accent.
    pub fn most_readable(self, candidates: &[Color]) -> Color {
        candidates
            .iter()
            .copied()
            .max_by(|a, b| self.contrast_ratio(*a).total_cmp(&self.contrast_ratio(*b)))
            .unwrap_or(self)
    }

    #[inline]
    pub fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    fn hue_to_rgb(c: f32, x: f32, h: f32) -> (f32, f32, f32) {
        // Callers pass h already normalized to [0, 360) (from_hsla applies rem_euclid).
        match h as u32 / 60 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        }
    }

    fn linear_to_srgb(c: f32) -> f32 {
        let c = c.clamp(0.0, 1.0);
        if c <= 0.0031308 {
            12.92 * c
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    }

    fn srgb_to_linear(c: f32) -> f32 {
        let c = c.clamp(0.0, 1.0);
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    #[inline]
    pub fn to_rgba8(self) -> [u8; 4] {
        [
            (self.r * 255.0).clamp(0.0, 255.0) as u8,
            (self.g * 255.0).clamp(0.0, 255.0) as u8,
            (self.b * 255.0).clamp(0.0, 255.0) as u8,
            (self.a * 255.0).clamp(0.0, 255.0) as u8,
        ]
    }

    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);
    pub const RED: Self = Self::rgb(1.0, 0.0, 0.0);
    pub const GREEN: Self = Self::rgb(0.0, 1.0, 0.0);
    pub const BLUE: Self = Self::rgb(0.0, 0.0, 1.0);
    pub const TRANSPARENT: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_sets_alpha_to_one() {
        let color = Color::rgb(0.5, 0.5, 0.5);
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn rgba_stores_all_components() {
        let color = Color::rgba(0.1, 0.2, 0.3, 0.4);
        assert_eq!(color.r, 0.1);
        assert_eq!(color.g, 0.2);
        assert_eq!(color.b, 0.3);
        assert_eq!(color.a, 0.4);
    }

    #[test]
    fn from_rgb_u8_normalizes_to_float() {
        let color = Color::from_rgb_u8(255, 0, 0);
        assert_eq!(color.r, 1.0);
        assert_eq!(color.g, 0.0);
        assert_eq!(color.b, 0.0);
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn to_rgba8_white() {
        assert_eq!(Color::WHITE.to_rgba8(), [255, 255, 255, 255]);
    }

    #[test]
    fn to_rgba8_black() {
        assert_eq!(Color::BLACK.to_rgba8(), [0, 0, 0, 255]);
    }

    #[test]
    fn to_rgba8_transparent() {
        assert_eq!(Color::TRANSPARENT.to_rgba8(), [0, 0, 0, 0]);
    }

    #[test]
    fn to_rgba8_clamps_above_one() {
        let color = Color::rgba(2.0, 2.0, 2.0, 2.0);
        assert_eq!(color.to_rgba8(), [255, 255, 255, 255]);
    }

    #[test]
    fn to_rgba8_clamps_below_zero() {
        let color = Color::rgba(-1.0, -1.0, -1.0, -1.0);
        assert_eq!(color.to_rgba8(), [0, 0, 0, 0]);
    }

    #[test]
    fn constant_red_components() {
        assert_eq!(Color::RED.r, 1.0);
        assert_eq!(Color::RED.g, 0.0);
        assert_eq!(Color::RED.b, 0.0);
    }

    #[test]
    fn constant_green_components() {
        assert_eq!(Color::GREEN.r, 0.0);
        assert_eq!(Color::GREEN.g, 1.0);
        assert_eq!(Color::GREEN.b, 0.0);
    }

    #[test]
    fn constant_blue_components() {
        assert_eq!(Color::BLUE.r, 0.0);
        assert_eq!(Color::BLUE.g, 0.0);
        assert_eq!(Color::BLUE.b, 1.0);
    }

    #[test]
    fn from_hsl_red() {
        let color = Color::from_hsl(0.0, 1.0, 0.5);
        assert!((color.r - 1.0).abs() < 1e-5);
        assert!(color.g.abs() < 1e-5);
        assert!(color.b.abs() < 1e-5);
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn from_hsl_white() {
        let color = Color::from_hsl(0.0, 0.0, 1.0);
        assert!((color.r - 1.0).abs() < 1e-5);
        assert!((color.g - 1.0).abs() < 1e-5);
        assert!((color.b - 1.0).abs() < 1e-5);
    }

    #[test]
    fn from_hsla_sets_alpha() {
        let color = Color::from_hsla(0.0, 0.0, 0.0, 0.5);
        assert_eq!(color.a, 0.5);
    }

    #[test]
    fn from_hsla_hue_wraps_above_360() {
        let color1 = Color::from_hsla(400.0, 1.0, 0.5, 1.0);
        let color2 = Color::from_hsla(40.0, 1.0, 0.5, 1.0);
        assert!((color1.r - color2.r).abs() < 1e-5);
        assert!((color1.g - color2.g).abs() < 1e-5);
        assert!((color1.b - color2.b).abs() < 1e-5);
    }

    #[test]
    fn from_hsla_hue_wraps_negative() {
        let color1 = Color::from_hsla(-30.0, 1.0, 0.5, 1.0);
        let color2 = Color::from_hsla(330.0, 1.0, 0.5, 1.0);
        assert!((color1.r - color2.r).abs() < 1e-5);
        assert!((color1.g - color2.g).abs() < 1e-5);
        assert!((color1.b - color2.b).abs() < 1e-5);
    }

    #[test]
    fn from_hsla_clamps_saturation_above_one() {
        let color = Color::from_hsla(0.0, 2.0, 0.5, 1.0);
        let expected = Color::from_hsla(0.0, 1.0, 0.5, 1.0);
        assert!((color.r - expected.r).abs() < 1e-5);
        assert!((color.g - expected.g).abs() < 1e-5);
        assert!((color.b - expected.b).abs() < 1e-5);
    }

    #[test]
    fn from_hsla_clamps_lightness_above_one() {
        let color = Color::from_hsla(0.0, 1.0, 1.5, 1.0);
        assert!((color.r - 1.0).abs() < 1e-5);
        assert!((color.g - 1.0).abs() < 1e-5);
        assert!((color.b - 1.0).abs() < 1e-5);
    }

    #[test]
    fn from_hsla_clamps_lightness_below_zero() {
        let color = Color::from_hsla(0.0, 1.0, -0.5, 1.0);
        assert!(color.r.abs() < 1e-5);
        assert!(color.g.abs() < 1e-5);
        assert!(color.b.abs() < 1e-5);
    }

    #[test]
    fn from_hsla_clamps_alpha_above_one() {
        let color = Color::from_hsla(0.0, 0.0, 0.5, 2.0);
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn from_hsla_clamps_alpha_below_zero() {
        let color = Color::from_hsla(0.0, 0.0, 0.5, -1.0);
        assert_eq!(color.a, 0.0);
    }

    fn assert_rgb(color: Color, r: f32, g: f32, b: f32, tol: f32) {
        assert!((color.r - r).abs() < tol, "r: {} != {}", color.r, r);
        assert!((color.g - g).abs() < tol, "g: {} != {}", color.g, g);
        assert!((color.b - b).abs() < tol, "b: {} != {}", color.b, b);
    }

    #[test]
    fn from_hsl_green_at_120() {
        assert_rgb(Color::from_hsl(120.0, 1.0, 0.5), 0.0, 1.0, 0.0, 1e-5);
    }

    #[test]
    fn from_hsl_blue_at_240() {
        assert_rgb(Color::from_hsl(240.0, 1.0, 0.5), 0.0, 0.0, 1.0, 1e-5);
    }

    #[test]
    fn from_hsl_yellow_at_60() {
        assert_rgb(Color::from_hsl(60.0, 1.0, 0.5), 1.0, 1.0, 0.0, 1e-5);
    }

    #[test]
    fn from_hsl_cyan_at_180() {
        assert_rgb(Color::from_hsl(180.0, 1.0, 0.5), 0.0, 1.0, 1.0, 1e-5);
    }

    #[test]
    fn from_hsl_magenta_at_300() {
        assert_rgb(Color::from_hsl(300.0, 1.0, 0.5), 1.0, 0.0, 1.0, 1e-5);
    }

    #[test]
    fn from_hsl_zero_saturation_is_gray() {
        assert_rgb(Color::from_hsl(200.0, 0.0, 0.5), 0.5, 0.5, 0.5, 1e-5);
    }

    #[test]
    fn from_hsv_primaries() {
        assert_rgb(Color::from_hsv(0.0, 1.0, 1.0), 1.0, 0.0, 0.0, 1e-5);
        assert_rgb(Color::from_hsv(120.0, 1.0, 1.0), 0.0, 1.0, 0.0, 1e-5);
        assert_rgb(Color::from_hsv(240.0, 1.0, 1.0), 0.0, 0.0, 1.0, 1e-5);
    }

    #[test]
    fn from_hsv_zero_value_is_black() {
        assert_rgb(Color::from_hsv(200.0, 1.0, 0.0), 0.0, 0.0, 0.0, 1e-5);
    }

    #[test]
    fn from_hsv_zero_saturation_is_gray() {
        assert_rgb(Color::from_hsv(200.0, 0.0, 0.5), 0.5, 0.5, 0.5, 1e-5);
    }

    #[test]
    fn from_hsv_half_value_red() {
        assert_rgb(Color::from_hsv(0.0, 1.0, 0.5), 0.5, 0.0, 0.0, 1e-5);
    }

    #[test]
    fn from_hsva_sets_alpha() {
        assert_eq!(Color::from_hsva(0.0, 0.0, 0.0, 0.5).a, 0.5);
    }

    #[test]
    fn from_hex_six_digits() {
        assert_rgb(Color::from_hex("#ff0000").unwrap(), 1.0, 0.0, 0.0, 1e-5);
    }

    #[test]
    fn from_hex_without_prefix() {
        assert_rgb(Color::from_hex("00ff00").unwrap(), 0.0, 1.0, 0.0, 1e-5);
    }

    #[test]
    fn from_hex_three_digits_expands() {
        let short = Color::from_hex("#f00").unwrap();
        assert_rgb(short, 1.0, 0.0, 0.0, 1e-5);
    }

    #[test]
    fn from_hex_eight_digits_reads_alpha() {
        let color = Color::from_hex("#0000ff80").unwrap();
        assert_rgb(color, 0.0, 0.0, 1.0, 1e-5);
        assert!((color.a - 128.0 / 255.0).abs() < 1e-5);
    }

    #[test]
    fn from_hex_four_digits_reads_alpha() {
        let color = Color::from_hex("#00f8").unwrap();
        assert_rgb(color, 0.0, 0.0, 1.0, 1e-5);
        assert!((color.a - 0x88 as f32 / 255.0).abs() < 1e-5);
    }

    #[test]
    fn from_hex_rejects_invalid() {
        assert!(Color::from_hex("#gggggg").is_none());
        assert!(Color::from_hex("12345").is_none());
        assert!(Color::from_hex("").is_none());
    }

    #[test]
    fn from_oklch_white() {
        assert_rgb(Color::from_oklch(1.0, 0.0, 0.0), 1.0, 1.0, 1.0, 1e-4);
    }

    #[test]
    fn from_oklch_black() {
        assert_rgb(Color::from_oklch(0.0, 0.0, 0.0), 0.0, 0.0, 0.0, 1e-4);
    }

    #[test]
    fn from_oklch_srgb_red() {
        // sRGB pure red expressed in OKLCH (culori reference).
        assert_rgb(
            Color::from_oklch(0.627_955, 0.257_683, 29.233_88),
            1.0,
            0.0,
            0.0,
            2e-2,
        );
    }

    #[test]
    fn from_oklcha_sets_alpha() {
        assert_eq!(Color::from_oklcha(0.5, 0.1, 120.0, 0.25).a, 0.25);
    }

    #[test]
    fn to_oklcha_round_trips_srgb() {
        for &color in &[
            Color::rgb(0.8, 0.2, 0.3),
            Color::rgb(0.1, 0.6, 0.9),
            Color::rgb(0.5, 0.5, 0.5),
            Color::rgba(0.2, 0.7, 0.4, 0.6),
        ] {
            let (l, c, h, a) = color.to_oklcha();
            let back = Color::from_oklcha(l, c, h, a);
            assert!(
                (back.r - color.r).abs() < 1e-4,
                "r: {} != {}",
                back.r,
                color.r
            );
            assert!(
                (back.g - color.g).abs() < 1e-4,
                "g: {} != {}",
                back.g,
                color.g
            );
            assert!(
                (back.b - color.b).abs() < 1e-4,
                "b: {} != {}",
                back.b,
                color.b
            );
            assert!(
                (back.a - color.a).abs() < 1e-6,
                "a: {} != {}",
                back.a,
                color.a
            );
        }
    }

    #[test]
    fn to_oklcha_gray_is_achromatic() {
        let (_, c, _, _) = Color::rgb(0.5, 0.5, 0.5).to_oklcha();
        assert!(c < 1e-4, "expected near-zero chroma, got {c}");
    }

    #[test]
    fn relative_luminance_black_and_white() {
        assert!(Color::BLACK.relative_luminance().abs() < 1e-6);
        assert!((Color::WHITE.relative_luminance() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn contrast_ratio_extremes_and_identity() {
        // WCAG max contrast is 21:1 (black vs white); a color against itself is 1:1.
        assert!((Color::WHITE.contrast_ratio(Color::BLACK) - 21.0).abs() < 1e-3);
        assert!((Color::BLACK.contrast_ratio(Color::WHITE) - 21.0).abs() < 1e-3);
        assert!((Color::RED.contrast_ratio(Color::RED) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn most_readable_picks_the_higher_contrast_foreground() {
        let ink = Color::from_rgb_u8(20, 20, 25);
        let paper = Color::from_rgb_u8(240, 240, 245);
        // On a light background, the dark ink is more legible; on a dark one, the light paper.
        let light_bg = Color::from_rgb_u8(230, 220, 180);
        let dark_bg = Color::from_rgb_u8(40, 50, 70);
        assert_eq!(light_bg.most_readable(&[ink, paper]), ink);
        assert_eq!(dark_bg.most_readable(&[ink, paper]), paper);
        // Empty candidates fall back to self.
        assert_eq!(light_bg.most_readable(&[]), light_bg);
    }

    #[test]
    fn oklch_round_trip_is_stable() {
        // Round-tripping an in-gamut sRGB color through OKLCH and back must be idempotent (out-of-gamut OKLCH inputs are clamped, so we start from sRGB).
        let start = Color::rgb(0.1, 0.6, 0.9);
        let (l1, c1, h1, _) = start.to_oklcha();
        let (l2, c2, h2, _) = Color::from_oklcha(l1, c1, h1, 1.0).to_oklcha();
        assert!((l1 - l2).abs() < 1e-3, "l: {l1} != {l2}");
        assert!((c1 - c2).abs() < 1e-3, "c: {c1} != {c2}");
        assert!((h1 - h2).abs() < 1e-2, "h: {h1} != {h2}");
    }
}
