//! The primitives a frame is spelled in: little-endian scalars, LEB128 lengths and length-prefixed UTF-8.

use super::WireError;

#[derive(Default)]
pub(crate) struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    pub(crate) fn clear(&mut self) {
        self.bytes.clear();
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub(crate) fn bool(&mut self, value: bool) {
        self.u8(value as u8);
    }

    pub(crate) fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(crate) fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(crate) fn f32(&mut self, value: f32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(crate) fn f64(&mut self, value: f64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(crate) fn varint(&mut self, mut value: u64) {
        while value >= 0x80 {
            self.bytes.push(value as u8 | 0x80);
            value >>= 7;
        }
        self.bytes.push(value as u8);
    }

    pub(crate) fn len(&mut self, len: usize) {
        self.varint(len as u64);
    }

    pub(crate) fn str(&mut self, text: &str) {
        self.len(text.len());
        self.bytes.extend_from_slice(text.as_bytes());
    }

    pub(crate) fn raw(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }
}

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub(crate) fn remaining(&self) -> usize {
        self.bytes.len()
    }

    pub(crate) fn raw(&mut self, len: usize) -> Result<&'a [u8], WireError> {
        let (head, rest) = self
            .bytes
            .split_at_checked(len)
            .ok_or(WireError::Truncated)?;
        self.bytes = rest;
        Ok(head)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], WireError> {
        let (head, rest) = self
            .bytes
            .split_first_chunk::<N>()
            .ok_or(WireError::Truncated)?;
        self.bytes = rest;
        Ok(*head)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, WireError> {
        self.array::<1>().map(|[byte]| byte)
    }

    pub(crate) fn bool(&mut self) -> Result<bool, WireError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            tag => Err(WireError::Tag { of: "bool", tag }),
        }
    }

    pub(crate) fn u16(&mut self) -> Result<u16, WireError> {
        self.array().map(u16::from_le_bytes)
    }

    pub(crate) fn u64(&mut self) -> Result<u64, WireError> {
        self.array().map(u64::from_le_bytes)
    }

    pub(crate) fn f32(&mut self) -> Result<f32, WireError> {
        self.array().map(f32::from_le_bytes)
    }

    pub(crate) fn f64(&mut self) -> Result<f64, WireError> {
        self.array().map(f64::from_le_bytes)
    }

    pub(crate) fn varint(&mut self) -> Result<u64, WireError> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = self.u8()?;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(WireError::Invalid("varint longer than 64 bits"))
    }

    pub(crate) fn varint_u32(&mut self) -> Result<u32, WireError> {
        u32::try_from(self.varint()?).map_err(|_| WireError::Invalid("varint wider than 32 bits"))
    }

    // Every element a count announces takes at least one byte, so a count beyond what is left is malformed and is refused before anything is reserved for it.
    pub(crate) fn len(&mut self) -> Result<usize, WireError> {
        let len = usize::try_from(self.varint()?).map_err(|_| WireError::Truncated)?;
        if len > self.bytes.len() {
            return Err(WireError::Truncated);
        }
        Ok(len)
    }

    pub(crate) fn str(&mut self) -> Result<&'a str, WireError> {
        let len = self.len()?;
        std::str::from_utf8(self.raw(len)?).map_err(|_| WireError::Invalid("text is not UTF-8"))
    }
}

#[cfg(test)]
#[path = "bytes_test.rs"]
mod tests;
