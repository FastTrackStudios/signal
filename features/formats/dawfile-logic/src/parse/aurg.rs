//! AuRg chunk parser — audio region pool segment (POOLSEG).
//!
//! Each top-level `AuRg` chunk contains one `POOLSEG::serialize` record
//! describing an audio region's placement and source reference.
//!
//! Field names are taken verbatim from the `POOLSEG::serialize` field-name
//! string table found in the `LogicPro_x86_64` binary at file offset 0x209c8d4.
//!
//! ## POOLSEG binary layout — fixed header (no struct-alignment padding)
//!
//! | Serial byte | Field                | Type     | Notes                                   |
//! |-------------|----------------------|----------|-----------------------------------------|
//! | 0–1         | `usedcount`          | u16 LE   |                                         |
//! | 2–3         | `poolflags`          | u16 LE   |                                         |
//! | 4           | `mTakeNumber`        | u8       | 0 = comp result; ≥1 = recorded take     |
//! | 5           | `segFlags`           | u8       |                                         |
//! | 6–9         | `offset`             | i32 LE   | Source file start in sample frames      |
//! | 10–13       | `offset_reserve`     | i32 LE   |                                         |
//! | 14–21       | `timeStamp`          | i64 LE   | Recording session timestamp (samples)   |
//! | 22–25       | `frames`             | i32 LE   | Region duration in sample frames        |
//! | 26–29       | `timeStampFileOffset`| i32 LE   | (was incorrectly labelled "unused")     |
//! | 30–33       | `hotspot`            | i32 LE   |                                         |
//! | 34–37       | (reserved/unknown)   | 4 bytes  | Identity not yet determined             |
//! | 38          | `mSelected`          | u8       |                                         |
//! | 39          | `mLock`              | u8       |                                         |
//! | 40–41       | `mElasticMode`       | 2 bytes  |                                         |
//! | 42–49       | `mModificationDate`  | i64 LE   | Windows FILETIME (100-ns since 1601)    |
//! | 50–73       | `legacy_segname` + padding | 24 bytes | Null-terminated name from old format |
//! | 74–75       | name `length`        | u16 LE   | Number of UTF-8 bytes in name           |
//! | 76..        | name                 | UTF-8    | `length` bytes, not null-terminated     |
//!
//! ## Post-name variable section
//!
//! Immediately after the name, `POOLSEG_FIXED_BEFORE_CLOCK` bytes of fixed
//! fields precede `startClockComp`.  Confirmed offsets relative to `name_end`:
//!
//! | Offset | Field                    | Type   | Notes                              |
//! |--------|--------------------------|--------|------------------------------------|
//! | +0     | `mFadeType`              | u8     |                                    |
//! | +1     | `mFadeCurve`             | u8     |                                    |
//! | +2–5   | `indexLeftFile`          | u32 LE |                                    |
//! | +6–9   | `offsetLeftFile`         | u32 LE |                                    |
//! | +10–13 | `indexRightFile`         | u32 LE |                                    |
//! | +14–17 | `offsetRightFile`        | u32 LE |                                    |
//! | +18–21 | `mFadeID`                | u32 LE |                                    |
//! | +22–25 | `loopStart`              | u32 LE |                                    |
//! | +26–29 | `loopLen`                | u32 LE |                                    |
//! | +30    | `keyNote`                | u8     |                                    |
//! | +31    | `detune`                 | u8     |                                    |
//! | +32    | `mFadeStyle`             | u8     |                                    |
//! | +33    | `mTakeNumberOnLane`      | u8     | UI lane index (0-based). Confirmed |
//! | +34–37 | `mFadeNumSourceFrames`   | u32 LE |                                    |
//! | +38–41 | `mFadeNumSourceFramesRight`| u32 LE|                                   |
//! | +42–45 | `userTimeStamp`          | u32 LE |                                    |
//! | +46    | `mGainLeftFile`          | u8     |                                    |
//! | +47    | `mGainRightFile`         | u8     |                                    |
//! | +48–49 | `oldindex`               | u16 LE | Confirmed at post-name +48         |
//! | +50    | (reserved/unknown)       | u8     | 1-byte gap before clock field      |
//! | +51–58 | `startClockComp`         | i64 LE | Arrangement start in ticks (240 PPQ) — **confirmed** |
//! | +59–66 | `endClockComp`           | i64 LE | Arrangement end in ticks — **confirmed** |
//!
//! Empirically confirmed against `FileDecrypt.logicx` and `Fire.logicx`:
//! `startClockComp = 0` for clips at bar 1 beat 1. ✓
//! `mTakeNumberOnLane` ranges 0–3 in `Fire.logicx` take folder clips. ✓

/// Bytes from end-of-name to `startClockComp`.
///
/// Breakdown (2+20+8+4+12+2+2+1 = 51):
///   mFadeType(1) + mFadeCurve(1) = 2
///   indexLeftFile(4)+offsetLeftFile(4)+indexRightFile(4)+offsetRightFile(4)+mFadeID(4) = 20
///   loopStart(4) + loopLen(4) = 8
///   keyNote(1) + detune(1) + mFadeStyle(1) + mTakeNumberOnLane(1) = 4
///   mFadeNumSourceFrames(4)+mFadeNumSourceFramesRight(4)+userTimeStamp(4) = 12
///   mGainLeftFile(1) + mGainRightFile(1) = 2
///   oldindex(2) = 2
///   reserved/unknown(1) = 1
///
/// The constant (51) is empirically confirmed against fixture binaries.
const POOLSEG_FIXED_BEFORE_CLOCK: usize = 51;

