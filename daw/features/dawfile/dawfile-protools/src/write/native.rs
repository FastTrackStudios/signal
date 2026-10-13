//! Native (template-patch) PTX writer.
//!
//! Strategy: start from a baseline PTX template (one default-state
//! track, generated once via the converter and embedded in this crate),
//! patch known field offsets to match the input, re-encrypt.
//!
//! Single-track covers: name, color, mute, solo, solo-defeat,
//! inactive, volume, pan, mute automation.
//!
//! Multi-track (`write_session_ptx`) clones the per-track `0x261c`
//! block N times and extends outer index lists (`0x1015`, `0x1054`,
//! `0x2519`) with N entries so the parser sees N distinct PT tracks
//! with the spec'd names. Per-track color and solo are scoped to
//! each track's `0x261c` range. Per-track mute on multi-track has
//! a known issue (parser-side resolver pairing breaks on cloned
//! files — tracked separately).
//!
//! Validation: a test compares native output against converter
//! shell-out output, ignoring the per-file UID region at `0x261b
//! +7433..+7581` (148-byte run of randomized bytes that differs
//! between any two converter invocations even with identical input).
//!
//! See `docs/pt-field-map.md` for the byte-level field map.

use crate::raw_block::{RawBlock, RawSession};
use crate::write::splice::{replace_string, splice};

/// A single breakpoint in a mute automation envelope.
#[derive(Debug, Clone, Copy)]
pub struct MuteAutomationPoint {
    /// Position in samples (at session sample rate).
    pub time_samples: u32,
    /// `true` = muted at this time, `false` = un-muted.
    pub muted: bool,
}

/// A single breakpoint in a volume automation envelope.
#[derive(Debug, Clone, Copy)]
pub struct VolumeAutomationPoint {
    /// Position in samples (at session sample rate).
    pub time_samples: u32,
    /// Volume in 0.1 dB units (centibel). `0` = unity; `-60` = -6 dB.
    pub value_centibel: i16,
}

/// Single-track PTX writer parameters.
#[derive(Debug, Clone)]
pub struct NativeTrackSpec {
    pub name: String,
    /// PT palette index, or 0 for default.
    pub color: u8,
    /// `true` writes the stored mute bit AND clears the send-routing
    /// flag, producing what the converter reads as effective-muted.
    pub mute: bool,
    pub solo: bool,
    /// Solo-defeat (track ignores other tracks' solo state).
    pub solo_defeat: bool,
    /// PT "Make Inactive"/bouncedSource state. Sets the mute bit AND
    /// KEEPS the send-routing flag (the inverse of `mute`). Mutually
    /// exclusive with `mute`: if both are true, `mute` wins.
    pub inactive: bool,
    /// Track volume in 0.1 dB units. 0 = unity.
    pub volume_centibel: i16,
    /// Pan -100..+100. -100 = full L (the default).
    pub pan: i16,
    /// Optional mute automation envelope. Breakpoints in time order.
    /// PT drops a redundant t=0 user point (matching its implicit
    /// default-unmuted state), so don't include a (0, false) point —
    /// it'll either be discarded or merged into the default.
    pub mute_automation: Vec<MuteAutomationPoint>,
    /// Optional volume automation envelope. Same shape as mute
    /// automation but each breakpoint carries an `i16` centibel value
    /// instead of a `bool`.
    pub volume_automation: Vec<VolumeAutomationPoint>,
    /// Optional output routing destination (e.g. `"Analog 1-2"`,
    /// `"Bus 1"`). When `None`, the baseline default is preserved.
    /// Written as a length-prefixed string into the track's `0x260e`
    /// (TrackRouting) block at payload +0x24.
    pub output: Option<String>,
}

impl Default for NativeTrackSpec {
    fn default() -> Self {
        Self {
            name: "ProbeTrack".to_string(),
            color: 0,
            mute: false,
            solo: false,
            solo_defeat: false,
            inactive: false,
            volume_centibel: 0,
            pan: -100,
            mute_automation: Vec::new(),
            volume_automation: Vec::new(),
            output: None,
        }
    }
}

/// Embedded single-track baseline template.
///
/// Generated 2026-05-17 by running:
/// `cargo run -p daw-reaper --example rpp_to_ptx_probe -- baseline`
/// then copying `/tmp/probe_baseline.ptx`.
///
/// This is an encrypted PTX. We decrypt → patch → re-encrypt.
const BASELINE_PTX: &[u8] =
    include_bytes!("../../tests/fixtures/native-writer/baseline-single-track.ptx");

/// Generate a single-track PTX file from `spec`. Returns the encrypted
/// PTX bytes ready to write to disk.
pub fn write_single_track_ptx(spec: &NativeTrackSpec) -> crate::PtResult<Vec<u8>> {
    // Decrypt baseline
    let mut session = crate::parse_raw(BASELINE_PTX.to_vec())?;

    // Step 1: rename the track from "ProbeTrack" to spec.name. After
    // splicing, the block tree offsets are stale; we walk the data
    // directly when patching numeric fields below, so the stale tree
    // is fine for fixed-offset writes inside the SAME block. But
    // rename SHIFTS later blocks, so we need to re-build the tree
    // without going through the decryption step again.
    let did_rename = if spec.name != "ProbeTrack" {
        rename_all_track_name_occurrences(&mut session, "ProbeTrack", &spec.name);
        true
    } else {
        false
    };
    if did_rename {
        // Re-parse the RAW block tree from the already-decrypted data
        // (do NOT go through parse_raw which would XOR-decrypt again).
        let is_be = session.is_bigendian;
        session.blocks = crate::raw_block::parse_raw_blocks_pub(&session.data, is_be);
    }

    // Step 2: patch numeric fields. These are all fixed-size so no splicing.
    patch_color(&mut session, spec.color);
    // `mute` and `inactive` are mutually-exclusive — both set the
    // +5 mix bit, but only `mute` clears the send-routing flag
    // (+8 in 0x260a[0]). If both are requested, mute wins.
    if spec.mute {
        patch_mute_bit(&mut session, true);
        patch_send_routing(&mut session, false);
    } else if spec.inactive {
        patch_mute_bit(&mut session, true);
        patch_send_routing(&mut session, true); // baseline default; explicit for clarity
    } else {
        patch_mute_bit(&mut session, false);
    }
    patch_solo(&mut session, spec.solo);
    patch_solo_defeat(&mut session, spec.solo_defeat);
    patch_volume(&mut session, spec.volume_centibel);
    patch_pan(&mut session, spec.pan);

    // Step 3.0: output routing destination is NOT yet writable from
    // the baseline because `set_track_output` requires a pre-existing
    // destination string (it refuses the 61-byte empty variant). The
    // baseline's 0x260e block is the empty variant. Spec carries the
    // field for forward-compatibility; ignored for now.
    let _ = &spec.output;

    // Step 3a: volume automation envelope (0x260a[0]).
    if !spec.volume_automation.is_empty() {
        write_volume_automation(&mut session, &spec.volume_automation);
    }

    // Step 3b: mute automation (variable-length splice into 0x260a[1]).
    if !spec.mute_automation.is_empty() {
        write_mute_automation(&mut session, &spec.mute_automation);
    }

    // Step 4: re-encrypt
    Ok(session.encrypt())
}

/// Multi-track session writer parameters.
#[derive(Debug, Clone, Default)]
pub struct NativeSessionSpec {
    pub tracks: Vec<NativeTrackSpec>,
}

