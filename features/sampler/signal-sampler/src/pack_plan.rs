//! Prioritized download plans for `.signalpack`s — the network
//! range-streaming planner (W7 of crates/signal/docs/browser-keys-rig.md).
//!
//! A plan tiles the pack file into segments, each with a fetch rank:
//!
//! - **Rank 0** covers everything `SignalPcmPack::open` touches — the
//!   64-byte [`PackFileHeader`](fts_sample::cache::PackFileHeader) plus
//!   the text index (which embeds the library spec). The header and the
//!   index are not contiguous (the index sits after the audio body), so
//!   rank 0 is normally TWO segments. Once they land, the pack opens and
//!   the lanes load; audio fills in behind.
//! - **Ranks 1..** are one audio entry each, ordered *musically*:
//!   primary = velocity-layer distance from the middle layer, secondary =
//!   key distance from middle C, tertiary = round-robin index (rr 0
//!   first). Entries whose metadata cannot be resolved go last, in file
//!   order.
//!
//! Ranking metadata comes from the pack's embedded spec: zone mode
//! (`spec.zones` non-empty) reads each zone's `vel_min..=vel_max`,
//! `root_key` and `rr_index`; convention mode parses the entry filename
//! ([`crate::sample_map::parse_sample_stem`]) and orders its dynamic
//! label on the canonical ppp→fff scale.
//!
//! Segments cover `total` bytes exactly once — any byte the index does
//! not account for (defensive; well-formed packs have none) is emitted as
//! a `"gap"` segment fetched last, so a byte-complete download and a
//! whole-file sha256 stay possible.

use std::path::Path;

use crate::{SamplerError, SignalPcmPack};

/// One contiguous byte span of the plan. Mirrors
/// `signal_packs_proto::PackSegment` without the proto dependency — the
/// pack-library host converts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanSegment {
    pub start: u64,
    pub len: u64,
    /// Fetch priority, 0 first. Ranks are dense ordinals (0, 1, 2, …).
    pub rank: u32,
    /// Diagnostics-only label ("header", "index", the entry's path…).
    pub label: String,
}

/// A whole pack's plan: segments tiling `total` bytes exactly once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackPlanOut {
    pub total: u64,
    pub segments: Vec<PlanSegment>,
}

/// Open `path` (header + index only — no audio) and plan it. `total` is
/// taken from the file's on-disk size.
#[cfg(not(target_arch = "wasm32"))]
pub fn plan_pack_file(path: &Path) -> Result<PackPlanOut, SamplerError> {
    let total = std::fs::metadata(path)?.len();
    let pack = SignalPcmPack::open(path)?;
    Ok(plan_pack(&pack, total))
}

/// Plan an already-opened pack. `total` must be the pack file's full byte
/// length (the opened pack knows it too, but callers streaming from disk
/// already have it).
pub fn plan_pack(pack: &SignalPcmPack, total: u64) -> PackPlanOut {
    // ── Rank 0: what `SignalPcmPack::from_pack_bytes` reads ──────────────
    // The header parse reads [0, 64); the index parse reads
    // [index_offset, index_offset + index_len) — recover that span from
    // the header rather than trusting entry layout.
    let header_len = fts_sample::cache::PackFileHeader::LEN as u64;
    let mut rank0: Vec<(u64, u64, &str)> = vec![(0, header_len.min(total), "header")];
    let (index_start, index_len) = pack.index_span();
    if index_start >= header_len && index_len > 0 {
        rank0.push((index_start, index_len, "index"));
    }

    // ── Audio entries, ranked musically ──────────────────────────────────
    let ranking = EntryRanking::from_pack(pack);
    let mut entries: Vec<(MusicalRank, u64, u64, String)> = pack
        .entries_iter()
        .map(|(path, entry)| {
            let label = path.to_string_lossy().into_owned();
            let rank = ranking.rank_of(path);
            (rank, entry.offset(), entry.bytes(), label)
        })
        .collect();
    entries.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));

    // ── Assemble + assign dense rank ordinals ────────────────────────────
    let mut segments: Vec<PlanSegment> = rank0
        .into_iter()
        .map(|(start, len, label)| PlanSegment { start, len, rank: 0, label: label.into() })
        .collect();
    for (ordinal, (_, start, len, label)) in entries.into_iter().enumerate() {
        if len == 0 {
            continue;
        }
        segments.push(PlanSegment { start, len, rank: ordinal as u32 + 1, label });
    }

    fill_gaps(&mut segments, total);
    PackPlanOut { total, segments }
}

