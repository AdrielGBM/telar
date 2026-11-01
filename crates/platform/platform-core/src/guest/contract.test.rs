use super::*;
use crate::guest::UpperServices;
use crate::guest::services::NoUpper;

const TABLE: ContractStamp = ContractStamp::of(
    GuestKind::HotReload,
    &[crate::type_layout!(Table { create, release })],
);

#[repr(C)]
struct Table {
    create: unsafe extern "C" fn() -> Handle<()>,
    release: unsafe extern "C" fn(Handle<()>) -> CallOutcome,
}

#[repr(C)]
struct Reordered {
    release: unsafe extern "C" fn(Handle<()>) -> CallOutcome,
    create: unsafe extern "C" fn() -> Handle<()>,
}

crate::export_contract_stamp!(TABLE);

#[test]
fn a_stamp_carries_this_contract_version() {
    assert_eq!(TABLE.version, CONTRACT_VERSION);
    assert_eq!(TELAR_GUEST_CONTRACT, TABLE);
    assert_eq!(CONTRACT_SYMBOL, b"telar_guest_contract\0");
}

#[test]
fn a_table_laid_out_differently_changes_the_stamp() {
    let reordered = ContractStamp::of(
        GuestKind::HotReload,
        &[crate::type_layout!(Reordered { create, release })],
    );
    assert_ne!(reordered.layout, TABLE.layout);
    assert_ne!(
        ContractStamp::of(GuestKind::HotReload, &[]).layout,
        TABLE.layout
    );
}

#[test]
fn a_setting_changes_the_stamp_whether_it_is_on_or_off() {
    let on = TABLE.with_setting("shaper", true);
    let off = TABLE.with_setting("shaper", false);
    assert_ne!(on.layout, TABLE.layout);
    assert_ne!(on.layout, off.layout);
    assert_eq!(on, TABLE.with_setting("shaper", true));
}

#[test]
fn the_stamp_a_guest_exports_is_accepted_when_it_matches() {
    assert_eq!(unsafe { TABLE.accept(&TELAR_GUEST_CONTRACT) }, Ok(()));
}

#[test]
fn another_layout_is_refused() {
    let guest = ContractStamp {
        layout: TABLE.layout ^ 1,
        ..TABLE
    };
    assert_eq!(
        unsafe { TABLE.accept(&guest) },
        Err(ContractMismatch::Layout {
            host: TABLE.layout,
            guest: TABLE.layout ^ 1
        })
    );
}

/// The two boundaries share every building block, so their stamps could agree on everything but the table each one reads next: the kind is what says which table is there.
#[test]
fn a_guest_of_the_other_kind_is_refused_as_that_kind() {
    let embedded = ContractStamp {
        kind: GuestKind::Embed.code(),
        ..TABLE
    };
    let refused = unsafe { TABLE.accept(&embedded) }.expect_err("another kind is refused");
    assert_eq!(
        refused,
        ContractMismatch::Kind {
            host: GuestKind::HotReload.code(),
            guest: GuestKind::Embed.code()
        }
    );
    assert_eq!(
        refused.to_string(),
        "this is an embedded guest, not a hot-reload library"
    );

    let unknown = ContractStamp { kind: 9, ..TABLE };
    let refused = unsafe { TABLE.accept(&unknown) }.expect_err("an unknown kind is refused");
    assert!(
        refused.to_string().contains("does not know (9)"),
        "{refused}"
    );
}

#[test]
fn another_version_is_refused_reading_nothing_past_it() {
    let older: u32 = CONTRACT_VERSION + 1;
    let stamp = std::ptr::from_ref(&older).cast::<ContractStamp>();
    assert_eq!(
        unsafe { TABLE.accept(stamp) },
        Err(ContractMismatch::Version {
            host: CONTRACT_VERSION,
            guest: CONTRACT_VERSION + 1
        })
    );
}

#[test]
fn every_building_block_is_hashed() {
    let names: Vec<&str> = BUILDING_BLOCKS.iter().map(|layout| layout.name).collect();
    assert_eq!(
        names,
        [
            "LentBytes<()>",
            "CallOutcome",
            "Handle<()>",
            "CWaker",
            "Timestamp",
            "ContractStamp",
            "FrameCtx",
            "ThreadStart",
            "HostThread",
            "Reply<()>",
            "SurfaceReport"
        ]
    );
    assert!(
        BUILDING_BLOCKS
            .iter()
            .all(|layout| !layout.fields.is_empty())
    );
}

mod guest {
    use super::*;
    use crate::guest::HostServices;
    use crate::guest::services::NoUpper;

    pub(super) extern "C" fn install_services(_: HostServices<NoUpper>) -> CallOutcome {
        CallOutcome::OK
    }

    pub(super) extern "C" fn release(_: Handle<u32>) -> CallOutcome {
        CallOutcome::OK
    }

    pub(super) extern "C" fn paint(_: Handle<u32>, _: *mut LentBytes<String>) -> CallOutcome {
        CallOutcome::OK
    }
}

impl crate::guest::ReportsSurface for u32 {
    fn access_snapshot(&self) -> Vec<crate::AccessNode> {
        Vec::new()
    }

    fn perform_access_action(&mut self, _: u64, _: bool) {}