/// Generate a multi-track PTX from `spec`. Returns the encrypted
/// PTX bytes ready to write to disk.
///
/// Strategy: start from the embedded single-track baseline. For
/// `N > 1` tracks, clone the baseline's `0x261c` block `N-1` times,
/// splice the clones after the original (inside the parent `0x2624`,
/// which auto-cascades), then patch each clone's identity fields
/// (name, 8-byte UID, send-slot bus indices). Finally, apply the
/// per-track field patches (mute/solo/vol/...) for each track in
/// turn.
///
/// Per-track variation map (see `docs/pt-field-map.md`): inside each
/// `0x261c`, the per-track-unique bytes are
/// - track name at `0x261b` block-start +31 (variable length)
/// - 8-byte UID at `0x261b` block-start +45, +61, +163
/// - send-slot bus indices at `0x261b` block-start +7350..+7386 (stride 4)
/// - 148-byte cosmetic nonce at `0x261b` block-start +7435
///
/// Empty `spec.tracks` produces the single-track baseline as-is
/// (named `ProbeTrack`).
pub fn write_session_ptx(spec: &NativeSessionSpec) -> crate::PtResult<Vec<u8>> {
    if spec.tracks.is_empty() {
        return write_single_track_ptx(&NativeTrackSpec::default());
    }
    if spec.tracks.len() == 1 {
        return write_single_track_ptx(&spec.tracks[0]);
    }

    let n = spec.tracks.len();
    let mut session = crate::parse_raw(BASELINE_PTX.to_vec())?;

    // Locate the single 0x261c container in the baseline.
    let (orig_start, orig_end) = {
        let b = find_first_by_ct(&session.blocks, 0x261c).expect("baseline missing 0x261c");
        (b.start, b.end)
    };
    let clone_bytes: Vec<u8> = session.data[orig_start..orig_end].to_vec();

    // Splice (N-1) clones APPENDED after the original 0x261c. We
    // advance `cursor` by clone_len each time so the resulting order
    // is [orig, clone_1, clone_2, ..., clone_{N-1}] — matching the
    // user's spec.tracks index order.
    let clone_len = clone_bytes.len();
    let mut cursor = orig_end;
    for _ in 0..(n - 1) {
        splice(&mut session, cursor, 0, &clone_bytes);
        cursor += clone_len;
    }

    // Extend outer index lists by (N-1) entries each so the parser
    // sees N distinct tracks instead of capping at the baseline's
    // single-entry channel-pair (= 2 audio channels for stereo).
    let track_names: Vec<String> = spec.tracks.iter().map(|t| t.name.clone()).collect();
    extend_audio_track_list(&mut session, n, &track_names)?;
    extend_playlist_list(&mut session, n, &track_names)?;
    extend_midi_track_list(&mut session, n, &track_names)?;

    // Now each 0x261c's bytes are identical. We need to make them
    // distinct: patch name, UID, send-slot indices, optional nonce.
    // Iterate forward, locating each 0x261c by walking the (now
    // up-to-date) block tree.
    let track_starts: Vec<usize> = collect_by_raw_ct(&session.blocks, 0x261c)
        .iter()
        .map(|b| b.start)
        .collect();
    assert_eq!(track_starts.len(), n, "post-splice 0x261c count mismatch");

    for (i, track_start) in track_starts.iter().enumerate() {
        // Find this track's 0x261b (the child of 0x261c at +9 payload).
        let b261b = find_first_by_ct_in_range(&session.blocks, 0x261b, *track_start)
            .expect("missing 0x261b under 0x261c");
        // Patch send-slot bus indices: track i (0-indexed) gets
        // bus indices i*10+1..i*10+10.
        for slot in 0..10 {
            let p = b261b.start + 7350 + 4 * slot;
            if p < session.data.len() {
                session.data[p] = (i * 10 + slot + 1) as u8;
            }
        }
        // Per-track UID: deterministic from index so round-trips
        // are reproducible. Real PT uses random bytes; the parser
        // doesn't care about specific values, just that they differ
        // across tracks.
        let uid: [u8; 8] = {
            let mut u = [0u8; 8];
            u[0] = 0x92;
            u[1] = (0x5a ^ i as u8).wrapping_add(1);
            u[2] = 0xa9;
            u[3] = 0xe5;
            u[4] = 0xb7;
            u[5] = (0xc3 ^ i as u8).wrapping_add(1);
            u[6] = 0x9d;
            u[7] = 0x1c;
            u
        };
        for uid_off in [45usize, 61, 163] {
            let p = b261b.start + uid_off;
            if p + 8 <= session.data.len() {
                session.data[p..p + 8].copy_from_slice(&uid);
            }
        }
    }
    // Rebuild block tree after raw patches (sizes unchanged so
    // structure is intact, but the cached tree may be stale).
    let is_be = session.is_bigendian;
    session.blocks = crate::raw_block::parse_raw_blocks_pub(&session.data, is_be);

    // Track-name renames.
    //
    // LIMITATION (2026-05-17): only ONE "ProbeTrack" occurrence lives
    // inside each 0x261c (in the per-track 0x2619 name block — child
    // of 0x102d). The other 7+ occurrences are in OUTER index lists
    // (0x2519, 0x1054, 0x1015, 0x2107) which are NOT inside 0x261c
    // and which the parser apparently reads when reporting track
    // names. To support distinct per-track names we'd need to clone
    // an entry inside each of these outer lists per track too.
    //
    // For now: all tracks must share a single name (or accept that
    // they all parse as "ProbeTrack"). If every spec.tracks[i].name
    // is identical, we use the global rename path. Otherwise we
    // emit a warning and rename to the first track's name.
    let all_same_name = spec.tracks.iter().all(|t| t.name == spec.tracks[0].name);
    if all_same_name && spec.tracks[0].name != "ProbeTrack" {
        rename_all_track_name_occurrences(&mut session, "ProbeTrack", &spec.tracks[0].name);
        let is_be = session.is_bigendian;
        session.blocks = crate::raw_block::parse_raw_blocks_pub(&session.data, is_be);
    }
    // Per-track in-wrapper rename still happens (covers the 0x2619
    // child) so future parser changes that read from there will Just
    // Work.
    for i in (0..n).rev() {
        let new_name = &spec.tracks[i].name;
        if new_name == "ProbeTrack" {
            continue;
        }
        let ranges: Vec<(usize, usize)> = collect_by_raw_ct(&session.blocks, 0x261c)
            .iter()
            .map(|b| (b.start, b.end))
            .collect();
        if let Some((ts, te)) = ranges.get(i).copied() {
            rename_probetrack_in_range(&mut session, ts, te, new_name);
        }
    }

    // Per-track field patches. The track tree is now stable; walk
    // 0x261c blocks and patch each track's mute/solo/etc. using
    // helpers scoped to that 0x261c's byte range.
    let track_ranges: Vec<(usize, usize)> = collect_by_raw_ct(&session.blocks, 0x261c)
        .iter()
        .map(|b| (b.start, b.end))
        .collect();
    for (i, (ts, te)) in track_ranges.iter().enumerate() {
        let t = &spec.tracks[i];
        patch_track_in_range(&mut session, *ts, *te, t);
    }

    Ok(session.encrypt())
}

