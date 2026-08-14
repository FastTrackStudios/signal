//! `fts signal pack …` — the official signalpack inspection/mutation CLI.
//!
//! Mounted by fts-cli (`fts signal pack <cmd>`); also runnable standalone in
//! tests via [`cli_main`]. Agent-friendly: `inspect --json` for machine
//! output, `check` exit codes (0 = PASS), and all spec mutation is text-level
//! via [`crate::styx_edit`] — embedded specs are never re-serialized through
//! facet (the known silent-pack killer).

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use eyre::{Context, Result, bail};

use crate::engine::cache::{
    PackCodec, PackSpecSource, SampleCache, SignalPcmPack, create_signal_pack_with,
    extract_signal_pack, load_sample, transcode_signal_pack,
};
use crate::pack_rewrite::{read_embedded_spec, rewrite_embedded_spec};
use crate::spec::LibrarySpec;
use crate::styx_edit;

#[derive(Parser)]
#[command(name = "pack", about = "Inspect, edit, and build .signalpack files")]
struct PackCli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Header, spec summary, and zone stats for a pack.
    Inspect {
        pack: PathBuf,
        /// Machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Read or replace the embedded library spec.
    Spec {
        #[command(subcommand)]
        command: SpecCmd,
    },
    /// Targeted edits to zone fields inside the embedded spec (or a loose .styx).
    Zones {
        #[command(subcommand)]
        command: ZonesCmd,
    },
    /// Loop-point tools.
    Loops {
        #[command(subcommand)]
        command: LoopsCmd,
    },
    /// Validate a pack: zone coverage, entry presence, codec decode probes.
    /// Exit 0 = PASS, non-zero = PARTIAL/FAIL.
    Check {
        pack: PathBuf,
        /// Extraction root — also A/B the decode against source files
        /// (exact length + correlation).
        #[arg(long)]
        src_root: Option<PathBuf>,
    },
    /// Identify WHAT a reference render is playing at a given moment, and
    /// where inside the samples — sample-exactly.
    ///
    /// A Kontakt bounce of a library we hold is a mix of the very samples in
    /// the pack, and the round-robin set is finite, so this is identification
    /// rather than estimation. Each candidate GROUP (the dynamic layers of one
    /// zone family, which sound together from a shared onset) is fitted to the
    /// reference window by least squares over a multi-second scan: the winner
    /// names the articulation, round-robin and layer Kontakt chose, the offset
    /// it started from (its `$1fvjk` skip), the note's onset in wall time, and
    /// the gain on each layer. `explained` is the fraction of the window's
    /// energy the group accounts for — a wrong group cannot fake it.
    ///
    /// STATUS: identification is reliable, alignment is not yet. Use a TIGHT
    /// `--scan-ms` around a note time you already know (the MIDI gives onsets
    /// to ±300 ms) — a blind multi-second sweep still returns offsets that
    /// imply a note starting before the file did. See [`crate::ref_match`] for
    /// why, and for what single-sample correlation gets wrong.
    MatchRef {
        pack: PathBuf,
        /// Reference render (e.g. a Kontakt bounce).
        #[arg(long, default_value = "")]
        reference: PathBuf,
        /// Seconds into the reference to identify.
        #[arg(long)]
        at: f64,
        /// Window fitted, in ms. Long enough to be unique, short enough to sit
        /// inside one note.
        #[arg(long, default_value_t = 250.0)]
        window_ms: f64,
        /// How far into each candidate sample to scan, in ms — i.e. how long
        /// before `at` the note may have started. Keep this tight: a wide
        /// scan is where alignment currently goes wrong.
        #[arg(long, default_value_t = 400.0)]
        scan_ms: f64,
        /// Restrict candidates by root key: "55,60-64" (names allowed).
        #[arg(long)]
        notes: Option<String>,
        #[arg(long)]
        articulation: Option<String>,
        #[arg(long)]
        mic: Option<String>,
        /// Also try candidates RESAMPLED by ±1..N semitones — Kontakt fills
        /// its whole-tone grid by resampling, so an off-grid note in the
        /// reference is a neighbouring root played faster or slower.
        #[arg(long, default_value_t = 2)]
        semitones: i32,
        /// How many candidate groups to report.
        #[arg(long, default_value_t = 8)]
        top: usize,
        /// Sweep every note of this MIDI — the file the reference was
        /// rendered from — and report, per note, what the reference actually
        /// played and WHEN. With the notes known, each window is scanned in a
        /// tight neighbourhood of its nominal onset, which is the regime the
        /// matcher is exact in.
        #[arg(long)]
        sweep: Option<PathBuf>,
        /// Seconds after the nominal onset to place each sweep window.
        ///
        /// Small ON PURPOSE. The attack is the only landmark a sustained
        /// string note has — its body is periodic, so a window placed deep in
        /// it matches at nearly any offset and the fit wanders (measured: a
        /// window at +200 ms put an on-grid note's onset 198 ms BEFORE its own
        /// note-on, pinned to the edge of the scan). Keep the attack inside
        /// the window and the same note reads +1 ms.
        #[arg(long, default_value_t = 0.03)]
        lead: f64,
        /// Only sweep notes starting at or after this second.
        #[arg(long)]
        from: Option<f64>,
        /// Only sweep notes starting before this second.
        #[arg(long)]
        to: Option<f64>,
        /// Ignore `--reference` and score the matcher against a reference
        /// BUILT from this pack's own samples at known offsets and gains.
        /// Real content, known answer — the check the noise-based unit tests
        /// cannot make, because noise has no periodic self-similarity.
        #[arg(long)]
        self_test: bool,
    },
    /// Waveform report for pack samples: loop points, sample window, lead-in/
    /// arrival markers over each decoded waveform. Self-contained HTML.
    InspectSamples {
        pack: PathBuf,
        /// Only zones of this articulation id.
        #[arg(long)]
        articulation: Option<String>,
        /// Only zones of this mic id.
        #[arg(long)]
        mic: Option<String>,
        /// Note filter: "55,60-64" (matches root_key). Names allowed ("C4").
        #[arg(long)]
        notes: Option<String>,
        /// Max samples in the report.
        #[arg(long, default_value_t = 12)]
        limit: usize,
        #[arg(long)]
        out: PathBuf,
    },
    /// Render a note script through a pack with FULL tracing and write a
    /// waveform+event-log analysis report (plus the rendered WAV).
    /// Script: "60@0:2,62@2:1.5,C5@4:2v80" = note[@start_s][:dur_s][vNN].
    RenderReport {
        pack: PathBuf,
        /// Note script (see above). Sequential legato by default.
        #[arg(long)]
        notes: String,
        /// CC1 (dynamics) value held for the whole render.
        #[arg(long, default_value_t = 90)]
        cc1: u8,
        /// CC2 (vibrato) value held for the whole render.
        #[arg(long, default_value_t = 90)]
        cc2: u8,
        #[arg(long)]
        out: PathBuf,
        /// WAV path (default: `<out>` with .wav extension).
        #[arg(long)]
        wav: Option<PathBuf>,
        /// Extra seconds rendered after the last note-off (release tails).
        #[arg(long, default_value_t = 2.0)]
        tail: f32,
        /// Draw a musical beat ruler at this tempo (beat 1 anchored at t=0).
        #[arg(long)]
        bpm: Option<f64>,
        /// Beats per bar for the ruler (default 4).
        #[arg(long, default_value_t = 4)]
        beats_per_bar: u32,
        /// Pure sample playback — one looped sample per note at straight gain
        /// (no CC1 layer crossfade, ENV_FLEX, or legato trim/bloom).
        #[arg(long)]
        pure: bool,
        /// Use an EXTERNAL audio file as the waveform (skip the engine render) —
        /// for showing a reference render (e.g. a real Kontakt bounce) with the
        /// beat grid. `--notes` may be empty when this is set.
        #[arg(long)]
        audio_in: Option<PathBuf>,
        /// Render a Standard MIDI File instead of `--notes`: notes, note-offs,
        /// and ALL timed CC events (CC1/CC2 sweeps included) are replayed
        /// through the document renderer. Tempo comes from the file unless
        /// `--bpm` overrides the grid.
        #[arg(long)]
        midi: Option<PathBuf>,
        /// Header label for the report (e.g. "CSS REFERENCE").
        #[arg(long)]
        label: Option<String>,
    },
    /// Render the SAME line at several legato velocities (slow/medium/fast) and
    /// write a single wrapper page that toggles between the full reports — so
    /// the legato-speed timing differences can be seen and heard side-by-side.
    RenderCompare {
        pack: PathBuf,
        /// Note script (velocity is overridden per variant).
        #[arg(long)]
        notes: String,
        #[arg(long, default_value_t = 90)]
        cc1: u8,
        #[arg(long, default_value_t = 90)]
        cc2: u8,
        /// Wrapper HTML path; variants are written alongside as `<stem>.<label>.html`.
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value_t = 2.0)]
        tail: f32,
        #[arg(long)]
        bpm: Option<f64>,
        #[arg(long, default_value_t = 4)]
        beats_per_bar: u32,
        #[arg(long)]
        pure: bool,
        /// Comma-separated `label:velocity` variants (velocity drives the
        /// slow/medium/fast legato zone).
        #[arg(long, default_value = "slow:40,medium:90,fast:115")]
        velocities: String,
    },
    /// Decode every entry back to WAVs.
    Extract { pack: PathBuf, out_dir: PathBuf },
    /// Build a pack from a flat samples dir (embeds `<dir>/library.styx`).
    Build {
        samples_root: PathBuf,
        out: PathBuf,
        #[arg(long, default_value = "flac")]
        codec: String,
        /// Vorbis base quality (-0.2..=1.0; 0.6 ≈ oggenc q6).
        #[arg(long, default_value_t = 0.6)]
        quality: f32,
    },
    /// Re-encode a pack's audio with a different codec (Full ⇄ Proxy) —
    /// no source samples needed; spec + frame metadata copied verbatim.
    Transcode {
        input: PathBuf,
        out: PathBuf,
        #[arg(long)]
        codec: String,
        #[arg(long, default_value_t = 0.6)]
        quality: f32,
    },
}

#[derive(Subcommand)]
enum SpecCmd {
    /// Print the embedded spec text to stdout.
    Get { pack: PathBuf },
    /// Replace the embedded spec with the given file (atomic rewrite;
    /// audio body untouched). Validates the new spec parses first.
    Set {
        pack: PathBuf,
        spec: PathBuf,
        /// Skip the parse-validation of the new spec.
        #[arg(long)]
        no_validate: bool,
    },
}

