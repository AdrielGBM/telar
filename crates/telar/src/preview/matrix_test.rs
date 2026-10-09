use super::*;
use crate::{Container, LayoutError, LayoutItem, LayoutStyle};

fn empty(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(Box::new(Container::new(LayoutStyle::new(), Vec::new())?))
}

fn entry() -> PreviewEntry {
    PreviewEntry::new("kit--button--primary", "button", "Primary", empty)
}

fn ids(cells: &[MatrixCell]) -> Vec<&str> {
    cells.iter().map(|cell| cell.id.as_str()).collect()
}

const PHONES: &[ViewportPreset] = &[ViewportPreset::new("phone", 390.0, 844.0)];

const PACKAGE: &[NamedMatrix] = &[
    NamedMatrix::new("rtl", &[Axis::Dir(&[Direction::Ltr, Direction::Rtl])]),
    NamedMatrix::new(
        "phones",
        &[
            Axis::Viewport(&["phone", "1280x800"]),
            Axis::Mode(&["light"]),
        ],
    ),
];

#[test]
fn an_inline_matrix_is_a_constant_a_preview_writes_in_place() {
    let entry = entry().matrix(Matrix::Axes(&[
        Axis::Mode(&["light", "dark"]),
        Axis::Locale(&["en", "ar"]),
    ]));
    let cells = entry
        .matrix
        .unwrap()
        .cells(&entry, &Matrices::default())
        .unwrap();
    assert_eq!(
        ids(&cells),
        [
            "kit--button--primary--mode=light--locale=en",
            "kit--button--primary--mode=light--locale=ar",
            "kit--button--primary--mode=dark--locale=en",
            "kit--button--primary--mode=dark--locale=ar",
        ]
    );
    assert_eq!(
        cells[1].coordinates,
        [("mode", "light".to_string()), ("locale", "ar".to_string())]
    );
    assert_eq!(cells[1].globals.mode.as_deref(), Some("light"));
    assert_eq!(cells[1].globals.locale.as_deref(), Some("ar"));
    assert_eq!(cells[1].globals.direction, None);
}

#[test]
fn every_global_axis_sets_its_global() {
    let matrix = Matrix::Axes(&[
        Axis::Dir(&[Direction::Rtl]),
        Axis::Viewport(&["390x844"]),
        Axis::ControlSize(&[ControlSize::Small]),
    ]);
    let cells = matrix.cells(&entry(), &Matrices::default()).unwrap();
    assert_eq!(
        ids(&cells),
        ["kit--button--primary--dir=rtl--viewport=390x844--control_size=small"]
    );
    let globals = &cells[0].globals;
    assert_eq!(globals.direction, Some(Direction::Rtl));
    assert_eq!(globals.viewport, Some(Size::new(390.0, 844.0)));
    assert_eq!(globals.control_size, Some(ControlSize::Small));
    assert!(cells[0].args.is_empty());
}

#[test]
fn an_arg_axis_reads_each_value_in_its_text_form() {
    let matrix = Matrix::Axes(&[
        Axis::Arg("size", &["12", "-1.5"]),
        Axis::Arg("label", &["\"Save all\""]),
        Axis::Arg("variant", &["Ghost"]),
    ]);
    let cells = matrix.cells(&entry(), &Matrices::default()).unwrap();
    assert_eq!(
        ids(&cells),
        [
            "kit--button--primary--size=12--label=Save-all--variant=Ghost",
            "kit--button--primary--size=-1.5--label=Save-all--variant=Ghost",
        ]
    );
    assert_eq!(
        cells[1].args,
        [
            ("size", ArgValue::Float(-1.5)),
            ("label", ArgValue::Text("Save all".into())),
            ("variant", ArgValue::Choice("Ghost".into())),
        ]
    );
    assert_eq!(
        cells[0].coordinates[1],
        ("label", "\"Save all\"".to_string())
    );
}