/// Emit `"gap"` segments (fetched last) for any bytes the plan does not
/// yet cover, so segments tile `total` exactly once. Overlaps would mean
/// a corrupt index; the guard trims them defensively rather than
/// double-fetching.
fn fill_gaps(segments: &mut Vec<PlanSegment>, total: u64) {
    let max_rank = segments.iter().map(|s| s.rank).max().unwrap_or(0);
    let mut spans: Vec<(u64, u64)> = segments.iter().map(|s| (s.start, s.start + s.len)).collect();
    spans.sort_unstable();
    let mut gaps: Vec<(u64, u64)> = Vec::new();
    let mut at = 0u64;
    for (start, end) in spans {
        if start > at {
            gaps.push((at, start));
        }
        at = at.max(end);
    }
    if at < total {
        gaps.push((at, total));
    }
    for (i, (start, end)) in gaps.into_iter().enumerate() {
        segments.push(PlanSegment {
            start,
            len: end - start,
            rank: max_rank + 1 + i as u32,
            label: "gap".into(),
        });
    }
}

/// Musical fetch priority of one entry — lower fetches first.
/// (velocity-layer distance from the middle layer, key distance from
/// middle C, round-robin index, known-metadata flag last so unknowns sort
/// behind everything ranked).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct MusicalRank {
    /// 0 = metadata resolved, 1 = unknown (sorts last, file order via the
    /// offset tiebreak in the caller).
    unknown: u8,
    vel_layer_dist: u32,
    note_dist: u32,
    rr: u32,
}

impl MusicalRank {
    const UNKNOWN: MusicalRank =
        MusicalRank { unknown: 1, vel_layer_dist: 0, note_dist: 0, rr: 0 };
}

/// Canonical dynamic-label order for convention-mode ranking.
const DYNAMIC_ORDER: [&str; 10] =
    ["pppp", "ppp", "pp", "p", "mp", "mf", "f", "ff", "fff", "ffff"];

/// Index of `dynamic` on the canonical scale; unknown labels land in the
/// middle (so they neither jump the queue nor sink).
fn canonical_dynamic_index(dynamic: &str) -> usize {
    DYNAMIC_ORDER
        .iter()
        .position(|d| d.eq_ignore_ascii_case(dynamic))
        .unwrap_or(DYNAMIC_ORDER.len() / 2)
}

/// Pre-computed ranking context: the pack's velocity layers plus a
/// per-entry-path rank lookup built from the embedded spec (zone mode) or
/// the filename convention.
struct EntryRanking {
    /// entry path (as stored in the index) → rank. Zone mode fills this
    /// eagerly; convention mode ranks lazily per path.
    by_path: std::collections::HashMap<std::path::PathBuf, MusicalRank>,
    /// Distinct dynamic labels present (convention mode), sorted on the
    /// canonical scale — layer index space for the middle-layer distance.
    dynamics: Vec<String>,
    zone_mode: bool,
}

impl EntryRanking {
    fn from_pack(pack: &SignalPcmPack) -> Self {
        let spec = crate::pack::parse_embedded_spec(pack).ok();
        if let Some(spec) = spec.as_ref().filter(|s| !s.zones.is_empty()) {
            return Self::from_zones(pack, spec);
        }
        Self::from_convention(pack)
    }

    /// Zone mode: velocity layers are the distinct (vel_min, vel_max)
    /// bands across all zones, ordered by band midpoint; each entry takes
    /// the best (minimum) rank over the zones that reference it.
    fn from_zones(pack: &SignalPcmPack, spec: &crate::LibrarySpec) -> Self {
        let mut bands: Vec<(u8, u8)> =
            spec.zones.iter().map(|z| (z.vel_min, z.vel_max)).collect();
        bands.sort_unstable_by_key(|(lo, hi)| (u16::from(*lo) + u16::from(*hi), *lo));
        bands.dedup();
        let middle = (bands.len().saturating_sub(1) / 2) as i64;

        let mut by_path = std::collections::HashMap::new();
        for z in &spec.zones {
            let layer = bands
                .iter()
                .position(|b| *b == (z.vel_min, z.vel_max))
                .unwrap_or(0) as i64;
            let key_mid = i64::from(z.key_min) + i64::from(z.key_max - z.key_min) / 2;
            let rank = MusicalRank {
                unknown: 0,
                vel_layer_dist: (layer - middle).unsigned_abs() as u32,
                note_dist: (key_mid - 60).unsigned_abs() as u32,
                rr: z.rr_index,
            };
            // The zone file may be stored under a longer path in the
            // index — resolve to the index's own key so `rank_of` (which
            // looks up by index path) hits.
            let file = Path::new(&z.file);
            let key = index_key_for(pack, file);
            let Some(key) = key else { continue };
            by_path
                .entry(key)
                .and_modify(|r: &mut MusicalRank| *r = (*r).min(rank))
                .or_insert(rank);
        }
        Self { by_path, dynamics: Vec::new(), zone_mode: true }
    }