#[derive(Subcommand)]
enum ZonesCmd {
    /// Append zones for articulations MISSING from a spec, generated from an
    /// `nki --zones` zones.tsv (Cinematic Studio naming:
    /// `<sec>_<Artic>_<Mic>_<REST>.ncw` → `<Mic>/<section>/[<Cat>/]<Artic>/<REST>.wav`).
    /// Existing articulations are never touched (they may carry measured
    /// fields — lead_in, arrival — the tsv cannot reproduce).
    AppendMissing {
        /// A .signalpack or a loose library.styx.
        target: PathBuf,
        /// zones.tsv from the `nki` decoder.
        #[arg(long)]
        from_tsv: PathBuf,
        /// Section label used in extracted WAV paths (e.g. "2 Trumpets").
        #[arg(long)]
        section: String,
        /// Extraction uses flat `<Mic>/<Section>/<Artic>/` (CSB/CSW/Solo
        /// Strings). Without this, the CSS category-folder taxonomy is used.
        #[arg(long)]
        flat: bool,
        /// Verify each generated zone's file exists under this root; error on
        /// missing files.
        #[arg(long)]
        wav_root: Option<PathBuf>,
        /// Report what would be appended without writing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Normalize Pacific (Performance Samples) articulation ids that bake
    /// direction/interval/RR/velocity codes into the id string:
    ///   leg_up2_18      → articulation "leg", direction "up", interval 2
    ///   legslur_dn3_3   → articulation "legslur", direction "down", interval 3
    ///   pizzrr3         → articulation "pizz", rr_index 2
    ///   pluckrr2_64     → articulation "pluck", rr_index 1, vel band 64..
    /// Idempotent: already-normalized zones are untouched.
    NormalizePacific {
        /// A .signalpack or a loose library.styx.
        target: PathBuf,
        /// Report what would change without writing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Set scalar fields on matching zones: `--set loop_xfade=4800 …`.
    Set {
        /// A .signalpack (embedded spec is rewritten) or a loose .styx
        /// (rewritten in place, .bak sibling kept).
        target: PathBuf,
        /// field=value pairs; allowed fields: loop_start loop_end loop_xfade
        /// gain_db tune_cents vel_min vel_max key_min key_max root_key
        /// sample_start sample_end
        #[arg(long = "set", required = true)]
        sets: Vec<String>,
        /// Only zones whose `file` contains this substring.
        #[arg(long)]
        file: Option<String>,
        /// Only zones with this exact articulation id.
        #[arg(long)]
        articulation: Option<String>,
        /// Only zones with this exact mic id.
        #[arg(long)]
        mic: Option<String>,
        /// Report matches without writing.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum LoopsCmd {
    /// Inject NKI loop points (from an `nki --zones` zones.tsv) into a pack's
    /// embedded spec or a loose library.styx — the Cinematic Studio flow.
    Inject {
        /// A .signalpack or a loose library.styx.
        target: PathBuf,
        /// zones.tsv from the `nki` decoder.
        #[arg(long)]
        from_tsv: PathBuf,
        /// Section label used in extracted WAV paths (e.g. "1st Violins").
        #[arg(long)]
        section: String,
        /// Compare against loops already present instead of writing.
        #[arg(long)]
        check: bool,
        /// Report what would be injected without writing.
        #[arg(long)]
        dry_run: bool,
    },
}

/// Entry point for fts-cli mounting (and tests). `argv` should NOT include
/// the program name; pass the args after `fts signal pack`.
pub fn cli_main(argv: impl IntoIterator<Item = OsString>) -> Result<()> {
    let argv = std::iter::once(OsString::from("pack")).chain(argv);
    match PackCli::parse_from(argv).command {
        Cmd::Inspect { pack, json } => inspect(&pack, json),
        Cmd::Spec {
            command: SpecCmd::Get { pack },
        } => {
            print!("{}", read_embedded_spec(&pack)?);
            Ok(())
        }
        Cmd::Spec {
            command:
                SpecCmd::Set {
                    pack,
                    spec,
                    no_validate,
                },
        } => spec_set(&pack, &spec, no_validate),
        Cmd::Zones {
            command: ZonesCmd::NormalizePacific { target, dry_run },
        } => zones_normalize_pacific(&target, dry_run),
        Cmd::Zones {
            command:
                ZonesCmd::AppendMissing {
                    target,
                    from_tsv,
                    section,
                    flat,
                    wav_root,
                    dry_run,
                },
        } => zones_append_missing(&target, &from_tsv, &section, flat, wav_root.as_deref(), dry_run),
        Cmd::Zones {
            command:
                ZonesCmd::Set {
                    target,
                    sets,
                    file,
                    articulation,
                    mic,
                    dry_run,
                },
        } => zones_set(&target, &sets, file, articulation, mic, dry_run),
        Cmd::Loops {
            command:
                LoopsCmd::Inject {
                    target,
                    from_tsv,
                    section,
                    check,
                    dry_run,
                },
        } => loops_inject(&target, &from_tsv, &section, check, dry_run),
        Cmd::Check { pack, src_root } => {
            if run_check(&pack, src_root.as_deref())? {
                Ok(())
            } else {
                bail!("check: PARTIAL")
            }
        }
        Cmd::MatchRef {
            pack,
            reference,
            at,
            window_ms,
            scan_ms,
            notes,
            articulation,
            mic,
            semitones,
            top,
            self_test,
            sweep,
            lead,
            from,
            to,
        } => {
            if let Some(midi) = sweep {
                match_ref_sweep(
                    &pack,
                    &reference,
                    &midi,
                    lead,
                    window_ms,
                    scan_ms,
                    articulation,
                    mic,
                    semitones,
                    from,
                    to,
                )
            } else if self_test {
                match_ref_self_test(&pack, window_ms, scan_ms, articulation, mic)
            } else {
                match_ref(
                    &pack,
                    &reference,
                    at,
                    window_ms,
                    scan_ms,
                    notes,
                    articulation,
                    mic,
                    semitones,
                    top,
                )
            }
        }
        Cmd::InspectSamples {
            pack,
            articulation,
            mic,
            notes,
            limit,
            out,
        } => inspect_samples(&pack, articulation, mic, notes, limit, &out),
        Cmd::RenderReport {
            pack,
            notes,
            cc1,
            cc2,
            out,
            wav,
            tail,
            bpm,
            beats_per_bar,
            pure,
            audio_in,
            midi,
            label,
        } => render_report(
            &pack, &notes, cc1, cc2, &out, wav, tail, bpm, beats_per_bar, pure, None, audio_in,
            midi, label,
        ),
        Cmd::RenderCompare {
            pack,
            notes,
            cc1,
            cc2,
            out,
            tail,
            bpm,
            beats_per_bar,
            pure,
            velocities,
        } => render_compare(
            &pack, &notes, cc1, cc2, &out, tail, bpm, beats_per_bar, pure, &velocities,
        ),
        Cmd::Extract { pack, out_dir } => {
            let stats = extract_signal_pack(&pack, &out_dir)?;
            println!("extracted {} sample(s) ({} failed)", stats.prepared, stats.failed);
            Ok(())
        }
        Cmd::Build {
            samples_root,
            out,
            codec,
            quality,
        } => build(&samples_root, &out, &codec, quality),
        Cmd::Transcode {
            input,
            out,
            codec,
            quality,
        } => {
            let codec = parse_codec(&codec, quality)?;
            let stats = transcode_signal_pack(&input, &out, codec)?;
            if stats.failed > 0 {
                bail!("transcode: {} entr(ies) failed", stats.failed);
            }
            let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
            println!(
                "transcoded {} entr(ies) -> {} ({:.1} MB)",
                stats.prepared,
                out.display(),
                size as f64 / 1e6
            );
            Ok(())
        }
    }
}

fn parse_codec(name: &str, quality: f32) -> Result<PackCodec> {
    match name {
        "flac" => Ok(PackCodec::FlacI24),
        "ogg" | "vorbis" | "ogg-vorbis" => Ok(PackCodec::OggVorbis { quality }),
        // Raw PCM: larger on disk, free in RAM — voices read it straight out
        // of the mapping instead of decoding it into the heap.
        "pcm16" | "pcm-i16" | "i16" => Ok(PackCodec::PcmI16),
        "pcm24" | "pcm-i24" | "i24" => Ok(PackCodec::PcmI24),
        other => bail!("unknown codec {other:?} (flac | ogg | pcm16 | pcm24)"),
    }
}

// ── inspect ───────────────────────────────────────────────────────────────────

fn inspect(pack_path: &Path, json: bool) -> Result<()> {
    let pack = SignalPcmPack::open(pack_path)?;
    let spec = crate::pack::read_pack_header(pack_path)?.spec;
    let looped = spec.zones.iter().filter(|z| z.loop_end > z.loop_start).count();

    // Per-articulation zone stats.
    let mut artics: BTreeMap<&str, (usize, u8, u8)> = BTreeMap::new();
    for z in &spec.zones {
        let e = artics.entry(z.articulation.as_str()).or_insert((0, 127, 0));
        e.0 += 1;
        e.1 = e.1.min(z.key_min);
        e.2 = e.2.max(z.key_max);
    }
    let size = std::fs::metadata(pack_path).map(|m| m.len()).unwrap_or(0);

    if json {
        let artics_json: Vec<serde_json::Value> = artics
            .iter()
            .map(|(id, (n, lo, hi))| {
                serde_json::json!({ "id": id, "zones": n, "key_min": lo, "key_max": hi })
            })
            .collect();
        let v = serde_json::json!({
            "path": pack_path,
            "kind": pack.kind_label(),
            "size_bytes": size,
            "entries": pack.entry_count(),
            "name": spec.name,
            "vendor": spec.vendor,
            "version": spec.version,
            "sections": spec.sections.iter().map(|s| s.label.clone()).collect::<Vec<_>>(),
            "mics": spec.mics.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
            "zones": spec.zones.len(),
            "looped_zones": looped,
            "articulations": artics_json,
        });
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else {
        println!("pack     {}", pack_path.display());
        println!("kind     {}  ({:.1} MB)", pack.kind_label(), size as f64 / 1e6);
        println!("name     {}  [{} · {}]", spec.name, spec.vendor, spec.version);
        println!(
            "entries  {}   zones {}   looped {}",
            pack.entry_count(),
            spec.zones.len(),
            looped
        );
        println!(
            "sections {:?}   mics {:?}",
            spec.sections.iter().map(|s| s.label.as_str()).collect::<Vec<_>>(),
            spec.mics.iter().map(|m| m.id.as_str()).collect::<Vec<_>>()
        );
        for (id, (n, lo, hi)) in &artics {
            println!("  artic {id:<16} {n:>6} zones  keys {lo}..={hi}");
        }
    }
    Ok(())
}

// ── spec set ─────────────────────────────────────────────────────────────────

fn spec_set(pack: &Path, spec_file: &Path, no_validate: bool) -> Result<()> {
    let new_spec = std::fs::read_to_string(spec_file)?;
    if !no_validate {
        LibrarySpec::from_styx(&new_spec)
            .map_err(|e| eyre::eyre!("new spec does not parse: {e}"))?;
    }
    rewrite_embedded_spec(pack, |_| new_spec.clone())?;
    println!("spec replaced in {}", pack.display());
    Ok(())
}

// ── spec text access for pack-or-styx targets ────────────────────────────────

enum Target {
    Pack(PathBuf),
    Styx(PathBuf),
}

impl Target {
    fn open(path: &Path) -> Result<(Self, String)> {
        if path.extension().and_then(|e| e.to_str()) == Some("styx") {
            Ok((Target::Styx(path.to_owned()), std::fs::read_to_string(path)?))
        } else {
            Ok((Target::Pack(path.to_owned()), read_embedded_spec(path)?))
        }
    }

    fn write(&self, new_spec: &str) -> Result<()> {
        // Guard: the edited spec must still parse before it lands anywhere.
        LibrarySpec::from_styx(new_spec)
            .map_err(|e| eyre::eyre!("edited spec does not parse (refusing to write): {e}"))?;
        match self {
            Target::Pack(p) => {
                rewrite_embedded_spec(p, |_| new_spec.to_string())?;
                println!("wrote embedded spec in {}", p.display());
            }
            Target::Styx(p) => {
                let bak = p.with_extension("styx.bak");
                std::fs::copy(p, &bak)?;
                std::fs::write(p, new_spec)?;
                println!("wrote {} (backup at {})", p.display(), bak.display());
            }
        }
        Ok(())
    }
}

/// Rewrite each zone block via `edit` (returning `Some(new_block)` to change
/// it). Returns (new_spec_text, matched_count).
fn edit_zones(
    spec_text: &str,
    mut edit: impl FnMut(&str) -> Option<String>,
) -> Result<(String, usize)> {
    let (_, is, ie, _) = styx_edit::find_list_block(spec_text, "zones")
        .ok_or_else(|| eyre::eyre!("spec has no `zones (…)` block"))?;
    let inner = &spec_text[is..ie];
    let mut new_inner = String::with_capacity(inner.len());
    let mut cursor = 0usize;
    let mut changed = 0usize;
    for (s, e) in styx_edit::split_entries(inner) {
        new_inner.push_str(&inner[cursor..s]);
        let block = &inner[s..e];
        match edit(block) {
            Some(new_block) => {
                new_inner.push_str(&new_block);
                changed += 1;
            }
            None => new_inner.push_str(block),
        }
        cursor = e;
    }
    new_inner.push_str(&inner[cursor..]);
    let mut out = String::with_capacity(spec_text.len());
    out.push_str(&spec_text[..is]);
    out.push_str(&new_inner);
    out.push_str(&spec_text[ie..]);
    Ok((out, changed))
}

// ── zones set ────────────────────────────────────────────────────────────────

const SETTABLE: &[&str] = &[
    "loop_start",
    "loop_end",
    "loop_xfade",
    "gain_db",
    "tune_cents",
    "vel_min",
    "vel_max",
    "key_min",
    "key_max",
    "root_key",
    "sample_start",
    "sample_end",
];

fn zones_set(
    target: &Path,
    sets: &[String],
    file: Option<String>,
    articulation: Option<String>,
    mic: Option<String>,
    dry_run: bool,
) -> Result<()> {
    let pairs: Vec<(String, String)> = sets
        .iter()
        .map(|s| {
            let (k, v) = s
                .split_once('=')
                .ok_or_else(|| eyre::eyre!("--set expects field=value, got {s:?}"))?;
            if !SETTABLE.contains(&k) {
                bail!("field {k:?} is not settable (allowed: {SETTABLE:?})");
            }
            // Values must be numeric — these are all scalar numeric fields.
            v.parse::<f64>()
                .with_context(|| format!("--set {k}: value {v:?} is not numeric"))?;
            Ok((k.to_string(), v.to_string()))
        })
        .collect::<Result<_>>()?;

    let (tgt, spec_text) = Target::open(target)?;
    let mut matched = 0usize;
    let (new_spec, changed) = edit_zones(&spec_text, |block| {
        if let Some(f) = &file {
            if !styx_edit::entry_field(block, "file").is_some_and(|v| v.contains(f.as_str())) {
                return None;
            }
        }
        if let Some(a) = &articulation {
            if styx_edit::entry_field(block, "articulation").as_deref() != Some(a.as_str()) {
                return None;
            }
        }
        if let Some(m) = &mic {
            if styx_edit::entry_field(block, "mic").as_deref() != Some(m.as_str()) {
                return None;
            }
        }
        matched += 1;
        let mut b = block.to_string();
        for (k, v) in &pairs {
            b = styx_edit::set_entry_field(&b, k, v);
        }
        Some(b)
    })?;

    println!("matched {matched} zone(s), edited {changed}");
    if matched == 0 {
        bail!("no zones matched the filters");
    }
    if dry_run {
        println!("(dry run — nothing written)");
        return Ok(());
    }
    tgt.write(&new_spec)
}

// ── zones normalize-pacific ──────────────────────────────────────────────────

/// Parsed form of a Pacific baked-suffix articulation id.
enum PacificId {
    /// `leg_up2_18` / `legslur_dn3_3` → (family, direction, interval)
    Legato(String, &'static str, u32),
    /// `pizzrr3` / `pluckrr2_64` → (family, rr_index, vel_code)
    RoundRobin(String, u32, Option<u32>),
}

fn parse_pacific_id(id: &str) -> Option<PacificId> {
    // leg[variant]_(up|dn)<N>_<velcode>
    if let Some(rest) = id.strip_prefix("leg") {
        let mut parts = rest.splitn(3, '_');
        let variant = parts.next().unwrap_or("");
        let dir_iv = parts.next()?;
        let _velcode = parts.next(); // redundant (16+n / n) when present — dropped
        let (dir, iv) = if let Some(n) = dir_iv.strip_prefix("up") {
            ("up", n.parse::<u32>().ok()?)
        } else if let Some(n) = dir_iv.strip_prefix("dn") {
            ("down", n.parse::<u32>().ok()?)
        } else {
            return None;
        };
        return Some(PacificId::Legato(format!("leg{variant}"), dir, iv));
    }
    // <family>rr<N>[_<velcode>]
    if let Some(rr_pos) = id.find("rr") {
        let family = &id[..rr_pos];
        if family.is_empty() || !family.chars().all(|c| c.is_ascii_alphabetic()) {
            return None;
        }
        let tail = &id[rr_pos + 2..];
        let (rr_str, vel) = match tail.split_once('_') {
            Some((rr, v)) => (rr, v.parse::<u32>().ok()),
            None => (tail, None),
        };
        let rr = rr_str.parse::<u32>().ok()?;
        return Some(PacificId::RoundRobin(family.to_string(), rr, vel));
    }
    None
}

fn zones_normalize_pacific(target: &Path, dry_run: bool) -> Result<()> {
    let (tgt, spec_text) = Target::open(target)?;

    // First pass: collect velocity-band codes per RR family so band edges can
    // be derived (codes are band STARTS; e.g. 1/32/64/96 → 0-31/32-63/…).
    let (_, is, ie, _) = styx_edit::find_list_block(&spec_text, "zones")
        .ok_or_else(|| eyre::eyre!("spec has no `zones (…)` block"))?;
    let inner = &spec_text[is..ie];
    let mut vel_codes: BTreeMap<String, std::collections::BTreeSet<u32>> = BTreeMap::new();
    for (s, e) in styx_edit::split_entries(inner) {
        if let Some(id) = styx_edit::entry_field(&inner[s..e], "articulation") {
            if let Some(PacificId::RoundRobin(fam, _, Some(v))) = parse_pacific_id(&id) {
                vel_codes.entry(fam).or_default().insert(v);
            }
        }
    }
    let vel_band = |fam: &str, code: u32| -> (u32, u32) {
        let codes = &vel_codes[fam];
        let lo = if code <= 1 { 0 } else { code };
        let hi = codes
            .iter()
            .find(|c| **c > code)
            .map(|next| next - 1)
            .unwrap_or(127);
        (lo, hi)
    };

    let mut stats: BTreeMap<String, usize> = BTreeMap::new();
    let (new_spec, changed) = edit_zones(&spec_text, |block| {
        let id = styx_edit::entry_field(block, "articulation")?;
        match parse_pacific_id(&id)? {
            PacificId::Legato(family, dir, interval) => {
                *stats.entry(format!("{family} (legato)")).or_default() += 1;
                let mut b = styx_edit::set_entry_field(block, "articulation", &format!("{family:?}"));
                b = styx_edit::set_entry_field(&b, "direction", &format!("{dir:?}"));
                b = styx_edit::set_entry_field(&b, "interval", &interval.to_string());
                b = styx_edit::set_entry_field(&b, "rr_index", "0");
                Some(b)
            }
            PacificId::RoundRobin(family, rr, vel) => {
                *stats.entry(format!("{family} (rr)")).or_default() += 1;
                let mut b = styx_edit::set_entry_field(block, "articulation", &format!("{family:?}"));
                b = styx_edit::set_entry_field(&b, "rr_index", &rr.saturating_sub(1).to_string());
                if let Some(code) = vel {
                    let (lo, hi) = vel_band(&family, code);
                    b = styx_edit::set_entry_field(&b, "vel_min", &lo.to_string());
                    b = styx_edit::set_entry_field(&b, "vel_max", &hi.to_string());
                }
                Some(b)
            }
        }
    })?;

    println!("normalized {changed} zone(s):");
    for (k, n) in &stats {
        println!("  {k:<24} {n}");
    }
    if changed == 0 {
        println!("nothing to normalize (already normalized?)");
        return Ok(());
    }
    if dry_run {
        println!("(dry run — nothing written)");
        return Ok(());
    }
    tgt.write(&new_spec)
}

// ── zones append-missing (Cinematic Studio NKI flow) ─────────────────────────

#[derive(Clone)]
struct TsvZone {
    key_min: u8,
    key_max: u8,
    root_key: u8,
    tune_cents: f64,
    rr_index: u32,
    mic: String,
    articulation: String,
    dynamic: String,
    direction: Option<String>,
    interval: u32,
    loop_start: u64,
    loop_end: u64,
    section: String,
    file: String,
}

fn cs_is_legato(artic: &str) -> bool {
    matches!(artic, "Leg" | "NVLeg" | "MLeg" | "Port")
}
fn cs_is_zero(artic: &str) -> bool {
    matches!(artic, "Legzero" | "NVLegzero")
}

/// Parse an `nki --zones` zones.tsv into per-FILE zones (key ranges unioned
/// across duplicate rows, loops first-wins) using the Cinematic Studio NCW
/// naming. `flat` = no category folder in the extracted tree (CSB/CSW/Solo
/// Strings); otherwise the CSS taxonomy applies.
fn load_tsv_zones(
    tsv: &Path,
    section_label: &str,
    flat: bool,
) -> Result<BTreeMap<String, TsvZone>> {
    let text = std::fs::read_to_string(tsv)?;
    let mut lines = text.lines();
    let header: Vec<&str> = lines
        .next()
        .ok_or_else(|| eyre::eyre!("empty zones.tsv"))?
        .split('\t')
        .collect();
    let col = |name: &str| {
        header
            .iter()
            .position(|h| *h == name)
            .ok_or_else(|| eyre::eyre!("zones.tsv missing column {name:?}"))
    };
    let (c_klo, c_khi, c_root, c_tune, c_ls, c_le, c_sample) = (
        col("key_lo")?,
        col("key_hi")?,
        col("root")?,
        col("tune")?,
        col("loop_start")?,
        col("loop_end")?,
        col("sample")?,
    );

    let mut by_file: BTreeMap<String, TsvZone> = BTreeMap::new();
    let mut skipped = 0usize;
    for line in lines {
        let f: Vec<&str> = line.split('\t').collect();
        let Some(sample) = f.get(c_sample).filter(|s| !s.is_empty()) else {
            skipped += 1;
            continue;
        };
        let base = sample.rsplit('/').next().unwrap_or(sample);
        let stem = base
            .strip_suffix(".ncw")
            .or_else(|| base.strip_suffix(".wav"))
            .unwrap_or(base);
        let parts: Vec<&str> = stem.split('_').collect();
        if parts.len() < 4 {
            skipped += 1;
            continue;
        }
        let (artic, mic) = (parts[1].to_string(), parts[2].to_string());
        let rest = &parts[3..];
        let wav_stem = rest.join("_");
        let file = if flat {
            format!("{mic}/{section_label}/{artic}/{wav_stem}.wav")
        } else {
            let Some(cat) = cs_category(&artic) else {
                skipped += 1;
                continue;
            };
            format!("{mic}/{section_label}/{cat}/{artic}/{wav_stem}.wav")
        };

        let dynamic = rest[0].to_string();
        let (direction, interval, rr_index) = if cs_is_legato(&artic) {
            if rest.len() >= 4 && (rest[1] == "up" || rest[1] == "down") {
                (
                    Some(rest[1].to_string()),
                    rest[3].parse::<u32>().unwrap_or(0),
                    0,
                )
            } else {
                (None, 0, 0)
            }
        } else if cs_is_zero(&artic) {
            let rr = rest.last().and_then(|s| s.parse::<u32>().ok()).unwrap_or(1);
            (None, 0, rr.saturating_sub(1))
        } else {
            let rr = if rest.len() >= 3 {
                rest.last().and_then(|s| s.parse::<u32>().ok()).unwrap_or(1)
            } else {
                1
            };
            (None, 0, rr.saturating_sub(1))
        };

        let key_lo: i32 = f.get(c_klo).and_then(|v| v.parse().ok()).unwrap_or(0);
        let key_hi: i32 = f.get(c_khi).and_then(|v| v.parse().ok()).unwrap_or(0);
        let root: i32 = f.get(c_root).and_then(|v| v.parse().ok()).unwrap_or(60);
        let tune: f64 = f.get(c_tune).and_then(|v| v.parse().ok()).unwrap_or(1.0);
        let (ls, le) = (
            f.get(c_ls).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0),
            f.get(c_le).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0),
        );

        let entry = by_file.entry(file.clone()).or_insert_with(|| TsvZone {
            key_min: key_lo.clamp(0, 127) as u8,
            key_max: key_hi.clamp(0, 127) as u8,
            root_key: root.clamp(0, 127) as u8,
            tune_cents: if tune > 0.0 { 1200.0 * tune.log2() } else { 0.0 },
            rr_index,
            mic,
            articulation: artic,
            dynamic,
            direction,
            interval,
            loop_start: 0,
            loop_end: 0,
            section: section_label.to_string(),
            file,
        });
        entry.key_min = entry.key_min.min(key_lo.clamp(0, 127) as u8);
        entry.key_max = entry.key_max.max(key_hi.clamp(0, 127) as u8);
        if le > ls && entry.loop_end <= entry.loop_start {
            entry.loop_start = ls;
            entry.loop_end = le;
        }
    }
    if skipped > 0 {
        eprintln!("  NOTE: {skipped} tsv row(s) skipped (unparseable/unmapped)");
    }
    Ok(by_file)
}

fn emit_zone_block(z: &TsvZone) -> String {
    let mut s = String::from("    {\n");
    let mut f = |k: &str, v: String| {
        s.push_str(&format!("        {k:<12} {v}\n"));
    };
    f("section", format!("{:?}", z.section));
    f("file", format!("{:?}", z.file));
    f("key_min", z.key_min.to_string());
    f("key_max", z.key_max.to_string());
    f("root_key", z.root_key.to_string());
    f("vel_min", "0".into());
    f("vel_max", "127".into());
    f("rr_index", z.rr_index.to_string());
    f("mic", format!("{:?}", z.mic));
    f("articulation", format!("{:?}", z.articulation));
    f("dynamic", format!("{:?}", z.dynamic));
    f("gain_db", "0.000".into());
    let cents = if z.tune_cents.abs() < 1e-2 { 0.0 } else { z.tune_cents };
    f("tune_cents", format!("{cents:.3}"));
    if let Some(d) = &z.direction {
        f("direction", format!("{d:?}"));
        f("interval", z.interval.to_string());
    }
    if z.loop_end > z.loop_start {
        f("loop_start", z.loop_start.to_string());
        f("loop_end", z.loop_end.to_string());
    }
    s.push_str("    }\n");
    s
}

fn zones_append_missing(
    target: &Path,
    from_tsv: &Path,
    section: &str,
    flat: bool,
    wav_root: Option<&Path>,
    dry_run: bool,
) -> Result<()> {
    let tsv_zones = load_tsv_zones(from_tsv, section, flat)?;
    let (tgt, spec_text) = Target::open(target)?;

    // Articulations already present in the spec are authoritative — skip.
    let (_, is, ie, _) = styx_edit::find_list_block(&spec_text, "zones")
        .ok_or_else(|| eyre::eyre!("spec has no `zones (…)` block"))?;
    let inner = &spec_text[is..ie];
    let mut present: std::collections::BTreeSet<String> = Default::default();
    for (s, e) in styx_edit::split_entries(inner) {
        if let Some(a) = styx_edit::entry_field(&inner[s..e], "articulation") {
            present.insert(a);
        }
    }

    let missing: Vec<&TsvZone> = tsv_zones
        .values()
        .filter(|z| !present.contains(&z.articulation))
        .collect();
    let mut per_artic: BTreeMap<&str, usize> = BTreeMap::new();
    for z in &missing {
        *per_artic.entry(z.articulation.as_str()).or_default() += 1;
    }
    println!(
        "spec has {} articulation(s); appending {} zone(s) across {} missing articulation(s):",
        present.len(),
        missing.len(),
        per_artic.len()
    );
    for (a, n) in &per_artic {
        println!("  + {a:<16} {n} zone(s)");
    }
    if missing.is_empty() {
        println!("nothing to append");
        return Ok(());
    }

    let missing: Vec<&TsvZone> = if let Some(root) = wav_root {
        let (present_on_disk, absent): (Vec<&TsvZone>, Vec<&TsvZone>) = missing
            .into_iter()
            .partition(|z| root.join(&z.file).exists());
        if !absent.is_empty() {
            println!(
                "  WARNING: dropping {} zone(s) whose file is missing under {}:",
                absent.len(),
                root.display()
            );
            for z in absent.iter().take(10) {
                println!("    - {}", z.file);
            }
        }
        println!(
            "all {} appended files exist under {}",
            present_on_disk.len(),
            root.display()
        );
        present_on_disk
    } else {
        missing
    };
    if missing.is_empty() {
        println!("nothing to append after file filtering");
        return Ok(());
    }

    if dry_run {
        println!("(dry run — nothing written)");
        return Ok(());
    }

    let mut appended = String::new();
    for z in &missing {
        appended.push_str(&emit_zone_block(z));
    }
    let mut new_spec = String::with_capacity(spec_text.len() + appended.len());
    new_spec.push_str(&spec_text[..ie]);
    new_spec.push_str(&appended);
    new_spec.push_str(&spec_text[ie..]);
    tgt.write(&new_spec)
}

// ── loops inject (Cinematic Studio NKI flow) ─────────────────────────────────

/// nki-styx's CSS articulation → category-folder taxonomy (+ the "Pizz"
/// token the non-1st-Violins sections use).
fn cs_category(artic: &str) -> Option<&'static str> {
    Some(match artic {
        "Nonvib" | "Vibsus" => "Sustain",
        "Leg" | "Legzero" | "NVLeg" | "NVLegzero" => "Legato",
        "Marcato" | "Sfz" | "Spiccato" | "Staccatissimo" | "Staccato" => "Short",
        "Bartokpizz" | "Pizzicato" | "Pizz" => "Pizzicato",
        "Clegno" | "Harm" | "HTrills" | "Port" | "Tremolo" | "WTrills" => "Special",
        "Harmrel" | "HTrel" | "Marcrel" | "NVrel" | "Tremrel" | "Vsusrel" | "WTrel" => "Releases",
        _ => return None,
    })
}

/// wav-relative-path → (loop_start, loop_end) for looped NKI zones.
/// First-wins on per-file loop variants (matches nki-styx accumulation).
fn load_loop_map(tsv: &Path, section_label: &str) -> Result<BTreeMap<String, (u64, u64)>> {
    let text = std::fs::read_to_string(tsv)?;
    let mut lines = text.lines();
    let header: Vec<&str> = lines
        .next()
        .ok_or_else(|| eyre::eyre!("empty zones.tsv"))?
        .split('\t')
        .collect();
    let col = |name: &str| {
        header
            .iter()
            .position(|h| *h == name)
            .ok_or_else(|| eyre::eyre!("zones.tsv missing column {name:?}"))
    };
    let (c_ls, c_le, c_sample) = (col("loop_start")?, col("loop_end")?, col("sample")?);

    let mut loops = BTreeMap::new();
    let mut conflicts = 0usize;
    for line in lines {
        let f: Vec<&str> = line.split('\t').collect();
        let (Some(ls), Some(le)) = (
            f.get(c_ls).and_then(|v| v.parse::<u64>().ok()),
            f.get(c_le).and_then(|v| v.parse::<u64>().ok()),
        ) else {
            continue;
        };
        if le <= ls || le == 0 {
            continue;
        }
        let Some(sample) = f.get(c_sample) else { continue };
        let base = sample.rsplit('/').next().unwrap_or(sample);
        let stem = base
            .strip_suffix(".ncw")
            .or_else(|| base.strip_suffix(".wav"))
            .unwrap_or(base);
        let parts: Vec<&str> = stem.split('_').collect();
        if parts.len() < 4 {
            continue;
        }
        let (artic, mic) = (parts[1], parts[2]);
        let Some(cat) = cs_category(artic) else {
            eprintln!("  WARNING: looped zone with unmapped articulation {artic:?}: {base}");
            continue;
        };
        let rest = parts[3..].join("_");
        let wav = format!("{mic}/{section_label}/{cat}/{artic}/{rest}.wav");
        if loops.contains_key(&wav) {
            if loops[&wav] != (ls, le) {
                conflicts += 1;
            }
            continue; // first-wins
        }
        loops.insert(wav, (ls, le));
    }
    if conflicts > 0 {
        eprintln!("  NOTE: {conflicts} file(s) had differing loops across zones (first wins)");
    }
    Ok(loops)
}

fn loops_inject(
    target: &Path,
    from_tsv: &Path,
    section: &str,
    check: bool,
    dry_run: bool,
) -> Result<()> {
    let loops = load_loop_map(from_tsv, section)?;
    println!("{section}: {} looped file(s) mapped from {}", loops.len(), from_tsv.display());

    let (tgt, spec_text) = Target::open(target)?;
    let mut unmatched = loops.clone();
    let (mut injected, mut already, mut matches, mut mismatches) = (0usize, 0usize, 0usize, 0usize);
    let (new_spec, _) = edit_zones(&spec_text, |block| {
        let file = styx_edit::entry_field(block, "file")?;
        let &(ls, le) = loops.get(&file)?;
        unmatched.remove(&file);
        let existing_ls = styx_edit::entry_field(block, "loop_start")
            .and_then(|v| v.parse::<u64>().ok());
        if let Some(els) = existing_ls {
            already += 1;
            if check {
                let ele = styx_edit::entry_field(block, "loop_end")
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);
                if (els, ele) == (ls, le) {
                    matches += 1;
                } else {
                    mismatches += 1;
                    println!("  MISMATCH {file}: spec ({els}, {ele}) vs nki ({ls}, {le})");
                }
            }
            return None;
        }
        injected += 1;
        let b = styx_edit::set_entry_field(block, "loop_start", &ls.to_string());
        Some(styx_edit::set_entry_field(&b, "loop_end", &le.to_string()))
    })?;

    print!("  injected={injected} already={already}");
    if check {
        print!(" match={matches} mismatch={mismatches}");
    }
    println!(" loops-without-spec-zone={}", unmatched.len());
    for wav in unmatched.keys().take(5) {
        println!("    e.g. no spec zone for looped {wav}");
    }

    if check {
        if mismatches > 0 {
            bail!("loop check: {mismatches} mismatch(es)");
        }
        return Ok(());
    }
    if dry_run || injected == 0 {
        println!("(nothing written)");
        return Ok(());
    }
    tgt.write(&new_spec)
}

// ── check ────────────────────────────────────────────────────────────────────

/// Zone-mode pack validation: per-articulation key coverage (vs the spec's
/// pitch tolerance), zone-file → pack-entry presence, decode probes (with
/// optional A/B correlation against source files). Returns Ok(true) = PASS.
pub fn run_check(pack_path: &Path, src_root: Option<&Path>) -> Result<bool> {
    let patch = crate::PlayerPatch::from_pack(pack_path)?;
    let spec = &patch.spec;
    if spec.zones.is_empty() {
        bail!(
            "{}: not a zone-mode pack — use the check_pack_resolve example for \
             convention-mode packs",
            pack_path.display()
        );
    }
    let pack = patch
        .pack
        .as_ref()
        .ok_or_else(|| eyre::eyre!("patch has no backing pack"))?;
    println!(
        "pack: {}  ({}, {} zones, {} entries)",
        spec.name,
        pack.kind_label(),
        spec.zones.len(),
        pack.entry_count()
    );

    // Per-articulation key coverage within the engine's pitch tolerance.
    let mut by_artic: BTreeMap<&str, std::collections::BTreeSet<u8>> = BTreeMap::new();
    for z in &spec.zones {
        let keys = by_artic.entry(z.articulation.as_str()).or_default();
        for k in z.key_min..=z.key_max {
            keys.insert(k);
        }
    }
    let tol = spec.performance.zone_pitch_tolerance.max(1);
    let mut gaps = 0usize;
    for (artic, keys) in &by_artic {
        let (lo, hi) = (*keys.first().unwrap(), *keys.last().unwrap());
        let mut missing: Vec<u8> = Vec::new();
        let mut last = lo;
        for k in keys.iter().copied() {
            if k > last && k - last > tol + 1 {
                missing.extend(last + 1..k);
            }
            last = k;
        }
        if missing.is_empty() {
            println!("  artic {artic:<16} keys {lo}..={hi} covered ({} keys)", keys.len());
        } else {
            gaps += 1;
            println!("  artic {artic:<16} keys {lo}..={hi} — GAPS beyond tolerance: {missing:?}");
        }
    }

    // Every zone file must have a pack entry.
    let entries: std::collections::BTreeSet<&Path> = pack.entry_paths().collect();
    let missing_files = spec
        .zones
        .iter()
        .map(|z| z.file.as_str())
        .filter(|f| !entries.contains(Path::new(f)))
        .collect::<Vec<_>>();
    if !missing_files.is_empty() {
        println!(
            "  {} zone file(s) with NO pack entry, e.g. {:?}",
            missing_files.len(),
            &missing_files[..missing_files.len().min(5)]
        );
    }

    // Decode probes (first/middle/last zone), optional source A/B.
    let cache = SampleCache::with_pack(pack.clone());
    let mut decode_fail = 0usize;
    for i in [0usize, spec.zones.len() / 2, spec.zones.len() - 1] {
        let z = &spec.zones[i];
        match cache.get(Path::new(&z.file)) {
            Ok(data) => {
                print!(
                    "  decode {}: {} ch, {} Hz, {} frames ok",
                    z.file, data.channels, data.sample_rate, data.num_frames
                );
                if let Some(root) = src_root {
                    match load_sample(&root.join(&z.file)) {
                        Ok(src) if src.num_frames == data.num_frames => {
                            let corr = correlation(&data.to_f32(), &src.to_f32());
                            print!("  corr={corr:.5}");
                            if corr < 0.95 {
                                decode_fail += 1;
                                print!("  LOW");
                            }
                        }
                        Ok(src) => {
                            decode_fail += 1;
                            print!(
                                "  LENGTH MISMATCH vs source ({} != {})",
                                data.num_frames, src.num_frames
                            );
                        }
                        Err(e) => print!("  (source unreadable: {e})"),
                    }
                }
                println!();
            }
            Err(e) => {
                decode_fail += 1;
                println!("  decode {}: FAILED — {e}", z.file);
            }
        }
    }

    let looped = spec.zones.iter().filter(|z| z.loop_end > z.loop_start).count();
    println!("  looped zones: {looped}");

    let pass = gaps == 0 && missing_files.is_empty() && decode_fail == 0;
    println!("{}", if pass { "PASS" } else { "PARTIAL" });
    Ok(pass)
}

fn correlation(a: &[f32], b: &[f32]) -> f64 {
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| (*x as f64) * (*y as f64)).sum();
    let na: f64 = a.iter().map(|x| (*x as f64).powi(2)).sum();
    let nb: f64 = b.iter().map(|x| (*x as f64).powi(2)).sum();
    dot / (na.sqrt() * nb.sqrt()).max(f64::EPSILON)
}

