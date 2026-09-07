//! RGBA colour, and the conversions a theme needs to derive one colour from another.

/// A straight-alpha RGBA colour with components in `0.0..=1.0`.
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

    /// Parses `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`, with or without the `#`.
    ///
    /// Those four lengths are the `.rsx` hex table (`telar_parser::parse_hex`), stated there and repeated here because geometry cannot depend on a tools crate. The two must accept the same set: a colour that parses at runtime and not in the DSL is a colour the author cannot write.
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

    /// WCAG 2.x relative luminance in `[0, 1]` (0 = black, 1 = white): the linearized sRGB channels weighted by human luminance sensitivity. Alpha is ignored — compose over a background first if it matters.
    pub fn relative_luminance(self) -> f32 {
        let r = Self::srgb_to_linear(self.r);
        let g = Self::srgb_to_linear(self.g);
        let b = Self::srgb_to_linear(self.b);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    /// WCAG 2.x contrast ratio between two colors, from `1.0` (identical) to `21.0` (black vs white). Order-independent. Use it to pick a legible foreground: `>= 4.5` passes AA for body text, `>= 3.0` for large text.
    pub fn contrast_ratio(self, other: Color) -> f32 {
        let a = self.relative_luminance();
        let b = other.relative_luminance();
        let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// Picks the most legible foreground for this color (treated as a background): the candidate with the highest [`contrast_ratio`](Self::contrast_ratio) against `self`. Returns `self` if `candidates` is empty. This is the "auto-contrast" pick — e.g. a filled chip choosing text vs base over its accent.
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

    /// The colour as eight bits a channel, rounded to the nearest.
    ///
    /// Rounded, not truncated, and the difference is a whole channel step: `as u8` throws the fraction away, so `0.5` came out 127 where every other path in the codebase produces 128 — tiny-skia and the GPU both round when they write their own final byte. Only the backends that name a colour in *text* go through here, so the truncation showed up as a document drawing every theme one step darker than the same theme rasterised.
    #[inline]
    pub fn to_rgba8(self) -> [u8; 4] {
        let byte = |channel: f32| (channel * 255.0).round().clamp(0.0, 255.0) as u8;
        [byte(self.r), byte(self.g), byte(self.b), byte(self.a)]
    }

    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);
    pub const RED: Self = Self::rgb(1.0, 0.0, 0.0);
    pub const GREEN: Self = Self::rgb(0.0, 1.0, 0.0);
    pub const BLUE: Self = Self::rgb(0.0, 0.0, 1.0);
    pub const TRANSPARENT: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);
}

#[cfg(test)]
#[path = "color_test.rs"]
mod tests;
