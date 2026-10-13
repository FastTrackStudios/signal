//! AAF property stream encoder and CFB index stream builders.
//!
//! [`PropWriter`] accumulates typed property entries and serialises them to
//! the binary `properties` stream format used by every AAF/CFB object.
//!
//! # Property stream layout
//! ```text
//! [0..2]  u16 LE  BOM (0xFFFE = little-endian)
//! [2..4]  u16 LE  version (0x0001)
//! [4..]   entries:
//!   [+0..2]  u16 LE  PID
//!   [+2..4]  u16 LE  stored form
//!   [+4..8]  u32 LE  byte count
//!   [+8..]   byte[count]  value
//! ```

use crate::parse::auid::{Auid, MobId};
use crate::types::EditRate;

// ─── Stored form constants ────────────────────────────────────────────────────

const SF_DATA: u16 = 0x0002;
const SF_STRONG_REF: u16 = 0x0022;
const SF_STRONG_REF_VECTOR: u16 = 0x0023;
const SF_STRONG_REF_SET: u16 = 0x0024;

// ─── PropWriter ──────────────────────────────────────────────────────────────

/// Builds the binary `properties` stream for a single AAF object.
#[derive(Default)]
pub struct PropWriter {
    entries: Vec<(u16, u16, Vec<u8>)>, // (pid, stored_form, value_bytes)
}

impl PropWriter {
    pub fn new() -> Self {
        Self::default()
    }

    // ── Class AUID ───────────────────────────────────────────────────────────

    /// Write the `PID_OBJ_CLASS` (0x0101) property — every AAF object needs this.
    pub fn class_id(&mut self, auid: Auid) -> &mut Self {
        self.raw(0x0101, SF_DATA, &auid.0)
    }

    // ── SF_DATA scalars ──────────────────────────────────────────────────────

    /// Raw SF_DATA property with an arbitrary byte slice.
    pub fn data(&mut self, pid: u16, value: &[u8]) -> &mut Self {
        self.raw(pid, SF_DATA, value)
    }

    pub fn u8_prop(&mut self, pid: u16, v: u8) -> &mut Self {
        self.raw(pid, SF_DATA, &[v])
    }

    pub fn u16_prop(&mut self, pid: u16, v: u16) -> &mut Self {
        self.raw(pid, SF_DATA, &v.to_le_bytes())
    }

    pub fn u32_prop(&mut self, pid: u16, v: u32) -> &mut Self {
        self.raw(pid, SF_DATA, &v.to_le_bytes())
    }

    pub fn i64_prop(&mut self, pid: u16, v: i64) -> &mut Self {
        self.raw(pid, SF_DATA, &v.to_le_bytes())
    }

    /// 16-byte AUID as SF_DATA.
    pub fn auid_prop(&mut self, pid: u16, auid: Auid) -> &mut Self {
        self.raw(pid, SF_DATA, &auid.0)
    }

    /// 32-byte Mob ID (UMID) as SF_DATA.
    pub fn mob_id_prop(&mut self, pid: u16, mob_id: MobId) -> &mut Self {
        self.raw(pid, SF_DATA, &mob_id.0)
    }

    /// Rational edit rate: `i32 numerator || i32 denominator` (LE).
    pub fn edit_rate_prop(&mut self, pid: u16, rate: EditRate) -> &mut Self {
        let mut v = [0u8; 8];
        v[0..4].copy_from_slice(&rate.numerator.to_le_bytes());
        v[4..8].copy_from_slice(&rate.denominator.to_le_bytes());
        self.raw(pid, SF_DATA, &v)
    }

    /// UTF-16LE string property (null-terminated).
    pub fn string_prop(&mut self, pid: u16, s: &str) -> &mut Self {
        let mut encoded: Vec<u8> = s.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
        encoded.extend_from_slice(&[0u8, 0u8]); // null terminator
        self.raw(pid, SF_DATA, &encoded)
    }

    // ── Strong reference properties ──────────────────────────────────────────

