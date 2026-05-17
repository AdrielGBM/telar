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

    pub fn from_rgba_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::rgba(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        )
    }

    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        let (r, g, b, a) = match hex.len() {
            3 => {
                let r = Self::parse_hex_byte(hex.as_bytes()[0], hex.as_bytes()[0])?;
                let g = Self::parse_hex_byte(hex.as_bytes()[1], hex.as_bytes()[1])?;
                let b = Self::parse_hex_byte(hex.as_bytes()[2], hex.as_bytes()[2])?;
                (r, g, b, 255)
            }
            4 => {
                let r = Self::parse_hex_byte(hex.as_bytes()[0], hex.as_bytes()[0])?;
                let g = Self::parse_hex_byte(hex.as_bytes()[1], hex.as_bytes()[1])?;
                let b = Self::parse_hex_byte(hex.as_bytes()[2], hex.as_bytes()[2])?;
                let a = Self::parse_hex_byte(hex.as_bytes()[3], hex.as_bytes()[3])?;
                (r, g, b, a)
            }
            6 => {
                let r = Self::parse_hex_byte(hex.as_bytes()[0], hex.as_bytes()[1])?;
                let g = Self::parse_hex_byte(hex.as_bytes()[2], hex.as_bytes()[3])?;
                let b = Self::parse_hex_byte(hex.as_bytes()[4], hex.as_bytes()[5])?;
                (r, g, b, 255)
            }
            8 => {
                let r = Self::parse_hex_byte(hex.as_bytes()[0], hex.as_bytes()[1])?;
                let g = Self::parse_hex_byte(hex.as_bytes()[2], hex.as_bytes()[3])?;
                let b = Self::parse_hex_byte(hex.as_bytes()[4], hex.as_bytes()[5])?;
                let a = Self::parse_hex_byte(hex.as_bytes()[6], hex.as_bytes()[7])?;
                (r, g, b, a)
            }
            _ => return None,
        };
        Some(Self::from_rgba_u8(r, g, b, a))
    }

    fn parse_hex_byte(hi: u8, lo: u8) -> Option<u8> {
        let hi_val = match hi {
            b'0'..=b'9' => hi - b'0',
            b'a'..=b'f' => hi - b'a' + 10,
            b'A'..=b'F' => hi - b'A' + 10,
            _ => return None,
        };
        let lo_val = match lo {
            b'0'..=b'9' => lo - b'0',
            b'a'..=b'f' => lo - b'a' + 10,
            b'A'..=b'F' => lo - b'A' + 10,
            _ => return None,
        };
        Some((hi_val << 4) | lo_val)
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

    pub fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    fn hue_to_rgb(c: f32, x: f32, h: f32) -> (f32, f32, f32) {
        match ((h % 360.0 + 360.0) % 360.0) as u32 / 60 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        }
    }

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
    fn from_hex_red() {
        let color = Color::from_hex("#FF0000").unwrap();
        assert_eq!(color.r, 1.0);
        assert_eq!(color.g, 0.0);
        assert_eq!(color.b, 0.0);
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn from_hex_green() {
        let color = Color::from_hex("#00FF00").unwrap();
        assert_eq!(color.r, 0.0);
        assert_eq!(color.g, 1.0);
        assert_eq!(color.b, 0.0);
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn from_hex_blue() {
        let color = Color::from_hex("#0000FF").unwrap();
        assert_eq!(color.r, 0.0);
        assert_eq!(color.g, 0.0);
        assert_eq!(color.b, 1.0);
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn from_hex_black() {
        let color = Color::from_hex("#000000").unwrap();
        assert_eq!(color.r, 0.0);
        assert_eq!(color.g, 0.0);
        assert_eq!(color.b, 0.0);
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn from_hex_white() {
        let color = Color::from_hex("#FFFFFF").unwrap();
        assert_eq!(color.r, 1.0);
        assert_eq!(color.g, 1.0);
        assert_eq!(color.b, 1.0);
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn from_hex_lowercase() {
        let color = Color::from_hex("#ff0000").unwrap();
        assert_eq!(color.r, 1.0);
        assert_eq!(color.g, 0.0);
        assert_eq!(color.b, 0.0);
    }

    #[test]
    fn from_hex_8digit_with_full_alpha() {
        let color = Color::from_hex("#00FF00FF").unwrap();
        assert_eq!(color.r, 0.0);
        assert_eq!(color.g, 1.0);
        assert_eq!(color.b, 0.0);
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn from_hex_8digit_with_zero_alpha() {
        let color = Color::from_hex("#FF000000").unwrap();
        assert_eq!(color.a, 0.0);
    }

    #[test]
    fn from_hex_3digit_white() {
        let color = Color::from_hex("#FFF").unwrap();
        assert_eq!(color.r, 1.0);
        assert_eq!(color.g, 1.0);
        assert_eq!(color.b, 1.0);
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn from_hex_3digit_black() {
        let color = Color::from_hex("#000").unwrap();
        assert_eq!(color.r, 0.0);
        assert_eq!(color.g, 0.0);
        assert_eq!(color.b, 0.0);
    }

    #[test]
    fn from_hex_4digit_with_alpha() {
        let color = Color::from_hex("#FFFF").unwrap();
        assert_eq!(color.r, 1.0);
        assert_eq!(color.g, 1.0);
        assert_eq!(color.b, 1.0);
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn from_hex_no_hash_prefix() {
        let color = Color::from_hex("FF0000").unwrap();
        assert_eq!(color.r, 1.0);
        assert_eq!(color.g, 0.0);
        assert_eq!(color.b, 0.0);
    }

    #[test]
    fn from_hex_invalid_characters_returns_none() {
        assert!(Color::from_hex("#GGGGGG").is_none());
    }

    #[test]
    fn from_hex_wrong_length_returns_none() {
        assert!(Color::from_hex("#12345").is_none());
    }

    #[test]
    fn from_hex_wrong_length_5_no_hash_returns_none() {
        assert!(Color::from_hex("ff000").is_none());
    }

    #[test]
    fn from_hex_wrong_length_9_returns_none() {
        assert!(Color::from_hex("ff0000ff0").is_none());
    }

    #[test]
    fn from_hex_whitespace_prefix_returns_none() {
        assert!(Color::from_hex(" ff0000").is_none());
    }

    #[test]
    fn from_hex_invalid_hex_chars_returns_none() {
        assert!(Color::from_hex("zz0000").is_none());
    }

    #[test]
    fn from_hex_uppercase_parses_correctly() {
        let color = Color::from_hex("FF0000").unwrap();
        assert_eq!(color.r, 1.0);
        assert_eq!(color.g, 0.0);
        assert_eq!(color.b, 0.0);
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn from_hex_empty_returns_none() {
        assert!(Color::from_hex("").is_none());
    }

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
    fn from_rgba_u8_normalizes_alpha() {
        let color = Color::from_rgba_u8(0, 0, 0, 128);
        assert!((color.a - 128.0 / 255.0).abs() < 1e-6);
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
    fn from_hsv_black() {
        let color = Color::from_hsv(0.0, 0.0, 0.0);
        assert_eq!(color.r, 0.0);
        assert_eq!(color.g, 0.0);
        assert_eq!(color.b, 0.0);
    }

    #[test]
    fn from_hsv_white() {
        let color = Color::from_hsv(0.0, 0.0, 1.0);
        assert!((color.r - 1.0).abs() < 1e-5);
        assert!((color.g - 1.0).abs() < 1e-5);
        assert!((color.b - 1.0).abs() < 1e-5);
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

    #[test]
    fn from_hsva_hue_wraps_above_360() {
        let color1 = Color::from_hsva(400.0, 1.0, 1.0, 1.0);
        let color2 = Color::from_hsva(40.0, 1.0, 1.0, 1.0);
        assert!((color1.r - color2.r).abs() < 1e-5);
        assert!((color1.g - color2.g).abs() < 1e-5);
        assert!((color1.b - color2.b).abs() < 1e-5);
    }

    #[test]
    fn from_hsva_hue_wraps_negative() {
        let color1 = Color::from_hsva(-30.0, 1.0, 1.0, 1.0);
        let color2 = Color::from_hsva(330.0, 1.0, 1.0, 1.0);
        assert!((color1.r - color2.r).abs() < 1e-5);
        assert!((color1.g - color2.g).abs() < 1e-5);
        assert!((color1.b - color2.b).abs() < 1e-5);
    }

    #[test]
    fn from_hsva_clamps_saturation_above_one() {
        let color = Color::from_hsva(0.0, 2.0, 1.0, 1.0);
        let expected = Color::from_hsva(0.0, 1.0, 1.0, 1.0);
        assert!((color.r - expected.r).abs() < 1e-5);
        assert!((color.g - expected.g).abs() < 1e-5);
        assert!((color.b - expected.b).abs() < 1e-5);
    }

    #[test]
    fn from_hsva_clamps_value_above_one() {
        let color = Color::from_hsva(0.0, 1.0, 1.5, 1.0);
        let expected = Color::from_hsva(0.0, 1.0, 1.0, 1.0);
        assert!((color.r - expected.r).abs() < 1e-5);
        assert!((color.g - expected.g).abs() < 1e-5);
        assert!((color.b - expected.b).abs() < 1e-5);
    }

    #[test]
    fn from_hsva_clamps_value_below_zero() {
        let color = Color::from_hsva(0.0, 1.0, -0.5, 1.0);
        assert!(color.r.abs() < 1e-5);
        assert!(color.g.abs() < 1e-5);
        assert!(color.b.abs() < 1e-5);
    }

    #[test]
    fn from_hsva_clamps_alpha_above_one() {
        let color = Color::from_hsva(0.0, 0.0, 1.0, 2.0);
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn from_hsva_clamps_alpha_below_zero() {
        let color = Color::from_hsva(0.0, 0.0, 1.0, -1.0);
        assert_eq!(color.a, 0.0);
    }
}
