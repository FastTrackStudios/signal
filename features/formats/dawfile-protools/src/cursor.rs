//! Endian-aware byte buffer reader.
//!
//! Pro Tools files can be little-endian or big-endian. This module provides
//! a zero-copy cursor that reads multi-byte integers with the correct byte
//! order, plus helpers for the variable-width encoding used in region data.

/// A zero-copy cursor into a byte buffer with endian awareness.
#[derive(Debug, Clone, Copy)]
pub struct Cursor<'a> {
    data: &'a [u8],
    is_bigendian: bool,
}

impl<'a> Cursor<'a> {
    /// Create a new cursor over the given data.
    pub fn new(data: &'a [u8], is_bigendian: bool) -> Self {
        Self { data, is_bigendian }
    }

    /// Read a single byte at the given offset.
    pub fn u8_at(&self, offset: usize) -> u8 {
        self.data[offset]
    }

    /// Read a u16 at the given offset, respecting endianness.
    pub fn u16_at(&self, offset: usize) -> u16 {
        let bytes = [self.data[offset], self.data[offset + 1]];
        if self.is_bigendian {
            u16::from_be_bytes(bytes)
        } else {
            u16::from_le_bytes(bytes)
        }
    }

    /// Read a u32 at the given offset, respecting endianness.
    pub fn u32_at(&self, offset: usize) -> u32 {
        let bytes = [
            self.data[offset],
            self.data[offset + 1],
            self.data[offset + 2],
            self.data[offset + 3],
        ];
        if self.is_bigendian {
            u32::from_be_bytes(bytes)
        } else {
            u32::from_le_bytes(bytes)
        }
    }

    /// Read a u64 at the given offset, respecting endianness.
    pub fn u64_at(&self, offset: usize) -> u64 {
        let bytes = [
            self.data[offset],
            self.data[offset + 1],
            self.data[offset + 2],
            self.data[offset + 3],
            self.data[offset + 4],
            self.data[offset + 5],
            self.data[offset + 6],
            self.data[offset + 7],
        ];
        if self.is_bigendian {
            u64::from_be_bytes(bytes)
        } else {
            u64::from_le_bytes(bytes)
        }
    }

    /// Read a variable-width integer (1-5 bytes) in **little-endian** order.
    ///
    /// Pro Tools uses LE for the three-point values regardless of file endianness.
    pub fn var_int_le(&self, offset: usize, n_bytes: usize) -> u64 {
        let mut value: u64 = 0;
        for i in 0..n_bytes.min(5) {
            value |= (self.data[offset + i] as u64) << (8 * i);
        }
        value
    }

    /// Read a 5-byte (40-bit) integer in little-endian order.
    pub fn u40_le(&self, offset: usize) -> u64 {
        self.var_int_le(offset, 5)
    }

    /// Read a length-prefixed string at the given offset.
    ///
    /// The length is a u32 (endian-aware), followed by that many UTF-8 bytes.
    /// Returns `(string, total_bytes_consumed)` where total includes the 4-byte
    /// length prefix.
    pub fn length_prefixed_string(&self, offset: usize) -> (String, usize) {
        let len = self.u32_at(offset) as usize;
        let start = offset + 4;
        let end = (start + len).min(self.data.len());
        let s = String::from_utf8_lossy(&self.data[start..end]).into_owned();
        (s, 4 + len)
    }

    /// Get the underlying data slice.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    /// Get the total length of the underlying buffer.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Return true when the underlying buffer has no bytes.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Whether this cursor uses big-endian byte order.
    pub fn is_bigendian(&self) -> bool {
        self.is_bigendian
    }
}

/// Parse the "three-point" variable-width position encoding.
///
/// At position `j` in the buffer, there is a 5-byte descriptor that encodes
/// the byte widths of three values (start, offset, length) in nibbles.
///
/// Returns `(start, sample_offset, length)` and the number of bytes consumed.
pub fn parse_three_point(cursor: &Cursor<'_>, j: usize) -> (u64, u64, u64) {
    // The nibble layout depends on endianness
    let (offset_bytes, length_bytes, start_bytes) = if !cursor.is_bigendian() {
        (
            (cursor.u8_at(j + 1) >> 4) as usize,
            (cursor.u8_at(j + 2) >> 4) as usize,
            (cursor.u8_at(j + 3) >> 4) as usize,
        )
    } else {
        (
            (cursor.u8_at(j + 4) >> 4) as usize,
            (cursor.u8_at(j + 3) >> 4) as usize,
            (cursor.u8_at(j + 2) >> 4) as usize,
        )
    };

    // Values are read sequentially in LE, starting at j+5
    let mut pos = j + 5;

    let sample_offset = cursor.var_int_le(pos, offset_bytes);
    pos += offset_bytes;

    let length = cursor.var_int_le(pos, length_bytes);
    pos += length_bytes;

    let start = cursor.var_int_le(pos, start_bytes);

    (start, sample_offset, length)
}
