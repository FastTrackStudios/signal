//! AUID and MobID types — the two fundamental identifiers in AAF.
//!
//! - [`Auid`]: 16-byte GUID / SMPTE UL, used for class IDs, data definition IDs,
//!   operation definition IDs, etc.  Stored in Microsoft GUID byte order.
//! - [`MobId`]: 32-byte SMPTE UMID identifying a `Mob` object.

// ─── AUID ────────────────────────────────────────────────────────────────────

/// A 16-byte AAF unique identifier (Microsoft GUID / SMPTE UL).
///
/// Stored in the file as four LE-encoded fields followed by eight big-endian bytes:
/// ```text
/// bytes[0..4]  = Data1  (u32 LE)
/// bytes[4..6]  = Data2  (u16 LE)
/// bytes[6..8]  = Data3  (u16 LE)
/// bytes[8..16] = Data4  (8 bytes, as-is)
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Auid(pub [u8; 16]);

impl Auid {
    pub const ZERO: Self = Self([0u8; 16]);

    /// Parse from 16 raw bytes in AAF stored (GUID) byte order.
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        b.try_into().ok().map(Self)
    }

    /// Format as a Windows GUID string: `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`.
    pub fn to_guid_string(&self) -> String {
        let d1 = u32::from_le_bytes(self.0[0..4].try_into().unwrap());
        let d2 = u16::from_le_bytes(self.0[4..6].try_into().unwrap());
        let d3 = u16::from_le_bytes(self.0[6..8].try_into().unwrap());
        format!(
            "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
            d1,
            d2,
            d3,
            self.0[8],
            self.0[9],
            self.0[10],
            self.0[11],
            self.0[12],
            self.0[13],
            self.0[14],
            self.0[15],
        )
    }
}

impl std::fmt::Debug for Auid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Auid({})", self.to_guid_string())
    }
}

impl std::fmt::Display for Auid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_guid_string())
    }
}

// ─── Const constructor ───────────────────────────────────────────────────────

