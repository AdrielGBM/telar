//! The guardrails that hold the catalogue to one set of proportions at every size: every single-line control is [`metrics::control_height`] tall and rounds by `radius_control`; every selection control stands a control height tall round a box scaled to the size; every list, menu, select, tree and context menu stands its rows at [`metrics::row_height`]; and every floating panel rounds by its role, `radius_surface` for a popup, a tooltip, a toast, a context menu or a code block and `radius_dialog` for a modal, a drawer or the command palette.

use telar::testing::{Scoped, scoped};
use telar::{
    AvailableSpace, BorderRadius, Children, ComponentList, DrawCommand, LayoutError, LayoutItem,
    LayoutStyle, Rect, RwSignal, ThemeTokens, compute_layout, focus::Role, relayout_if_dirty,
    set_theme, track_layout,
};

use crate::metrics;
use crate::test_support::{drawn, fresh_layout_runtime};
use telar::ControlSize;

/// One control in the row: how to build it, and where its principal surface is drawn.
struct Probe {
    name: &'static str,
    build: fn(ControlSize) -> Result<Box<dyn LayoutItem>, LayoutError>,
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
            build: |size| {
                crate::button(crate::ButtonProps::props().label("Save").size(size).build())
            },
            surface: Surface::First(Role::Button),
        },
        Probe {
            name: "text_field",
            build: |size| {
                crate::text_field(
                    crate::TextFieldProps::props()
                        .placeholder("Name")
                        .size(size)
                        .build(),
                )
            },
            surface: Surface::Root,
        },
        Probe {
            name: "search_field",
            build: |size| crate::search_field(crate::SearchFieldProps::props().size(size).build()),
            surface: Surface::Root,
        },
        Probe {
            name: "multiline text_field of one row",
            build: |size| {
                crate::text_field(
                    crate::TextFieldProps::props()
                        .multiline(true)
                        .rows(1)
                        .size(size)
                        .build(),
                )
            },
            surface: Surface::Root,
        },
        Probe {
            name: "select",
            build: |size| crate::select(crate::SelectProps::props().size(size).build()),
            surface: Surface::First(Role::ComboBox),
        },
        Probe {
            name: "tab",
            build: |size| {
                crate::tabs(
                    crate::TabsProps::props()
                        .items(vec!["One", "Two"])
                        .size(size)
                        .build(),
                )
            },
            surface: Surface::First(Role::Tab),
        },
        Probe {
            name: "scrub_field",
            build: |size| {
                crate::scrub_field(
                    crate::ScrubFieldProps::props()
                        .label("X")
                        .size(size)
                        .build(),
                )
            },
            surface: Surface::Root,
        },
    ];
    probes.push(Probe {
        name: "icon_button",
        build: |size| {
            crate::icon_button(
                crate::IconButtonProps::props()
                    .icon("lucide:search")
                    .label("Find")
                    .size(size)
                    .build(),
            )
        },
        surface: Surface::First(Role::Button),
    });
    probes.push(Probe {
        name: "menu",
        build: |size| {
            crate::menu(
                crate::MenuProps::props()
                    .label("File")
                    .fit(true)
                    .size(size)
                    .build(),
            )
        },
        surface: Surface::First(Role::Button),
    });
    probes.push(Probe {
        name: "stepper",
        build: |size| crate::stepper(crate::StepperProps::props().size(size).build()),
        surface: Surface::First(Role::Button),
    });
    probes
}

/// What one probe measured at one size.
#[derive(Debug)]
struct Measured {
    height: f32,
    radius: Option<BorderRadius>,
}

/// What `build` makes at `size`, alone in a row of an 800×600 window, laid out and mounted on a fresh layout runtime, with the signal its own rect is published through.
fn in_a_row(
    size: ControlSize,
    build: fn(ControlSize) -> Result<Box<dyn LayoutItem>, LayoutError>,
) -> Scoped<(RwSignal<Rect>, ComponentList)> {
    fresh_layout_runtime();
    set_theme(Distinct);
    scoped(|| {
        let control = build(size).expect("the control builds");
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
        (rect, ComponentList::new(telar::box_item(row)))
    })
}

fn measure(probe: &Probe, size: ControlSize) -> Measured {
    let built = in_a_row(size, probe.build);
    let (rect, tree) = &*built;
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
    let measured = Measured {
        height: surface.map_or(root.height, |surface| surface.height),
        radius,
    };
    drop(commands);
    drop(built);
    measured
}

