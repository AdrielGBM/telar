//! The one version both boundaries speak, the layout hash that proves two builds agree on every `repr(C)` type between them, the macro each declares its entry table with, and the loader that refuses a library whose builds do not agree.

use std::fmt;

use super::clock::Timestamp;
use super::frame_ctx::FrameCtx;
use super::hash_bytes;
use super::lent::{Handle, LentBytes};
use super::lifecycle::{Entries, LIFECYCLE_LAYOUT, Lifecycle};
use super::outcome::CallOutcome;
use super::services::{HOST_SERVICES_LAYOUT, REPLY_LAYOUT};
use super::threads::{HostThread, ThreadStart};
use super::waker::CWaker;

/// The version of the contract hot reload and embed both load guests through. Bumped whenever a rule of it changes in a way the layout hash cannot see: what an entry means, who frees what, how long a lent slice lives.
pub const CONTRACT_VERSION: u32 = 1;

/// The symbol every guest exports its [`ContractStamp`] as, through [`export_contract_stamp!`](crate::export_contract_stamp).
pub const CONTRACT_SYMBOL: &[u8] = b"telar_guest_contract\0";

/// The size, alignment and field offsets of one `repr(C)` type of the contract, as [`type_layout!`](crate::type_layout) reads them.
#[derive(Clone, Copy, Debug)]
pub struct TypeLayout {
    pub name: &'static str,
    pub size: usize,
    pub align: usize,
    pub fields: &'static [(&'static str, usize)],
}

/// The [`TypeLayout`] of a `repr(C)` type and the listed fields, for a boundary's entry table to put in its [`ContractStamp`]: `type_layout!(Table { create, release })`.
#[macro_export]
macro_rules! type_layout {
    ($ty:ty { $($field:ident),* $(,)? }) => {
        $crate::guest::TypeLayout {
            name: stringify!($ty),
            size: ::std::mem::size_of::<$ty>(),
            align: ::std::mem::align_of::<$ty>(),
            fields: &[$((stringify!($field), ::std::mem::offset_of!($ty, $field))),*],
        }
    };
}

/// The building blocks every boundary shares, hashed into every stamp.
pub const BUILDING_BLOCKS: [TypeLayout; 12] = [
    crate::type_layout!(LentBytes { ptr, len }),
    crate::type_layout!(CallOutcome {
        status,
        message,
        location
    }),
    crate::type_layout!(Handle<()> { ptr }),
    crate::type_layout!(CWaker {
        data,
        wake,
        clone,
        release
    }),
    crate::type_layout!(Timestamp { nanos }),
    crate::type_layout!(ContractStamp {
        version,
        kind,
        layout
    }),
    crate::type_layout!(FrameCtx {
        redraw_requested,
        waker,
        handles
    }),
    crate::type_layout!(ThreadStart { name, main, data }),
    crate::type_layout!(HostThread { ptr }),
    REPLY_LAYOUT,
    HOST_SERVICES_LAYOUT,
    LIFECYCLE_LAYOUT,
];

/// Which boundary a library was built to be loaded through. The two share every rule and building block but not their entry tables, so a loader of one kind refuses a library of the other before it reads a table that is not there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuestKind {
    /// An application `cargo telar dev` swaps in on each rebuild.
    HotReload,
    /// A UI an application composites into its own frame.
    Embed,
}

impl GuestKind {
    const fn code(self) -> u32 {
        match self {
            Self::HotReload => 1,
            Self::Embed => 2,
        }
    }

    fn from_code(code: u32) -> Option<Self> {
        [Self::HotReload, Self::Embed]
            .into_iter()
            .find(|kind| kind.code() == code)
    }
}

impl fmt::Display for GuestKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::HotReload => "a hot-reload library",
            Self::Embed => "an embedded guest",
        })
    }
}

