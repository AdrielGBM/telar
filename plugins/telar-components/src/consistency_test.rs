//! The guardrails that hold the catalogue to one set of proportions at every control size: every single-line control is [`metrics::control_height`] tall and rounds by `radius_control`; every selection control stands a control height tall round a box scaled to the size; every list, menu, select, tree and context menu stands its rows at [`metrics::row_height`]; and every floating panel rounds by its role, `radius_surface` for a popup, a tooltip, a toast, a context menu or a code block and `radius_dialog` for a modal, a drawer or the command palette.

use telar::{
    AvailableSpace, BorderRadius, Children, ComponentList, ControlSize, DrawCommand, LayoutError,
    LayoutItem, LayoutStyle, Rect, ThemeTokens, compute_layout, focus::Role, provide_control_size,
    relayout_if_dirty, set_theme, track_layout,
};

use crate::metrics;
use crate::test_support::{drawn, fresh_layout_runtime};

/// One control in the row: how to build it, and where its principal surface is drawn.
struct Probe {
    name: &'static str,
    build: fn() -> Result<Box<dyn LayoutItem>, LayoutError>,
    surface: Surface,
}

/// Where to find a probe's principal surface among what the row draws.
#[derive(Clone, Copy)]
enum Surface {
    /// The control's own box.
    Root,
    /// The first focusable of this role inside the control: a tab of a tab list, the − of a stepper.
    First(Role),
}

/// A theme whose control radius no primitive step happens to equal, so a component rounding by `radius()`, `radius_md()` or a factor of either shows up as a mismatch.
#[derive(Clone)]
struct Distinct;

impl ThemeTokens for Distinct {
    fn radius(&self) -> f32 {
        4.0
    }
    fn radius_control(&self) -> f32 {
        7.0
    }
    fn radius_surface(&self) -> f32 {
        9.0
    }
    fn radius_dialog(&self) -> f32 {
        13.0
    }
}

fn probes() -> Vec<Probe> {
    #[allow(unused_mut)]
    let mut probes = vec![
        Probe {
            name: "button",
            build: || {
                crate::button(
                    crate::ButtonProps::props().label("Save").build(),
                    Children::default(),
                )
            },
            surface: Surface::First(Role::Button),
        },
        Probe {
            name: "text_field",
            build: || {
                crate::text_field(
                    crate::TextFieldProps::props().placeholder("Name").build(),
                    Children::default(),
                )
            },
            surface: Surface::Root,
        },
        Probe {
            name: "select",
            build: || crate::select(crate::SelectProps::props().build(), Children::default()),
            surface: Surface::First(Role::ComboBox),
        },
        Probe {
            name: "tab",
            build: || {
                crate::tabs(
                    crate::TabsProps::props().items(vec!["One", "Two"]).build(),
                    Children::default(),
                )
            },
            surface: Surface::First(Role::Tab),
        },
        Probe {
            name: "scrub_field",
            build: || {
                crate::scrub_field(
                    crate::ScrubFieldProps::props().label("X").build(),
                    Children::default(),
                )
            },
            surface: Surface::Root,
        },
    ];
    #[cfg(feature = "workbench")]
    probes.push(Probe {
        name: "icon_button",
        build: || {
            crate::icon_button(
                crate::IconButtonProps::props()
                    .icon("lucide:search")
                    .label("Find")
                    .build(),
                Children::default(),
            )
        },
        surface: Surface::First(Role::Button),
    });
    #[cfg(feature = "overlays")]
    probes.push(Probe {
        name: "menu",
        build: || {
            crate::menu(
                crate::MenuProps::props().label("File").fit(true).build(),
                Children::default(),
            )
        },
        surface: Surface::First(Role::Button),
    });
    #[cfg(feature = "advanced")]
    probes.push(Probe {
        name: "stepper",
        build: || crate::stepper(crate::StepperProps::props().build(), Children::default()),
        surface: Surface::First(Role::Button),
    });
    probes
}

/// What one probe measured at one control size.
#[derive(Debug)]
struct Measured {
    height: f32,
    radius: Option<BorderRadius>,
}

