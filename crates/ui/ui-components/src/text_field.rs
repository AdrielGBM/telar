use layout_core::{LayoutError, LayoutStyle};
use reactive_core::{RwSignal, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, Stroke, TextStyle};
use theme_core::use_theme_tokens;
use ui_core::{Container, Input, LayoutItem, StyledContainer, Text, box_item};

/// Fallback text colour ("ink") when `color` is unset, matching the button catalogue's ghost-variant text.
const DEFAULT_INK: Color = Color::rgba(0.12, 0.12, 0.16, 1.0);
const BOX_RADIUS: f32 = 8.0;
const PAD_X: f32 = 12.0;
const PAD_Y: f32 = 10.0;
const DEFAULT_WIDTH: f32 = 300.0;
const FONT_SIZE: f32 = 15.0;
const LABEL_SIZE: f32 = 12.0;
const LABEL_GAP: f32 = 6.0;

/// A labelled, bordered text input: the `Input` primitive (kernel, unstyled) wrapped in a padded/rounded
/// box (see the raw `box fill:surface_alt stroke:border radius:8 pad_x:12 pad_y:10 > input` pattern in
/// `apps/sandbox/src/features/reactivity.rsx`), with an optional caption label stacked above it. High-level
/// sugar; lives in `ui-components`, not the kernel, so an app can drop it or ship its own.
pub struct TextFieldProps {
    /// `None` (the default) makes the field uncontrolled: it owns an internal `signal(String::new())`.
    /// `Some` binds it to a caller-owned signal (a controlled field), like `button`'s reactive props.
    pub value: Option<RwSignal<String>>,
    /// Muted text shown in the box in place of the `Input` while `value` is empty — see the module's
    /// `text_field` doc for the swap's focus limitation.
    pub placeholder: &'static str,
    /// A small caption stacked above the box; omitted entirely (no extra row) when empty.
    pub label: &'static str,
    /// Box width in logical px. `0.0` (the default) means "unset" and resolves to `DEFAULT_WIDTH`.
    pub width: f32,
    /// The entered text's colour. `Color::TRANSPARENT` (the default) means "unset" -> `DEFAULT_INK`. A
    /// closure (re-read every frame) so a theme token or `$signal` colour re-colours live, like `button`'s
    /// `fill`/`outline`.
    pub color: Box<dyn Fn() -> Color>,
    /// Runs when Enter is pressed while the field is focused.
    pub on_submit: Option<Box<dyn Fn()>>,
}

impl Default for TextFieldProps {
    fn default() -> Self {
        Self {
            value: None,
            placeholder: "",
            label: "",
            width: 0.0,
            color: Box::new(|| Color::TRANSPARENT),
            on_submit: None,
        }
    }
}

/// Builds a `text_field`: a bordered/padded box around `ui_core::Input`, swapping in a muted placeholder
/// muted hint via the `Input`'s own `placeholder` while the value is empty — the field stays a live,
/// always-mounted `Input`, so it is tappable/typable from a cold start (no swapped-in `Text` that would
/// refuse focus).
pub fn text_field(props: TextFieldProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let TextFieldProps {
        value,
        placeholder,
        label,
        width,
        color,
        on_submit,
    } = props;
    let value = value.unwrap_or_else(|| signal(String::new()));
    let width = if width > 0.0 { width } else { DEFAULT_WIDTH };

    // Always a live `Input` (with a muted placeholder shown while empty) so the field is tappable/typable
    // from a cold start — a swapped-in placeholder `Text` takes no focus, so an empty field couldn't be
    // clicked to begin typing.
    let mut input = Input::new(
        value.clone(),
        LayoutStyle::new().height(FONT_SIZE * 1.4),
        move || {
            let c = color();
            TextStyle::new(
                FONT_SIZE,
                if c == Color::TRANSPARENT {
                    DEFAULT_INK
                } else {
                    c
                },
            )
        },
    )?
    .placeholder(placeholder);
    if let Some(cb) = on_submit {
        input = input.on_submit(move || cb());
    }
    let field = box_item(input);

    let box_ = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .width(width)
            .padding_horizontal(PAD_X)
            .padding_vertical(PAD_Y),
        |_r| {
            RectStyle::default()
                .with_fill(Color::rgba(0.5, 0.5, 0.55, 0.10))
                .with_stroke(Stroke::new(Color::rgba(0.5, 0.5, 0.55, 0.35), 1.0))
                .with_radius(BorderRadius::all(BOX_RADIUS))
        },
        vec![box_item(field)],
    )?;

    if label.is_empty() {
        return Ok(box_item(box_));
    }
    let caption = Text::new(
        move || label.to_string(),
        LayoutStyle::new().height(LABEL_SIZE * 1.4),
        || TextStyle::new(LABEL_SIZE, muted_color()),
    )?;
    let col = Container::new(
        LayoutStyle::new().flex_column().gap(LABEL_GAP).width(width),
        vec![box_item(caption), box_item(box_)],
    )?;
    Ok(box_item(col))
}

