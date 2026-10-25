//! How the standard types every value is built from are spelled: scalars, text, paths, options, results, sequences, ranges and durations.

use std::borrow::Cow;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::{Reader, Wire, WireError, Writer};

macro_rules! wire_scalar {
    ($($ty:ty => $put:ident / $take:ident),+ $(,)?) => {
        $(impl Wire for $ty {
            fn put(&self, w: &mut Writer) {
                w.$put(*self);
            }
            fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
                r.$take()
            }
        })+
    };
}

wire_scalar!(
    bool => bool / bool,
    u8 => u8 / u8,
    u16 => u16 / u16,
    i32 => i32 / i32,
    u64 => u64 / u64,
    f32 => f32 / f32,
    f64 => f64 / f64,
);

impl Wire for u32 {
    fn put(&self, w: &mut Writer) {
        w.varint(u64::from(*self));
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        r.varint_u32()
    }
}

impl Wire for usize {
    fn put(&self, w: &mut Writer) {
        w.varint(*self as u64);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        usize::try_from(r.varint()?).map_err(|_| WireError::Invalid("a count wider than usize"))
    }
}

impl Wire for char {
    fn put(&self, w: &mut Writer) {
        u32::from(*self).put(w);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        char::from_u32(u32::take(r)?).ok_or(WireError::Invalid("a char that is no Unicode scalar"))
    }
}

impl Wire for String {
    fn put(&self, w: &mut Writer) {
        w.str(self);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        r.str().map(str::to_owned)
    }
}

impl Wire for Box<str> {
    fn put(&self, w: &mut Writer) {
        w.str(self);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        r.str().map(Box::from)
    }
}

impl Wire for Arc<str> {
    fn put(&self, w: &mut Writer) {
        w.str(self);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        r.str().map(Arc::from)
    }
}

impl Wire for Cow<'static, str> {
    fn put(&self, w: &mut Writer) {
        w.str(self);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        r.str().map(|text| Cow::Owned(text.to_owned()))
    }
}

impl Wire for () {
    fn put(&self, _: &mut Writer) {}
    fn take(_: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok(())
    }
}

/// Spelled as the platform keeps it, so a path that is no UTF-8 crosses intact: both sides of a boundary run on the one platform.
impl Wire for PathBuf {
    #[cfg(unix)]
    fn put(&self, w: &mut Writer) {
        use std::os::unix::ffi::OsStrExt;
        w.bytes(self.as_os_str().as_bytes());
    }
    #[cfg(unix)]
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        use std::os::unix::ffi::OsStrExt;
        Ok(PathBuf::from(std::ffi::OsStr::from_bytes(r.bytes()?)))
    }

    #[cfg(windows)]
    fn put(&self, w: &mut Writer) {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = self.as_os_str().encode_wide().collect();
        wide.put(w);
    }
    #[cfg(windows)]
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        use std::os::windows::ffi::OsStringExt;
        Ok(PathBuf::from(std::ffi::OsString::from_wide(
            &Vec::<u16>::take(r)?,
        )))
    }

    #[cfg(not(any(unix, windows)))]
    fn put(&self, w: &mut Writer) {
        w.str(&self.to_string_lossy());
    }
    #[cfg(not(any(unix, windows)))]
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        r.str().map(PathBuf::from)
    }
}

impl<T: Wire, E: Wire> Wire for Result<T, E> {
    fn put(&self, w: &mut Writer) {
        match self {
            Ok(value) => {
                w.u8(0);
                value.put(w);
            }
            Err(error) => {
                w.u8(1);
                error.put(w);
            }
        }
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        match r.u8()? {
            0 => T::take(r).map(Ok),
            1 => E::take(r).map(Err),
            tag => Err(WireError::Tag { of: "Result", tag }),
        }
    }
}

impl<T: Wire> Wire for Option<T> {
    fn put(&self, w: &mut Writer) {
        match self {
            None => w.u8(0),
            Some(value) => {
                w.u8(1);
                value.put(w);
            }
        }
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        match r.u8()? {
            0 => Ok(None),
            1 => T::take(r).map(Some),
            tag => Err(WireError::Tag { of: "Option", tag }),
        }
    }
}

impl<T: Wire> Wire for Box<T> {
    fn put(&self, w: &mut Writer) {
        T::put(self, w);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        T::take(r).map(Box::new)
    }
}

impl<T: Wire> Wire for Arc<T> {
    fn put(&self, w: &mut Writer) {
        T::put(self, w);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        T::take(r).map(Arc::new)
    }
}

fn put_all<T: Wire>(w: &mut Writer, items: &[T]) {
    w.len(items.len());
    for item in items {
        item.put(w);
    }
}

impl<T: Wire> Wire for Vec<T> {
    fn put(&self, w: &mut Writer) {
        put_all(w, self);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        let len = r.len()?;
        (0..len).map(|_| T::take(r)).collect()
    }
}

impl<T: Wire> Wire for Arc<[T]> {
    fn put(&self, w: &mut Writer) {
        put_all(w, self);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        let len = r.len()?;
        (0..len).map(|_| T::take(r)).collect()
    }
}

impl<T: Wire, const N: usize> Wire for [T; N] {
    fn put(&self, w: &mut Writer) {
        for item in self {
            item.put(w);
        }
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        let items: Vec<T> = (0..N).map(|_| T::take(r)).collect::<Result<_, _>>()?;
        items
            .try_into()
            .map_err(|_| WireError::Invalid("fixed array length"))
    }
}

impl<A: Wire, B: Wire> Wire for (A, B) {
    fn put(&self, w: &mut Writer) {
        self.0.put(w);
        self.1.put(w);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok((A::take(r)?, B::take(r)?))
    }
}

impl<T: Wire> Wire for Range<T> {
    fn put(&self, w: &mut Writer) {
        self.start.put(w);
        self.end.put(w);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok(T::take(r)?..T::take(r)?)
    }
}

impl Wire for Duration {
    fn put(&self, w: &mut Writer) {
        w.varint(self.as_secs());
        self.subsec_nanos().put(w);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        let secs = r.varint()?;
        let nanos = u32::take(r)?;
        if nanos >= 1_000_000_000 {
            return Err(WireError::Invalid(
                "a duration with a second of nanoseconds or more",
            ));
        }
        Ok(Duration::new(secs, nanos))
    }
}
