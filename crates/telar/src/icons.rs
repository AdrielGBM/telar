//! The icon-set vocabulary `telar-icons` is written in and republishes: icon ids, the Iconify format, the sources an icon resolves through and the licence terms a set declares.

pub use icons_core::{
    Author, Icon, IconError, IconId, IconSet, IconSource, IconifyDir, License, SetInfo,
    SourcedIcon, Sources, SvgDir, uses_current_color,
};