// ── analysis reports ─────────────────────────────────────────────────────────

/// Parse "55,60-64,C4" into a MIDI-note set.
fn parse_note_set(s: &str) -> Result<std::collections::BTreeSet<u8>> {
    let mut out = std::collections::BTreeSet::new();
    for tok in s.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        if let Some((a, b)) = tok.split_once('-') {
            let (a, b) = (parse_note(a)?, parse_note(b)?);
            for n in a.min(b)..=a.max(b) {
                out.insert(n);
            }
        } else {
            out.insert(parse_note(tok)?);
        }
    }
    Ok(out)
}

/// "60" or a note name like "C4" / "G#3" (C4 = 60).
fn parse_note(s: &str) -> Result<u8> {
    if let Ok(n) = s.parse::<u8>() {
        return Ok(n);
    }
    let b = s.as_bytes();
    let step = match b.first().map(|c| c.to_ascii_uppercase()) {
        Some(b'C') => 0i32,
        Some(b'D') => 2,
        Some(b'E') => 4,
        Some(b'F') => 5,
        Some(b'G') => 7,
        Some(b'A') => 9,
        Some(b'B') => 11,
        _ => bail!("bad note {s:?}"),
    };
    let mut i = 1;
    let mut semis = step;
    if b.get(i) == Some(&b'#') {
        semis += 1;
        i += 1;
    } else if b.get(i) == Some(&b'b') {
        semis -= 1;
        i += 1;
    }
    let oct: i32 = s[i..].parse().map_err(|_| eyre::eyre!("bad note {s:?}"))?;
    Ok(((oct + 1) * 12 + semis).clamp(0, 127) as u8)
}

