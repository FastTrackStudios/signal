//! AAF property stream decoder.
//!
//! Every AAF object stores its property values in a CFB stream named
//! `"properties"`.  This module parses both the standard SMPTE format and the
//! Avid internal format into a typed [`Properties`] map keyed by PID.
//!
//! # Standard property stream layout (BOM = 0xFFFE)
//!
//! ```text
//! [0..2]   u16 LE  byte-order mark  (0xFFFE = little-endian)
//! [2..4]   u16 LE  version          (expect 0x0001)
//! [4..]    entries (repeated until end of stream):
//!   [+0..2]  u16 LE  PID
//!   [+2..4]  u16 LE  stored form
//!   [+4..8]  u32 LE  byte count
//!   [+8..]   bytes[byte_count]  value
//! ```
//!
//! # Avid internal format (BOM = 0x204C)
//!
//! Used by Avid Media Composer, Pro Tools, Adobe Premiere, DaVinci Resolve,
//! and many other tools.
//!
//! ```text
//! [0..2]   u16 LE  byte-order mark  (0x204C)
//! [2..4]   u16 LE  entry count
//! [4..]    entries:
//!   [+0..2]  u16 LE  PID
//!   [+2..4]  u16 LE  stored form
//!   [+4..6]  u16 LE  byte count  (2 bytes, not 4!)
//!   [+6..]   bytes[byte_count]  value
//! ```
//!
//! Stored form codes for Avid format:
//! - `0x0082` — SF_DATA (raw typed bytes — same role as 0x0002 in standard)
//! - `0x0022` — SF_STRONG_OBJECT_REFERENCE (same code point as standard)
//! - `0x0032` — SF_STRONG_OBJECT_REFERENCE_VECTOR
//! - `0x003A` — SF_STRONG_OBJECT_REFERENCE_SET
//!
//! Strong ref values in Avid format are **UTF-16LE** encoded child directory
//! names (null-terminated), not ASCII.  Collection strong-ref values are just
//! the UTF-16LE base name with no count prefix.

use crate::error::{AafError, AafResult};
use crate::parse::auid::{Auid, MobId};
use crate::types::EditRate;
use std::path::Path;

// ─── StoredForm ──────────────────────────────────────────────────────────────

/// How a property value is stored in the `properties` stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredForm {
    Data,
    DataStream,
    StrongRef,
    StrongRefVector,
    StrongRefSet,
    WeakRef,
    WeakRefVector,
    WeakRefSet,
    Unknown(u16),
}

/// Map a raw stored-form code to [`StoredForm`], with format-aware handling.
///
/// Avid format reuses some code points differently from standard AAF.
fn parse_stored_form(code: u16, is_avid: bool) -> StoredForm {
    if is_avid {
        match code {
            0x0002 => StoredForm::Data, // DataStream in standard; Data in Avid
            0x0082 => StoredForm::Data,
            0x0022 => StoredForm::StrongRef,
            0x0032 => StoredForm::StrongRefVector,
            0x003A => StoredForm::StrongRefSet,
            other => StoredForm::Unknown(other),
        }
    } else {
        match code {
            0x0002 => StoredForm::Data,
            0x0082 => StoredForm::DataStream,
            0x0022 => StoredForm::StrongRef,
            0x0023 => StoredForm::StrongRefVector,
            0x0024 => StoredForm::StrongRefSet,
            0x0032 => StoredForm::WeakRef,
            0x0033 => StoredForm::WeakRefVector,
            0x0034 => StoredForm::WeakRefSet,
            other => StoredForm::Unknown(other),
        }
    }
}

// ─── PropertyEntry ───────────────────────────────────────────────────────────

/// A single property decoded from the stream.
#[derive(Debug)]
pub struct PropertyEntry {
    /// Property identifier.
    pub pid: u16,
    /// How the value is stored.
    pub stored_form: StoredForm,
    /// Raw value bytes (owned; length given by the stream's byte_count field).
    pub value: Vec<u8>,
}

// ─── Properties ──────────────────────────────────────────────────────────────

/// Decoded set of properties for a single AAF object.
#[derive(Debug)]
pub struct Properties {
    entries: Vec<PropertyEntry>,
    /// True when parsed from an Avid-format property stream (BOM = 0x204C).
    pub is_avid: bool,
}

impl Properties {
    // ─── Parse ───────────────────────────────────────────────────────────────

