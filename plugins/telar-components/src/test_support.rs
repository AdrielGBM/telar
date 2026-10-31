//! Setup every catalogue test starts from.

/// A layout runtime with nothing in it, a measurer for the text these widgets contain set in the test face alone, and shortcuts shown as a PC shows them.
///
/// The measurer is the half that is easy to forget: laying a control out asks how wide its label is, and outside a runner nobody has installed anything to answer. The face and the keyboard are pinned so a label wraps and a shortcut reads the same whichever machine runs the tests.
pub(crate) fn fresh_layout_runtime() {
    telar::testing::install_test_face();
    telar::reset_layout_runtime();
    telar::set_keyboard_convention(telar::KeyboardConvention::Pc);
}

/// Where each control in `role` is drawn now, in the order a reader meets them: after every transform and scroll between it and the surface.
pub(crate) fn drawn(role: telar::focus::Role) -> Vec<telar::Rect> {
    telar::focus::exposed()
        .into_iter()
        .filter(|exposed| exposed.role == role)
        .filter_map(|exposed| telar::visible_rect(exposed.node))
        .collect()
}

/// A theme whose control and item corners are `role`, supplying `tokens` as a component's own when there are any: what every `XTokens` test is written against.
#[derive(Clone)]
pub(crate) struct TokensTheme<K> {
    pub(crate) role: f32,
    pub(crate) tokens: Option<K>,
}

impl<K: Clone + Default + 'static> telar::ThemeTokens for TokensTheme<K> {
    fn radius_control(&self) -> f32 {
        self.role
    }
    fn radius_item(&self) -> f32 {
        self.role
    }
    fn register_extensions(&self, extensions: &mut telar::ThemeExtensions) {
        if let Some(tokens) = &self.tokens {
            extensions.insert(tokens.clone());
        }
    }
}

/// Holds a component's `XTokens::radius` to the rule every token follows: left `None` it is the corner `role_corner` works out from the theme's role, read when the component paints, and a radius the theme names wins until the theme stops naming it.
pub(crate) fn assert_corner_follows_the_role_until_the_tokens_name_one<
    K: Clone + Default + 'static,
>(
    mount: impl FnOnce() -> telar::ComponentList,
    corner: impl Fn(&telar::ComponentList) -> telar::BorderRadius,
    role_corner: impl Fn(&telar::ComponentList, f32) -> telar::BorderRadius,
    with_radius: impl Fn(f32) -> K,
) {
    telar::set_theme(TokensTheme::<K> {
        role: 7.0,
        tokens: None,
    });
    let tree = mount();
    assert_eq!(
        corner(&tree),
        role_corner(&tree, 7.0),
        "an unset radius follows the role"
    );

    telar::set_theme(TokensTheme {
        role: 7.0,
        tokens: Some(with_radius(2.0)),
    });
    assert_eq!(
        corner(&tree),
        telar::BorderRadius::all(2.0),
        "the theme's own radius wins, and the paint follows the switch"
    );

    telar::set_theme(TokensTheme {
        role: 12.0,
        tokens: Some(K::default()),
    });
    assert_eq!(
        corner(&tree),
        role_corner(&tree, 12.0),
        "tokens left unset keep following the role"
    );
}

/// The paint of the first box `tree` draws: the principal surface of a component whose own box is it.
pub(crate) fn first_painted(tree: &telar::ComponentList) -> telar::RectStyle {
    tree.commands()
        .iter()
        .find_map(|command| match command {
            telar::DrawCommand::Rect { style, .. } => Some(**style),
            _ => None,
        })
        .expect("the component paints a box")
}

/// The paint of the last box `tree` draws exactly at `at`: a principal surface found where it is laid out, such as a control's own [`drawn`] rect.
pub(crate) fn painted_at(tree: &telar::ComponentList, at: telar::Rect) -> telar::RectStyle {
    let same = |rect: telar::Rect| {
        [
            rect.x - at.x,
            rect.y - at.y,
            rect.width - at.width,
            rect.height - at.height,
        ]
        .iter()
        .all(|d| d.abs() < 0.5)
    };
    let mut painted = None;
    telar::for_each_with_matrix(&tree.commands(), |command, matrix| {
        if let telar::DrawCommand::Rect { rect, style, .. } = command
            && same(telar::transform_clip_rect(matrix, *rect))
        {
            painted = Some(**style);
        }
    });
    painted.expect("a box is drawn there")
}

/// The corner role a floating panel is rounded by.
#[cfg(any(feature = "overlays", feature = "workbench"))]
#[derive(Clone, Copy, Debug)]
pub(crate) enum PanelRole {
    Surface,
    Dialog,
}

/// A theme that rounds `role` by `radius` and the other panel role by a value nothing should land on, supplying `tokens` as a component's own when there are any.
#[cfg(any(feature = "overlays", feature = "workbench"))]
#[derive(Clone)]
pub(crate) struct PanelTheme<K> {
    pub(crate) role: PanelRole,
    pub(crate) radius: f32,
    pub(crate) tokens: Option<K>,
}