fn measure(probe: &Probe, size: ControlSize) -> Measured {
    fresh_layout_runtime();
    set_theme(Distinct);
    let control = provide_control_size(size, probe.build).expect("the control builds");
    let rect = track_layout(control.layout_node()).expect("the control has a node");
    let row = telar::Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(telar::AlignItems::START),
        vec![Box::new(control)],
    )
    .expect("the row builds");
    compute_layout(
        row.layout_node(),
        AvailableSpace::Definite(800.0),
        AvailableSpace::Definite(600.0),
    )
    .expect("the row lays out");
    relayout_if_dirty();
    let tree = ComponentList::new(telar::box_item(row));
    let commands = tree.commands();
    let root = rect.get();
    let surface = match probe.surface {
        Surface::Root => Some(root),
        Surface::First(role) => drawn(role).into_iter().next(),
    };
    let radius = surface.and_then(|surface| {
        commands.iter().rev().find_map(|command| match command {
            DrawCommand::Rect { rect, style, .. } if same(*rect, surface) => Some(style.radius),
            _ => None,
        })
    });
    Measured {
        height: surface.map_or(root.height, |surface| surface.height),
        radius,
    }
}

fn same(a: Rect, b: Rect) -> bool {
    [a.x - b.x, a.y - b.y, a.width - b.width, a.height - b.height]
        .iter()
        .all(|d| d.abs() < 0.5)
}

fn expected_height(size: ControlSize) -> f32 {
    fresh_layout_runtime();
    set_theme(Distinct);
    let mut height = 0.0;
    let _scope = provide_control_size(size, || {
        height = metrics::control_height();
        telar::Container::new(LayoutStyle::new(), vec![]).map(telar::box_item)
    });
    height
}

const SIZES: [ControlSize; 4] = [
    ControlSize::Mini,
    ControlSize::Small,
    ControlSize::Regular,
    ControlSize::Large,
];