/// One candidate group's best explanation of the reference window.
struct GroupHit {
    articulation: String,
    rr_index: u32,
    direction: String,
    interval: u32,
    root_key: u8,
    /// Semitones the group was shifted by to match (grid-fill).
    shift: i32,
    /// Which grid-fill model produced this candidate: "resampled" (rate
    /// change, classic sampler) or "shifted" (time-preserving, ours).
    fill: String,
    /// Per-member `(dynamic label, gain dB)`, loudest first.
    layers: Vec<(String, f32)>,
    offset_frames: usize,
    sample_rate: u32,
    explained: f32,
}

/// Mono-sum a decoded sample to `f32`.
fn mono_of(pcm: &[f32], channels: usize, frames: usize) -> Vec<f32> {
    let ch = channels.max(1);
    (0..frames)
        .map(|f| {
            let base = f * ch;
            (0..ch).map(|c| pcm[base + c]).sum::<f32>() / ch as f32
        })
        .collect()
}

/// Pitch-shift `src` by `semitones` WITHOUT changing its duration — our
/// engine's grid-fill, as opposed to [`resample`], which is Kontakt's if
/// Kontakt fills its whole-tone grid the way a classic sampler does.
///
/// Offering the matcher both is the only way to tell the two apart. A note
/// that is off the sampling grid has no unshifted candidate, so the matcher is
/// FORCED to pick a neighbour whichever way it was filled; only the fit
/// quality of the two variants distinguishes them.
fn pitch_shifted(src: &[f32], semitones: i32) -> Vec<f32> {
    use crate::engine::pitch_shift::PitchShifter;
    let cents = f64::from(semitones) * 100.0;
    if PitchShifter::is_unity(cents) {
        return src.to_vec();
    }
    let mut sh = PitchShifter::new(cents);
    let startup = sh.startup_frames().min(src.len());
    // Same startup compensation the engine applies: feed the head through
    // first, then emit from where that left off.
    for &v in &src[..startup] {
        sh.tick(v);
    }
    src[startup..].iter().map(|&v| sh.tick(v)).collect()
}