/// Build an [`Auid`] from GUID components at compile time.
const fn auid(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> Auid {
    let b1 = d1.to_le_bytes();
    let b2 = d2.to_le_bytes();
    let b3 = d3.to_le_bytes();
    Auid([
        b1[0], b1[1], b1[2], b1[3], b2[0], b2[1], b3[0], b3[1], d4[0], d4[1], d4[2], d4[3], d4[4],
        d4[5], d4[6], d4[7],
    ])
}

// ─── Well-known class AUIDs (SMPTE ST 2001-1 / AAF SDK AAFClassIDs.h) ────────

// Common Data4 suffix for AAF interoperability classes.
const D4_INTEROP: [u8; 8] = [0x06, 0x0E, 0x2B, 0x34, 0x02, 0x06, 0x01, 0x01];

pub const CLASS_HEADER: Auid = auid(0x0D010101, 0x0101, 0x2F00, D4_INTEROP);
pub const CLASS_CONTENT_STORAGE: Auid = auid(0x0D010101, 0x0101, 0x1800, D4_INTEROP);

// Mob subclasses
pub const CLASS_COMPOSITION_MOB: Auid = auid(0x0D010101, 0x0101, 0x0100, D4_INTEROP);
pub const CLASS_MASTER_MOB: Auid = auid(0x0D010101, 0x0101, 0x0200, D4_INTEROP);
pub const CLASS_SOURCE_MOB: Auid = auid(0x0D010101, 0x0101, 0x0300, D4_INTEROP);

// MobSlot subclasses
pub const CLASS_TIMELINE_MOB_SLOT: Auid = auid(0x0D010101, 0x0101, 0x3900, D4_INTEROP);
pub const CLASS_EVENT_MOB_SLOT: Auid = auid(0x0D010101, 0x0101, 0x3A00, D4_INTEROP);
pub const CLASS_STATIC_MOB_SLOT: Auid = auid(0x0D010101, 0x0101, 0x3B00, D4_INTEROP);

// Segment / Component subclasses
pub const CLASS_SEQUENCE: Auid = auid(0x0D010101, 0x0101, 0x0F00, D4_INTEROP);
pub const CLASS_SOURCE_CLIP: Auid = auid(0x0D010101, 0x0101, 0x1100, D4_INTEROP);
pub const CLASS_FILLER: Auid = auid(0x0D010101, 0x0101, 0x0900, D4_INTEROP);
pub const CLASS_TRANSITION: Auid = auid(0x0D010101, 0x0101, 0x0E00, D4_INTEROP);
pub const CLASS_OPERATION_GROUP: Auid = auid(0x0D010101, 0x0101, 0x1000, D4_INTEROP);
pub const CLASS_SELECTOR: Auid = auid(0x0D010101, 0x0101, 0x0D00, D4_INTEROP);
pub const CLASS_ESSENCE_GROUP: Auid = auid(0x0D010101, 0x0101, 0x0C00, D4_INTEROP);
pub const CLASS_NESTED_SCOPE: Auid = auid(0x0D010101, 0x0101, 0x0B00, D4_INTEROP);
pub const CLASS_TIMECODE: Auid = auid(0x0D010101, 0x0101, 0x1400, D4_INTEROP);
pub const CLASS_COMMENT_MARKER: Auid = auid(0x0D010101, 0x0101, 0x0800, D4_INTEROP);
pub const CLASS_DESCRIPTIVE_MARKER: Auid = auid(0x0D010101, 0x0101, 0x4100, D4_INTEROP);

// Locator subclasses
pub const CLASS_NETWORK_LOCATOR: Auid = auid(0x0D010101, 0x0101, 0x3200, D4_INTEROP);
pub const CLASS_TEXT_LOCATOR: Auid = auid(0x0D010101, 0x0101, 0x3300, D4_INTEROP);

// Essence Descriptor subclasses
pub const CLASS_SOUND_DESCRIPTOR: Auid = auid(0x0D010101, 0x0101, 0x4200, D4_INTEROP);
pub const CLASS_PCM_DESCRIPTOR: Auid = auid(0x0D010101, 0x0101, 0x5100, D4_INTEROP);
pub const CLASS_WAVE_DESCRIPTOR: Auid = auid(0x0D010101, 0x0101, 0x4400, D4_INTEROP);
pub const CLASS_AIFF_DESCRIPTOR: Auid = auid(0x0D010101, 0x0101, 0x4500, D4_INTEROP);
pub const CLASS_MULTI_DESCRIPTOR: Auid = auid(0x0D010101, 0x0101, 0x4800, D4_INTEROP);
pub const CLASS_TAPE_DESCRIPTOR: Auid = auid(0x0D010101, 0x0101, 0x4600, D4_INTEROP);
pub const CLASS_FILM_DESCRIPTOR: Auid = auid(0x0D010101, 0x0101, 0x4700, D4_INTEROP);
pub const CLASS_CDCI_DESCRIPTOR: Auid = auid(0x0D010101, 0x0101, 0x5000, D4_INTEROP);
pub const CLASS_RGBA_DESCRIPTOR: Auid = auid(0x0D010101, 0x0101, 0x4F00, D4_INTEROP);

// ─── Well-known DataDefinition AUIDs (SMPTE ST 2001-1) ───────────────────────

// Common Data4 for data definition ULs
const D4_DATADEFS: [u8; 8] = [0x06, 0x0E, 0x2B, 0x34, 0x04, 0x01, 0x01, 0x01];

/// Audio/sound data definition — identifies audio tracks (v1).
pub const DATADEF_SOUND: Auid = auid(0x01030202, 0x0100, 0x0000, D4_DATADEFS);
/// Audio/sound data definition — identifies audio tracks (v2 / Avid Media Composer).
pub const DATADEF_SOUND_V2: Auid = auid(0x01030202, 0x0200, 0x0000, D4_DATADEFS);
/// Picture/video data definition — identifies video tracks.
pub const DATADEF_PICTURE: Auid = auid(0x01030201, 0x0100, 0x0000, D4_DATADEFS);
/// SMPTE timecode data definition.
pub const DATADEF_TIMECODE: Auid = auid(0x01030201, 0x0200, 0x0000, D4_DATADEFS);
/// Legacy SMPTE 12M timecode.
pub const DATADEF_LEGACY_TC: Auid = auid(0x01030201, 0x0300, 0x0000, D4_DATADEFS);
/// Edgecode.
pub const DATADEF_EDGECODE: Auid = auid(0x01030201, 0x0400, 0x0000, D4_DATADEFS);
/// Descriptive metadata.
pub const DATADEF_DESCRIPTIVE: Auid = auid(0x01030201, 0x0500, 0x0000, D4_DATADEFS);

// ─── Legacy OMF / Avid DataDefinition AUIDs ──────────────────────────────────
//
// Used by Avid Media Composer, Pro Tools, and other tools that generate
// Avid-internal-format AAF.  The AUID is embedded in a 21-byte value
// (5-byte header + 16-byte AUID) at PID_COMPONENT_DATA_DEFINITION_AVID.

const D4_OMF_DEFS: [u8; 8] = [0x80, 0x7D, 0x00, 0x60, 0x08, 0x14, 0x3E, 0x6F];

/// OMF/Avid legacy sound data definition (Pro Tools, Media Composer).
pub const DATADEF_OMF_SOUND: Auid = auid(0x78E1EBE1, 0x6CEF, 0x11D2, D4_OMF_DEFS);
/// OMF/Avid legacy picture data definition.
pub const DATADEF_OMF_PICTURE: Auid = auid(0xB6BFA481, 0x6CEF, 0x11D2, D4_OMF_DEFS);
/// OMF/Avid legacy timecode data definition.
pub const DATADEF_OMF_TIMECODE: Auid = auid(0x7F275E00, 0x6CEF, 0x11D2, D4_OMF_DEFS);

// ─── MobId ───────────────────────────────────────────────────────────────────

/// A 32-byte AAF Mob Identifier (SMPTE UMID — Unique Material Identifier).
///
/// The structure is `[prefix(12) | length(1) | instance(3) | material(16)]`,
/// but for our purposes the 32 bytes are treated as an opaque key.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct MobId(pub [u8; 32]);

impl MobId {
    pub const ZERO: Self = Self([0u8; 32]);

    /// Parse from 32 raw bytes.
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        b.try_into().ok().map(Self)
    }

    /// Returns true if all bytes are zero (indicates "no source" in SourceClips).
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 32]
    }
}

impl std::fmt::Debug for MobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MobId({:02X?})", &self.0[..8])
    }
}

impl std::fmt::Display for MobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for b in &self.0 {
            write!(f, "{:02X}", b)?;
        }
        Ok(())
    }
}