/// Apply per-track field patches scoped to a single 0x261c byte range.
/// Extend the audio-track list (`0x1015 → 0x1014`) from its baseline
/// single entry up to `n` entries. The baseline entry encodes a
/// stereo track (channels 0 and 1). The clone for track `i` (0-indexed)
/// gets channel indices `i*2` and `i*2+1` patched in the channel-map
/// fields, plus a deterministic per-entry UID.
///
/// `0x1014` entry byte layout (offsets from block start, baseline
/// with 10-char name "ProbeTrack"):
/// - +0..+8  : 9-byte block header
/// - +9..+12 : u32 LE name length-prefix
/// - +13..+22: name bytes ("ProbeTrack")
/// - +28     : u8 channel index 0
/// - +30     : u8 channel index 1
/// - +40..+47: 8-byte per-entry UID
/// - +55     : u8 channel index 0 mirror
/// - +59     : u8 channel index 1 mirror
///
/// (See `docs/pt-field-map.md` § Outer index lists — derived by
/// diffing `two_tracks_eq` trk1 vs trk2 0x1014 entries.)
fn extend_audio_track_list(
    session: &mut RawSession,
    n: usize,
    names: &[String],
) -> crate::PtResult<()> {
    if n < 2 {
        return Ok(());
    }
    // Find baseline's single 0x1014 entry.
    let (start, end) = {
        let b = find_first_by_ct(&session.blocks, 0x1014).expect("baseline missing 0x1014");
        (b.start, b.end)
    };
    let template: Vec<u8> = session.data[start..end].to_vec();
    let clone_len = template.len();
    let mut cursor = end;
    for _ in 0..(n - 1) {
        splice(session, cursor, 0, &template);
        cursor += clone_len;
    }
    // Patch each entry: name (variable-length splice) + channel
    // indices + UID. Walk entries in REVERSE so name-length splices
    // don't invalidate earlier entries' offsets.
    let entries: Vec<(usize, usize)> = collect_by_raw_ct(&session.blocks, 0x1014)
        .iter()
        .map(|b| (b.start, b.end))
        .collect();
    assert_eq!(entries.len(), n, "post-splice 0x1014 count mismatch");

    for i in (0..n).rev() {
        // Re-locate entry i (positions shift after each name splice).
        let entries_now: Vec<(usize, usize)> = collect_by_raw_ct(&session.blocks, 0x1014)
            .iter()
            .map(|b| (b.start, b.end))
            .collect();
        let (s, _) = entries_now[i];

        // Splice name if different from "ProbeTrack". The length-prefix
        // u32 sits at block-start +9.
        let new_name = &names[i];
        if new_name != "ProbeTrack" {
            replace_string(session, s + 9, new_name);
            let is_be = session.is_bigendian;
            session.blocks = crate::raw_block::parse_raw_blocks_pub(&session.data, is_be);
        }

        // Re-locate again post-rename, then compute offsets from
        // updated name length.
        let entries_now: Vec<(usize, usize)> = collect_by_raw_ct(&session.blocks, 0x1014)
            .iter()
            .map(|b| (b.start, b.end))
            .collect();
        let (s, _) = entries_now[i];
        let name_len = u32::from_le_bytes([
            session.data[s + 9],
            session.data[s + 10],
            session.data[s + 11],
            session.data[s + 12],
        ]) as usize;
        let ch0 = 13 + name_len + 5;
        let ch1 = ch0 + 2;
        let ch0_mir = ch0 + 27;
        let ch1_mir = ch0_mir + 4;
        let uid_off = ch1 + 10;
        let pair_a = (i * 2) as u8;
        let pair_b = (i * 2 + 1) as u8;
        for (off, val) in [
            (ch0, pair_a),
            (ch1, pair_b),
            (ch0_mir, pair_a),
            (ch1_mir, pair_b),
        ] {
            let p = s + off;
            if p < session.data.len() {
                session.data[p] = val;
            }
        }
        let uid: [u8; 8] = [0x92, 0x5a, 0xa9, 0xe5, 0xb7, 0xc3, 0x9d, 0x1c ^ i as u8];
        let p = s + uid_off;
        if p + 8 <= session.data.len() {
            session.data[p..p + 8].copy_from_slice(&uid);
        }
    }
    let is_be = session.is_bigendian;
    session.blocks = crate::raw_block::parse_raw_blocks_pub(&session.data, is_be);
    Ok(())
}

/// Extend the playlist-name list (`0x1054 → 0x1052`). Each track owns
/// 2× `0x1052` entries (active + alternate playlist). The parser maps
/// these positionally 1:1 to `0x1014` channel-entries and reads the
/// track display name from the `0x1052` name field (see
/// `parse::tracks::assign_regions_new`).
///
/// `0x1052` entry layout (offsets from block start):
/// - +0..+6:   7-byte header (`5A` magic + block_type + size_u32)
/// - +7..+8:   ct = `0x1052`
/// - +9..+12:  u32 LE name length-prefix
/// - +13..:    name bytes
/// - after:    6-byte trailer `00 00 00 00 01 00`
fn extend_playlist_list(
    session: &mut RawSession,
    n: usize,
    names: &[String],
) -> crate::PtResult<()> {
    if n < 2 {
        return Ok(());
    }
    // Find baseline's first 0x1054 (the top-level one — there's also a
    // smaller 0x1054 elsewhere). The first one in document order is the
    // playlist list.
    let blocks_1054 = collect_by_raw_ct(&session.blocks, 0x1054);
    let parent_1054 = blocks_1054
        .first()
        .copied()
        .expect("baseline missing 0x1054");
    let parent_end = parent_1054.end;
    // Find baseline's existing 0x1052 entries (the LAST one is the
    // alternate; we'll clone the 2 entries together so each new track
    // gets active + alternate of its own).
    let entries_1052: Vec<(usize, usize)> = collect_by_raw_ct(&parent_1054.children, 0x1052)
        .iter()
        .map(|b| (b.start, b.end))
        .collect();
    if entries_1052.len() < 2 {
        return Ok(());
    }
    // Take the LAST 2 entries as the template pair (so cloning preserves
    // the active/alt distinction if there is any in trailer bytes).
    let pair_start = entries_1052[entries_1052.len() - 2].0;
    let pair_end = entries_1052[entries_1052.len() - 1].1;
    let template_pair: Vec<u8> = session.data[pair_start..pair_end].to_vec();
    let _ = parent_end;
    let pair_len = template_pair.len();
    // Splice (n-1) clone-pairs APPENDED after the last entry.
    let mut cursor = pair_end;
    for _ in 0..(n - 1) {
        splice(session, cursor, 0, &template_pair);
        cursor += pair_len;
    }

    // Now rename each track's pair. Walk in reverse (per-track-pair)
    // so name-length splices don't invalidate earlier positions.
    // Each track i owns entries [2i, 2i+1].
    for i in (0..n).rev() {
        let new_name = &names[i];
        if new_name == "ProbeTrack" {
            continue;
        }
        // Splice both entries of this pair (reverse within pair too).
        let entries_now: Vec<(usize, usize)> = collect_by_raw_ct(&session.blocks, 0x1052)
            .iter()
            .filter(|b| {
                // Only count the entries that live inside the top-level
                // 0x1054 (skip the trailing smaller 0x1054 in 0xc000s).
                let blocks_1054 = collect_by_raw_ct(&session.blocks, 0x1054);
                if let Some(top) = blocks_1054.first() {
                    b.start >= top.start && b.end <= top.end
                } else {
                    false
                }
            })
            .map(|b| (b.start, b.end))
            .collect();
        if entries_now.len() < (i + 1) * 2 {
            continue;
        }
        // Reverse within the pair: entry 2i+1 first, then 2i.
        for entry_idx in [2 * i + 1, 2 * i] {
            let entries_now: Vec<(usize, usize)> = collect_by_raw_ct(&session.blocks, 0x1052)
                .iter()
                .filter(|b| {
                    let blocks_1054 = collect_by_raw_ct(&session.blocks, 0x1054);
                    if let Some(top) = blocks_1054.first() {
                        b.start >= top.start && b.end <= top.end
                    } else {
                        false
                    }
                })
                .map(|b| (b.start, b.end))
                .collect();
            if entry_idx >= entries_now.len() {
                continue;
            }
            let (s, _) = entries_now[entry_idx];
            replace_string(session, s + 9, new_name);
            let is_be = session.is_bigendian;
            session.blocks = crate::raw_block::parse_raw_blocks_pub(&session.data, is_be);
        }
    }
    Ok(())
}