    /// Convention mode: rank from the filename's parsed
    /// (dynamic, note, rr).
    fn from_convention(pack: &SignalPcmPack) -> Self {
        let mut dynamics: Vec<String> = pack
            .entry_paths()
            .filter_map(|p| p.file_stem()?.to_str())
            .filter_map(crate::sample_map::parse_sample_stem)
            .map(|k| k.dynamic)
            .collect();
        dynamics.sort_by_key(|d| canonical_dynamic_index(d));
        dynamics.dedup();
        Self { by_path: std::collections::HashMap::new(), dynamics, zone_mode: false }
    }

    fn rank_of(&self, path: &Path) -> MusicalRank {
        if self.zone_mode {
            return self.by_path.get(path).copied().unwrap_or(MusicalRank::UNKNOWN);
        }
        let Some(key) = path.file_stem().and_then(|s| s.to_str()) else {
            return MusicalRank::UNKNOWN;
        };
        let Some(parsed) = crate::sample_map::parse_sample_stem(key) else {
            return MusicalRank::UNKNOWN;
        };
        let layer = self
            .dynamics
            .iter()
            .position(|d| *d == parsed.dynamic)
            .unwrap_or(self.dynamics.len() / 2) as i64;
        let middle = (self.dynamics.len().saturating_sub(1) / 2) as i64;
        MusicalRank {
            unknown: 0,
            vel_layer_dist: (layer - middle).unsigned_abs() as u32,
            note_dist: (i64::from(parsed.note) - 60).unsigned_abs() as u32,
            rr: parsed.rr as u32,
        }
    }
}