/// Key data extracted from an `AuRg` chunk payload.
#[derive(Debug, Clone)]
pub struct AudioRegion {
    /// User-visible name of this region (e.g. `"Audio Track 1 #01"`).
    pub name: String,
    /// Take number within a Take Folder (`mTakeNumber`).
    ///
    /// - `0` = comp result layer (the composite output; `start_ticks`/`end_ticks` are 0)
    /// - `1..N` = individual recorded takes
    pub take_number: u8,
    /// UI lane index within a Take Folder (`mTakeNumberOnLane`, post-name +33).
    ///
    /// 0-based lane ordering as displayed in Logic Pro's take folder view.
    /// Ranges 0–3 for a folder with 4 takes. Zero for non-take-folder regions.
    pub take_number_on_lane: u8,
    /// Source file start offset in sample frames.
    pub source_offset_frames: i32,
    /// Region duration in sample frames.
    pub duration_frames: i32,
    /// Arrangement start position in ticks (Logic clock, 240 PPQ).
    /// Zero = bar 1 beat 1. Convert to beats by dividing by 240.
    /// For take regions (take_number ≥ 1): the start of the active comp range.
    pub start_ticks: i64,
    /// Arrangement end position in ticks (may be zero for non-comped regions).
    /// For take regions (take_number ≥ 1): the end of the active comp range.
    pub end_ticks: i64,
    /// Whether this region is selected in the project.
    pub selected: bool,
    /// Whether this region is locked.
    pub locked: bool,
    /// RFC 4122 UUID identifying this region (version 1 or 4).
    ///
    /// Located in the post-endClockComp section:
    /// - `take_number == 0`: first valid UUID found at approximately post-name +87
    /// - `take_number >= 1`: first valid UUID found at approximately post-name +86
    ///
    /// `None` if the payload is too short or no valid UUID is found.
    pub uuid: Option<[u8; 16]>,
    /// Offset to the related take folder start chunk, or -1 if none.
    ///
    /// Located at post-name +103 (post-endClockComp +36).
    /// Value -1 = not part of a take folder (or no folder linkage in this version).
    pub offset_to_take_folder_start: i32,
}

/// Parse an `AuRg` chunk payload into an [`AudioRegion`].
///
/// Returns `None` if the payload is too short.
pub fn parse_aurg(data: &[u8]) -> Option<AudioRegion> {
    // Minimum fixed header before name: 76 bytes (name_length at byte 74, minimum 0-len name)
    if data.len() < 76 {
        return None;
    }

    let take_number = data[4];
    let source_offset_frames = i32::from_le_bytes(data[6..10].try_into().ok()?);
    let duration_frames = i32::from_le_bytes(data[22..26].try_into().ok()?);
    let selected = data[38] != 0;
    let locked = data[39] != 0;

    // Name: u16 length at bytes 74–75, then UTF-8 bytes
    let name_len = u16::from_le_bytes([data[74], data[75]]) as usize;
    let name_start = 76;
    let name_end = name_start + name_len;

    if data.len() < name_end {
        return None;
    }

    let name = String::from_utf8_lossy(&data[name_start..name_end]).into_owned();

    // mTakeNumberOnLane: post-name offset +33 (confirmed via binary analysis of Fire.logicx)
    let take_number_on_lane = if data.len() > name_end + 33 {
        data[name_end + 33]
    } else {
        0
    };

    // startClockComp: name_end + POOLSEG_FIXED_BEFORE_CLOCK bytes
    let clock_start = name_end + POOLSEG_FIXED_BEFORE_CLOCK;
    let clock_end = clock_start + 8;
    let end_clock_start = clock_end;
    let end_clock_end = end_clock_start + 8;

    let start_ticks = if data.len() >= clock_end {
        i64::from_le_bytes(data[clock_start..clock_end].try_into().ok()?)
    } else {
        0
    };

    let end_ticks = if data.len() >= end_clock_end {
        i64::from_le_bytes(data[end_clock_start..end_clock_end].try_into().ok()?)
    } else {
        0
    };

    // Post-endClockComp section begins at name_end + 67.
    // UUID: first valid RFC 4122 v1 or v4 UUID in the range post-name[84..104].
    //   take=0 → UUID at post-name +87 (confirmed), take≥1 → +86.
    //   We scan for validity rather than hardcoding the offset.
    let uuid = find_uuid_in_range(data, name_end + 84, name_end + 104);

    // offsetToTakeFolderStart: i32 LE at post-name +103 (post-endClockComp +36).
    // -1 = no take folder linkage.
    let offset_to_take_folder_start = if data.len() >= name_end + 107 {
        i32::from_le_bytes(data[name_end + 103..name_end + 107].try_into().ok()?)
    } else {
        -1
    };

    Some(AudioRegion {
        name,
        take_number,
        take_number_on_lane,
        source_offset_frames,
        duration_frames,
        start_ticks,
        end_ticks,
        selected,
        locked,
        uuid,
        offset_to_take_folder_start,
    })
}

/// Scan `data[start..end]` for the first RFC 4122 UUID (version 1 or 4, variant 1).
///
/// Returns `Some([u8; 16])` at the first aligned window where:
/// - byte 6 has version bits 0001xxxx or 0100xxxx (version 1 or 4)
/// - byte 8 has variant bits 10xxxxxx (variant 1 / RFC 4122)
fn find_uuid_in_range(data: &[u8], start: usize, end: usize) -> Option<[u8; 16]> {
    let end = end.min(data.len().saturating_sub(15));
    for i in start..end {
        let ver = (data[i + 6] >> 4) & 0x0f;
        let var = (data[i + 8] >> 6) & 0x03;
        if matches!(ver, 1 | 4) && var == 0b10 {
            return data[i..i + 16].try_into().ok();
        }
    }
    None
}