/// Extend the MidiTrackList (`0x2519 → 0x251a`). The mute_resolver and
/// other parser passes iterate `0x251a` entries inside the top-level
/// `0x2519` to enumerate tracks by document order, then pair each name
/// with a `0x260d` (track mix wrapper). Without this extension the
/// resolver sees only the baseline's 2 entries and mis-pairs cloned
/// wrappers.
///
/// `0x251a` entry layout:
/// - +0..+6: 7-byte header
/// - +7..+8: ct = 0x251a (1a 25 LE)
/// - +9..+10: reserved
/// - +11..+14: u32 LE name length-prefix
/// - +15..: name bytes
/// - trailer: ~74 bytes (UIDs, flags) — copied verbatim
fn extend_midi_track_list(
    session: &mut RawSession,
    n: usize,
    names: &[String],
) -> crate::PtResult<()> {
    if n < 2 {
        return Ok(());
    }
    let parent_blocks = collect_by_raw_ct(&session.blocks, 0x2519);
    let Some(parent) = parent_blocks.first().copied() else {
        return Ok(());
    };
    let entries: Vec<(usize, usize)> = collect_by_raw_ct(&parent.children, 0x251a)
        .iter()
        .map(|b| (b.start, b.end))
        .collect();
    if entries.len() < 2 {
        return Ok(());
    }
    let pair_start = entries[entries.len() - 2].0;
    let pair_end = entries[entries.len() - 1].1;
    let template_pair: Vec<u8> = session.data[pair_start..pair_end].to_vec();
    let pair_len = template_pair.len();
    let mut cursor = pair_end;
    for _ in 0..(n - 1) {
        splice(session, cursor, 0, &template_pair);
        cursor += pair_len;
    }

    // Rename each track's pair in reverse-track-order.
    for i in (0..n).rev() {
        let new_name = &names[i];
        if new_name == "ProbeTrack" {
            continue;
        }
        for entry_idx in [2 * i + 1, 2 * i] {
            let entries_now: Vec<(usize, usize)> = {
                let parent_blocks = collect_by_raw_ct(&session.blocks, 0x2519);
                let Some(parent) = parent_blocks.first().copied() else {
                    return Ok(());
                };
                collect_by_raw_ct(&parent.children, 0x251a)
                    .iter()
                    .map(|b| (b.start, b.end))
                    .collect()
            };
            if entry_idx >= entries_now.len() {
                continue;
            }
            let (s, _) = entries_now[entry_idx];
            // Length-prefix at block_start + 11 for 0x251a.
            replace_string(session, s + 11, new_name);
            let is_be = session.is_bigendian;
            session.blocks = crate::raw_block::parse_raw_blocks_pub(&session.data, is_be);
        }
    }
    Ok(())
}

fn patch_track_in_range(
    session: &mut RawSession,
    range_start: usize,
    range_end: usize,
    spec: &NativeTrackSpec,
) {
    // Color, mute, solo, vol, pan — each scoped to blocks whose
    // start lies inside [range_start, range_end).
    patch_color_in_range(session, range_start, range_end, spec.color);
    if spec.mute {
        patch_mute_bit_in_range(session, range_start, range_end, true);
        patch_send_routing_in_range(session, range_start, range_end, false);
    } else if spec.inactive {
        patch_mute_bit_in_range(session, range_start, range_end, true);
        patch_send_routing_in_range(session, range_start, range_end, true);
    } else {
        patch_mute_bit_in_range(session, range_start, range_end, false);
    }
    patch_solo_in_range(session, range_start, range_end, spec.solo);
    patch_solo_defeat_in_range(session, range_start, range_end, spec.solo_defeat);
    patch_volume_in_range(session, range_start, range_end, spec.volume_centibel);
    patch_pan_in_range(session, range_start, range_end, spec.pan);
}

fn find_first_by_ct(blocks: &[RawBlock], ct: u16) -> Option<&RawBlock> {
    for b in blocks {
        if b.content_type_raw == ct {
            return Some(b);
        }
        if let Some(f) = find_first_by_ct(&b.children, ct) {
            return Some(f);
        }
    }
    None
}

fn find_first_by_ct_in_range(
    blocks: &[RawBlock],
    ct: u16,
    range_start: usize,
) -> Option<&RawBlock> {
    for b in blocks {
        if b.start >= range_start && b.content_type_raw == ct {
            return Some(b);
        }
        if let Some(f) = find_first_by_ct_in_range(&b.children, ct, range_start) {
            return Some(f);
        }
    }
    None
}

/// Rename every length-prefixed-string occurrence of "ProbeTrack"
/// inside `[range_start, range_end)` to `new_name`. Splices in
/// reverse-buffer order so within-range positions stay valid.
fn rename_probetrack_in_range(
    session: &mut RawSession,
    range_start: usize,
    range_end: usize,
    new_name: &str,
) {
    let needle = "ProbeTrack".as_bytes();
    let mut pattern = (needle.len() as u32).to_le_bytes().to_vec();
    pattern.extend_from_slice(needle);
    loop {
        // Find all occurrences in [range_start, range_end).
        let positions: Vec<usize> = {
            let data = &session.data;
            let end = range_end.min(data.len());
            let mut out = Vec::new();
            if end > range_start + pattern.len() {
                let mut start = range_start;
                while start + pattern.len() <= end {
                    if let Some(p) = data[start..end]
                        .windows(pattern.len())
                        .position(|w| w == pattern)
                    {
                        let abs = start + p;
                        out.push(abs);
                        start = abs + pattern.len();
                    } else {
                        break;
                    }
                }
            }
            out
        };
        if positions.is_empty() {
            break;
        }
        // Splice the LAST one first — keeps earlier positions valid.
        let last = *positions.last().unwrap();
        replace_string(session, last, new_name);
        // Re-parse block tree so subsequent searches stay correct.
        let is_be = session.is_bigendian;
        session.blocks = crate::raw_block::parse_raw_blocks_pub(&session.data, is_be);
    }
}

fn _unused_rename_last_track_name_occurrences(session: &mut RawSession, old: &str, new: &str) {
    // Find the LAST length-prefixed-string match of `old` and splice
    // ONE occurrence to `new`. The 0x251a/0x1014 pair is consecutive
    // within a track's 0x261c, so two consecutive splices per call
    // are needed to rename one full track. We loop until we've done
    // two splices or no more matches exist within ~32 bytes of the
    // last one (= within the same track's 0x261c).
    let needle = old.as_bytes();
    let mut pattern = (needle.len() as u32).to_le_bytes().to_vec();
    pattern.extend_from_slice(needle);

    // Find ALL occurrences first (positions).
    let mut positions: Vec<usize> = Vec::new();
    {
        let data = &session.data;
        let mut start = 0;
        while let Some(p) = data[start..]
            .windows(pattern.len())
            .position(|w| w == pattern)
        {
            let abs = start + p;
            positions.push(abs);
            start = abs + pattern.len();
        }
    }
    if positions.is_empty() {
        return;
    }
    // Splice the LAST occurrence and its immediate neighbor (within
    // ~512 bytes — they live in the same track's 0x261c block group).
    // Iterate from the back and splice up to 2 occurrences for this
    // track's name pair.
    let last = *positions.last().unwrap();
    replace_string(session, last, new);
    // After the splice, the buffer shifted by (new.len() - old.len())
    // bytes. The remaining earlier occurrences are at their previous
    // positions (we worked from the back). Now check if there's
    // another `old` occurrence within ~512 bytes earlier — that's
    // the pair entry. Re-scan.
    let positions2: Vec<usize> = {
        let data = &session.data;
        let mut out = Vec::new();
        let mut start = 0usize;
        while let Some(p) = data[start..]
            .windows(pattern.len())
            .position(|w| w == pattern)
        {
            let abs = start + p;
            out.push(abs);
            start = abs + pattern.len();
        }
        out
    };
    if let Some(&prev) = positions2.last() {
        // If this previous occurrence is within ~600 bytes of the
        // splice we just did (now at `last`), treat it as the pair
        // partner and splice it too.
        if last >= prev && last - prev < 600 {
            replace_string(session, prev, new);
        }
    }
}