/// Resolve a spec-relative zone file to the path key the pack index
/// stores it under (the index may keep a shorter suffix of the source
/// path, or vice versa).
fn index_key_for(pack: &SignalPcmPack, file: &Path) -> Option<std::path::PathBuf> {
    // Exact hit first.
    if pack.entries_iter().any(|(p, _)| p == file) {
        return Some(file.to_path_buf());
    }
    // Suffix match either direction (mirrors `entry_for_path`).
    pack.entry_paths()
        .find(|p| p.ends_with(file) || file.ends_with(p))
        .map(Path::to_path_buf)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use fts_sample::cache::{create_signal_pack_with, PackCodec, PackSpecSource};

    /// Write a tiny mono wav so the pack encoder has real audio to pack.
    fn write_wav(path: &Path, frames: usize) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).expect("wav create");
        for i in 0..frames {
            w.write_sample(((i % 64) as i16) << 6).expect("wav sample");
        }
        w.finalize().expect("wav finalize");
    }

    /// Build a small ZONED pack: three velocity layers at three keys
    /// (24 / 60 / 96), the middle key's middle layer with two RRs.
    fn build_zoned_pack(dir: &Path) -> std::path::PathBuf {
        let samples = dir.join("samples");
        std::fs::create_dir_all(&samples).expect("samples dir");
        let files = [
            "n24_lo.wav", "n24_mid.wav", "n24_hi.wav", "n60_lo.wav", "n60_mid.wav",
            "n60_mid_rr1.wav", "n60_hi.wav", "n96_lo.wav", "n96_mid.wav", "n96_hi.wav",
        ];
        for (i, f) in files.iter().enumerate() {
            write_wav(&samples.join(f), 256 + i * 32);
        }
        let zone = |file: &str, key: u8, vel_min: u8, vel_max: u8, rr: u32| {
            format!(
                "  {{\n    file \"{file}\"\n    key_min {k}\n    key_max {k}\n    \
                 root_key {k}\n    vel_min {vel_min}\n    vel_max {vel_max}\n    \
                 rr_index {rr}\n  }}\n",
                k = key
            )
        };
        let mut spec = String::from("name \"plan-test\"\nzones (\n");
        for &key in &[24u8, 60, 96] {
            spec.push_str(&zone(&format!("n{key}_lo.wav"), key, 0, 42, 0));
            spec.push_str(&zone(&format!("n{key}_mid.wav"), key, 43, 85, 0));
            spec.push_str(&zone(&format!("n{key}_hi.wav"), key, 86, 127, 0));
        }
        spec.push_str(&zone("n60_mid_rr1.wav", 60, 43, 85, 1));
        spec.push_str(")\n");

        let pack_path = dir.join("plan-test.signalpack");
        let paths: Vec<std::path::PathBuf> =
            files.iter().map(|f| samples.join(f)).collect();
        create_signal_pack_with(
            &pack_path,
            PackSpecSource::Text { text: &spec, format: "styx" },
            &samples,
            paths.iter().map(|p| p.as_path()),
            PackCodec::OggVorbis { quality: 0.4 },
        )
        .expect("pack build");
        pack_path
    }

    fn segment_label_order(plan: &PackPlanOut) -> Vec<&str> {
        let mut segs: Vec<&PlanSegment> = plan.segments.iter().collect();
        segs.sort_by_key(|s| (s.rank, s.start));
        segs.iter().map(|s| s.label.as_str()).collect()
    }

    #[test]
    fn rank0_covers_exactly_what_open_reads() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pack_path = build_zoned_pack(dir.path());
        let plan = plan_pack_file(&pack_path).expect("plan");
        let pack = SignalPcmPack::open(&pack_path).expect("open");
        let (index_start, index_len) = pack.index_span();

        let rank0: Vec<&PlanSegment> =
            plan.segments.iter().filter(|s| s.rank == 0).collect();
        assert_eq!(rank0.len(), 2, "header + index");
        assert!(rank0.iter().any(|s| s.start == 0 && s.len == 64), "the 64-byte header");
        assert!(
            rank0.iter().any(|s| s.start == index_start && s.len == index_len),
            "the index span ({index_start}+{index_len}) — got {rank0:?}"
        );

        // Proof by construction: a pack made of ONLY the rank-0 bytes
        // (rest zeroed) must open — header + index + embedded spec all
        // parse without touching audio.
        let full = std::fs::read(&pack_path).expect("read pack");
        let mut sparse = vec![0u8; full.len()];
        for s in &rank0 {
            let (a, b) = (s.start as usize, (s.start + s.len) as usize);
            sparse[a..b].copy_from_slice(&full[a..b]);
        }
        let opened = SignalPcmPack::open_bytes(sparse).expect("rank-0-only open");
        assert_eq!(opened.entry_count(), pack.entry_count());
        assert!(opened.embedded_spec().is_some());
    }

    #[test]
    fn middle_c_mid_velocity_ranks_first_and_extremes_last() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pack_path = build_zoned_pack(dir.path());
        let plan = plan_pack_file(&pack_path).expect("plan");
        let order = segment_label_order(&plan);
        // After header + index: middle key's middle velocity layer, rr 0.
        assert_eq!(&order[..3], &["header", "index", "n60_mid.wav"]);
        // Its RR sibling comes after rr 0 but before other keys' mids
        // (same layer + key, higher rr).
        assert_eq!(order[3], "n60_mid_rr1.wav");
        // Every mid-layer entry precedes every lo/hi-layer entry.
        let pos = |l: &str| order.iter().position(|o| *o == l).unwrap_or(usize::MAX);
        for mid in ["n60_mid.wav", "n24_mid.wav", "n96_mid.wav"] {
            for edge in ["n60_lo.wav", "n60_hi.wav", "n24_lo.wav", "n96_hi.wav"] {
                assert!(
                    pos(mid) < pos(edge),
                    "{mid} (middle layer) must precede {edge} — order {order:?}"
                );
            }
        }
        // Within a layer, middle keys precede extremes.
        assert!(pos("n60_mid.wav") < pos("n24_mid.wav"));
        assert!(pos("n60_mid.wav") < pos("n96_mid.wav"));
    }

    #[test]
    fn segments_tile_the_file_exactly_once() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pack_path = build_zoned_pack(dir.path());
        let plan = plan_pack_file(&pack_path).expect("plan");
        let total = std::fs::metadata(&pack_path).expect("meta").len();
        assert_eq!(plan.total, total);

        let mut spans: Vec<(u64, u64)> =
            plan.segments.iter().map(|s| (s.start, s.start + s.len)).collect();
        spans.sort_unstable();
        let mut at = 0u64;
        for (start, end) in spans {
            assert_eq!(start, at, "no gap/overlap at byte {at}");
            assert!(end > start);
            at = end;
        }
        assert_eq!(at, total, "coverage ends at the file end");
    }
}