#[cfg(any(feature = "overlays", feature = "workbench"))]
impl<K> PanelTheme<K> {
    const ELSEWHERE: f32 = 31.0;

    fn corner(&self, role: PanelRole) -> f32 {
        match (self.role, role) {
            (PanelRole::Surface, PanelRole::Surface) | (PanelRole::Dialog, PanelRole::Dialog) => {
                self.radius
            }
            _ => Self::ELSEWHERE,
        }
    }
}

#[cfg(any(feature = "overlays", feature = "workbench"))]
impl<K: Clone + Default + 'static> telar::ThemeTokens for PanelTheme<K> {
    fn radius_surface(&self) -> f32 {
        self.corner(PanelRole::Surface)
    }
    fn radius_dialog(&self) -> f32 {
        self.corner(PanelRole::Dialog)
    }
    fn register_extensions(&self, extensions: &mut telar::ThemeExtensions) {
        if let Some(tokens) = &self.tokens {
            extensions.insert(tokens.clone());
        }
    }
}

/// [`assert_corner_follows_the_role_until_the_tokens_name_one`] for a panel rounded by `role`: left `None` the radius is the role's, read when the panel paints, a radius the theme names wins, and `shape` says which corners a radius reaches.
#[cfg(any(feature = "overlays", feature = "workbench"))]
pub(crate) fn assert_panel_corner_follows_its_role<K: Clone + Default + 'static>(
    role: PanelRole,
    mount: impl FnOnce() -> telar::testing::Scoped<telar::ComponentList>,
    corner: impl Fn(&telar::ComponentList) -> telar::BorderRadius,
    shape: impl Fn(f32) -> telar::BorderRadius,
    with_radius: impl Fn(f32) -> K,
) {
    telar::set_theme(PanelTheme::<K> {
        role,
        radius: 7.0,
        tokens: None,
    });
    let tree = mount();
    assert_eq!(
        corner(&tree),
        shape(7.0),
        "an unset radius follows the {role:?} role"
    );

    telar::set_theme(PanelTheme {
        role,
        radius: 7.0,
        tokens: Some(with_radius(2.0)),
    });
    assert_eq!(
        corner(&tree),
        shape(2.0),
        "the theme's own radius wins, and the paint follows the switch"
    );

    telar::set_theme(PanelTheme {
        role,
        radius: 12.0,
        tokens: Some(K::default()),
    });
    assert_eq!(
        corner(&tree),
        shape(12.0),
        "tokens left unset keep following the role"
    );
}

/// How far the run of `label` the tree draws stands from either edge of `control`: measured to the glyphs' advance, not to the box the run is set in, which is where a box wider than its text shows.
pub(crate) fn run_insets(
    tree: &telar::ComponentList,
    label: &str,
    control: telar::Rect,
) -> (f32, f32) {
    let mut run = None;
    telar::for_each_with_matrix(&tree.commands(), |command, matrix| {
        if let telar::DrawCommand::Text {
            text, rect, style, ..
        } = command
            && text.as_ref() == label
            && run.is_none()
        {
            let (width, _) = telar::measure_text(text, None, rect.width, style);
            run = Some((matrix[4] + rect.x, width));
        }
    });
    let (x, width) = run.expect("the label is drawn");
    (x - control.x, control.x + control.width - (x + width))
}

/// A body of one 20 px line reading `label`: what an accordion, a dialog or a drawer is opened on when only its own box is under test.
#[cfg(any(feature = "advanced", feature = "overlays"))]
pub(crate) fn text_body(label: &'static str) -> telar::Children {
    telar::Children::one(move || {
        let body = telar::Text::declaring(
            move || label.to_string(),
            telar::LayoutStyle::new().height(20.0),
            |t| t,
        )?;
        Ok(telar::box_item(body))
    })
}

/// A body of `lines` 20 px lines, `Line 0` onwards: enough of them to be taller than the window a dialog or a drawer is opened in.
#[cfg(feature = "overlays")]
pub(crate) fn tall_body(lines: usize) -> telar::Children {
    telar::Children::new(move || {
        (0..lines)
            .map(|line| {
                let text = telar::Text::declaring(
                    move || format!("Line {line}"),
                    telar::LayoutStyle::new().height(20.0),
                    |t| t,
                )?;
                Ok(telar::box_item(text) as Box<dyn telar::LayoutItem>)
            })
            .collect()
    })
}

/// Whether any text `cmds` draw reads exactly `needle`.
pub(crate) fn find_text(cmds: &[telar::DrawCommand], needle: &str) -> bool {
    cmds.iter()
        .any(|c| matches!(c, telar::DrawCommand::Text { text, .. } if text.as_ref() == needle))
}