    /// Decode the raw `properties` stream bytes for the object at `dir_path`.
    pub fn parse(data: &[u8], dir_path: &Path) -> AafResult<Self> {
        if data.len() < 4 {
            return Err(AafError::TruncatedStream {
                offset: 0,
                path: dir_path.to_path_buf(),
            });
        }

        let bom = u16::from_le_bytes([data[0], data[1]]);
        let is_avid = match bom {
            0xFFFE => false,
            0x204C => true,
            _ => {
                return Err(AafError::UnsupportedByteOrder {
                    bom,
                    path: dir_path.to_path_buf(),
                });
            }
        };

        let mut entries = Vec::new();

        if is_avid {
            // Avid split format:
            // [2..4]  u16 LE  entry count
            // [4..]   count × (PID:2 + SF:2 + SIZE:2)   — all headers first
            // [4 + count*6 ..]  value1 || value2 || ...  — all values after
            let count = u16::from_le_bytes([data[2], data[3]]) as usize;
            let headers_end = 4 + count * 6;
            if data.len() < headers_end {
                return Err(AafError::TruncatedStream {
                    offset: 4,
                    path: dir_path.to_path_buf(),
                });
            }

            // Phase 1: collect all headers.
            let mut header_list: Vec<(u16, StoredForm, usize)> = Vec::with_capacity(count);
            for i in 0..count {
                let off = 4 + i * 6;
                let pid = u16::from_le_bytes([data[off], data[off + 1]]);
                let sf_code = u16::from_le_bytes([data[off + 2], data[off + 3]]);
                let sf = parse_stored_form(sf_code, true);
                let size = u16::from_le_bytes([data[off + 4], data[off + 5]]) as usize;
                header_list.push((pid, sf, size));
            }

            // Phase 2: read values sequentially from the values section.
            let mut val_pos = headers_end;
            for (pid, sf, size) in header_list {
                if val_pos + size > data.len() {
                    return Err(AafError::TruncatedStream {
                        offset: val_pos,
                        path: dir_path.to_path_buf(),
                    });
                }
                let value = data[val_pos..val_pos + size].to_vec();
                val_pos += size;
                entries.push(PropertyEntry {
                    pid,
                    stored_form: sf,
                    value,
                });
            }
        } else {
            // Standard interleaved format:
            // [2..4]  u16 LE  version (ignored)
            // [4..]   repeated until end: PID(2) + SF(2) + SIZE(4) + value(SIZE)
            let mut pos = 4usize;
            while pos + 8 <= data.len() {
                let pid = u16::from_le_bytes([data[pos], data[pos + 1]]);
                let sf_code = u16::from_le_bytes([data[pos + 2], data[pos + 3]]);
                let sf = parse_stored_form(sf_code, false);
                let byte_count = u32::from_le_bytes([
                    data[pos + 4],
                    data[pos + 5],
                    data[pos + 6],
                    data[pos + 7],
                ]) as usize;
                pos += 8;

                if pos + byte_count > data.len() {
                    return Err(AafError::TruncatedStream {
                        offset: pos,
                        path: dir_path.to_path_buf(),
                    });
                }

                let value = data[pos..pos + byte_count].to_vec();
                pos += byte_count;
                entries.push(PropertyEntry {
                    pid,
                    stored_form: sf,
                    value,
                });
            }
        }

        Ok(Self { entries, is_avid })
    }

    // ─── Raw access ──────────────────────────────────────────────────────────

    /// Find the first entry with the given PID.
    pub fn get(&self, pid: u16) -> Option<&PropertyEntry> {
        self.entries.iter().find(|e| e.pid == pid)
    }

    /// Iterate over all property entries.
    pub fn iter(&self) -> impl Iterator<Item = &PropertyEntry> {
        self.entries.iter()
    }

    // ─── Class AUID ──────────────────────────────────────────────────────────

    /// The class AUID of this object (`PID_OBJ_CLASS = 0x0101`).
    ///
    /// Returns `None` for objects parsed from Avid-format streams, since Avid
    /// does not store `PID_OBJ_CLASS` in its property streams.  Use
    /// [`Self::effective_class`] to get an inferred class when the stored AUID
    /// is absent.
    pub fn class_auid(&self) -> Option<Auid> {
        self.auid(super::pids::PID_OBJ_CLASS)
    }