// Range-scoped variants of the existing in-place patchers.
fn for_blocks_in_range<F>(
    blocks: &[RawBlock],
    range_start: usize,
    range_end: usize,
    ct: u16,
    mut f: F,
) where
    F: FnMut(&RawBlock),
{
    fn walk<G: FnMut(&RawBlock)>(
        blocks: &[RawBlock],
        range_start: usize,
        range_end: usize,
        ct: u16,
        f: &mut G,
    ) {
        for b in blocks {
            if b.start >= range_start && b.start < range_end && b.content_type_raw == ct {
                f(b);
            }
            walk(&b.children, range_start, range_end, ct, f);
        }
    }
    walk(blocks, range_start, range_end, ct, &mut f);
}

fn patch_color_in_range(s: &mut RawSession, rs: usize, re: usize, color: u8) {
    let color_i16 = if color == 0 { -2i16 } else { color as i16 };
    let bytes = color_i16.to_le_bytes();
    let blocks = s.blocks.clone();
    for (ct, off) in [(0x200bu16, 106usize), (0x200a, 97), (0x2015, 88)] {
        for_blocks_in_range(&blocks, rs, re, ct, |b| {
            let p = b.start + 9 + off;
            if p + 2 <= s.data.len() {
                s.data[p..p + 2].copy_from_slice(&bytes);
            }
        });
    }
}

fn patch_mute_bit_in_range(s: &mut RawSession, rs: usize, re: usize, mute: bool) {
    let v: u8 = if mute { 1 } else { 0 };
    let blocks = s.blocks.clone();
    for (ct, offs) in [
        (0x1029u16, vec![5usize]),
        (0x260d, vec![14, 447]),
        (0x261b, vec![414, 847]),
        (0x261c, vec![423, 856]),
        (0x2624, vec![436, 869]),
    ] {
        for_blocks_in_range(&blocks, rs, re, ct, |b| {
            for off in &offs {
                let p = b.start + 9 + off;
                if p < s.data.len() {
                    s.data[p] = v;
                }
            }
        });
    }
}

fn patch_send_routing_in_range(s: &mut RawSession, rs: usize, re: usize, enabled: bool) {
    let v: u8 = if enabled { 1 } else { 0 };
    let blocks = s.blocks.clone();
    // First 0x260a inside the range only.
    let mut done = false;
    for_blocks_in_range(&blocks, rs, re, 0x260a, |b| {
        if done {
            return;
        }
        let p = b.start + 9 + 8;
        if p < s.data.len() {
            s.data[p] = v;
        }
        done = true;
    });
}

fn patch_solo_in_range(s: &mut RawSession, rs: usize, re: usize, solo: bool) {
    let v: u8 = if solo { 1 } else { 0 };
    let blocks = s.blocks.clone();
    for (ct, off) in [
        (0x102du16, 162usize),
        (0x261b, 171),
        (0x261c, 180),
        (0x2624, 193),
    ] {
        for_blocks_in_range(&blocks, rs, re, ct, |b| {
            let p = b.start + 9 + off;
            if p < s.data.len() {
                s.data[p] = v;
            }
        });
    }
}

fn patch_solo_defeat_in_range(s: &mut RawSession, rs: usize, re: usize, defeat: bool) {
    let v: u8 = if defeat { 1 } else { 0 };
    let blocks = s.blocks.clone();
    for_blocks_in_range(&blocks, rs, re, 0x200b, |b| {
        let p = b.start + 9 + 268;
        if p < s.data.len() {
            s.data[p] = v;
        }
    });
    for_blocks_in_range(&blocks, rs, re, 0x200a, |b| {
        let p = b.start + 9 + 259;
        if p < s.data.len() {
            s.data[p] = v;
        }
    });
}

fn patch_volume_in_range(s: &mut RawSession, rs: usize, re: usize, vol: i16) {
    let bytes = vol.to_le_bytes();
    let blocks = s.blocks.clone();
    // Write the i32 LE volume into every 0x1029 in range — the parser
    // reads from `0x1029 +1..+5` (in payload bytes, = `b.start + 9 + 1`).
    let vol_i32_bytes = (vol as i32).to_le_bytes();
    for_blocks_in_range(&blocks, rs, re, 0x1029, |b| {
        let p = b.start + 9 + 1;
        if p + 4 <= s.data.len() {
            s.data[p..p + 4].copy_from_slice(&vol_i32_bytes);
        }
    });
    let mut first_260a = true;
    for_blocks_in_range(&blocks, rs, re, 0x260a, |b| {
        if first_260a {
            let p = b.start + 9 + 26;
            if p + 2 <= s.data.len() {
                s.data[p..p + 2].copy_from_slice(&bytes);
            }
            first_260a = false;
        }
    });
    for (ct, off) in [
        (0x260du16, 407usize),
        (0x261b, 807),
        (0x261c, 816),
        (0x2624, 829),
    ] {
        for_blocks_in_range(&blocks, rs, re, ct, |b| {
            let p = b.start + 9 + off;
            if p + 2 <= s.data.len() {
                s.data[p..p + 2].copy_from_slice(&bytes);
            }
        });
    }
}

fn patch_pan_in_range(s: &mut RawSession, rs: usize, re: usize, pan: i16) {
    let bytes = pan.to_le_bytes();
    let blocks = s.blocks.clone();
    // Parser reads pan from `0x1029 +13..+17` (i32 LE).
    let pan_i32 = (pan as i32).to_le_bytes();
    for_blocks_in_range(&blocks, rs, re, 0x1029, |b| {
        let p = b.start + 9 + 13;
        if p + 4 <= s.data.len() {
            s.data[p..p + 4].copy_from_slice(&pan_i32);
        }
    });
    let mut nth_260a = 0;
    for_blocks_in_range(&blocks, rs, re, 0x260a, |b| {
        if nth_260a == 2 {
            let p = b.start + 9 + 26;
            if p + 2 <= s.data.len() {
                s.data[p..p + 2].copy_from_slice(&bytes);
            }
        }
        nth_260a += 1;
    });
    let mut first_260c = true;
    for_blocks_in_range(&blocks, rs, re, 0x260c, |b| {
        if first_260c {
            let p = b.start + 9 + 36;
            if p + 2 <= s.data.len() {
                s.data[p..p + 2].copy_from_slice(&bytes);
            }
            first_260c = false;
        }
    });
    for (ct, off) in [
        (0x260du16, 497usize),
        (0x261b, 897),
        (0x261c, 906),
        (0x2624, 919),
    ] {
        for_blocks_in_range(&blocks, rs, re, ct, |b| {
            let p = b.start + 9 + off;
            if p + 2 <= s.data.len() {
                s.data[p..p + 2].copy_from_slice(&bytes);
            }
        });
    }
}

/// Splice a new track name everywhere "ProbeTrack" appears. Multiple
/// occurrences exist (0x1014 audio entry, 0x251a MIDI list entry).
fn rename_all_track_name_occurrences(session: &mut RawSession, old_name: &str, new_name: &str) {
    if old_name == new_name {
        return;
    }
    // Find all length-prefixed strings equal to old_name and splice.
    // We do this iteratively because each splice shifts all subsequent
    // offsets — re-walk the data each time.
    loop {
        let data = &session.data;
        let needle = old_name.as_bytes();
        // Build pattern: u32 LE length + bytes
        let mut pattern = (needle.len() as u32).to_le_bytes().to_vec();
        pattern.extend_from_slice(needle);

        let pos = data.windows(pattern.len()).position(|w| w == pattern);
        let Some(p) = pos else {
            break;
        };
        replace_string(session, p, new_name);
    }
}