    fn text_input(&self) -> Option<crate::TextInput> {
        None
    }

    fn arrive(&mut self, _: bool) -> bool {
        false
    }

    fn tab(&mut self, _: &crate::Event) -> bool {
        false
    }

    fn blur(&mut self) {}

    fn holds_focus(&self) -> bool {
        false
    }

    fn text_entry_focused(&self) -> bool {
        false
    }

    fn keeps(&self, _: crate::ConsumedKeys) -> bool {
        false
    }
}

crate::entry_table! {
    /// A table as a boundary declares one.
    pub struct Declared {
        kind: Embed,
        symbol: "telar_declared_entries",
        guest: guest,
        settings: ["shaper" => true],
        services: crate::guest::services::NoUpper,
        create: fn(LentBytes<Vec<String>>, *mut Handle<u32>) as CreateDeclared;
        report: u32;
        release: fn(Handle<u32>);
        paint: fn(Handle<u32>, *mut LentBytes<String>);
    }
}

extern "C" fn create_declared(_: LentBytes<Vec<String>>, _: *mut Handle<u32>) -> CallOutcome {
    CallOutcome::OK
}

#[test]
fn a_declared_table_starts_with_its_lifecycle_and_hashes_every_entry() {
    check_entry_table::<Declared>();
    assert_eq!(Declared::SYMBOL, b"telar_declared_entries\0");
    let fields: Vec<&str> = Declared::LAYOUT
        .fields
        .iter()
        .map(|(field, _)| *field)
        .collect();
    assert_eq!(
        fields,
        [
            "lifecycle: Lifecycle",
            "create: fn(LentBytes<Vec<String>>, *mut Handle<u32>, )",
            "report: SurfaceReport",
            "release: fn(Handle<u32>, )",
            "paint: fn(Handle<u32>, *mut LentBytes<String>, )"
        ]
    );
    assert_eq!(
        Declared::CONTRACT,
        ContractStamp::of(
            GuestKind::Embed,
            &[
                Declared::LAYOUT,
                Lifecycle::<NoUpper>::LAYOUT,
                crate::guest::HostServices::<NoUpper>::LAYOUT,
                NoUpper::LAYOUT,
            ]
        )
        .with_setting("shaper", true)
    );
    let served = Declared::serving(create_declared);
    let installs: unsafe extern "C" fn(crate::guest::HostServices<NoUpper>) -> CallOutcome =
        guest::install_services;
    assert!(std::ptr::fn_addr_eq(
        served.lifecycle().install_services,
        installs
    ));
}

/// One table, `Lending`, whose `paint` lends a `Payload` holding a `$field`, declared in a module of its own so two of them differ in nothing else, the names they write included.
macro_rules! lending_table {
    ($module:ident, $field:ty) => {
        // Only the layouts and stamps are compared: nothing serves the table or reads the payload.
        #[allow(dead_code)]
        mod $module {
            use super::*;

            pub struct Payload {
                value: $field,
            }

            wire_core::wire_struct!(Payload { value });

            mod guest {
                use super::*;
                use crate::guest::HostServices;
                use crate::guest::services::NoUpper;

                pub(super) extern "C" fn install_services(_: HostServices<NoUpper>) -> CallOutcome {
                    CallOutcome::OK
                }

                pub(super) extern "C" fn release(_: Handle<u32>) -> CallOutcome {
                    CallOutcome::OK
                }

                pub(super) extern "C" fn paint(
                    _: Handle<u32>,
                    _: *mut LentBytes<Payload>,
                ) -> CallOutcome {
                    CallOutcome::OK
                }
            }

            crate::entry_table! {
                /// A table lending one value.
                pub struct Lending {
                    kind: Embed,
                    symbol: "telar_lending_entries",
                    guest: guest,
                    settings: [],
                    services: crate::guest::services::NoUpper,
                    create: fn(*mut Handle<u32>) as CreateLending;
                    report: u32;
                    release: fn(Handle<u32>);
                    paint: fn(Handle<u32>, *mut LentBytes<Payload>);
                }
            }

            #[repr(C)]
            pub struct Block {
                pub bytes: LentBytes<Payload>,
            }

            pub fn block_layout() -> TypeLayout {
                crate::type_layout!(Block { bytes })
            }
        }
    };
}

lending_table!(lends_text, String);
lending_table!(lends_number, u64);

/// Two tables laid out alike whose entries lend different values would read each other's bytes as the wrong thing, so their stamps differ though no offset does.
#[test]
fn an_entry_that_lends_something_else_changes_the_stamp() {
    let (text, number) = (lends_text::Lending::LAYOUT, lends_number::Lending::LAYOUT);
    assert_eq!(
        (text.name, text.size, text.fields),
        (number.name, number.size, number.fields)
    );
    assert_ne!(
        lends_text::Lending::CONTRACT,
        lends_number::Lending::CONTRACT
    );
}

/// What a building block lends is part of its layout, as much as where its fields sit.
#[test]
fn what_a_building_block_lends_is_part_of_its_layout() {
    let (text, number) = (lends_text::block_layout(), lends_number::block_layout());
    assert_eq!(
        (text.name, text.size, text.fields),
        (number.name, number.size, number.fields)
    );
    assert_ne!(text.shape, number.shape);
}