    /// Infer the class AUID from PID presence when `PID_OBJ_CLASS` is absent.
    ///
    /// Uses a priority-ordered set of discriminating PIDs unique to each class.
    pub fn infer_class(&self) -> Option<Auid> {
        use super::auid::*;
        use super::pids::*;

        // SourceMob: has EssenceDescriptor strong ref
        if self.get(PID_SOURCE_MOB_ESSENCE_DESCRIPTION).is_some() {
            return Some(CLASS_SOURCE_MOB);
        }
        // Audio essence descriptors (SoundDescriptor / PCMDescriptor family)
        if self.get(PID_SOUND_DESCRIPTOR_AUDIO_SAMPLING_RATE).is_some()
            || self.get(PID_SOUND_DESCRIPTOR_CHANNELS).is_some()
            || self.get(PID_PCM_DESCRIPTOR_BLOCK_ALIGN).is_some()
        {
            return Some(CLASS_PCM_DESCRIPTOR);
        }
        // Locators
        if self.get(PID_NETWORK_LOCATOR_URL).is_some() {
            return Some(CLASS_NETWORK_LOCATOR);
        }
        if self.get(PID_TEXT_LOCATOR_NAME).is_some() {
            return Some(CLASS_TEXT_LOCATOR);
        }
        // MobSlot subclasses
        if self.get(PID_TIMELINE_MOB_SLOT_EDIT_RATE).is_some() {
            return Some(CLASS_TIMELINE_MOB_SLOT);
        }
        if self.get(PID_EVENT_MOB_SLOT_EDIT_RATE).is_some() {
            return Some(CLASS_EVENT_MOB_SLOT);
        }
        // Segment subclasses
        if self.get(PID_SEQUENCE_COMPONENTS).is_some() {
            return Some(CLASS_SEQUENCE);
        }
        if self.get(PID_SOURCE_CLIP_START_POSITION).is_some() {
            return Some(CLASS_SOURCE_CLIP);
        }
        if self.get(PID_TIMECODE_START).is_some() {
            return Some(CLASS_TIMECODE);
        }
        if self.get(PID_TRANSITION_CUT_POINT).is_some()
            || self.get(PID_TRANSITION_OPERATION_GROUP).is_some()
        {
            return Some(CLASS_TRANSITION);
        }
        if self.get(PID_OPERATION_GROUP_OPERATION).is_some()
            || self.get(PID_OPERATION_GROUP_INPUT_SEGMENTS).is_some()
        {
            return Some(CLASS_OPERATION_GROUP);
        }
        if self.get(PID_EVENT_POSITION).is_some() || self.get(PID_EVENT_POSITION_AVID).is_some() {
            return Some(CLASS_COMMENT_MARKER);
        }
        None
    }

    /// Return the stored class AUID if present, otherwise try to infer it from
    /// PID presence (needed for Avid-format streams that omit `PID_OBJ_CLASS`).
    pub fn effective_class(&self) -> Option<Auid> {
        self.class_auid().or_else(|| self.infer_class())
    }

    // ─── Scalar accessors ────────────────────────────────────────────────────

    pub fn u8(&self, pid: u16) -> Option<u8> {
        let v = &self.get(pid)?.value;
        if !v.is_empty() { Some(v[0]) } else { None }
    }

    pub fn u16_le(&self, pid: u16) -> Option<u16> {
        let v = &self.get(pid)?.value;
        if v.len() >= 2 {
            Some(u16::from_le_bytes([v[0], v[1]]))
        } else {
            None
        }
    }

    pub fn i32_le(&self, pid: u16) -> Option<i32> {
        let v = &self.get(pid)?.value;
        if v.len() >= 4 {
            Some(i32::from_le_bytes([v[0], v[1], v[2], v[3]]))
        } else {
            None
        }
    }

    pub fn u32_le(&self, pid: u16) -> Option<u32> {
        let v = &self.get(pid)?.value;
        if v.len() >= 4 {
            Some(u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
        } else {
            None
        }
    }

    pub fn i64_le(&self, pid: u16) -> Option<i64> {
        let v = &self.get(pid)?.value;
        if v.len() >= 8 {
            Some(i64::from_le_bytes([
                v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7],
            ]))
        } else {
            None
        }
    }