fn patch_color(session: &mut RawSession, color: u8) {
    // The track color lives in each TrackAuxState (0x200b) block as an i16
    // inside the frame `01 <color:i16> 01 .. 01 <height:u16>` — an `0x01`
    // byte at relative offsets +0, +3, +9 (see the parser's `find_color_frame`
    // for the version-independent signature). The color i16 is at frame + 1
    // (-2 / 0xfffe = "no color"). The offset varies, so scan for the frame.
    let ranges: Vec<(usize, usize)> = collect_by_raw_ct(&session.blocks, 0x200b)
        .iter()
        .map(|b| (b.start, b.start + 11 + b.block_size as usize))
        .collect();
    let color_i16 = if color == 0 { -2i16 } else { color as i16 };
    for (lo, hi) in ranges {
        let hi = hi.min(session.data.len());
        let mut p = lo;
        while p + 12 <= hi {
            let d = &session.data;
            if d[p] == 0x01 && d[p + 3] == 0x01 && d[p + 9] == 0x01 {
                session.data[p + 1..p + 3].copy_from_slice(&color_i16.to_le_bytes());
                break;
            }
            p += 1;
        }
    }
}

/// Patch the stored mute bit (the +5 mix-byte and all wrapper mirrors).
///
/// This bit is set by BOTH user-mute and Make-Inactive. It does NOT
/// alone determine effective mute — see `patch_send_routing` for the
/// discriminator.
fn patch_mute_bit(session: &mut RawSession, mute: bool) {
    let v: u8 = if mute { 1 } else { 0 };
    // 0x1029 +5
    for b in collect_by_raw_ct(&session.blocks, 0x1029) {
        let p = b.start + 9 + 5;
        if p < session.data.len() {
            session.data[p] = v;
        }
    }
    // 0x260d +14 and +447 (per-track wrapper mirrors)
    for b in collect_by_raw_ct(&session.blocks, 0x260d) {
        for off in [14usize, 447] {
            let p = b.start + 9 + off;
            if p < session.data.len() {
                session.data[p] = v;
            }
        }
    }
    // 0x261b +414 and +847
    for b in collect_by_raw_ct(&session.blocks, 0x261b) {
        for off in [414usize, 847] {
            let p = b.start + 9 + off;
            if p < session.data.len() {
                session.data[p] = v;
            }
        }
    }
    // 0x261c +423 and +856
    for b in collect_by_raw_ct(&session.blocks, 0x261c) {
        for off in [423usize, 856] {
            let p = b.start + 9 + off;
            if p < session.data.len() {
                session.data[p] = v;
            }
        }
    }
    // 0x2624 +436 and +869
    for b in collect_by_raw_ct(&session.blocks, 0x2624) {
        for off in [436usize, 869] {
            let p = b.start + 9 + off;
            if p < session.data.len() {
                session.data[p] = v;
            }
        }
    }
}

/// Patch the send-routing enabled flag (`0x260a[0] +8`).
///
/// User-mute clears this byte (effective mute). Make-Inactive leaves
/// it set (the track is silent but routing still nominally active).
fn patch_send_routing(session: &mut RawSession, enabled: bool) {
    let v: u8 = if enabled { 1 } else { 0 };
    let blocks_260a = collect_by_raw_ct(&session.blocks, 0x260a);
    if let Some(b) = blocks_260a.first() {
        let p = b.start + 9 + 8;
        if p < session.data.len() {
            session.data[p] = v;
        }
    }
}

/// Patch solo-defeat (`0x200b +268`, with `0x200a +259` mirror).
fn patch_solo_defeat(session: &mut RawSession, defeat: bool) {
    let v: u8 = if defeat { 1 } else { 0 };
    for b in collect_by_raw_ct(&session.blocks, 0x200b) {
        let p = b.start + 9 + 268;
        if p < session.data.len() {
            session.data[p] = v;
        }
    }
    for b in collect_by_raw_ct(&session.blocks, 0x200a) {
        let p = b.start + 9 + 259;
        if p < session.data.len() {
            session.data[p] = v;
        }
    }
}

/// Write a mute automation envelope into `0x260a[1]`.
///
/// Block layout for the automation child:
/// - 28-byte header
///   - +4..+8 u32 LE: payload size (header tail + breakpoints)
///   - +10 u8: total breakpoint count (1 implicit + N user points)
///   - +16 u8: user breakpoint count (N)
/// - N × 6 bytes: each breakpoint is `u32 LE time_samples + u8 value + u8 shape`
///
/// We splice the breakpoint payload at the end of `0x260a[1]` and
/// patch the header counters/size in place. Ancestor block sizes
/// cascade through `splice`.
fn write_mute_automation(session: &mut RawSession, points: &[MuteAutomationPoint]) {
    // The mute-automation envelope lives in the SECOND 0x260a child
    // of the first 0x260d wrapper (the per-track mix wrapper). Use
    // the nested lookup so the writer matches the parser's view.
    let envelope = {
        let wrappers = collect_by_raw_ct(&session.blocks, 0x260d);
        let Some(wrap) = wrappers.first() else {
            return;
        };
        let mut children_260a = Vec::new();
        for c in &wrap.children {
            if c.content_type_raw == 0x260a {
                children_260a.push((c.start, c.end));
            }
        }
        let Some(&entry) = children_260a.get(1) else {
            return;
        };
        entry
    };
    let (block_start, block_end) = envelope;
    let payload_start = block_start + 9;
    let n = points.len() as u8;
    let total = n.saturating_add(1);
    let cnt_total = payload_start + 10;
    let cnt_user = payload_start + 16;
    if cnt_total < session.data.len() {
        session.data[cnt_total] = total;
    }
    if cnt_user < session.data.len() {
        session.data[cnt_user] = n;
    }

    let mut bp = Vec::with_capacity(points.len() * 6);
    for pt in points {
        bp.extend_from_slice(&pt.time_samples.to_le_bytes());
        bp.push(if pt.muted { 1 } else { 0 });
        bp.push(0); // shape: square/step
    }

    // Patch in-payload size at +4 (u32 LE).
    let size_off = payload_start + 4;
    if size_off + 4 <= session.data.len() {
        let cur = u32::from_le_bytes([
            session.data[size_off],
            session.data[size_off + 1],
            session.data[size_off + 2],
            session.data[size_off + 3],
        ]);
        let new = cur.wrapping_add(bp.len() as u32);
        session.data[size_off..size_off + 4].copy_from_slice(&new.to_le_bytes());
    }

    // Splice at the exact payload offset the parser reads from
    // (`payload_start + 28`). This is inside the block (so splice()
    // updates the 0x260a[1] block_size) and matches the converter's
    // user-breakpoint location. Pushes the trailing pad bytes forward
    // but parser stops at user_count.
    let splice_at = payload_start + 28;
    let _ = block_end;
    splice(session, splice_at, 0, &bp);
}

/// Symmetric writer for the volume automation envelope at `0x260a[0]`.
/// Same layout as the mute envelope (`0x260a[1]`) but each breakpoint
/// is `u32 time + i16 centibel value` (no shape/byte pad).
fn write_volume_automation(session: &mut RawSession, points: &[VolumeAutomationPoint]) {
    let envelope = {
        let wrappers = collect_by_raw_ct(&session.blocks, 0x260d);
        let Some(wrap) = wrappers.first() else {
            return;
        };
        let mut children_260a = Vec::new();
        for c in &wrap.children {
            if c.content_type_raw == 0x260a {
                children_260a.push((c.start, c.end));
            }
        }
        let Some(&entry) = children_260a.first() else {
            return;
        };
        entry
    };
    let (block_start, _block_end) = envelope;
    let payload_start = block_start + 9;
    let n = points.len() as u8;
    let total = n.saturating_add(1);
    let cnt_total = payload_start + 10;
    let cnt_user = payload_start + 16;
    if cnt_total < session.data.len() {
        session.data[cnt_total] = total;
    }
    if cnt_user < session.data.len() {
        session.data[cnt_user] = n;
    }
    let mut bp = Vec::with_capacity(points.len() * 6);
    for pt in points {
        bp.extend_from_slice(&pt.time_samples.to_le_bytes());
        bp.extend_from_slice(&pt.value_centibel.to_le_bytes());
    }
    let size_off = payload_start + 4;
    if size_off + 4 <= session.data.len() {
        let cur = u32::from_le_bytes([
            session.data[size_off],
            session.data[size_off + 1],
            session.data[size_off + 2],
            session.data[size_off + 3],
        ]);
        let new = cur.wrapping_add(bp.len() as u32);
        session.data[size_off..size_off + 4].copy_from_slice(&new.to_le_bytes());
    }
    let splice_at = payload_start + 28;
    splice(session, splice_at, 0, &bp);
}