/// Linear-resample `src` by `ratio` input frames per output frame.
fn resample(src: &[f32], ratio: f64, out_len: usize) -> Vec<f32> {
    (0..out_len)
        .map(|i| {
            let p = i as f64 * ratio;
            let idx = p as usize;
            if idx + 1 >= src.len() {
                return 0.0;
            }
            let frac = (p - idx as f64) as f32;
            src[idx] * (1.0 - frac) + src[idx + 1] * frac
        })
        .collect()
}



/// Sweep a whole reference render note by note — see [`Cmd::MatchRef::sweep`].
///
/// The MIDI that produced the reference gives every note's NOMINAL onset. For
/// each note this places a window `lead` seconds later and asks what the
/// reference is playing there; the recovered offset says how far into its
/// sample the reference had got, so `at − offset` is the note's ACTUAL onset.
/// The difference between the two is the drift, per note, sample-exact — and
/// the same sweep run against our own render makes the two directly
/// comparable.
#[allow(clippy::too_many_arguments)]
fn match_ref_sweep(
    pack_path: &Path,
    reference: &Path,
    midi: &Path,
    lead: f64,
    window_ms: f64,
    scan_ms: f64,
    articulation: Option<String>,
    mic: Option<String>,
    semitones: i32,
    from: Option<f64>,
    to: Option<f64>,
) -> Result<()> {
    let (notes, _ccs, _bpm) = parse_smf(midi)?;
    let patch = crate::PlayerPatch::from_pack(pack_path)?;
    let pack = patch.pack.clone().ok_or_else(|| eyre::eyre!("not a pack"))?;
    let cache = SampleCache::with_pack(pack);

    let refd = crate::engine::cache::load_sample(reference)
        .map_err(|e| eyre::eyre!("load {}: {e}", reference.display()))?;
    let sr = refd.sample_rate;
    let refmono = mono_of(&refd.to_f32(), refd.channels as usize, refd.num_frames);
    let wlen = ((window_ms / 1000.0) * f64::from(sr)) as usize;
    let scan = ((scan_ms / 1000.0) * f64::from(sr)) as usize;

    println!(
        "sweep {} against {} — {} note(s), window {:.0} ms at +{:.0} ms\n",
        midi.display(),
        reference.display(),
        notes.len(),
        window_ms,
        lead * 1000.0
    );
    println!(
        "  {:>7}  {:>4}  {:>9}  {:>9}  {:>7}  {:<22} {}",
        "nominal", "note", "actual", "drift", "share", "played", "gains"
    );

    // What the MIDI says each note IS: a step from the note before it on the
    // same line, or a fresh start when nothing was sounding. The matcher is
    // never told this — it is the independent check that turns a plausible fit
    // into ground truth.
    let mut expect: Vec<Option<(String, u32)>> = Vec::with_capacity(notes.len());
    for (i, n) in notes.iter().enumerate() {
        expect.push(if i == 0 {
            None
        } else {
            let prev = &notes[i - 1];
            let gap = f64::from(n.start) - f64::from(prev.start + prev.dur);
            // A note well after the line fell silent starts a phrase; CSS
            // plays a body, not a transition.
            if gap > 0.5 || prev.note == n.note {
                None
            } else {
                let delta = i32::from(n.note) - i32::from(prev.note);
                Some((
                    if delta > 0 { "up" } else { "down" }.to_string(),
                    delta.unsigned_abs(),
                ))
            }
        });
    }

    let mut agreed: Vec<f64> = Vec::new();
    let mut checked = 0usize;
    // Decode each sample ONCE. `to_f32` on a streamed sample decodes the whole
    // thing (it has to — a partial decode is what made every offline reading
    // of a FLAC pack wrong), so calling it per zone per note turned a 9-note
    // run into 20 minutes. Resampled and shifted variants are cached the same
    // way, keyed by the semitone move.
    let mut mono: std::collections::HashMap<(String, i32), std::sync::Arc<Vec<f32>>> =
        std::collections::HashMap::new();

    for (ni, n) in notes.iter().enumerate() {
        if from.is_some_and(|f| f64::from(n.start) < f) {
            continue;
        }
        if to.is_some_and(|t| f64::from(n.start) >= t) {
            continue;
        }
        let at = f64::from(n.start) + lead;
        let start = (at * f64::from(sr)) as usize;
        if start + wlen >= refmono.len() {
            continue;
        }
        let window = &refmono[start..start + wlen];

        // Only zones this note could plausibly be: Kontakt grid-fills by
        // resampling a neighbour, so roots within `semitones` of the note.
        type Key = (String, u8, String, u32, u32);
        let mut groups: std::collections::BTreeMap<Key, Vec<&crate::spec::ZoneSpec>> =
            std::collections::BTreeMap::new();
        for z in &patch.spec.zones {
            if articulation.as_ref().is_some_and(|a| &z.articulation != a) {
                continue;
            }
            if mic.as_ref().is_some_and(|m| &z.mic != m) {
                continue;
            }
            // A release sample is what a note ENDS with, never what it starts
            // with — but it is the same players in the same room, so it fits an
            // onset window well enough to win one (measured: a release zone
            // took the first note at 79.6%). Not a candidate here.
            if patch
                .spec
                .articulation(&z.articulation)
                .is_some_and(|a| matches!(a.kind, crate::spec::ArticulationKind::Release))
            {
                continue;
            }
            // Which note this zone SOUNDS. A sustain sounds its root; a
            // transition sounds its destination, which for an upward zone is
            // root+interval. Filtering on the root alone would offer a
            // transition group for the wrong note entirely.
            let sounds = if z.direction.eq_ignore_ascii_case("up") {
                i32::from(z.root_key) + z.interval as i32
            } else {
                i32::from(z.root_key)
            };
            if (sounds - i32::from(n.note)).abs() > semitones {
                continue;
            }
            groups
                .entry((
                    z.articulation.clone(),
                    z.root_key,
                    z.direction.clone(),
                    z.interval,
                    z.rr_index,
                ))
                .or_default()
                .push(z);
        }

        // Every plausible group, each resampled to this note the way Kontakt
        // would have, then peeled apart: mid-phrase a window holds the
        // outgoing body, the transition and the incoming body at once, and a
        // single-group fit can only ever explain its share of that.
        let mut labels: Vec<String> = Vec::new();
        let mut keys: Vec<Key> = Vec::new();
        let mut sets: Vec<Vec<Vec<f32>>> = Vec::new();
        for (key, zones) in &groups {
            let sounds = if key.2.eq_ignore_ascii_case("up") {
                i32::from(key.1) + key.3 as i32
            } else {
                i32::from(key.1)
            };
            let shift = i32::from(n.note) - sounds;
            let mut lab = Vec::new();
            let mut members: Vec<Vec<f32>> = Vec::new();
            for z in zones {
                let key_c = (z.file.clone(), shift);
                if !mono.contains_key(&key_c) {
                    let Ok(data) = cache.get(Path::new(&z.file)) else {
                        continue;
                    };
                    let base = match mono.get(&(z.file.clone(), 0)) {
                        Some(b) => std::sync::Arc::clone(b),
                        None => {
                            let b = std::sync::Arc::new(mono_of(
                                &data.to_f32(),
                                data.channels as usize,
                                data.num_frames,
                            ));
                            mono.insert((z.file.clone(), 0), std::sync::Arc::clone(&b));
                            b
                        }
                    };
                    let made = if shift == 0 {
                        base
                    } else {
                        let ratio = 2.0f64.powf(f64::from(shift) / 12.0);
                        let need = scan + wlen + 2;
                        std::sync::Arc::new(resample(
                            &base,
                            ratio,
                            need.min((base.len() as f64 / ratio) as usize),
                        ))
                    };
                    mono.insert(key_c.clone(), made);
                }
                if let Some(m) = mono.get(&key_c) {
                    lab.push(z.dynamic.clone());
                    members.push(m.as_ref().clone());
                }
            }
            if members.is_empty() {
                continue;
            }
            labels.push(lab.join("/"));
            keys.push(key.clone());
            sets.push(members);
        }

        let voices = crate::ref_match::decompose(window, &sets, scan, 48, 24, 3, 0.03);
        if voices.is_empty() {
            println!("  {:>6.3}s  {:>4}  (no fit)", n.start, n.note);
            checked += 1;
            continue;
        }
        checked += 1;
        for (vi, v) in voices.iter().enumerate() {
            let off_s = v.fit.offset as f64 / f64::from(sr);
            let actual = at - off_s;
            let drift = (actual - f64::from(n.start)) * 1000.0;
            let key = &keys[v.group];
            let sounds = if key.2.eq_ignore_ascii_case("up") {
                i32::from(key.1) + key.3 as i32
            } else {
                i32::from(key.1)
            };
            let shift = i32::from(n.note) - sounds;
            let gains = v
                .fit
                .gains
                .iter()
                .zip(labels[v.group].split('/'))
                .filter(|(g, _)| **g > 0.01)
                .map(|(g, l)| format!("{l} {:+.1}", 20.0 * g.log10()))
                .collect::<Vec<_>>()
                .join("  ");
            // Does the primary voice agree with the move the MIDI makes?
            let mark = if vi > 0 {
                " "
            } else {
                match (&expect[ni], key.2.is_empty()) {
                    // A transition was expected: direction and interval must
                    // both match the step.
                    (Some((dir, iv)), false) => {
                        if key.2.eq_ignore_ascii_case(dir) && key.3 == *iv {
                            agreed.push(drift);
                            "OK"
                        } else {
                            "xx"
                        }
                    }
                    // A phrase start was expected: a body, not a transition.
                    (None, true) => {
                        agreed.push(drift);
                        "OK"
                    }
                    _ => "xx",
                }
            };
            println!(
                "{mark}{:>6}  {:>4}  {:>8.3}s  {:>+7.1}ms  {:>6.1}%  {:<22} {}",
                if vi == 0 {
                    format!("{:.3}s", n.start)
                } else {
                    String::new()
                },
                if vi == 0 {
                    format!("{}", n.note)
                } else {
                    String::new()
                },
                actual,
                drift,
                v.share * 100.0,
                format!(
                    "{} {}{}{}",
                    key.0,
                    key.1,
                    if shift == 0 {
                        String::new()
                    } else {
                        format!("{shift:+}st")
                    },
                    if key.2.is_empty() {
                        String::new()
                    } else {
                        format!(" {}{}", key.2, key.3)
                    }
                ),
                gains
            );
        }
    }

    // Only rows whose zone agrees with the MIDI are worth averaging: the rest
    // are misidentifications, and their drift is noise about a note that was
    // never played.
    if agreed.is_empty() {
        println!("\n  no row agreed with the MIDI — nothing to calibrate from");
        return Ok(());
    }
    let mut sorted = agreed.clone();
    sorted.sort_by(f64::total_cmp);
    let median = sorted[sorted.len() / 2];
    let mean = agreed.iter().sum::<f64>() / agreed.len() as f64;
    let spread = (agreed.iter().map(|d| (d - mean).powi(2)).sum::<f64>()
        / agreed.len() as f64)
        .sqrt();
    println!(
        "\n  {}/{} notes agreed with the MIDI\n  drift  median {:+.1} ms   mean {:+.1} ms   sd {:.1} ms",
        agreed.len(),
        checked,
        median,
        mean,
        spread
    );
    println!(
        "  the median is the reference's own constant offset; subtract it and\n  what remains is per-note drift worth tuning against"
    );
    Ok(())
}

