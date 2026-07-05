use rsx::{Color, Theme, WidgetTheme, use_theme};

/// Semantic color tokens for the documentation app. Every `.rsx` color reference
/// (`fill:primary`, `color:muted`, …) resolves to one of these fields via `use_theme`,
/// so swapping the whole struct at runtime re-colors the entire UI reactively.
#[derive(Clone)]
pub struct SandboxTheme {
    pub name: &'static str,
    // Structure
    pub background: Color,
    pub surface: Color,
    pub surface_alt: Color,
    pub border: Color,
    // Text
    pub ink: Color,
    pub muted: Color,
    // Brand + accents
    pub primary: Color,
    pub on_primary: Color,
    pub success: Color,
    pub danger: Color,
    pub warning: Color,
    pub purple: Color,
    pub cyan: Color,
    // Code blocks
    pub code_bg: Color,
    pub code_fg: Color,
}

impl Theme for SandboxTheme {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl WidgetTheme for SandboxTheme {
    fn widget_primary(&self) -> Color {
        self.primary
    }
    fn widget_on_primary(&self) -> Color {
        self.on_primary
    }
    fn widget_muted(&self) -> Color {
        self.muted
    }
    fn widget_scrollbar(&self) -> Color {
        Color::rgba(self.muted.r, self.muted.g, self.muted.b, 0.55)
    }
}

/// Shorthand: parse a `#rrggbb` literal into a `Color`. Only called with hard-coded constants below.
fn c(hex: &str) -> Color {
    Color::from_hex(hex).expect("valid hex color")
}

impl SandboxTheme {
    pub fn modern() -> Self {
        Self {
            name: "Modern",
            background: c("#f6f7fb"),
            surface: c("#ffffff"),
            surface_alt: c("#eef1f8"),
            border: c("#e2e5ee"),
            ink: c("#14161f"),
            muted: c("#6b7186"),
            primary: c("#3b5bdb"),
            on_primary: c("#ffffff"),
            success: c("#2f9e6f"),
            danger: c("#e0484a"),
            warning: c("#f2a624"),
            purple: c("#7a45f0"),
            cyan: c("#22aecb"),
            code_bg: c("#1b1e2b"),
            code_fg: c("#d7dcec"),
        }
    }

    pub fn pastel() -> Self {
        Self {
            name: "Pastel",
            background: c("#f7f3fb"),
            surface: c("#ffffff"),
            surface_alt: c("#f1eaf8"),
            border: c("#ece2f4"),
            ink: c("#3a2f47"),
            muted: c("#8b7fa0"),
            primary: c("#8a7bf0"),
            on_primary: c("#ffffff"),
            success: c("#4fbf94"),
            danger: c("#e58a8c"),
            warning: c("#e8bf6b"),
            purple: c("#b18cf0"),
            cyan: c("#5bc7c0"),
            code_bg: c("#322a41"),
            code_fg: c("#e7def4"),
        }
    }

    pub fn midnight() -> Self {
        Self {
            name: "Midnight",
            background: c("#0e1017"),
            surface: c("#181b26"),
            surface_alt: c("#12141d"),
            border: c("#262a38"),
            ink: c("#e7eaf3"),
            muted: c("#8b93a7"),
            primary: c("#5b7cfa"),
            on_primary: c("#ffffff"),
            success: c("#34c98a"),
            danger: c("#f26d6f"),
            warning: c("#f2b53c"),
            purple: c("#9d78ff"),
            cyan: c("#38c6e6"),
            code_bg: c("#0a0c12"),
            code_fg: c("#cfd6ea"),
        }
    }
}

pub fn theme() -> SandboxTheme {
    use_theme::<SandboxTheme>()
}