fn patch_solo(session: &mut RawSession, solo: bool) {
    let v: u8 = if solo { 1 } else { 0 };
    // 0x102d +162
    for b in collect_by_raw_ct(&session.blocks, 0x102d) {
        let p = b.start + 9 + 162;
        if p < session.data.len() {
            session.data[p] = v;
        }
    }
    // 0x261b +171, 0x261c +180, 0x2624 +193 (mirrors)
    for (ct, off) in [(0x261bu16, 171usize), (0x261c, 180), (0x2624, 193)] {
        for b in collect_by_raw_ct(&session.blocks, ct) {
            let p = b.start + 9 + off;
            if p < session.data.len() {
                session.data[p] = v;
            }
        }
    }
}

fn patch_volume(session: &mut RawSession, vol: i16) {
    let bytes = vol.to_le_bytes();
    // 0x260a[0] +26..+27 (master-send volume — only the FIRST 0x260a).
    // 0x260d +407..+408 mirror.
    // 0x261b +807..+808, 0x261c +816..+817, 0x2624 +829..+830 mirrors.
    let blocks_260a = collect_by_raw_ct(&session.blocks, 0x260a);
    if let Some(b) = blocks_260a.first() {
        let p = b.start + 9 + 26;
        if p + 2 <= session.data.len() {
            session.data[p..p + 2].copy_from_slice(&bytes);
        }
    }
    let mirror = [
        (0x260du16, 407usize),
        (0x261b, 807),
        (0x261c, 816),
        (0x2624, 829),
    ];
    for (ct, off) in mirror {
        for b in collect_by_raw_ct(&session.blocks, ct) {
            let p = b.start + 9 + off;
            if p + 2 <= session.data.len() {
                session.data[p..p + 2].copy_from_slice(&bytes);
            }
        }
    }
}

fn patch_pan(session: &mut RawSession, pan: i16) {
    let bytes = pan.to_le_bytes();
    // 0x260a[2] +26..+27 (left-channel pan — the THIRD 0x260a).
    let blocks_260a = collect_by_raw_ct(&session.blocks, 0x260a);
    if let Some(b) = blocks_260a.get(2) {
        let p = b.start + 9 + 26;
        if p + 2 <= session.data.len() {
            session.data[p..p + 2].copy_from_slice(&bytes);
        }
    }
    // 0x260c[0] +36..+37 mirror
    let blocks_260c = collect_by_raw_ct(&session.blocks, 0x260c);
    if let Some(b) = blocks_260c.first() {
        let p = b.start + 9 + 36;
        if p + 2 <= session.data.len() {
            session.data[p..p + 2].copy_from_slice(&bytes);
        }
    }
    let mirror = [
        (0x260du16, 497usize),
        (0x261b, 897),
        (0x261c, 906),
        (0x2624, 919),
    ];
    for (ct, off) in mirror {
        for b in collect_by_raw_ct(&session.blocks, ct) {
            let p = b.start + 9 + off;
            if p + 2 <= session.data.len() {
                session.data[p..p + 2].copy_from_slice(&bytes);
            }
        }
    }
}