/// Score the matcher against a reference built from the pack's OWN samples at
/// known offsets and gains — see [`Cmd::MatchRef::self_test`].
///
/// The unit tests in [`crate::ref_match`] fit synthetic noise, which has no
/// periodic structure and so cannot exhibit the ambiguity that real sustained
/// strings do. This builds the same experiment out of real samples: place one
/// zone at a known onset, sum a second zone at a different known onset (the
/// crossfade case — a transition is two samples overlapping), and ask the
/// matcher to recover what it was given.
fn match_ref_self_test(
    pack_path: &Path,
    window_ms: f64,
    scan_ms: f64,
    articulation: Option<String>,
    mic: Option<String>,
) -> Result<()> {
    let patch = crate::PlayerPatch::from_pack(pack_path)?;
    let pack = patch.pack.clone().ok_or_else(|| eyre::eyre!("not a pack"))?;
    let cache = SampleCache::with_pack(pack);

    // One group: the dynamic layers of a single zone family.
    let mut chosen: Option<(String, u8, String, u32, u32)> = None;
    let mut group: Vec<(String, Vec<f32>)> = Vec::new();
    let mut sr = 48_000u32;
    for z in &patch.spec.zones {
        if articulation.as_ref().is_some_and(|a| &z.articulation != a) {
            continue;
        }
        if mic.as_ref().is_some_and(|m| &z.mic != m) {
            continue;
        }
        let key = (
            z.articulation.clone(),
            z.root_key,
            z.direction.clone(),
            z.interval,
            z.rr_index,
        );
        if chosen.as_ref().is_some_and(|c| c != &key) {
            continue;
        }
        let Ok(data) = cache.get(Path::new(&z.file)) else {
            continue;
        };
        sr = data.sample_rate;
        chosen = Some(key);
        group.push((
            z.dynamic.clone(),
            mono_of(&data.to_f32(), data.channels as usize, data.num_frames),
        ));
    }
    let (Some(key), false) = (chosen, group.is_empty()) else {
        bail!("no zones matched the filters");
    };
    let members: Vec<Vec<f32>> = group.iter().map(|(_, a)| a.clone()).collect();
    let shortest = members.iter().map(|m| m.len()).min().unwrap_or(0);

    let wlen = ((window_ms / 1000.0) * f64::from(sr)) as usize;
    let scan = ((scan_ms / 1000.0) * f64::from(sr)) as usize;
    // A known offset inside every member, comfortably past the attack.
    let truth = (scan / 3).min(shortest.saturating_sub(wlen + 1));
    if truth == 0 || wlen == 0 {
        bail!("samples too short for a {window_ms} ms window");
    }
    let gains: Vec<f32> = (0..members.len())
        .map(|i| 0.8 / (1.0 + i as f32))
        .collect();

    println!(
        "self-test on {} root {} rr {} — {} layer(s), truth offset {:.1} ms\n",
        key.0,
        key.1,
        key.4,
        members.len(),
        truth as f64 / f64::from(sr) * 1000.0
    );

    // Case 1: a static mixture of the group's layers at one shared onset.
    let window: Vec<f32> = (0..wlen)
        .map(|i| {
            members
                .iter()
                .zip(&gains)
                .map(|(m, g)| g * m[truth + i])
                .sum()
        })
        .collect();
    match crate::ref_match::best_fit(&window, &members, scan, 48, 24) {
        Some(fit) => {
            let err = fit.offset as f64 - truth as f64;
            println!(
                "  one group      offset {:+.1} ms off truth   explained {:.1}%   gains {}",
                err / f64::from(sr) * 1000.0,
                fit.explained * 100.0,
                fit.gains
                    .iter()
                    .map(|g| format!("{g:.3}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            println!(
                "                 {}",
                if err.abs() <= 2.0 {
                    "PASS — sample-exact on real content"
                } else {
                    "FAIL — the offset is wrong on real content"
                }
            );
        }
        None => println!("  one group      FAIL — no fit"),
    }

    // Case 1b: the same mixture, but placed at offset ZERO. If the search can
    // find a mixture only when it sits at the very start of the scan, the
    // coarse stage is what is broken, not the fit.
    let window0: Vec<f32> = (0..wlen)
        .map(|i| members.iter().zip(&gains).map(|(m, g)| g * m[i]).sum())
        .collect();
    match crate::ref_match::best_fit(&window0, &members, scan, 48, 24) {
        Some(fit) => println!(
            "  at offset 0    found {:.1} ms   explained {:.1}%",
            fit.offset as f64 / f64::from(sr) * 1000.0,
            fit.explained * 100.0
        ),
        None => println!("  at offset 0    no fit"),
    }

    // Case 2: the crossfade case — the same layers, but a SECOND copy laid in
    // at a different onset, as an overlapping transition would be. A model
    // that assumes one shared offset cannot express this; the point is to
    // measure how badly it goes wrong before the multi-voice fit lands.
    let second = truth / 2;
    let window2: Vec<f32> = (0..wlen)
        .map(|i| {
            let a: f32 = members
                .iter()
                .zip(&gains)
                .map(|(m, g)| g * m[truth + i])
                .sum();
            let b: f32 = members
                .iter()
                .map(|m| 0.5 * m[second + i])
                .sum();
            a + b
        })
        .collect();
    println!(
        "\n  crossfade truth: a voice at {:.1} ms and another at {:.1} ms",
        truth as f64 / f64::from(sr) * 1000.0,
        second as f64 / f64::from(sr) * 1000.0
    );
    let groups = vec![members.clone()];
    let voices = crate::ref_match::decompose(&window2, &groups, scan, 48, 24, 3, 0.02);
    if voices.is_empty() {
        println!("  two onsets     FAIL — nothing found");
    }
    for (i, v) in voices.iter().enumerate() {
        let off_ms = v.fit.offset as f64 / f64::from(sr) * 1000.0;
        let d_late = off_ms - truth as f64 / f64::from(sr) * 1000.0;
        let d_early = off_ms - second as f64 / f64::from(sr) * 1000.0;
        let hit = if d_late.abs() <= 2.0 {
            "= the later voice "
        } else if d_early.abs() <= 2.0 {
            "= the earlier voice"
        } else {
            "?? matches neither"
        };
        println!(
            "  voice {}        offset {:8.1} ms  {}   share {:5.1}%   gains {}",
            i + 1,
            off_ms,
            hit,
            v.share * 100.0,
            v.fit
                .gains
                .iter()
                .map(|g| format!("{g:.3}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
    Ok(())
}

/// See [`Cmd::MatchRef`].
#[allow(clippy::too_many_arguments)]
fn match_ref(
    pack_path: &Path,
    reference: &Path,
    at: f64,
    window_ms: f64,
    scan_ms: f64,
    notes: Option<String>,
    articulation: Option<String>,
    mic: Option<String>,
    semitones: i32,
    top: usize,
) -> Result<()> {
    let patch = crate::PlayerPatch::from_pack(pack_path)?;
    let pack = patch.pack.clone().ok_or_else(|| eyre::eyre!("not a pack"))?;
    let cache = SampleCache::with_pack(pack);
    let note_set = notes.as_deref().map(parse_note_set).transpose()?;

    let refd = crate::engine::cache::load_sample(reference)
        .map_err(|e| eyre::eyre!("load {}: {e}", reference.display()))?;
    let sr = refd.sample_rate;
    let refmono = mono_of(&refd.to_f32(), refd.channels as usize, refd.num_frames);

    let wlen = ((window_ms / 1000.0) * f64::from(sr)) as usize;
    let start = (at * f64::from(sr)) as usize;
    if start + wlen >= refmono.len() {
        bail!(
            "--at {at}s + window is past the end of {}",
            reference.display()
        );
    }
    let window = &refmono[start..start + wlen];
    let scan = ((scan_ms / 1000.0) * f64::from(sr)) as usize;

    // Group the zones that sound TOGETHER: one zone family's dynamic layers,
    // which share an onset and are crossfaded by CC1. Fitting them jointly is
    // what lets the score separate the group that is playing from one that
    // merely resembles part of the mixture.
    type Key = (String, u8, String, u32, u32);
    let mut groups: std::collections::BTreeMap<Key, Vec<&crate::spec::ZoneSpec>> =
        std::collections::BTreeMap::new();
    for z in &patch.spec.zones {
        if articulation.as_ref().is_some_and(|a| &z.articulation != a) {
            continue;
        }
        if mic.as_ref().is_some_and(|m| &z.mic != m) {
            continue;
        }
        if note_set.as_ref().is_some_and(|s| !s.contains(&z.root_key)) {
            continue;
        }
        groups
            .entry((
                z.articulation.clone(),
                z.root_key,
                z.direction.clone(),
                z.interval,
                z.rr_index,
            ))
            .or_default()
            .push(z);
    }
    if groups.is_empty() {
        bail!("no zones matched the filters");
    }

    let mut hits: Vec<GroupHit> = Vec::new();
    for (key, zones) in &groups {
        let mut decoded: Vec<(String, Vec<f32>)> = Vec::new();
        for z in zones {
            let Ok(data) = cache.get(Path::new(&z.file)) else {
                continue;
            };
            decoded.push((
                z.dynamic.clone(),
                mono_of(&data.to_f32(), data.channels as usize, data.num_frames),
            ));
        }
        if decoded.is_empty() {
            continue;
        }
        for shift in -semitones..=semitones {
            // Both grid-fill models, so the fit can say which one the
            // reference actually used.
            let variants: Vec<(&str, Vec<Vec<f32>>)> = if shift == 0 {
                vec![("", decoded.iter().map(|(_, a)| a.clone()).collect())]
            } else {
                let ratio = 2.0f64.powf(f64::from(shift) / 12.0);
                let need = scan + wlen + 2;
                vec![
                    (
                        "resampled",
                        decoded
                            .iter()
                            .map(|(_, a)| {
                                resample(a, ratio, need.min((a.len() as f64 / ratio) as usize))
                            })
                            .collect(),
                    ),
                    (
                        "shifted",
                        decoded
                            .iter()
                            .map(|(_, a)| pitch_shifted(a, shift))
                            .collect(),
                    ),
                ]
            };
            for (fill, members) in variants {
            let Some(fit) = crate::ref_match::best_fit(window, &members, scan, 48, 24) else {
                continue;
            };
            let mut layers: Vec<(String, f32)> = decoded
                .iter()
                .zip(&fit.gains)
                .map(|((label, _), g)| {
                    (
                        label.clone(),
                        if *g > 0.0 { 20.0 * g.log10() } else { f32::NEG_INFINITY },
                    )
                })
                .collect();
            layers.sort_by(|a, b| b.1.total_cmp(&a.1));
            hits.push(GroupHit {
                articulation: key.0.clone(),
                root_key: key.1,
                direction: key.2.clone(),
                interval: key.3,
                rr_index: key.4,
                shift,
                fill: fill.to_string(),
                layers,
                offset_frames: fit.offset,
                sample_rate: sr,
                explained: fit.explained,
            });
            }
        }
    }
    if hits.is_empty() {
        bail!("no candidate group could be fitted");
    }
    hits.sort_by(|a, b| b.explained.total_cmp(&a.explained));

    println!(
        "reference {} at {at:.3}s, {window_ms:.0} ms window, {scan_ms:.0} ms scan — {} group(s)\n",
        reference.display(),
        groups.len()
    );
    println!(
        "  {:>8}  {:>9}  {:>9}  {:<10} {:>2}  {:<26} {}",
        "explained", "offset", "onset", "artic", "rr", "root", "layer gains"
    );
    for h in hits.iter().take(top) {
        let off_ms = h.offset_frames as f64 / f64::from(h.sample_rate) * 1000.0;
        // The sample is `off_ms` in at time `at`, so it was triggered that
        // much earlier — the note's true onset in the reference.
        let onset = at - off_ms / 1000.0;
        let shift = if h.shift == 0 {
            String::new()
        } else {
            format!(" {:+}st {}", h.shift, h.fill)
        };
        let dir = if h.direction.is_empty() {
            String::new()
        } else {
            format!(" {}{}", h.direction, h.interval)
        };
        let gains = h
            .layers
            .iter()
            .filter(|(_, db)| db.is_finite())
            .map(|(l, db)| format!("{l} {db:+.1}"))
            .collect::<Vec<_>>()
            .join("  ");
        println!(
            "  {:>7.1}%  {:>7.1}ms  {:>8.3}s  {:<10} {:>2}  {:<26} {}",
            h.explained * 100.0,
            off_ms,
            onset,
            h.articulation,
            h.rr_index,
            format!("{}{}{}", h.root_key, shift, dir),
            gains
        );
    }
    Ok(())
}

fn inspect_samples(
    pack_path: &Path,
    articulation: Option<String>,
    mic: Option<String>,
    notes: Option<String>,
    limit: usize,
    out: &Path,
) -> Result<()> {
    let patch = crate::PlayerPatch::from_pack(pack_path)?;
    let pack = patch.pack.clone().ok_or_else(|| eyre::eyre!("not a pack"))?;
    let cache = SampleCache::with_pack(pack);
    let note_set = notes.as_deref().map(parse_note_set).transpose()?;

    let mut entries = Vec::new();
    for z in &patch.spec.zones {
        if let Some(a) = &articulation {
            if &z.articulation != a {
                continue;
            }
        }
        if let Some(m) = &mic {
            if &z.mic != m {
                continue;
            }
        }
        if let Some(set) = &note_set {
            if !set.contains(&z.root_key) {
                continue;
            }
        }
        let data = match cache.get(Path::new(&z.file)) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("  skip {} ({e})", z.file);
                continue;
            }
        };
        entries.push(crate::report::SampleView {
            title: z.file.clone(),
            audio: data.to_f32().into_owned(),
            channels: data.channels as usize,
            sample_rate: data.sample_rate,
            zone: z.clone(),
        });
        if entries.len() >= limit {
            break;
        }
    }
    if entries.is_empty() {
        bail!("no zones matched the filters");
    }
    let name = format!(
        "{} — samples",
        pack_path.file_stem().unwrap_or_default().to_string_lossy()
    );
    let n = entries.len();
    let data = crate::report::sample_report_json(&name, &entries);
    crate::report::write_report_html(out, &data)?;
    println!("wrote {} ({n} sample(s))", out.display());
    Ok(())
}

/// One scripted note: start/dur in seconds.
struct ScriptNote {
    note: u8,
    velocity: u8,
    start: f32,
    dur: f32,
}

/// "60@0:2,62@2:1.5,C5@4:2v80" — note[@start_s][:dur_s][vNN]. Missing start =
/// previous end; missing dur = 2 s.
/// What [`parse_smf`] yields: merged notes, timed CC events as
/// `(seconds, controller, value)`, and the file's first tempo in BPM.
type Smf = (Vec<ScriptNote>, Vec<(f32, u8, u8)>, f64);

/// Minimal Standard-MIDI-File reader: merged note list (paired on/off) +
/// timed CC events + the file's first tempo. Format 0/1, running status,
/// channel-blind (an instrument render). Times in seconds.
fn parse_smf(path: &Path) -> Result<Smf> {
    let d = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    if d.len() < 14 || &d[0..4] != b"MThd" {
        bail!("not a Standard MIDI File");
    }
    let ntrk = u16::from(d[10]) << 8 | u16::from(d[11]);
    let tpq = (u16::from(d[12]) << 8 | u16::from(d[13])) as f64;
    let mut bpm = 120.0f64;
    let mut notes: Vec<ScriptNote> = Vec::new();
    let mut ccs: Vec<(f32, u8, u8)> = Vec::new();
    let mut open: BTreeMap<u8, (f64, u8)> = BTreeMap::new(); // pitch → (tick, vel)
    let vlq = |d: &[u8], i: &mut usize| -> u64 {
        let mut v = 0u64;
        loop {
            let c = d[*i];
            *i += 1;
            v = (v << 7) | u64::from(c & 0x7f);
            if c & 0x80 == 0 {
                return v;
            }
        }
    };
    let mut i = 14usize;
    let mut trk = 0;
    while i + 8 <= d.len() && trk < ntrk {
        if &d[i..i + 4] != b"MTrk" {
            i += 1;
            continue;
        }
        let len = u32::from_be_bytes([d[i + 4], d[i + 5], d[i + 6], d[i + 7]]) as usize;
        i += 8;
        let end = i + len;
        trk += 1;
        let mut t = 0f64;
        let mut status = 0u8;
        while i < end {
            t += vlq(&d, &mut i) as f64;
            let mut st = d[i];
            if st >= 0x80 {
                i += 1;
                if st < 0xF0 {
                    status = st;
                }
            } else {
                st = status;
            }
            match st {
                0xFF => {
                    let meta = d[i];
                    i += 1;
                    let n = vlq(&d, &mut i) as usize;
                    if meta == 0x51 && n == 3 {
                        let us = u32::from(d[i]) << 16 | u32::from(d[i + 1]) << 8 | u32::from(d[i + 2]);
                        bpm = 60_000_000.0 / f64::from(us);
                    }
                    i += n;
                    if meta == 0x2F {
                        break;
                    }
                }
                0xF0 | 0xF7 => {
                    let n = vlq(&d, &mut i) as usize;
                    i += n;
                }
                _ => {
                    let sec_per_tick = 60.0 / bpm / tpq;
                    match st & 0xF0 {
                        0x90 | 0x80 => {
                            let (p, v) = (d[i], d[i + 1]);
                            i += 2;
                            if (st & 0xF0) == 0x90 && v > 0 {
                                open.insert(p, (t, v));
                            } else if let Some((t0, vel)) = open.remove(&p) {
                                notes.push(ScriptNote {
                                    note: p,
                                    velocity: vel,
                                    start: (t0 * sec_per_tick) as f32,
                                    dur: ((t - t0) * sec_per_tick) as f32,
                                });
                            }
                        }
                        0xB0 => {
                            let (cc, v) = (d[i], d[i + 1]);
                            i += 2;
                            ccs.push(((t * sec_per_tick) as f32, cc, v));
                        }
                        0xC0 | 0xD0 => i += 1,
                        _ => i += 2,
                    }
                }
            }
        }
        i = end;
    }
    notes.sort_by(|a, b| a.start.total_cmp(&b.start));
    ccs.sort_by(|a, b| a.0.total_cmp(&b.0));
    if notes.is_empty() {
        bail!("no notes in {}", path.display());
    }
    Ok((notes, ccs, bpm))
}

fn parse_note_script(s: &str) -> Result<Vec<ScriptNote>> {
    let mut out: Vec<ScriptNote> = Vec::new();
    let mut cursor = 0.0f32;
    for tok in s.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        let (tok, velocity) = match tok.rsplit_once('v') {
            Some((rest, v)) if v.chars().all(|c| c.is_ascii_digit()) && !rest.is_empty() => {
                (rest, v.parse::<u8>()?)
            }
            _ => (tok, 90),
        };
        let (note_s, timing) = match tok.split_once('@') {
            Some((n, t)) => (n, Some(t)),
            None => (tok, None),
        };
        let note = parse_note(note_s)?;
        let (start, dur) = match timing {
            Some(t) => match t.split_once(':') {
                Some((st, d)) => (st.parse::<f32>()?, d.parse::<f32>()?),
                None => (t.parse::<f32>()?, 2.0),
            },
            None => (cursor, 2.0),
        };
        cursor = start + dur;
        out.push(ScriptNote {
            note,
            velocity,
            start,
            dur,
        });
    }
    if out.is_empty() {
        bail!("empty note script");
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn render_report(
    pack_path: &Path,
    notes: &str,
    cc1: u8,
    cc2: u8,
    out: &Path,
    wav: Option<PathBuf>,
    _tail: f32,
    bpm: Option<f64>,
    beats_per_bar: u32,
    pure: bool,
    vel_override: Option<u8>,
    audio_in: Option<PathBuf>,
    midi: Option<PathBuf>,
    label: Option<String>,
) -> Result<()> {
    use crate::document::{DocCc, DocNote, DocumentRenderOptions, TempoPoint, TrackDocument};
    const SR: u32 = 48_000;
    const ID: &str = "report";

    // External-audio report: show a reference bounce (e.g. a real Kontakt CSS
    // render) as the waveform with the beat grid — no engine render, no voices.
    if let Some(audio_path) = audio_in.as_ref() {
        let data = crate::engine::cache::load_sample(audio_path)
            .map_err(|e| eyre::eyre!("load {}: {e}", audio_path.display()))?;
        let ch = data.channels.max(1) as usize;
        let pcm = data.to_f32();
        // Interleave to stereo (duplicate mono).
        let audio: Vec<f32> = if ch >= 2 {
            (0..data.num_frames)
                .flat_map(|f| [pcm[f * ch], pcm[f * ch + 1]])
                .collect()
        } else {
            pcm.iter().flat_map(|&s| [s, s]).collect()
        };
        let sr = data.sample_rate;
        let wav_path = wav.unwrap_or_else(|| out.with_extension("wav"));
        {
            let mut w = hound::WavWriter::create(
                &wav_path,
                hound::WavSpec {
                    channels: 2,
                    sample_rate: sr,
                    bits_per_sample: 32,
                    sample_format: hound::SampleFormat::Float,
                },
            )?;
            for s in &audio {
                w.write_sample(*s)?;
            }
            w.finalize()?;
        }
        let audio_href = wav_path.file_name().map(|f| f.to_string_lossy().into_owned());
        let click_href = match bpm {
            Some(b) => {
                let cs = std::path::Path::new(crate::report::DEFAULT_CLICK_SAMPLE);
                let click =
                    crate::report::click_track(audio.len() / 2, sr, b, beats_per_bar, cs.exists().then_some(cs));
                let stem = wav_path.file_stem().unwrap_or_default().to_string_lossy();
                let mut mixed = audio.clone();
                for (m, c) in mixed.iter_mut().zip(click.iter()) {
                    *m = (*m + c * 0.7).clamp(-1.0, 1.0);
                }
                let fname = format!("{stem}.click.wav");
                let mut w = hound::WavWriter::create(
                    wav_path.with_file_name(&fname),
                    hound::WavSpec { channels: 2, sample_rate: sr, bits_per_sample: 32, sample_format: hound::SampleFormat::Float },
                )?;
                for s in &mixed { w.write_sample(*s)?; }
                w.finalize()?;
                Some(fname)
            }
            None => None,
        };
        let sources = crate::report::ReportSources {
            trace: crate::engine::RenderTrace::default(),
            fires: vec![],
            markers: vec![],
            emitted: vec![],
            audio_href,
            stems: vec![],
            tempo: bpm.map(|b| (b, beats_per_bar)),
            click_href,
            mode_label: label.unwrap_or_else(|| "REFERENCE".into()),
            reactive_fallbacks: 0,
        };
        let name = format!(
            "{} — {}",
            audio_path.file_stem().unwrap_or_default().to_string_lossy(),
            sources.mode_label
        );
        let data_json = crate::report::render_report_json(&name, &audio, 2, sr, &sources);
        crate::report::write_report_html(out, &data_json)?;
        println!("wrote {} (external audio {})", out.display(), audio_path.display());
        return Ok(());
    }

    const SEED: u64 = 0x00DA_11A5_EED0_0001;
    // MIDI-file input: notes + note-offs + ALL timed CCs from the SMF (so
    // mid-render CC1/CC2 sweeps replay exactly); tempo from the file unless
    // `--bpm` overrides the grid. `--notes` is the hand-script alternative.
    let (mut script, midi_ccs, bpm) = match midi.as_ref() {
        Some(p) => {
            let (s, ccs, file_bpm) = parse_smf(p)?;
            (s, ccs, Some(bpm.unwrap_or(file_bpm)))
        }
        None => (parse_note_script(notes)?, Vec::new(), bpm),
    };
    // `render-compare` forces one velocity across the whole line so the ONLY
    // difference between variants is the legato speed zone (slow/medium/fast).
    if let Some(v) = vel_override {
        for n in &mut script {
            n.velocity = v;
        }
    }

    // What one render pass returns (rebased to the audible window).
    type PassOut = (
        Vec<f32>,
        crate::engine::RenderTrace,
        Vec<crate::engine::LegatoFireEvent>,
        Vec<(u64, String, u8, u8)>, // markers (frame, kind, note, line)
        Vec<crate::engine::EmittedMarker>,
        u64, // reactive_fallbacks (must be 0 in document mode)
    );

    // ALWAYS document mode (ARA-style prefire): the scheduler fires each note
    // early so the sample starts BEFORE the beat and its arrival lands ON the
    // beat. Live/reactive triggering is intentionally not used here — it was a
    // constant source of "why is the timing different" confusion. `--bpm` only
    // sets the grid/ruler reference; the qn conversion defaults to 120.
    let bpm_v = bpm.unwrap_or(120.0);

    let render_pass = |solo: Option<std::collections::BTreeSet<u8>>| -> Result<PassOut> {
        let rig = crate::SamplerRig::new_offline_with_cache_budget(SR, Some(8 * 1024 * 1024 * 1024));
        rig.load_pack(ID, pack_path)?;
        // note_on dispatches on MIDI channel 0 — an unmapped instrument is
        // silent (the MM2 trap).
        rig.set_midi_channel(ID, 0);
        rig.set_pure_playback(ID, pure);
        rig.set_trace_enabled(ID, true);
        rig.set_legato_fire_log_enabled(ID, true);
        rig.set_solo_notes(ID, solo);
        for n in &script {
            rig.warm_note(ID, n.note);
        }

        {
            // Seconds → quarter-notes at the given tempo. Notes touch/overlap
            // a hair so the engine reads them as one legato line.
            let sec_to_qn = |s: f32| (s as f64) * bpm_v / 60.0;
            let notes_doc: Vec<DocNote> = script
                .iter()
                .map(|n| DocNote {
                    start_qn: sec_to_qn(n.start),
                    end_qn: sec_to_qn(n.start + n.dur) + 1.0 / 64.0,
                    chan: 0,
                    pitch: n.note,
                    vel: n.velocity,
                })
                .collect();
            let doc = TrackDocument {
                version: 1,
                seed: SEED,
                auto_divisi: false,
                // Document mode is always Expressive (velocity drives the
                // slow/medium/fast zone → 333/250/100 ms); no CC58 needed.
                // With `--midi`, the file's timed CC events follow the fixed
                // initial values (sweeps replay exactly).
                ccs: {
                    let mut v = vec![
                        DocCc { qn: 0.0, chan: 0, cc: 1, val: cc1 },
                        DocCc { qn: 0.0, chan: 0, cc: 2, val: cc2 },
                    ];
                    v.extend(midi_ccs.iter().map(|&(sec, cc, val)| DocCc {
                        qn: sec_to_qn(sec),
                        chan: 0,
                        cc,
                        val,
                    }));
                    v
                },
                notes: notes_doc,
                tempo: vec![TempoPoint { qn: 0.0, bpm: bpm_v }],
            };
            let res = rig
                .render_offline_document(ID, &doc, &DocumentRenderOptions::default())
                .map_err(|e| eyre::eyre!("document render: {e}"))?;
            // res.audio starts at res.start_frame; trace is engine-absolute.
            // Rebase everything to the audible window (frame 0 = start_frame).
            let base = res.start_frame;
            let mut trace = rig.render_trace(ID);
            trace.events.retain(|e| e.frame >= base);
            for e in &mut trace.events {
                e.frame -= base;
            }
            let mut fires = res.transitions.clone();
            for f in &mut fires {
                f.frame = f.frame.saturating_sub(base);
                f.arrival = f.arrival.saturating_sub(base);
            }
            let markers: Vec<(u64, String, u8, u8)> = res
                .markers
                .iter()
                .filter(|m| m.frame >= base)
                .map(|m| (m.frame - base, format!("{:?}", m.kind), m.note, m.line))
                .collect();
            let mut emitted = res.emitted_markers.clone();
            emitted.retain(|m| m.frame >= base);
            for m in &mut emitted {
                m.frame -= base;
            }
            Ok((res.audio, trace, fires, markers, emitted, res.reactive_fallbacks))
        }
    };

    let write_wav = |path: &Path, audio: &[f32]| -> Result<()> {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: SR,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec)?;
        for s in audio {
            w.write_sample(*s)?;
        }
        w.finalize()?;
        Ok(())
    };

    // Full mix (also the source of the trace + fires shown on the timeline).
    let (audio, trace, fires, markers, emitted, reactive_fallbacks) = render_pass(None)?;
    let wav_path = wav.unwrap_or_else(|| out.with_extension("wav"));
    write_wav(&wav_path, &audio)?;
    let audio_href = wav_path.file_name().map(|f| f.to_string_lossy().into_owned());

    // Per-note solo stems, one WAV per distinct scripted note.
    let mut distinct: Vec<u8> = script.iter().map(|n| n.note).collect();
    distinct.sort_unstable();
    distinct.dedup();
    let stem_base = wav_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "render".into());
    let mut stems = Vec::new();
    for note in &distinct {
        let (stem_audio, _, _, _, _, _) =
            render_pass(Some(std::iter::once(*note).collect()))?;
        let fname = format!("{stem_base}.n{note}.wav");
        write_wav(&wav_path.with_file_name(&fname), &stem_audio)?;
        stems.push((*note, note_name(*note), fname));
    }

    // Metronome click BAKED into a full-length copy of the mix (same buffer,
    // frame-0 aligned) — so the click can never drift or be "faked": it IS the
    // rendered audio. The viewer's click toggle just swaps the player source
    // between the dry mix (`audio_href`) and this baked one (like solo),
    // never a separate synced <audio> element. Only meaningful with a tempo.
    let click_href = match bpm {
        Some(b) => {
            let click_sample = std::path::Path::new(crate::report::DEFAULT_CLICK_SAMPLE);
            let click = crate::report::click_track(
                audio.len() / 2,
                SR,
                b,
                beats_per_bar,
                click_sample.exists().then_some(click_sample),
            );
            let mut mixed = audio.clone();
            for (m, c) in mixed.iter_mut().zip(click.iter()) {
                *m = (*m + c * 0.7).clamp(-1.0, 1.0);
            }
            let fname = format!("{stem_base}.click.wav");
            write_wav(&wav_path.with_file_name(&fname), &mixed)?;
            Some(fname)
        }
        None => None,
    };

    let sources = crate::report::ReportSources {
        trace,
        fires,
        markers,
        emitted,
        audio_href,
        stems,
        tempo: bpm.map(|b| (b, beats_per_bar)),
        click_href,
        mode_label: format!("DOCUMENT (prefire){}", if pure { " · PURE" } else { "" }),
        reactive_fallbacks,
    };
    let name = format!(
        "{} — {notes}",
        pack_path.file_stem().unwrap_or_default().to_string_lossy()
    );
    let data = crate::report::render_report_json(&name, &audio, 2, SR, &sources);
    crate::report::write_report_html(out, &data)?;
    println!(
        "wrote {} (+ {} + {} solo stem(s)) — {} trace events, {} fires",
        out.display(),
        wav_path.display(),
        distinct.len(),
        sources.trace.events.len(),
        sources.fires.len()
    );
    Ok(())
}

/// Render the same line at several legato velocities and write a wrapper page
/// that toggles between the full per-variant reports (iframe swap) so the
/// slow/medium/fast timing can be seen and heard side-by-side.
#[allow(clippy::too_many_arguments)]
fn render_compare(
    pack_path: &Path,
    notes: &str,
    cc1: u8,
    cc2: u8,
    out: &Path,
    tail: f32,
    bpm: Option<f64>,
    beats_per_bar: u32,
    pure: bool,
    velocities: &str,
) -> Result<()> {
    // Parse "label:vel,label:vel,…".
    let variants: Vec<(String, u8)> = velocities
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|tok| {
            let (label, vel) = tok
                .split_once(':')
                .ok_or_else(|| eyre::eyre!("variant must be label:velocity, got {tok:?}"))?;
            Ok((label.to_string(), vel.trim().parse::<u8>()?))
        })
        .collect::<Result<_>>()?;
    if variants.is_empty() {
        bail!("no variants given");
    }

    let stem = out
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "compare".into());

    // Render each variant to its own full report next to the wrapper.
    let mut entries: Vec<(String, u8, String)> = Vec::new(); // (label, vel, html filename)
    for (label, vel) in &variants {
        let html_name = format!("{stem}.{label}.html");
        let variant_out = out.with_file_name(&html_name);
        let variant_wav = out.with_file_name(format!("{stem}.{label}.wav"));
        render_report(
            pack_path,
            notes,
            cc1,
            cc2,
            &variant_out,
            Some(variant_wav),
            tail,
            bpm,
            beats_per_bar,
            pure,
            Some(*vel),
            None,
            None,
            None,
        )?;
        entries.push((label.clone(), *vel, html_name));
    }

    // Wrapper: a button per variant swaps the iframe source. Each variant is a
    // complete, independent report (play / click / zoom / playhead intact).
    let buttons: String = entries
        .iter()
        .enumerate()
        .map(|(i, (label, vel, href))| {
            format!(
                "<button data-src=\"{href}\" onclick=\"pick(this)\"{cls}>{label} · vel {vel}</button>",
                cls = if i == 0 { " class=\"on\"" } else { "" }
            )
        })
        .collect();
    let first = &entries[0].2;
    let title = format!(
        "{} — legato speed compare",
        pack_path.file_stem().unwrap_or_default().to_string_lossy()
    );
    let html = format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><title>{title}</title>
<style>
  html,body{{margin:0;height:100%;background:#0d0f13;color:#d8dce2;font:13px/1.4 ui-sans-serif,system-ui,sans-serif}}
  #bar{{display:flex;gap:8px;align-items:center;padding:8px 12px;border-bottom:1px solid #222}}
  #bar b{{color:#9aa4b2;font-weight:600;margin-right:4px}}
  #bar button{{background:#1b1f26;color:#d8dce2;border:1px solid #3a404c;border-radius:4px;padding:4px 12px;font:inherit;cursor:pointer}}
  #bar button.on{{background:#31506e;border-color:#4a7bb0}}
  #f{{border:0;width:100%;height:calc(100% - 40px);display:block;background:#0d0f13}}
  .hint{{color:#6b7280;margin-left:auto}}
</style></head><body>
<div id="bar"><b>legato:</b>{buttons}<span class="hint">same line, same tempo — only the transition speed changes</span></div>
<iframe id="f" src="{first}"></iframe>
<script>
function pick(b){{
  document.querySelectorAll('#bar button').forEach(x=>x.classList.toggle('on', x===b));
  document.getElementById('f').src = b.dataset.src;
}}
</script></body></html>
"#
    );
    std::fs::write(out, html).with_context(|| format!("write {}", out.display()))?;
    println!(
        "wrote {} — {} variants: {}",
        out.display(),
        entries.len(),
        entries
            .iter()
            .map(|(l, v, _)| format!("{l}(v{v})"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(())
}

/// MIDI note → name (C4 = 60).
fn note_name(n: u8) -> String {
    const NN: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!("{}{}", NN[(n % 12) as usize], (n / 12) as i32 - 1)
}

// ── build ────────────────────────────────────────────────────────────────────

fn build(samples_root: &Path, out: &Path, codec: &str, quality: f32) -> Result<()> {
    let codec = parse_codec(codec, quality)?;
    let spec = samples_root.join("library.styx");
    if !spec.exists() {
        bail!("no library.styx in {}", samples_root.display());
    }
    let mut paths: Vec<PathBuf> = std::fs::read_dir(samples_root)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| matches!(e.to_ascii_lowercase().as_str(), "flac" | "wav" | "aif" | "aiff"))
                .unwrap_or(false)
        })
        .collect();
    paths.sort();
    let stats = create_signal_pack_with(
        out,
        PackSpecSource::Path(&spec),
        samples_root,
        paths.iter().map(PathBuf::as_path),
        codec,
    )?;
    println!("built {} — {} packed, {} failed", out.display(), stats.prepared, stats.failed);
    if stats.prepared == 0 {
        bail!("no samples packed");
    }
    Ok(())
}