#[test]
fn every_single_line_control_is_one_height_and_one_corner_at_every_control_size() {
    let corner = Some(BorderRadius::all(Distinct.radius_control()));
    let mut failures = Vec::new();
    for probe in probes() {
        for size in SIZES {
            let height = expected_height(size);
            let measured = measure(&probe, size);
            if (measured.height - height).abs() >= 0.5 || measured.radius != corner {
                failures.push(format!(
                    "`{}` at {size:?}: {measured:?}, want height {height} and corner {corner:?}",
                    probe.name
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A selection control — a checkbox, a radio, a switch — and the box it draws inside its row.
struct Selection {
    name: &'static str,
    build: fn() -> Result<Box<dyn LayoutItem>, LayoutError>,
    role: Role,
    /// The drawn box's size at the control size in force.
    drawn: fn() -> (f32, f32),
    /// The drawn box's corner, laid out at `rect`.
    corner: fn(Rect) -> BorderRadius,
}

fn selections() -> Vec<Selection> {
    vec![
        Selection {
            name: "checkbox",
            build: || {
                crate::checkbox(
                    crate::CheckboxProps::props().label("Agree").build(),
                    Children::default(),
                )
            },
            role: Role::CheckBox,
            drawn: || (metrics::icon_size(), metrics::icon_size()),
            corner: |_| BorderRadius::all(Distinct.radius_item()),
        },
        Selection {
            name: "radio",
            build: || {
                crate::radio(
                    crate::RadioProps::props().label("Small").build(),
                    Children::default(),
                )
            },
            role: Role::Radio,
            drawn: || (metrics::icon_size(), metrics::icon_size()),
            corner: metrics::pill,
        },
        Selection {
            name: "toggle",
            build: || {
                crate::toggle(
                    crate::ToggleProps::props().label("Wi-Fi").build(),
                    Children::default(),
                )
            },
            role: Role::Switch,
            drawn: || {
                let (knob, inset) = (metrics::icon_size(), metrics::gap_xs());
                (knob * 2.0 + inset * 2.0, knob + inset * 2.0)
            },
            corner: metrics::pill,
        },
    ]
}

/// Whether a laid-out length is `want` once layout has snapped it to the pixel grid, which moves a fractional length or a centre by up to half a pixel.
fn snapped(laid: f32, want: f32) -> bool {
    (laid - want).abs() <= 0.5
}

/// What `selection` asks of the size in force, read under it.
fn expected_box(selection: &Selection, size: ControlSize) -> (f32, f32) {
    fresh_layout_runtime();
    set_theme(Distinct);
    let mut drawn = (0.0, 0.0);
    let _scope = provide_control_size(size, || {
        drawn = (selection.drawn)();
        telar::Container::new(LayoutStyle::new(), vec![]).map(telar::box_item)
    });
    drawn
}

/// A selection control's row stands a control height tall at every control size, like a button beside it, and the box drawn in it scales with the size, is centred in the row and takes its corner from the theme's roles.
#[test]
fn every_selection_control_stands_a_control_height_tall_with_a_box_scaled_to_the_size() {
    let mut failures = Vec::new();
    for selection in selections() {
        for size in SIZES {
            let height = expected_height(size);
            let (width, tall) = expected_box(&selection, size);
            fresh_layout_runtime();
            set_theme(Distinct);
            let control = provide_control_size(size, selection.build).expect("the control builds");
            let row = telar::Container::new(
                LayoutStyle::new()
                    .flex_row()
                    .align_items(telar::AlignItems::START),
                vec![Box::new(control)],
            )
            .expect("the row builds");
            compute_layout(
                row.layout_node(),
                AvailableSpace::Definite(800.0),
                AvailableSpace::Definite(600.0),
            )
            .expect("the row lays out");
            relayout_if_dirty();
            let tree = ComponentList::new(telar::box_item(row));
            let commands = tree.commands();
            let Some(target) = drawn(selection.role).into_iter().next() else {
                failures.push(format!(
                    "`{}` at {size:?} exposes no control",
                    selection.name
                ));
                continue;
            };
            if (target.height - height).abs() >= 0.5 {
                failures.push(format!(
                    "`{}` at {size:?} is {} tall, want {height}",
                    selection.name, target.height
                ));
            }
            let painted = commands.iter().find_map(|command| match command {
                DrawCommand::Rect { rect, style, .. }
                    if snapped(rect.width, width) && snapped(rect.height, tall) =>
                {
                    Some((*rect, style.radius))
                }
                _ => None,
            });
            match painted {
                None => failures.push(format!(
                    "`{}` at {size:?} draws no {width}×{tall} box",
                    selection.name
                )),
                Some((rect, radius)) => {
                    let centre = rect.y + rect.height / 2.0;
                    let row_centre = target.y + target.height / 2.0;
                    if !snapped(centre, row_centre) {
                        failures.push(format!(
                            "`{}` at {size:?} draws its box off the row's centre",
                            selection.name
                        ));
                    }
                    if radius != (selection.corner)(rect) {
                        failures.push(format!(
                            "`{}` at {size:?} rounds its box by {radius:?}",
                            selection.name
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Lays `item` out in an 800×600 window and draws it until it settles.
fn mounted(item: Box<dyn LayoutItem>) -> ComponentList {
    let window = telar::Container::new(
        LayoutStyle::new()
            .flex_column()
            .align_items(telar::AlignItems::START),
        vec![item],
    )
    .expect("the window builds");
    compute_layout(
        window.layout_node(),
        AvailableSpace::Definite(800.0),
        AvailableSpace::Definite(600.0),
    )
    .expect("the window lays out");
    let tree = ComponentList::new(telar::box_item(window));
    settle(&tree);
    tree
}

fn settle(tree: &ComponentList) {
    for _ in 0..3 {
        relayout_if_dirty();
        let _ = tree.commands();
    }
}

/// Opens the list the focused trigger drops, from the keyboard.
fn open_from_keyboard(tree: &mut ComponentList) {
    telar::focus::focus_next();
    telar::testing::route(tree, &telar::testing::named(telar::NamedKey::Enter));
}

fn nothing_to_open(_: &mut ComponentList) {}

fn two_items() -> Children {
    Children::new(|| {
        let mut slots = telar::Slots::new();
        for label in ["Cut", "Copy"] {
            slots.push(
                None,
                crate::item(
                    crate::ItemProps::props().label(label).build(),
                    Children::default(),
                )?,
            );
        }
        Ok(slots)
    })
}

/// A component that draws rows, how to show them, and where its first row is.
struct Rows {
    name: &'static str,
    build: fn() -> Result<Box<dyn LayoutItem>, LayoutError>,
    open: fn(&mut ComponentList),
    row: Surface,
}

fn row_probes() -> Vec<Rows> {
    #[allow(unused_mut)]
    let mut probes = vec![
        Rows {
            name: "list item",
            build: || {
                crate::item(
                    crate::ItemProps::props().label("Cut").build(),
                    Children::default(),
                )
            },
            open: nothing_to_open,
            row: Surface::Root,
        },
        Rows {
            name: "select",
            build: || crate::select(crate::SelectProps::props().build(), two_items()),
            open: open_from_keyboard,
            row: Surface::First(Role::Option),
        },
    ];
    #[cfg(feature = "overlays")]
    probes.extend([
        Rows {
            name: "menu",
            build: || crate::menu(crate::MenuProps::props().label("Edit").build(), two_items()),
            open: open_from_keyboard,
            row: Surface::First(Role::MenuItem),
        },
        Rows {
            name: "context menu",
            build: || {
                crate::context_menu(
                    crate::ContextMenuProps::props()
                        .at((40.0, 30.0))
                        .on_close(std::rc::Rc::new(|| {}))
                        .within(Rect::new(0.0, 0.0, 800.0, 600.0))
                        .build(),
                    two_items(),
                )
            },
            open: nothing_to_open,
            row: Surface::First(Role::MenuItem),
        },
    ]);
    #[cfg(feature = "workbench")]
    probes.push(Rows {
        name: "tree",
        build: || {
            crate::tree_view(
                crate::TreeViewProps::props()
                    .items(vec![
                        crate::TreeNode::new("src", "src"),
                        crate::TreeNode::new("lib", "lib.rs"),
                    ])
                    .label("Files")
                    .build(),
                Children::default(),
            )
        },
        open: nothing_to_open,
        row: Surface::First(Role::TreeItem),
    });
    probes
}

fn expected_row_height(size: ControlSize) -> f32 {
    fresh_layout_runtime();
    set_theme(Distinct);
    let mut height = 0.0;
    let _scope = provide_control_size(size, || {
        height = metrics::row_height();
        telar::Container::new(LayoutStyle::new(), vec![]).map(telar::box_item)
    });
    height
}

/// Every list, menu, select, tree and context menu stands its rows at the one row height of the control size in force, so a menu and the list beside it cannot disagree about how tall a row is.
#[test]
fn every_row_of_every_list_is_the_one_row_height_at_every_control_size() {
    let mut failures = Vec::new();
    for probe in row_probes() {
        for size in SIZES {
            let want = expected_row_height(size);
            fresh_layout_runtime();
            set_theme(Distinct);
            let item = provide_control_size(size, probe.build).expect("the rows build");
            let own = track_layout(item.layout_node()).expect("the rows have a node");
            let mut tree = mounted(Box::new(item));
            (probe.open)(&mut tree);
            settle(&tree);
            let row = match probe.row {
                Surface::Root => Some(own.get()),
                Surface::First(role) => drawn(role).into_iter().next(),
            };
            match row {
                None => failures.push(format!("`{}` at {size:?} shows no row", probe.name)),
                Some(row) if (row.height - want).abs() >= 0.5 => failures.push(format!(
                    "`{}` at {size:?} stands a row {} tall, want {want}",
                    probe.name, row.height
                )),
                Some(_) => {}
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Which corner role a floating panel is rounded by.
#[derive(Clone, Copy, Debug)]
enum Corner {
    Surface,
    Dialog,
}

impl Corner {
    fn radius(self) -> f32 {
        match self {
            Corner::Surface => Distinct.radius_surface(),
            Corner::Dialog => Distinct.radius_dialog(),
        }
    }

    fn other(self) -> Corner {
        match self {
            Corner::Surface => Corner::Dialog,
            Corner::Dialog => Corner::Surface,
        }
    }
}

/// A floating panel, shown, and the role it is rounded by.
struct Panel {
    name: &'static str,
    corner: Corner,
    show: fn() -> ComponentList,
}

fn shown(build: impl FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError>) -> ComponentList {
    mounted(build().expect("the panel builds"))
}

fn panel_probes() -> Vec<Panel> {
    #[allow(unused_mut)]
    let mut probes = vec![Panel {
        name: "popup",
        corner: Corner::Surface,
        show: || {
            let mut tree =
                shown(|| crate::select(crate::SelectProps::props().build(), two_items()));
            open_from_keyboard(&mut tree);
            settle(&tree);
            tree
        },
    }];
    #[cfg(feature = "overlays")]
    probes.extend([
        Panel {
            name: "tooltip",
            corner: Corner::Surface,
            show: || {
                let tree = shown(|| {
                    let trigger =
                        telar::Container::new(LayoutStyle::new().width(80.0).height(30.0), vec![])?;
                    let mut slots = telar::Slots::new();
                    slots.push(None, telar::box_item(trigger));
                    crate::tooltip(
                        crate::TooltipProps::props().text("Find").build(),
                        Children::from(slots),
                    )
                });
                let mut tree = tree;
                tree.on_event(&telar::testing::moved(40.0, 15.0));
                settle(&tree);
                tree
            },
        },
        Panel {
            name: "toast",
            corner: Corner::Surface,
            show: || {
                let queue = crate::Toasts::new();
                let tree = shown(|| {
                    crate::toaster(
                        crate::ToasterProps::props().toasts(queue).build(),
                        Children::default(),
                    )
                });
                queue.push(crate::Toast::new("Saved").until_dismissed());
                settle(&tree);
                tree
            },
        },
        Panel {
            name: "context menu",
            corner: Corner::Surface,
            show: || {
                shown(|| {
                    crate::context_menu(
                        crate::ContextMenuProps::props()
                            .at((40.0, 30.0))
                            .on_close(std::rc::Rc::new(|| {}))
                            .within(Rect::new(0.0, 0.0, 800.0, 600.0))
                            .build(),
                        two_items(),
                    )
                })
            },
        },
        Panel {
            name: "modal",
            corner: Corner::Dialog,
            show: || {
                shown(|| {
                    crate::modal(
                        crate::ModalProps::props()
                            .open(telar::signal(true))
                            .title("Confirm")
                            .build(),
                        Children::default(),
                    )
                })
            },
        },
        Panel {
            name: "drawer",
            corner: Corner::Dialog,
            show: || {
                shown(|| {
                    crate::drawer(
                        crate::DrawerProps::props()
                            .open(telar::signal(true))
                            .build(),
                        Children::default(),
                    )
                })
            },
        },
        Panel {
            name: "command palette",
            corner: Corner::Dialog,
            show: || {
                shown(|| {
                    crate::command_palette(
                        crate::CommandPaletteProps::props()
                            .open(telar::signal(true))
                            .commands(vec![crate::Command::new("save", "Save")])
                            .build(),
                        Children::default(),
                    )
                })
            },
        },
    ]);
    #[cfg(feature = "workbench")]
    probes.push(Panel {
        name: "code view",
        corner: Corner::Surface,
        show: || {
            shown(|| {
                crate::code_view(
                    crate::CodeViewProps::props()
                        .code("fn main() {}")
                        .copyable(false)
                        .build(),
                    Children::default(),
                )
            })
        },
    });
    probes
}

/// Every corner a filled box in `tree` is drawn with.
fn filled_corners(tree: &ComponentList) -> Vec<f32> {
    tree.commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Rect { style, .. } if style.fill.is_some() => Some(style.radius),
            _ => None,
        })
        .flat_map(|r| [r.top_left, r.top_right, r.bottom_right, r.bottom_left])
        .collect()
}

/// A panel floating over the page is rounded by the role for what it is — `radius_surface` for one that comes and goes beside what it is about, `radius_dialog` for one that takes the screen's attention — and never by the other one.
#[test]
fn every_floating_panel_rounds_by_its_role() {
    let mut failures = Vec::new();
    for probe in panel_probes() {
        fresh_layout_runtime();
        set_theme(Distinct);
        telar::set_surface_size(telar::Size::new(800.0, 600.0));
        let tree = (probe.show)();
        let corners = filled_corners(&tree);
        let has = |corner: Corner| corners.iter().any(|r| (r - corner.radius()).abs() < 0.01);
        if !has(probe.corner) {
            failures.push(format!(
                "`{}` draws no {:?} corner ({}): {corners:?}",
                probe.name,
                probe.corner,
                probe.corner.radius()
            ));
        }
        if has(probe.corner.other()) {
            failures.push(format!(
                "`{}` draws a {:?} corner where it is a {:?}: {corners:?}",
                probe.name,
                probe.corner.other(),
                probe.corner
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