fn collect_by_raw_ct(
    blocks: &[crate::raw_block::RawBlock],
    ct: u16,
) -> Vec<&crate::raw_block::RawBlock> {
    let mut out = Vec::new();
    fn walk<'a>(
        blocks: &'a [crate::raw_block::RawBlock],
        ct: u16,
        out: &mut Vec<&'a crate::raw_block::RawBlock>,
    ) {
        for b in blocks {
            if b.content_type_raw == ct {
                out.push(b);
            }
            walk(&b.children, ct, out);
        }
    }
    walk(blocks, ct, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_baseline_identity() {
        // Writing default spec should produce a file that round-trips through
        // our parser to the same baseline values.
        let spec = NativeTrackSpec::default();
        let bytes = write_single_track_ptx(&spec).unwrap();

        // Write to a temp file and re-parse via the full read_session
        // (which gives us track-level fields).
        let tmp = std::env::temp_dir().join("native_roundtrip_baseline.ptx");
        std::fs::write(&tmp, &bytes).unwrap();
        let session = crate::read_session(tmp.to_str().unwrap(), 48000).unwrap();
        let all: Vec<_> = session.all_tracks().collect();
        assert!(
            all.iter().any(|t| t.name == "ProbeTrack"),
            "track ProbeTrack should be present after round-trip"
        );
    }

    fn parse_native(spec: &NativeTrackSpec) -> crate::types::ProToolsSession {
        let bytes = write_single_track_ptx(spec).unwrap();
        let tmp = std::env::temp_dir().join(format!("native_{}.ptx", spec.name));
        std::fs::write(&tmp, &bytes).unwrap();
        crate::read_session(tmp.to_str().unwrap(), 48000).unwrap()
    }

    #[test]
    fn write_with_color() {
        let spec = NativeTrackSpec {
            color: 0x18,
            ..NativeTrackSpec::default()
        };
        let session = parse_native(&spec);
        let tracks: Vec<_> = session.all_tracks().collect();
        let t = tracks
            .iter()
            .find(|t| t.name == spec.name)
            .expect("track present");
        assert_eq!(t.color_byte, 0x18);
    }

    #[test]
    fn write_with_solo() {
        let spec = NativeTrackSpec {
            solo: true,
            name: "SoloTrack".to_string(),
            ..NativeTrackSpec::default()
        };
        let session = parse_native(&spec);
        let tracks: Vec<_> = session.all_tracks().collect();
        let t = tracks
            .iter()
            .find(|t| t.name == spec.name)
            .expect("track present");
        assert!(t.solo, "solo should round-trip");
    }

    #[test]
    fn write_with_mute() {
        let spec = NativeTrackSpec {
            mute: true,
            name: "MutedTrack".to_string(),
            ..NativeTrackSpec::default()
        };
        let session = parse_native(&spec);
        let tracks: Vec<_> = session.all_tracks().collect();
        let t = tracks.iter().find(|t| t.name == spec.name).expect("track");
        assert!(t.mute, "mute should round-trip as effective-mute");
        assert!(
            !t.inactive,
            "user-mute should NOT set inactive (send routing cleared)"
        );
    }

    #[test]
    fn write_with_inactive() {
        let spec = NativeTrackSpec {
            inactive: true,
            name: "InactiveTrack".to_string(),
            ..NativeTrackSpec::default()
        };
        let session = parse_native(&spec);
        let tracks: Vec<_> = session.all_tracks().collect();
        let t = tracks.iter().find(|t| t.name == spec.name).expect("track");
        assert!(t.inactive, "inactive should round-trip");
        assert!(
            !t.mute,
            "inactive (with send routing kept) is not effective-mute"
        );
    }

    #[test]
    fn write_with_solo_defeat() {
        let spec = NativeTrackSpec {
            solo_defeat: true,
            name: "DefeatTrack".to_string(),
            ..NativeTrackSpec::default()
        };
        let session = parse_native(&spec);
        let tracks: Vec<_> = session.all_tracks().collect();
        let t = tracks.iter().find(|t| t.name == spec.name).expect("track");
        assert!(t.solo_defeat, "solo_defeat should round-trip");
    }

    #[test]
    fn write_with_volume_automation_round_trip() {
        let spec = NativeTrackSpec {
            name: "VolEnvTrack".to_string(),
            volume_automation: vec![
                VolumeAutomationPoint {
                    time_samples: 48000,
                    value_centibel: -60,
                },
                VolumeAutomationPoint {
                    time_samples: 96000,
                    value_centibel: 60,
                },
            ],
            ..NativeTrackSpec::default()
        };
        let session = parse_native(&spec);
        let tracks: Vec<_> = session.all_tracks().collect();
        let t = tracks
            .iter()
            .find(|t| t.name == "VolEnvTrack")
            .expect("track");
        assert_eq!(t.volume_automation.len(), 2);
        assert_eq!(t.volume_automation[0].time_samples, 48000);
        assert_eq!(t.volume_automation[0].value_centibel, -60);
        assert_eq!(t.volume_automation[1].time_samples, 96000);
        assert_eq!(t.volume_automation[1].value_centibel, 60);
    }

    #[test]
    fn write_with_mute_automation_round_trip() {
        // Single-track + two mute breakpoints. Parser surfaces them as
        // Track.mute_automation; spec values should round-trip.
        let spec = NativeTrackSpec {
            name: "EnvTrack".to_string(),
            mute_automation: vec![
                MuteAutomationPoint {
                    time_samples: 48000,
                    muted: true,
                },
                MuteAutomationPoint {
                    time_samples: 96000,
                    muted: false,
                },
            ],
            ..NativeTrackSpec::default()
        };
        let session = parse_native(&spec);
        let tracks: Vec<_> = session.all_tracks().collect();
        let t = tracks.iter().find(|t| t.name == "EnvTrack").expect("track");
        assert_eq!(
            t.mute_automation.len(),
            2,
            "expected 2 breakpoints, got {}",
            t.mute_automation.len()
        );
        assert_eq!(t.mute_automation[0].time_samples, 48000);
        assert!(t.mute_automation[0].muted);
        assert_eq!(t.mute_automation[1].time_samples, 96000);
        assert!(!t.mute_automation[1].muted);
    }

    #[test]
    fn write_with_rename() {
        let spec = NativeTrackSpec {
            name: "MyTrack".to_string(),
            ..NativeTrackSpec::default()
        };
        let session = parse_native(&spec);
        let names: Vec<&str> = session.all_tracks().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"MyTrack"), "expected MyTrack in {names:?}");
    }

    fn parse_multi(specs: &[NativeTrackSpec]) -> crate::types::ProToolsSession {
        let session_spec = NativeSessionSpec {
            tracks: specs.to_vec(),
        };
        let bytes = write_session_ptx(&session_spec).unwrap();
        // Unique temp file per test invocation to avoid parallel collisions.
        let id: String = specs
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join("_");
        let tmp = std::env::temp_dir().join(format!("native_multi_{}_{}.ptx", specs.len(), id));
        std::fs::write(&tmp, &bytes).unwrap();
        crate::read_session(tmp.to_str().unwrap(), 48000).unwrap()
    }

    #[test]
    fn write_multi_produces_cloned_261c_blocks() {
        // Structural-only test: write_session_ptx clones 0x261c
        // (the per-track wrapper) N times. The parser, however,
        // currently reports tracks based on 0x1014 (audio track list)
        // entries — NOT 0x261c count — and the baseline is stereo,
        // so N 0x261c blocks yield only 2 parsed audio tracks (the
        // two channels of the single baseline 0x1014).
        //
        // To produce N TRUE PT tracks (each appearing distinctly in
        // the parser), the outer index lists 0x1015 / 0x1054 / 0x2519
        // / 0x2107 must also gain entries per track. See GH #26.
        //
        // This test asserts only the structural fact that N×0x261c
        // exist in the output.
        let specs = vec![NativeTrackSpec::default(); 3];
        let session_spec = NativeSessionSpec { tracks: specs };
        let bytes = write_session_ptx(&session_spec).unwrap();
        let tmp = std::env::temp_dir().join("native_multi_struct.ptx");
        std::fs::write(&tmp, &bytes).unwrap();
        let s = crate::parse_raw(std::fs::read(&tmp).unwrap()).unwrap();
        let n261c = collect_by_raw_ct(&s.blocks, 0x261c).len();
        assert_eq!(n261c, 3, "expected 3× 0x261c structural clones");
    }

    #[test]
    fn write_two_distinct_tracks() {
        let specs = vec![
            NativeTrackSpec {
                name: "Alpha".into(),
                ..Default::default()
            },
            NativeTrackSpec {
                name: "Beta".into(),
                ..Default::default()
            },
        ];
        let session = parse_multi(&specs);
        let names: Vec<String> = session.all_tracks().map(|t| t.name.clone()).collect();
        assert!(names.contains(&"Alpha".into()), "got {names:?}");
        assert!(names.contains(&"Beta".into()), "got {names:?}");
    }

    #[test]
    fn write_multi_with_per_track_color_and_solo() {
        // Per-track color + solo work via in-range patching. Mute is
        // tracked separately — see write_multi_track_mute_known_issue.
        let specs = vec![
            NativeTrackSpec {
                name: "Drums".into(),
                color: 0x12,
                ..Default::default()
            },
            NativeTrackSpec {
                name: "Bass".into(),
                ..Default::default()
            },
            NativeTrackSpec {
                name: "Guitar".into(),
                solo: true,
                ..Default::default()
            },
        ];
        let session = parse_multi(&specs);
        let tracks: Vec<_> = session.all_tracks().collect();
        let drums = tracks.iter().find(|t| t.name == "Drums").expect("Drums");
        let guitar = tracks.iter().find(|t| t.name == "Guitar").expect("Guitar");
        assert_eq!(drums.color_byte, 0x12, "Drums color");
        assert!(guitar.solo, "Guitar solo");
    }

    #[test]
    fn write_multi_track_mute() {
        let specs = vec![
            NativeTrackSpec {
                name: "Drums".into(),
                ..Default::default()
            },
            NativeTrackSpec {
                name: "Bass".into(),
                mute: true,
                ..Default::default()
            },
        ];
        let session = parse_multi(&specs);
        let bass = session
            .all_tracks()
            .find(|t| t.name == "Bass")
            .expect("Bass");
        assert!(bass.mute, "Bass should be muted (currently fails)");
    }

    #[test]
    fn write_multi_with_vol_pan() {
        let specs = vec![
            NativeTrackSpec {
                name: "Quiet".into(),
                volume_centibel: -120, // -12 dB
                ..Default::default()
            },
            NativeTrackSpec {
                name: "Loud".into(),
                volume_centibel: 60, // +6 dB
                ..Default::default()
            },
        ];
        let session = parse_multi(&specs);
        let tracks: Vec<_> = session.all_tracks().collect();
        let quiet = tracks.iter().find(|t| t.name == "Quiet").expect("Quiet");
        let loud = tracks.iter().find(|t| t.name == "Loud").expect("Loud");
        assert_eq!(quiet.volume_centibel, -120, "Quiet vol");
        assert_eq!(loud.volume_centibel, 60, "Loud vol");
    }

    #[test]
    fn write_five_distinct_tracks() {
        let specs: Vec<_> = ["Drums", "Bass", "Guitar", "Synth", "Vox"]
            .iter()
            .map(|n| NativeTrackSpec {
                name: (*n).into(),
                ..Default::default()
            })
            .collect();
        let session = parse_multi(&specs);
        let names: Vec<String> = session.all_tracks().map(|t| t.name.clone()).collect();
        for expected in ["Drums", "Bass", "Guitar", "Synth", "Vox"] {
            assert!(
                names.contains(&expected.into()),
                "missing {expected:?}: got {names:?}"
            );
        }
    }
}