/// What a guest exports for its host to check before reading anything else of it: the [`CONTRACT_VERSION`] it was built for at offset 0, readable whatever version that is, then the [`GuestKind`] it was built as, then a hash of the layout of every `repr(C)` type between them, of [`WIRE_VERSION`](wire_core::WIRE_VERSION) and of the settings both builds must share.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContractStamp {
    pub version: u32,
    pub kind: u32,
    pub layout: u64,
}

impl ContractStamp {
    /// The stamp of a `kind` of boundary whose own entry tables are `tables`, on top of the [`BUILDING_BLOCKS`] every boundary shares.
    pub const fn of(kind: GuestKind, tables: &[TypeLayout]) -> Self {
        let mut hash = 0xcbf2_9ce4_8422_2325;
        hash = hash_bytes(hash, &wire_core::WIRE_VERSION.to_le_bytes());
        hash = hash_layouts(hash, &BUILDING_BLOCKS);
        hash = hash_layouts(hash, tables);
        Self {
            version: CONTRACT_VERSION,
            kind: kind.code(),
            layout: hash,
        }
    }

    /// The stamp with `setting`, on or off, hashed in: a build choice no layout shows that both builds still have to share for the guest to behave the way its host draws it.
    pub const fn with_setting(self, setting: &str, on: bool) -> Self {
        let hash = hash_bytes(self.layout, setting.as_bytes());
        Self {
            layout: hash_bytes(hash, &[on as u8]),
            ..self
        }
    }

    /// Refuses the stamp at `stamp` unless it is `self`, reading its version before anything after it.
    ///
    /// # Safety
    ///
    /// `stamp` must point at a stamp a guest exported, built against this or any other contract version: at least its leading `u32` has to be readable, and the rest is read only when the version matches.
    pub unsafe fn accept(self, stamp: *const ContractStamp) -> Result<(), ContractMismatch> {
        let version = unsafe { stamp.cast::<u32>().read() };
        if version != self.version {
            return Err(ContractMismatch::Version {
                host: self.version,
                guest: version,
            });
        }
        let ContractStamp { kind, layout, .. } = unsafe { stamp.read() };
        if kind != self.kind {
            return Err(ContractMismatch::Kind {
                host: self.kind,
                guest: kind,
            });
        }
        if layout != self.layout {
            return Err(ContractMismatch::Layout {
                host: self.layout,
                guest: layout,
            });
        }
        Ok(())
    }
}

const fn hash_layouts(mut hash: u64, layouts: &[TypeLayout]) -> u64 {
    let mut index = 0;
    while index < layouts.len() {
        let layout = layouts[index];
        hash = hash_bytes(hash, layout.name.as_bytes());
        hash = hash_bytes(hash, &(layout.size as u64).to_le_bytes());
        hash = hash_bytes(hash, &(layout.align as u64).to_le_bytes());
        let mut field = 0;
        while field < layout.fields.len() {
            let (name, offset) = layout.fields[field];
            hash = hash_bytes(hash, name.as_bytes());
            hash = hash_bytes(hash, &(offset as u64).to_le_bytes());
            field += 1;
        }
        index += 1;
    }
    hash
}

/// Exports `stamp` as the guest's [`CONTRACT_SYMBOL`].
#[macro_export]
macro_rules! export_contract_stamp {
    ($stamp:expr) => {
        #[unsafe(export_name = "telar_guest_contract")]
        pub static TELAR_GUEST_CONTRACT: $crate::guest::ContractStamp = $stamp;
    };
}

/// A guest built against another contract than its host's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractMismatch {
    Version { host: u32, guest: u32 },
    Kind { host: u32, guest: u32 },
    Layout { host: u64, guest: u64 },
}

struct KindOf(u32);

impl fmt::Display for KindOf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match GuestKind::from_code(self.0) {
            Some(kind) => kind.fmt(f),
            None => write!(f, "a kind of guest this telar does not know ({})", self.0),
        }
    }
}