/// The shared muted tone for the label caption and the placeholder text, re-read every frame so it tracks
/// the active theme (falls back to `ThemeTokens::muted`'s own default when no theme is installed).
fn muted_color() -> Color {
    use_theme_tokens()
        .map(|t| t.muted())
        .unwrap_or(Color::rgba(0.5, 0.5, 0.6, 0.6))
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;
    use ui_core::reset_layout_runtime;

    use layout_core::AvailableSpace;
    use platform_core::{Event, Key, ModifiersState, NamedKey, PointerButton, PointerSource};
    use renderer_core::DrawCommand;

    use super::*;
    use ui_core::{ComponentList, compute_layout, new_container, track_layout};

    fn press(x: f64, y: f64) -> Event {
        Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        }
    }

    // Lays a `text_field` out inside a fixed-size root and returns it plus its absolute box rect (the
    // outermost node's rect, valid only when `label` is empty — a labelled field's outer node is the
    // wrapping column instead, offset above the box by the caption row).
    fn laid_out_field(props: TextFieldProps) -> (Box<dyn LayoutItem>, geometry_core::Rect) {
        reset_layout_runtime();
        let field = text_field(props).unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(400.0).height(200.0),
            &[field.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();
        let rect = track_layout(field.layout_node()).unwrap().get();
        (field, rect)
    }

    fn find_text(cmds: &[DrawCommand], needle: &str) -> bool {
        cmds.iter()
            .any(|c| matches!(c, DrawCommand::Text { text, .. } if text.as_ref() == needle))
    }

    // A controlled, non-empty value renders through the real Input (not the placeholder).
    #[test]
    fn controlled_value_renders_as_input_text() {
        let value = signal("hi".to_string());
        let (field, _) = laid_out_field(TextFieldProps {
            value: Some(value.clone()),
            ..Default::default()
        });
        let tree = ComponentList::new(field);
        assert!(find_text(&tree.commands(), "hi"));
    }

    // An empty field swaps in the muted placeholder Text instead of the Input — the documented limitation
    // (see `text_field`'s doc comment): it is inert here, not a focusable Input.
    #[test]
    fn empty_value_renders_placeholder() {
        let (field, _) = laid_out_field(TextFieldProps {
            placeholder: "Search…",
            ..Default::default()
        });
        let tree = ComponentList::new(field);
        assert!(find_text(&tree.commands(), "Search…"));
    }

    // Tapping inside a non-empty (so Input-backed) field focuses it, and a subsequent keypress edits the
    // bound signal — dispatched through the whole component tree via `ComponentList`, like a real app.
    #[test]
    fn typing_into_a_focused_field_edits_the_bound_signal() {
        let value = signal("a".to_string());
        let (field, rect) = laid_out_field(TextFieldProps {
            value: Some(value.clone()),
            ..Default::default()
        });
        let mut tree = ComponentList::new(field);
        let _ = tree.commands(); // build the initial segments before dispatching events

        let press_x = (rect.x + PAD_X + 2.0) as f64;
        let press_y = (rect.y + PAD_Y + 2.0) as f64;
        tree.on_event(&press(press_x, press_y));
        tree.on_event(&Event::KeyPressed {
            key: Key::Char('z'),
            modifiers: ModifiersState::default(),
        });

        assert_eq!(value.get(), "az");
    }

    // Enter, while focused, fires the field's on_submit.
    #[test]
    fn enter_fires_on_submit_while_focused() {
        use std::cell::Cell;

        let value = signal("a".to_string());
        let fired = Rc::new(Cell::new(false));
        let sink = fired.clone();
        let (field, rect) = laid_out_field(TextFieldProps {
            value: Some(value.clone()),
            on_submit: Some(Box::new(move || sink.set(true))),
            ..Default::default()
        });
        let mut tree = ComponentList::new(field);
        let _ = tree.commands();

        let press_x = (rect.x + PAD_X + 2.0) as f64;
        let press_y = (rect.y + PAD_Y + 2.0) as f64;
        tree.on_event(&press(press_x, press_y));
        tree.on_event(&Event::KeyPressed {
            key: Key::Named(NamedKey::Enter),
            modifiers: ModifiersState::default(),
        });

        assert!(fired.get(), "Enter while focused should fire on_submit");
    }

    // A `label` stacks a caption above the box instead of the box being the field's only node.
    #[test]
    fn label_adds_a_caption_row_above_the_box() {
        let (field, _) = laid_out_field(TextFieldProps {
            label: "Name",
            placeholder: "Type your name",
            ..Default::default()
        });
        let tree = ComponentList::new(field);
        assert!(find_text(&tree.commands(), "Name"));
        assert!(find_text(&tree.commands(), "Type your name"));
    }
}