    /// SF_STRONG_OBJECT_REFERENCE — single child.
    ///
    /// `child_name` must match the CFB storage name immediately below this
    /// object's directory.
    pub fn strong_ref(&mut self, pid: u16, child_name: &str) -> &mut Self {
        let mut v: Vec<u8> = child_name.bytes().collect();
        v.push(0); // null terminator
        self.entries.push((pid, SF_STRONG_REF, v));
        self
    }

    /// SF_STRONG_OBJECT_REFERENCE_VECTOR — ordered collection.
    ///
    /// `coll_name` is the name of the collection storage (which must contain
    /// an `index` stream and hex-keyed child storages).
    pub fn strong_ref_vector(&mut self, pid: u16, coll_name: &str, count: u32) -> &mut Self {
        let mut v = Vec::with_capacity(8 + coll_name.len() + 1);
        v.extend_from_slice(&count.to_le_bytes());
        v.extend_from_slice(&count.to_le_bytes()); // first_free = count
        v.extend_from_slice(coll_name.as_bytes());
        v.push(0); // null terminator
        self.entries.push((pid, SF_STRONG_REF_VECTOR, v));
        self
    }

    /// SF_STRONG_OBJECT_REFERENCE_SET — unordered collection.
    pub fn strong_ref_set(&mut self, pid: u16, coll_name: &str, count: u32) -> &mut Self {
        let mut v = Vec::with_capacity(8 + coll_name.len() + 1);
        v.extend_from_slice(&count.to_le_bytes());
        v.extend_from_slice(&count.to_le_bytes()); // first_free = count
        v.extend_from_slice(coll_name.as_bytes());
        v.push(0); // null terminator
        self.entries.push((pid, SF_STRONG_REF_SET, v));
        self
    }

    // ── Finalise ─────────────────────────────────────────────────────────────

    /// Serialise all accumulated entries to raw `properties` stream bytes.
    pub fn finish(self) -> Vec<u8> {
        let total: usize = self
            .entries
            .iter()
            .map(|(_, _, v)| 8 + v.len())
            .sum::<usize>()
            + 4; // BOM + version header
        let mut out = Vec::with_capacity(total);

        // BOM: 0xFFFE (little-endian AAF)
        out.extend_from_slice(&[0xFE, 0xFF]);
        // Version 0x0001
        out.extend_from_slice(&[0x01, 0x00]);

        for (pid, sf, value) in self.entries {
            out.extend_from_slice(&pid.to_le_bytes());
            out.extend_from_slice(&sf.to_le_bytes());
            out.extend_from_slice(&(value.len() as u32).to_le_bytes());
            out.extend_from_slice(&value);
        }

        out
    }

    // ── Internal ─────────────────────────────────────────────────────────────

    fn raw(&mut self, pid: u16, sf: u16, value: &[u8]) -> &mut Self {
        self.entries.push((pid, sf, value.to_vec()));
        self
    }
}

// ─── Index stream builders ────────────────────────────────────────────────────

/// Build a vector index stream: `[count: u32] [key: u32] × N`.
///
/// Keys are assigned sequentially starting at 0.
pub fn vector_index(count: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + count as usize * 4);
    out.extend_from_slice(&count.to_le_bytes());
    for i in 0..count {
        out.extend_from_slice(&i.to_le_bytes());
    }
    out
}

/// Build a set index stream: `[count: u32] [key: u32 + class_auid(16)] × N`.
///
/// Keys are assigned sequentially starting at 0.  `class_auids` provides the
/// 16-byte class AUID for each element (used by readers to look them up).
pub fn set_index(class_auids: &[Auid]) -> Vec<u8> {
    let count = class_auids.len() as u32;
    let mut out = Vec::with_capacity(4 + class_auids.len() * 20);
    out.extend_from_slice(&count.to_le_bytes());
    for (i, auid) in class_auids.iter().enumerate() {
        out.extend_from_slice(&(i as u32).to_le_bytes());
        out.extend_from_slice(&auid.0);
    }
    out
}