#[test]
fn a_named_matrix_is_the_packages_and_then_a_built_in_one() {
    let matrices = Matrices::new(PACKAGE, PHONES).with_modes(["light", "dark", "contrast"]);
    let rtl = Matrix::Named("rtl").cells(&entry(), &matrices).unwrap();
    assert_eq!(
        ids(&rtl),
        [
            "kit--button--primary--dir=ltr",
            "kit--button--primary--dir=rtl"
        ]
    );
    let themes = Matrix::Named("themes").cells(&entry(), &matrices).unwrap();
    assert_eq!(
        ids(&themes),
        [
            "kit--button--primary--mode=light",
            "kit--button--primary--mode=dark",
            "kit--button--primary--mode=contrast",
        ]
    );
    assert_eq!(
        Matrix::Named("missing").cells(&entry(), &matrices),
        Err(MatrixError::Unknown { name: "missing" })
    );
}

#[test]
fn a_package_may_replace_a_built_in_matrix() {
    const OWN_THEMES: &[NamedMatrix] = &[NamedMatrix::new("themes", &[Axis::Mode(&["paper"])])];
    let matrices = Matrices::new(OWN_THEMES, &[]).with_modes(["light", "dark"]);
    let cells = Matrix::Named("themes").cells(&entry(), &matrices).unwrap();
    assert_eq!(ids(&cells), ["kit--button--primary--mode=paper"]);
}

#[test]
fn a_viewport_is_a_size_or_a_name_the_package_declares() {
    let matrices = Matrices::new(PACKAGE, PHONES);
    let cells = Matrix::Named("phones").cells(&entry(), &matrices).unwrap();
    assert_eq!(
        ids(&cells),
        [
            "kit--button--primary--viewport=phone--mode=light",
            "kit--button--primary--viewport=1280x800--mode=light",
        ]
    );
    assert_eq!(cells[0].globals.viewport, Some(Size::new(390.0, 844.0)));
    assert_eq!(cells[1].globals.viewport, Some(Size::new(1280.0, 800.0)));

    let unknown = Matrix::Axes(&[Axis::Viewport(&["watch"])]);
    assert_eq!(
        unknown.cells(&entry(), &Matrices::default()),
        Err(MatrixError::Viewport { text: "watch" })
    );
}

#[test]
fn an_axis_with_no_values_varies_nothing() {
    let themes = Matrix::Named("themes");
    let matrices = Matrices::default();
    assert_eq!(themes.cells(&entry(), &matrices), Ok(Vec::new()));
    assert_eq!(themes.cell_count(&matrices), Ok(0));

    let mixed = Matrix::Axes(&[Axis::RegisteredModes, Axis::Locale(&["en", "ar"])]);
    let cells = mixed.cells(&entry(), &matrices).unwrap();
    assert_eq!(
        ids(&cells),
        [
            "kit--button--primary--locale=en",
            "kit--button--primary--locale=ar"
        ]
    );
}

#[test]
fn the_cell_count_is_known_before_expanding() {
    let matrices = Matrices::default().with_modes(["light", "dark"]);
    let matrix = Matrix::Axes(&[
        Axis::RegisteredModes,
        Axis::Locale(&["en", "ar", "ja"]),
        Axis::Arg("ghost", &["true", "false"]),
    ]);
    assert_eq!(matrix.cell_count(&matrices), Ok(12));
    assert_eq!(matrix.cells(&entry(), &matrices).unwrap().len(), 12);
    assert_eq!(
        Matrix::Named("missing").cell_count(&matrices),
        Err(MatrixError::Unknown { name: "missing" })
    );
}