fn same(a: Rect, b: Rect) -> bool {
    [a.x - b.x, a.y - b.y, a.width - b.width, a.height - b.height]
        .iter()
        .all(|d| d.abs() < 0.5)
}

fn expected_height(size: ControlSize) -> f32 {
    fresh_layout_runtime();
    set_theme(Distinct);
    metrics::control_height(size)
}

const SIZES: &[ControlSize] = &[ControlSize::Small, ControlSize::Regular, ControlSize::Large];

#[test]
fn every_single_line_control_is_one_height_and_one_corner_at_every_size() {
    let corner = Some(BorderRadius::all(Distinct.radius_control()));
    let mut failures = Vec::new();
    for probe in probes() {
        for &size in SIZES {
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
    build: fn(ControlSize) -> Result<Box<dyn LayoutItem>, LayoutError>,
    role: Role,
    /// The drawn box's size at a given size.
    drawn: fn(ControlSize) -> (f32, f32),
    /// The drawn box's corner, laid out at `rect`.
    corner: fn(Rect) -> BorderRadius,
}

fn selections() -> Vec<Selection> {
    vec![
        Selection {
            name: "checkbox",
            build: |size| {
                crate::checkbox(
                    crate::CheckboxProps::props()
                        .label("Agree")
                        .size(size)
                        .build(),
                )
            },
            role: Role::CheckBox,
            drawn: |size| (metrics::icon_size(size), metrics::icon_size(size)),
            corner: |_| BorderRadius::all(Distinct.radius_item()),
        },
        Selection {
            name: "radio",
            build: |size| {
                crate::radio(crate::RadioProps::props().label("Small").size(size).build())
            },
            role: Role::Radio,
            drawn: |size| (metrics::icon_size(size), metrics::icon_size(size)),
            corner: metrics::pill,
        },
        Selection {
            name: "toggle",
            build: |size| {
                crate::toggle(
                    crate::ToggleProps::props()
                        .label("Wi-Fi")
                        .size(size)
                        .build(),
                )
            },
            role: Role::Switch,
            drawn: |size| {
                let (knob, inset) = (metrics::icon_size(size), metrics::gap_xs(size));
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

/// What `selection` asks of `size`, under the probes' theme.
fn expected_box(selection: &Selection, size: ControlSize) -> (f32, f32) {
    fresh_layout_runtime();
    set_theme(Distinct);
    (selection.drawn)(size)
}

/// A selection control's row stands a control height tall at every size, like a button beside it, and the box drawn in it scales with the size, is centred in the row and takes its corner from the theme's roles.
#[test]
fn every_selection_control_stands_a_control_height_tall_with_a_box_scaled_to_the_size() {
    let mut failures = Vec::new();
    for selection in selections() {
        for &size in SIZES {
            let height = expected_height(size);
            let (width, tall) = expected_box(&selection, size);
            let built = in_a_row(size, selection.build);
            let commands = built.1.commands();
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

fn two_items(size: ControlSize) -> Children {
    Children::new(move || {
        let mut items = Vec::new();
        for label in ["Cut", "Copy"] {
            items.push(crate::item(
                crate::ItemProps::props().label(label).size(size).build(),
            )?);
        }
        Ok(items)
    })
}

/// A component that draws rows, how to show them, and where its first row is.
struct Rows {
    name: &'static str,
    build: fn(ControlSize) -> Result<Box<dyn LayoutItem>, LayoutError>,
    open: fn(&mut ComponentList),
    row: Surface,
}

fn row_probes() -> Vec<Rows> {
    #[allow(unused_mut)]
    let mut probes = vec![
        Rows {
            name: "list item",
            build: |size| crate::item(crate::ItemProps::props().label("Cut").size(size).build()),
            open: nothing_to_open,
            row: Surface::Root,
        },
        Rows {
            name: "select",
            build: |size| {
                crate::select(
                    crate::SelectProps::props()
                        .size(size)
                        .children(two_items(size))
                        .build(),
                )
            },
            open: open_from_keyboard,
            row: Surface::First(Role::Option),
        },
    ];
    probes.extend([
        Rows {
            name: "menu",
            build: |size| {
                crate::menu(
                    crate::MenuProps::props()
                        .label("Edit")
                        .size(size)
                        .children(two_items(size))
                        .build(),
                )
            },
            open: open_from_keyboard,
            row: Surface::First(Role::MenuItem),
        },
        Rows {
            name: "context menu",
            build: |size| {
                crate::context_menu(
                    crate::ContextMenuProps::props()
                        .at((40.0, 30.0))
                        .on_close(std::rc::Rc::new(|| {}))
                        .within(Rect::new(0.0, 0.0, 800.0, 600.0))
                        .size(size)
                        .children(two_items(size))
                        .build(),
                )
            },
            open: nothing_to_open,
            row: Surface::First(Role::MenuItem),
        },
    ]);
    probes.push(Rows {
        name: "tree",
        build: |size| {
            crate::tree_view(
                crate::TreeViewProps::props()
                    .items(vec![
                        crate::TreeNode::new("src", "src"),
                        crate::TreeNode::new("lib", "lib.rs"),
                    ])
                    .label("Files")
                    .size(size)
                    .build(),
            )
        },
        open: nothing_to_open,
        row: Surface::First(Role::TreeItem),
    });
    probes.extend([
        Rows {
            name: "virtual list",
            build: |size| {
                crate::virtual_list(
                    crate::VirtualListProps::props()
                        .items(vec![
                            crate::ListRow::new("cut", "Cut"),
                            crate::ListRow::new("copy", "Copy"),
                        ])
                        .label("Edit")
                        .size(size)
                        .build(),
                )
            },
            open: nothing_to_open,
            row: Surface::First(Role::Option),
        },
        Rows {
            name: "data table",
            build: |size| {
                crate::data_table(
                    crate::DataTableProps::props()
                        .columns(vec![crate::Column::new("name", "Name").fixed(120.0)])
                        .rows(vec![crate::TableRow::new("a", ["lib.rs"])])
                        .label("Files")
                        .size(size)
                        .build(),
                )
            },
            open: nothing_to_open,
            row: Surface::First(Role::GridCell),
        },
    ]);
    probes
}

fn expected_row_height(size: ControlSize) -> f32 {
    fresh_layout_runtime();
    set_theme(Distinct);
    metrics::row_height(size)
}

/// Every list, menu, select, tree, context menu, virtual list and data table stands its rows at the one row height of their size, so a menu and the list beside it cannot disagree about how tall a row is.
#[test]
fn every_row_of_every_list_is_the_one_row_height_at_every_size() {
    let mut failures = Vec::new();
    for probe in row_probes() {
        for &size in SIZES {
            let want = expected_row_height(size);
            fresh_layout_runtime();
            set_theme(Distinct);
            let mut built = scoped(|| {
                let item = (probe.build)(size).expect("the rows build");
                let own = track_layout(item.layout_node()).expect("the rows have a node");
                (own, mounted(Box::new(item)))
            });
            let (own, tree) = &mut *built;
            (probe.open)(tree);
            settle(tree);
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
    show: fn() -> Scoped<ComponentList>,
}

fn shown(
    build: impl FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError>,
) -> Scoped<ComponentList> {
    scoped(|| mounted(build().expect("the panel builds")))
}

fn panel_probes() -> Vec<Panel> {
    #[allow(unused_mut)]
    let mut probes = vec![Panel {
        name: "popup",
        corner: Corner::Surface,
        show: || {
            let mut tree = shown(|| {
                crate::select(
                    crate::SelectProps::props()
                        .children(two_items(ControlSize::Regular))
                        .build(),
                )
            });
            open_from_keyboard(&mut tree);
            settle(&tree);
            tree
        },
    }];
    probes.extend([
        Panel {
            name: "tooltip",
            corner: Corner::Surface,
            show: || {
                let tree = shown(|| {
                    let trigger = Children::one(|| {
                        let trigger = telar::Container::new(
                            LayoutStyle::new().width(80.0).height(30.0),
                            vec![],
                        )?;
                        Ok(telar::box_item(trigger))
                    });
                    crate::tooltip(
                        crate::TooltipProps::props()
                            .text("Find")
                            .children(trigger)
                            .build(),
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
                let tree =
                    shown(|| crate::toaster(crate::ToasterProps::props().toasts(queue).build()));
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
                            .children(two_items(ControlSize::Regular))
                            .build(),
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
                    )
                })
            },
        },
    ]);
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

/// Something a pointer operates, as the sweep of pointer targets mounts it: how to build it at a size and how to show what it holds.
struct Pointed {
    name: &'static str,
    build: fn(ControlSize) -> Result<Box<dyn LayoutItem>, LayoutError>,
    open: fn(&mut ComponentList),
}

/// Every probe above, and the controls drawn smaller than a target that only an outset holds at the smallest one: a selection control with no label, a swatch, a slider's thumb, a field's clear button, a dialog's close button.
fn pointed() -> Vec<Pointed> {
    let mut pointed: Vec<Pointed> = probes()
        .into_iter()
        // A field of several lines takes presses on its text area alone, which at one row is a line tall inside the frame's padding; growing it reaches into `TextArea` itself.
        .filter(|probe| probe.name != "multiline text_field of one row")
        .map(|probe| Pointed {
            name: probe.name,
            build: probe.build,
            open: nothing_to_open,
        })
        .chain(selections().into_iter().map(|selection| Pointed {
            name: selection.name,
            build: selection.build,
            open: nothing_to_open,
        }))
        .chain(row_probes().into_iter().map(|rows| Pointed {
            name: rows.name,
            build: rows.build,
            open: rows.open,
        }))
        .collect();
    pointed.extend([
        Pointed {
            name: "checkbox with no label",
            build: |size| crate::checkbox(crate::CheckboxProps::props().size(size).build()),
            open: nothing_to_open,
        },
        Pointed {
            name: "radio with no label",
            build: |size| crate::radio(crate::RadioProps::props().size(size).build()),
            open: nothing_to_open,
        },
        Pointed {
            name: "toggle with no label",
            build: |size| crate::toggle(crate::ToggleProps::props().size(size).build()),
            open: nothing_to_open,
        },
        Pointed {
            name: "swatches",
            build: |size| {
                crate::swatches(
                    crate::SwatchesProps::props()
                        .colors(vec![telar::Color::WHITE, telar::Color::BLACK])
                        .names(vec!["White".into(), "Black".into()])
                        .size(size)
                        .build(),
                )
            },
            open: nothing_to_open,
        },
        Pointed {
            name: "slider",
            build: |size| {
                crate::slider(
                    crate::SliderProps::props()
                        .label("Volume")
                        .size(size)
                        .build(),
                )
            },
            open: nothing_to_open,
        },
        Pointed {
            name: "search_field with text",
            build: |size| {
                crate::search_field(
                    crate::SearchFieldProps::props()
                        .value(telar::signal(String::from("lib")))
                        .size(size)
                        .build(),
                )
            },
            open: nothing_to_open,
        },
    ]);
    pointed.push(Pointed {
        name: "accordion",
        build: |size| {
            crate::accordion(
                crate::AccordionProps::props()
                    .title("Details")
                    .size(size)
                    .build(),
            )
        },
        open: nothing_to_open,
    });
    pointed.push(Pointed {
        name: "modal",
        build: |size| {
            crate::modal(
                crate::ModalProps::props()
                    .open(telar::signal(true))
                    .title("Confirm")
                    .size(size)
                    .build(),
            )
        },
        open: nothing_to_open,
    });
    pointed.push(Pointed {
        name: "window controls",
        build: |size| {
            crate::window_controls(
                crate::WindowControlsProps::props()
                    .on_close(std::rc::Rc::new(|| {}))
                    .size(size)
                    .build(),
            )
        },
        open: nothing_to_open,
    });
    pointed.push(Pointed {
        name: "color picker",
        build: |size| {
            crate::color_picker(
                crate::ColorPickerProps::props()
                    .alpha(true)
                    .swatches(vec![telar::Color::WHITE])
                    .label("Fill")
                    .size(size)
                    .build(),
            )
        },
        open: nothing_to_open,
    });
    pointed
}

/// Every control a pointer operates is found across at least [`metrics::MIN_TARGET`] each way at every size, however small it is drawn: what a reader and a target-size check are told its rect is, the drawn box grown by any outset it declares.
#[test]
fn every_pointer_target_is_at_least_the_smallest_target_at_every_size() {
    let mut failures = Vec::new();
    for pointed in pointed() {
        for &size in SIZES {
            fresh_layout_runtime();
            set_theme(Distinct);
            telar::set_surface_size(telar::Size::new(800.0, 600.0));
            let mut built = scoped(|| mounted((pointed.build)(size).expect("the control builds")));
            (pointed.open)(&mut built);
            settle(&built);
            let small: Vec<String> = telar::inspect::snapshot(&built.commands())
                .into_iter()
                .filter(|node| node.id.is_some() && node.enabled)
                .filter(|node| {
                    node.rect.width + 0.01 < metrics::MIN_TARGET
                        || node.rect.height + 0.01 < metrics::MIN_TARGET
                })
                .map(|node| format!("{:?} {:?} {:?}", node.role, node.name, node.rect))
                .collect();
            if !small.is_empty() {
                failures.push(format!(
                    "`{}` at {size:?}: {}",
                    pointed.name,
                    small.join(", ")
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