    pub fn u64_le(&self, pid: u16) -> Option<u64> {
        let v = &self.get(pid)?.value;
        if v.len() >= 8 {
            Some(u64::from_le_bytes([
                v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7],
            ]))
        } else {
            None
        }
    }

    // ─── Compound accessors ──────────────────────────────────────────────────

    /// Read a 16-byte AUID from a `SF_DATA` property.
    ///
    /// Also accepts `SF_WEAK_OBJECT_REFERENCE` since many DataDefinition and
    /// similar weak refs store just the 16-byte AUID in the value field.
    ///
    /// In Avid-format files, DataDefinition values are 21 bytes:
    /// `[5-byte header][16-byte AUID]`.  This method extracts the AUID from
    /// bytes `5..21` when the value is 21 bytes and the stream is Avid-format.
    pub fn auid(&self, pid: u16) -> Option<Auid> {
        let entry = self.get(pid)?;
        if entry.value.len() == 16 {
            Auid::from_bytes(&entry.value)
        } else if self.is_avid && entry.value.len() == 21 {
            Auid::from_bytes(&entry.value[5..21])
        } else {
            Auid::from_bytes(&entry.value)
        }
    }

    /// Try each PID in order and return the first AUID found.
    pub fn auid_any(&self, pids: &[u16]) -> Option<Auid> {
        pids.iter().find_map(|&pid| self.auid(pid))
    }

    /// Try each PID in order and return the first i64 found.
    pub fn i64_le_any(&self, pids: &[u16]) -> Option<i64> {
        pids.iter().find_map(|&pid| self.i64_le(pid))
    }

    /// Try each PID in order and return the first u32 found.
    pub fn u32_le_any(&self, pids: &[u16]) -> Option<u32> {
        pids.iter().find_map(|&pid| self.u32_le(pid))
    }

    /// Read a 32-byte MobID from a `SF_DATA` property.
    pub fn mob_id(&self, pid: u16) -> Option<MobId> {
        let entry = self.get(pid)?;
        MobId::from_bytes(&entry.value)
    }

    /// Read a rational edit rate: 8 bytes = i32 numerator + i32 denominator (LE).
    pub fn edit_rate(&self, pid: u16) -> Option<EditRate> {
        let v = &self.get(pid)?.value;
        if v.len() >= 8 {
            let num = i32::from_le_bytes([v[0], v[1], v[2], v[3]]);
            let den = i32::from_le_bytes([v[4], v[5], v[6], v[7]]);
            if den != 0 {
                Some(EditRate {
                    numerator: num,
                    denominator: den,
                })
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Decode a UTF-16LE string property, stripping any null terminator.
    pub fn string(&self, pid: u16) -> Option<String> {
        let v = &self.get(pid)?.value;
        decode_utf16le(v)
    }

    // ─── Strong reference accessors ──────────────────────────────────────────

    /// Extract the child-directory name from a `SF_STRONG_OBJECT_REFERENCE`
    /// property value.
    ///
    /// In standard format the value is a null-terminated ASCII string.
    /// In Avid format the value is a null-terminated UTF-16LE string.
    pub fn strong_ref_name(&self, pid: u16) -> Option<String> {
        let entry = self.get(pid)?;
        if entry.stored_form != StoredForm::StrongRef {
            return None;
        }
        if self.is_avid {
            decode_utf16le(&entry.value)
        } else {
            let null = entry.value.iter().position(|&b| b == 0)?;
            String::from_utf8(entry.value[..null].to_vec()).ok()
        }
    }

    /// Try each PID in order and return the first strong ref name found.
    pub fn strong_ref_name_any(&self, pids: &[u16]) -> Option<String> {
        pids.iter().find_map(|&pid| self.strong_ref_name(pid))
    }

    /// Extract the `(count, collection_dir_name)` from a
    /// `SF_STRONG_OBJECT_REFERENCE_VECTOR` or `SF_STRONG_OBJECT_REFERENCE_SET`
    /// property value.
    ///
    /// **Standard format** layout:
    /// ```text
    /// [0..4]  u32 LE  count
    /// [4..8]  u32 LE  first_free_key  (used for writing; ignored here)
    /// [8..]   null-terminated ASCII   collection directory name
    /// ```
    ///
    /// **Avid format**: the value is just a null-terminated UTF-16LE base name
    /// (no count prefix).  The returned count is 0 (unknown); elements are
    /// located by scanning the parent directory for `{name}{n}` children.
    pub fn strong_ref_collection(&self, pid: u16) -> Option<(u32, String)> {
        let entry = self.get(pid)?;
        if !matches!(
            entry.stored_form,
            StoredForm::StrongRefVector | StoredForm::StrongRefSet
        ) {
            return None;
        }
        let v = &entry.value;

        if self.is_avid {
            let name = decode_utf16le(v)?;
            Some((0, name))
        } else {
            if v.len() < 9 {
                return None; // 4 + 4 + at least one byte of name
            }
            let count = u32::from_le_bytes([v[0], v[1], v[2], v[3]]);
            let null = v[8..].iter().position(|&b| b == 0)?;
            let name = String::from_utf8(v[8..8 + null].to_vec()).ok()?;
            Some((count, name))
        }
    }
}

// ─── Private helpers ─────────────────────────────────────────────────────────

/// Decode a null-terminated UTF-16LE byte slice to a `String`.
fn decode_utf16le(v: &[u8]) -> Option<String> {
    if v.len() < 2 || !v.len().is_multiple_of(2) {
        return None;
    }
    let words: Vec<u16> = v
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let end = words.iter().position(|&w| w == 0).unwrap_or(words.len());
    String::from_utf16(&words[..end]).ok()
}