#[test]
fn a_matrix_that_cannot_name_its_cells_apart_is_refused() {
    let matrices = Matrices::default().with_modes(["light"]);
    for (matrix, expected) in [
        (
            Matrix::Axes(&[Axis::Mode(&["dark"]), Axis::RegisteredModes]),
            MatrixError::RepeatedAxis { axis: "mode" },
        ),
        (
            Matrix::Axes(&[Axis::Locale(&["en", "en"])]),
            MatrixError::RepeatedValue {
                axis: "locale",
                value: "en".into(),
            },
        ),
        (
            Matrix::Axes(&[Axis::Arg("label", &["\"a b\"", "\"a-b\""])]),
            MatrixError::RepeatedValue {
                axis: "label",
                value: "\"a-b\"".into(),
            },
        ),
        (
            Matrix::Axes(&[Axis::Arg("label", &["\"\""])]),
            MatrixError::Unnamed {
                axis: "label",
                value: "\"\"".into(),
            },
        ),
        (
            Matrix::Axes(&[Axis::Arg("label", &["two words"])]),
            MatrixError::ArgValue {
                axis: "label",
                text: "two words",
            },
        ),
    ] {
        assert_eq!(matrix.cells(&entry(), &matrices), Err(expected));
    }
}

#[test]
fn a_cell_mounts_with_the_preview_env_under_its_own() {
    let entry = entry().mode("dark").locale("es").matrix(Matrix::Axes(&[
        Axis::Locale(&["ar"]),
        Axis::Arg("size", &["16"]),
    ]));
    let cells = entry
        .matrix
        .unwrap()
        .cells(&entry, &Matrices::default())
        .unwrap();
    let ctx = cells[0].ctx(&entry).unwrap();
    let globals = ctx.globals();
    assert_eq!(globals.mode().get().as_deref(), Some("dark"));
    assert_eq!(globals.locale().get().as_deref(), Some("ar"));
    assert_eq!(ctx.arg("size", 12u32), 16);
}

#[test]
fn the_installed_matrices_are_the_applications() {
    // The shape `app!` emits: statics inside its constructor, every value a constant.
    static MATRICES: &[NamedMatrix] = &[NamedMatrix::new(
        "layouts",
        &[
            Axis::Dir(&[crate::Direction::Ltr, crate::Direction::Rtl]),
            Axis::Viewport(&["phone"]),
            Axis::ControlSize(&[crate::ControlSize::Small]),
            Axis::Arg("size", &["12", "1.5"]),
        ],
    )];
    static VIEWPORTS: &[ViewportPreset] = &[ViewportPreset::new("phone", 390f32, 844f32)];
    const OTHER: &[NamedMatrix] = &[NamedMatrix::new("other", &[Axis::Mode(&["x"])])];
    __install_matrices(MATRICES, VIEWPORTS);
    __install_matrices_if_unset(OTHER, &[]);
    let installed = Matrices::installed();
    assert_eq!(installed.named(), MATRICES);
    assert_eq!(installed.viewports(), VIEWPORTS);
    assert_eq!(
        Matrix::Named("layouts").cell_count(&installed),
        Ok(4),
        "its viewport names resolve against its own"
    );
    assert!(Matrix::Named("layouts").cells(&entry(), &installed).is_ok());
    assert!(installed.get("other").is_none());
}

#[test]
fn the_installed_matrices_take_the_modes_registered_when_they_are_read() {
    theme_core::register_mode("light", || {});
    theme_core::register_mode("dusk", || {});
    let matrix = Matrix::Axes(&[Axis::RegisteredModes]);
    let installed = Matrices::installed();
    let registered = theme_core::registered_modes();
    assert_eq!(installed.modes(), registered);
    assert_eq!(matrix.cell_count(&installed), Ok(registered.len()));

    theme_core::register_mode("late", || {});
    assert_eq!(
        Matrices::installed().modes().len(),
        registered.len() + 1,
        "read at call time"
    );
    assert_eq!(installed.modes(), registered);
}

#[test]
fn with_modes_overrides_the_registered_modes() {
    theme_core::register_mode("light", || {});
    let matrices = Matrices::installed().with_modes(["paper"]);
    let cells = Matrix::Named("themes").cells(&entry(), &matrices).unwrap();
    assert_eq!(ids(&cells), ["kit--button--primary--mode=paper"]);
}