impl fmt::Display for ContractMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Version { host, guest } => write!(
                f,
                "the guest was built for guest contract {guest} and this host speaks {host}: rebuild it against this telar"
            ),
            Self::Kind { host, guest } => {
                write!(f, "this is {}, not {}", KindOf(*guest), KindOf(*host))
            }
            Self::Layout { host, guest } => write!(
                f,
                "the guest lays out what crosses the boundary differently from this host, or was built with other features (host {host:016x}, guest {guest:016x}): rebuild both from the same telar with matching features"
            ),
        }
    }
}

impl std::error::Error for ContractMismatch {}

/// Declares a boundary's entry table and its [`Entries`] impl: a `repr(C)` struct of `extern "C"` entries after its [`Lifecycle`], the entry that builds an instance with a type of its own, and a `serving` constructor that takes that entry and fills every other from the function of the same name in the `guest` module, the lifecycle's `install_services` included.
///
/// The table's layout hashes every entry with its signature as written, so an entry that changes what it takes changes the stamp as surely as one that moves, and the stamp hashes each setting, on or off, as [`ContractStamp::with_setting`] does.
///
/// ```ignore
/// entry_table! {
///     /// Every entry of an embedded guest.
///     pub struct EmbedEntries {
///         kind: Embed,
///         symbol: "telar_embed_entries",
///         guest: guest,
///         settings: ["shaper" => cfg!(feature = "shaper")],
///         create: fn(LentBytes, *mut Handle<GuestInstance>) as CreateInstance;
///         release: fn(Handle<GuestInstance>);
///     }
/// }
/// ```
#[macro_export]
macro_rules! entry_table {
    (
        $(#[$meta:meta])*
        pub struct $table:ident {
            kind: $kind:ident,
            symbol: $symbol:literal,
            guest: $guest:ident,
            settings: [$($setting:literal => $on:expr),* $(,)?],
            $create:ident: fn($($create_arg:ty),* $(,)?) as $create_ty:ident;
            $($field:ident: fn($($arg:ty),* $(,)?);)*
        }
    ) => {
        #[doc = concat!("The entry of [`", stringify!($table), "`] that builds an instance, the one entry each guest writes itself.")]
        pub type $create_ty = unsafe extern "C" fn($($create_arg),*) -> $crate::guest::CallOutcome;

        $(#[$meta])*
        #[repr(C)]
        #[derive(Clone, Copy)]
        pub struct $table {
            pub lifecycle: $crate::guest::Lifecycle,
            pub $create: $create_ty,
            $(pub $field: unsafe extern "C" fn($($arg),*) -> $crate::guest::CallOutcome,)*
        }

        impl $table {
            #[doc = concat!("The table a library serves, with `", stringify!($create), "` building its instances.")]
            pub const fn serving($create: $create_ty) -> Self {
                Self {
                    lifecycle: $crate::guest::Lifecycle::serving($guest::install_services),
                    $create,
                    $($field: $guest::$field,)*
                }
            }
        }

        unsafe impl $crate::guest::Entries for $table {
            const SYMBOL: &'static [u8] = concat!($symbol, "\0").as_bytes();
            const LAYOUT: $crate::guest::TypeLayout = $crate::guest::TypeLayout {
                name: stringify!($table),
                size: ::std::mem::size_of::<$table>(),
                align: ::std::mem::align_of::<$table>(),
                fields: &[
                    ("lifecycle: Lifecycle", ::std::mem::offset_of!($table, lifecycle)),
                    (
                        concat!(stringify!($create), ": fn(", $(stringify!($create_arg), ", ",)* ")"),
                        ::std::mem::offset_of!($table, $create),
                    ),
                    $((
                        concat!(stringify!($field), ": fn(", $(stringify!($arg), ", ",)* ")"),
                        ::std::mem::offset_of!($table, $field),
                    ),)*
                ],
            };
            const CONTRACT: $crate::guest::ContractStamp = $crate::guest::ContractStamp::of(
                $crate::guest::GuestKind::$kind,
                &[<Self as $crate::guest::Entries>::LAYOUT],
            )
            $(.with_setting($setting, $on))*;

            fn lifecycle(&self) -> &$crate::guest::Lifecycle {
                &self.lifecycle
            }
        }
    };
}

/// What each boundary's tests assert of its table, `E`: its lifecycle first, one entry per field after it and nothing else, each hashed with its signature, and a library of the other kind refused as that kind.
#[doc(hidden)]
pub fn check_entry_table<E: Entries>() {
    let layout = E::LAYOUT;
    assert_eq!(
        layout.fields.first(),
        Some(&("lifecycle: Lifecycle", 0)),
        "the lifecycle comes first"
    );
    let entry = std::mem::size_of::<unsafe extern "C" fn() -> CallOutcome>();
    assert_eq!(
        std::mem::size_of::<Lifecycle>() + (layout.fields.len() - 1) * entry,
        layout.size,
        "one field per entry, and nothing else in the table"
    );
    for (field, _) in &layout.fields[1..] {
        assert!(
            field.contains(": fn("),
            "{field} is hashed with what it takes"
        );
    }
    let kind = GuestKind::from_code(E::CONTRACT.kind).expect("a kind this telar knows");
    let other = [GuestKind::HotReload, GuestKind::Embed]
        .into_iter()
        .find(|other| *other != kind)
        .expect("two kinds");
    let foreign = ContractStamp {
        kind: other.code(),
        ..E::CONTRACT
    };
    let refused = unsafe { E::CONTRACT.accept(&foreign) }.expect_err("the other kind is refused");
    assert_eq!(refused.to_string(), format!("this is {other}, not {kind}"));
    assert_eq!(unsafe { E::CONTRACT.accept(&E::CONTRACT) }, Ok(()));
}

/// Why a library could not be loaded as a guest.
#[cfg(all(feature = "dylib", not(target_arch = "wasm32")))]
#[derive(Debug, thiserror::Error)]
pub enum OpenGuestError {
    #[error("could not load the library: {0}")]
    Open(Box<dyn std::error::Error>),
    #[error(
        "the library exports no guest contract, so it is no telar guest or was built against a telar older than this host: {0}"
    )]
    Unstamped(libloading::Error),
    #[error(transparent)]
    Mismatch(#[from] ContractMismatch),
    #[error("the library is stamped as a guest but exports no entry table: {0}")]
    NoEntries(libloading::Error),
}

/// Loads a private copy of the library at `path` and copies out its `E` table, refusing the library unless its [`ContractStamp`] is `E`'s before anything else of it is read or called: the one way a library is loaded as a guest.
///
/// Each load opens a copy of its own, since dlopen hands an earlier load of the same file back, statics and all. A matching stamp proves the two builds agree on the contract, not that the library is well behaved: it is still foreign code in this process.
///
/// # Safety
///
/// Loading runs the library's initializers, and everything it exports is trusted once the stamp matches, so `path` must name a guest built from a telar this host trusts. Whatever comes out of the library — its entries, handles, lent bytes, wakers it made — must be released or dropped before the library is, and the library unmapped only through [`Guest::unload`](super::Guest::unload).
#[cfg(all(feature = "dylib", not(target_arch = "wasm32")))]
pub unsafe fn open_guest<E: Entries>(
    path: &std::path::Path,
) -> Result<(E, libloading::Library), OpenGuestError> {
    let library = unsafe { super::open_unique(path) }.map_err(OpenGuestError::Open)?;
    let stamp = unsafe { library.get::<*const ContractStamp>(CONTRACT_SYMBOL) }
        .map_err(OpenGuestError::Unstamped)?;
    unsafe { E::CONTRACT.accept(*stamp) }?;
    let entries = unsafe {
        **library
            .get::<*const E>(E::SYMBOL)
            .map_err(OpenGuestError::NoEntries)?
    };
    Ok((entries, library))
}

#[cfg(test)]
#[path = "contract_test.rs"]
mod tests;
