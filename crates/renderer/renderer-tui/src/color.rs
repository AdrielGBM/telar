//! Colour on a terminal: 24-bit values composited in sRGB, then quantised to whatever the terminal admits.

use renderer_core::Color;

/// An opaque cell colour. The buffer never stores alpha: every paint composites against what is already
/// there, exactly as the raster backends do, so what reaches the terminal is a finished colour.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0 };
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
    };

    pub fn from_color(c: Color) -> Self {
        let [r, g, b, _] = c.to_rgba8();
        Self { r, g, b }
    }

    /// `src` over `self`, in sRGB with straight alpha — the same space and the same formula the raster
    /// backends blend in, so a colour that looks a certain way on the desktop looks that way here.
    pub fn under(self, src: Color) -> Self {
        let a = src.a.clamp(0.0, 1.0);
        if a >= 1.0 {
            return Self::from_color(src);
        }
        if a <= 0.0 {
            return self;
        }
        let mix = |s: f32, d: u8| {
            let s = (s.clamp(0.0, 1.0) * 255.0).round();
            (s * a + d as f32 * (1.0 - a)).round().clamp(0.0, 255.0) as u8
        };
        Self {
            r: mix(src.r, self.r),
            g: mix(src.g, self.g),
            b: mix(src.b, self.b),
        }
    }

    fn luma(self) -> f32 {
        0.2126 * self.r as f32 + 0.7152 * self.g as f32 + 0.0722 * self.b as f32
    }
}

/// How many colours the terminal can be told about.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ColorDepth {
    /// 24-bit `SGR 38;2;r;g;b`. What every terminal worth targeting has had for a decade.
    #[default]
    TrueColor,
    /// The xterm 256-colour palette: a 6×6×6 cube plus a 24-step grey ramp.
    Ansi256,
    /// The 16 base colours. The floor, for a TTY or a terminal that admits nothing else.
    Ansi16,
}

impl ColorDepth {
    /// What the environment says the terminal can do. Reads `COLORTERM` first because it is the only
    /// variable that answers the question directly; `TERM` is a database key, not a capability list, and
    /// terminals that support 24-bit colour routinely still report `xterm-256color`.
    pub fn detect() -> Self {
        let var = |k: &str| std::env::var(k).unwrap_or_default().to_ascii_lowercase();
        let colorterm = var("COLORTERM");
        if colorterm.contains("truecolor") || colorterm.contains("24bit") {
            return Self::TrueColor;
        }
        let term = var("TERM");
        if term.contains("256color") || term.contains("direct") {
            return Self::Ansi256;
        }
        if term.is_empty() || term == "dumb" {
            return Self::Ansi16;
        }
        Self::Ansi16
    }
}

/// The 6×6×6 cube's axis values, which are not evenly spaced: the first step is 95 and the rest are 40 apart.
const CUBE_STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];

fn nearest_cube_axis(v: u8) -> usize {
    let mut best = 0;
    let mut best_d = u16::MAX;
    for (i, step) in CUBE_STEPS.iter().enumerate() {
        let d = v.abs_diff(*step) as u16;
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    best
}

fn dist2(a: Rgb, b: Rgb) -> u32 {
    let d = |x: u8, y: u8| {
        let d = x.abs_diff(y) as u32;
        d * d
    };
    d(a.r, b.r) + d(a.g, b.g) + d(a.b, b.b)
}

/// The nearest xterm-256 index, choosing between the colour cube and the grey ramp by actual distance
/// rather than by guessing which one a colour "is" — a near-grey blue lands in the cube, a true grey in
/// the ramp, and neither needs a special case.
pub fn to_ansi256(c: Rgb) -> u8 {
    let cube_idx = (
        nearest_cube_axis(c.r),
        nearest_cube_axis(c.g),
        nearest_cube_axis(c.b),
    );
    let cube = Rgb {
        r: CUBE_STEPS[cube_idx.0],
        g: CUBE_STEPS[cube_idx.1],
        b: CUBE_STEPS[cube_idx.2],
    };
    let cube_code = 16 + 36 * cube_idx.0 as u8 + 6 * cube_idx.1 as u8 + cube_idx.2 as u8;

    let grey_level = ((c.luma() - 8.0) / 10.0).round().clamp(0.0, 23.0) as u8;
    let grey_value = 8 + 10 * grey_level;
    let grey = Rgb {
        r: grey_value,
        g: grey_value,
        b: grey_value,
    };

    if dist2(c, grey) < dist2(c, cube) {
        232 + grey_level
    } else {
        cube_code
    }
}

/// The 16 base colours as most terminals actually render them (the xterm defaults). Used only to pick the
/// nearest index — the terminal draws with its own palette, which is the point.
const ANSI16: [Rgb; 16] = [
    Rgb { r: 0, g: 0, b: 0 },
    Rgb { r: 205, g: 0, b: 0 },
    Rgb { r: 0, g: 205, b: 0 },
    Rgb {
        r: 205,
        g: 205,
        b: 0,
    },
    Rgb { r: 0, g: 0, b: 238 },
    Rgb {
        r: 205,
        g: 0,
        b: 205,
    },
    Rgb {
        r: 0,
        g: 205,
        b: 205,
    },
    Rgb {
        r: 229,
        g: 229,
        b: 229,
    },
    Rgb {
        r: 127,
        g: 127,
        b: 127,
    },
    Rgb { r: 255, g: 0, b: 0 },
    Rgb { r: 0, g: 255, b: 0 },
    Rgb {
        r: 255,
        g: 255,
        b: 0,
    },
    Rgb {
        r: 92,
        g: 92,
        b: 255,
    },
    Rgb {
        r: 255,
        g: 0,
        b: 255,
    },
    Rgb {
        r: 0,
        g: 255,
        b: 255,
    },
    Rgb {
        r: 255,
        g: 255,
        b: 255,
    },
];

pub fn to_ansi16(c: Rgb) -> u8 {
    let mut best = 0u8;
    let mut best_d = u32::MAX;
    for (i, candidate) in ANSI16.iter().enumerate() {
        let d = dist2(c, *candidate);
        if d < best_d {
            best_d = d;
            best = i as u8;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_source_replaces() {
        let dst = Rgb::BLACK;
        assert_eq!(dst.under(Color::WHITE), Rgb::WHITE);
    }

    #[test]
    fn half_alpha_meets_in_the_middle() {
        let out = Rgb::BLACK.under(Color::rgba(1.0, 1.0, 1.0, 0.5));
        assert!(out.r.abs_diff(128) <= 1, "got {}", out.r);
    }

    #[test]
    fn transparent_source_leaves_destination() {
        let dst = Rgb {
            r: 10,
            g: 20,
            b: 30,
        };
        assert_eq!(dst.under(Color::rgba(1.0, 0.0, 0.0, 0.0)), dst);
    }

    #[test]
    fn greys_land_on_the_grey_ramp() {
        assert_eq!(
            to_ansi256(Rgb {
                r: 128,
                g: 128,
                b: 128
            }),
            244
        );
    }

    #[test]
    fn saturated_colors_land_in_the_cube() {
        assert_eq!(to_ansi256(Rgb { r: 255, g: 0, b: 0 }), 196);
    }

    #[test]
    fn pure_colors_reach_their_ansi16_slot() {
        assert_eq!(to_ansi16(Rgb::BLACK), 0);
        assert_eq!(to_ansi16(Rgb::WHITE), 15);
        assert_eq!(to_ansi16(Rgb { r: 255, g: 0, b: 0 }), 9);
    }
}
