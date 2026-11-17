//! Library specification types.
//!
//! Specs are `.styx` files (preferred) or `.toml` files.
//! Load with [`LibrarySpec::from_file`] — format is auto-detected by extension.
//!
//! Third-party libraries can be supported by writing a new spec file with no
//! code changes.

use std::collections::HashMap;
use std::path::Path;

use facet::Facet;
use signal_proto::tagging::{StructuredTag, TagSet};

use crate::SamplerError;

// ── Top-level ─────────────────────────────────────────────────────────────────

/// Complete specification for one sample library.
///
/// Loaded from a `.styx` file (preferred) or `.toml` file.
#[derive(Debug, Clone, Facet)]
pub struct LibrarySpec {
    /// Display name, e.g. "Cinematic Strings".
    pub name: String,
    /// Library version string.
    #[facet(default)]
    pub version: String,
    /// Vendor / developer name.
    #[facet(default)]
    pub vendor: String,

    /// Instrument sections (e.g. 1v / 2v / Va / Ce / Ba for strings).
    #[facet(default)]
    pub sections: Vec<SectionSpec>,

    /// Microphone positions.
    #[facet(default)]
    pub mics: Vec<MicSpec>,

    /// Dynamic control model (CC1, velocity, etc.).
    #[facet(default)]
    pub dynamics: DynamicsSpec,

    /// All articulations in this library.
    #[facet(default)]
    pub articulations: Vec<ArticulationSpec>,

    /// Legato engine configuration (absent for piano/drums).
    pub legato_engine: Option<LegatoEngineSpec>,

    /// Library-wide playback-policy numbers (makeup gain, master tune,
    /// note-off fades, loop crossfade, …). All fields default to the values
    /// the engine historically hardcoded, so specs that don't set them are
    /// bit-identical to the pre-data-driven behaviour.
    // r[impl signal.soundsource.declarative]
    #[facet(default)]
    pub performance: PerformanceSpec,

    /// Short-note pre-delay compensation.
    pub short_note_timing: Option<ShortNoteTimingSpec>,

    /// Keyswitch and CC58 articulation switching.
    pub keyswitch: Option<KeyswitchSpec>,

    /// A fretted instrument's strings: when set, every note is placed on
    /// one string and fret and plays that string's zones
    /// (`ZoneSpec::string`). `None` = not fretted.
    pub fretboard: Option<FretboardSpec>,

    /// Articulation-selector source — enables the **latched-CC selector**:
    /// a single continuous controller whose latched VALUE selects the
    /// articulation for all subsequent notes (like a keyswitch, but a CC).
    ///
    /// `"uacc"` is currently the only source and configures the selector
    /// with the UACC defaults (Spitfire's Universal Articulation Controller
    /// Channel convention): CC32 + the published standard code table
    /// ([`UACC_STANDARD_TABLE`]). Empty = no selector (default; existing
    /// packs are untouched).
    ///
    /// Per-articulation codes come from the articulation's explicit
    /// `uacc <code>` field, falling back to the standard table matched by
    /// id/label/aliases — so `selector uacc` alone gets the published
    /// mapping for conventionally-named articulations.
    // r[impl signal.sampling.articulation.select]
    #[facet(default)]
    pub selector: String,
    /// CC number carrying the latched-CC selector value. 0 = the source's
    /// default (UACC: CC32).
    #[facet(default)]
    pub selector_cc: u8,

    /// Live auto-divisi legato interval gate (semitones): in strict live
    /// mode a note continues an existing mono line as a legato transition
    /// only if it is within this interval of the line's sounding (or
    /// just-released) note — anything wider is a fresh attack on its own
    /// line. Small by default (2 = major 2nd) because low-latency sampled
    /// transitions only sound right on small intervals; the document path
    /// has no such gate. See `docs/plan/document-mode.md`, "Auto-divisi".
    #[facet(default = 2)]
    pub live_legato_interval_max: u8,

    /// Live auto-divisi chord window (ms): notes arriving within this of
    /// each other are a chord — they fan out to separate lines as fresh
    /// sustain attacks, never legato.
    #[facet(default = 30.0f32)]
    pub live_chord_window_ms: f32,

    /// Explicit zone map — sample-per-(key range × velocity range × RR slot).
    ///
    /// When non-empty, the engine plays in **zone mode**: every note-on looks
    /// up matching zones by `key_min..=key_max` and `vel_min..=vel_max`,
    /// RR-cycles within the matching set, and uses each zone's `root_key`,
    /// `gain_db`, and `tune_cents` for playback. This bypasses the
    /// section/articulation/dynamic filename-convention path entirely.
    ///
    /// Used by Spectrasonics-style libraries (Omnisphere, Trilian) where the
    /// keymap is encoded in patch metadata rather than filenames.
    #[facet(default)]
    pub zones: Vec<ZoneSpec>,

    /// Wavetables exposed by this library, for the synth side of Signal.
    ///
    /// Sampler engine ignores these; future synth/oscillator engine consumes
    /// them. Stored alongside zones in the same `LibrarySpec` so a single
    /// `.styx` can describe a hybrid library (e.g. Omnisphere-style sampled
    /// soundsources + wavetable bank).
    #[facet(default)]
    pub wavetables: Vec<WavetableSpec>,

    /// Groove loops with tempo + slice metadata (Stylus RMX-style).
    ///
    /// Each `GrooveSpec` is a single audio file (a loop) at a fixed BPM,
    /// with a list of slice positions in original-tempo sample frames.
    /// At runtime the engine time-stretches the loop to host tempo and
    /// MIDI keys can either play the whole loop or trigger individual
    /// slices.
    ///
    /// Sampler engine ignores these; future `GrooveBlock` runtime
    /// consumes them.
    #[facet(default)]
    pub grooves: Vec<GrooveSpec>,

    /// Free-form structured tags using the project-wide `StructuredTag`
    /// schema. Stored as a flat `Vec` (instead of the Map-shaped
    /// `signal_proto::tagging::TagSet`) so it round-trips cleanly through
    /// facet-styx; call [`LibrarySpec::tag_set`] to materialize a `TagSet`
    /// for the collection browser.
    #[facet(default)]
    pub tags: Vec<StructuredTag>,
    /// Primary instrument family (`"violin"`, `"drums"`, `"rhodes"`,
    /// `"synth-bass"`, …). Empty when not classified.
    #[facet(default)]
    pub instrument: String,
    /// High-level content category (`"orchestral"`, `"drum-kit"`,
    /// `"groove"`, `"electric-piano"`, `"synth"`, …). Empty when not
    /// classified.
    #[facet(default)]
    pub category: String,
    /// Stylistic descriptors / sub-genres (Stylus suite name,
    /// articulation list, drum sub-style, etc.).
    #[facet(default)]
    pub style: Vec<String>,
}

impl LibrarySpec {
    /// Load a spec from a `.styx` or `.toml` file (format detected by extension).
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed as a valid spec.
    pub fn from_file(path: &Path) -> Result<Self, SamplerError> {
        let text = std::fs::read_to_string(path).map_err(SamplerError::Io)?;
        let mut spec = match path.extension().and_then(|e| e.to_str()) {
            Some("toml") => Self::from_toml(&text)?,
            _ => Self::from_styx(&text)?,
        };
        if let Some(dir) = path.parent() {
            spec.fill_loops_from_flac_stinfo(dir);
        }
        Ok(spec)
    }

    /// Omnisphere soundsource FLACs carry their sustain loop in a `STINFO`
    /// Vorbis comment (`enabled loop_start loop_end xfade`), but the extraction
    /// that wrote `library.styx` dropped it — so a sustained pad plays as a
    /// one-shot and decays like a piano. For any zone that declares no loop,
    /// read the embedded loop back from the FLAC (metadata only) so the engine
    /// sustains it. Gated to Spectrasonics/Omnisphere libraries to avoid the
    /// per-zone I/O on everything else (Keyscape packs carry their own loops).
    fn fill_loops_from_flac_stinfo(&mut self, dir: &Path) {
        let vendor = self.vendor.to_ascii_lowercase();
        if !(vendor.contains("omnisphere") || vendor.contains("spectrasonics")) {
            return;
        }
        for z in &mut self.zones {
            if z.loop_end > z.loop_start {
                continue; // already has an explicit loop
            }
            if !z.file.to_ascii_lowercase().ends_with(".flac") {
                continue;
            }
            if let Some((ls, le)) = read_flac_stinfo_loop(&dir.join(&z.file)) {
                z.loop_start = ls;
                z.loop_end = le;
            }
        }
    }

    /// Parse from styx format.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is not a valid spec.
    pub fn from_styx(s: &str) -> Result<Self, SamplerError> {
        facet_styx::from_str(s).map_err(|e| SamplerError::SpecParse(e.to_string()))
    }

    /// Parse from TOML format.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is not a valid spec.
    pub fn from_toml(s: &str) -> Result<Self, SamplerError> {
        facet_toml::from_str(s).map_err(|e| SamplerError::SpecParse(e.to_string()))
    }

    /// # Errors
    ///
    /// Returns an error if the SFZ input is invalid.
    pub fn from_sfz(s: &str) -> Result<Self, SamplerError> {
        let zones = parse_sfz_zones(s)?;
        Ok(Self {
            name: "Imported SFZ".to_string(),
            version: String::new(),
            vendor: String::new(),
            sections: Vec::new(),
            mics: Vec::new(),
            dynamics: DynamicsSpec::default(),
            articulations: Vec::new(),
            legato_engine: None,
            performance: PerformanceSpec::default(),
            short_note_timing: None,
            keyswitch: None,
            fretboard: None,
            selector: String::new(),
            selector_cc: 0,
            live_legato_interval_max: 2,
            live_chord_window_ms: 30.0,
            zones,
            wavetables: Vec::new(),
            grooves: Vec::new(),
            tags: Vec::new(),
            instrument: String::new(),
            category: String::new(),
            style: Vec::new(),
        })
    }

    /// Look up an articulation by its `id` field.
    #[must_use]
    pub fn articulation(&self, id: &str) -> Option<&ArticulationSpec> {
        self.articulations.iter().find(|a| a.id == id)
    }

    /// The Con Sordino counterpart of `artic_id` when toggling sordino
    /// `active`: the articulation's explicit `sordino_pair` when authored
    /// (`""` = none), else the CSS `Sord` id-prefix convention — in both
    /// cases only if the counterpart exists in the spec.
    #[must_use]
    pub fn sordino_counterpart(&self, artic_id: &str, active: bool) -> Option<String> {
        let a = self.articulation(artic_id);
        if let Some(pair) = a.and_then(|a| a.sordino_pair.clone()) {
            if pair.is_empty() {
                return None;
            }
            let target = self.articulation(&pair)?;
            if target.is_sordino() == active {
                return Some(pair);
            }
            return None;
        }
        if active {
            if !artic_id.starts_with("Sord") {
                let sord_id = format!("Sord{artic_id}");
                if self.articulation(&sord_id).is_some() {
                    return Some(sord_id);
                }
            }
        } else if let Some(base) = artic_id.strip_prefix("Sord") {
            if self.articulation(base).is_some() {
                return Some(base.to_string());
            }
        }
        None
    }

    /// The CC2 vibrato-crossfade counterpart of `artic_id`: the explicit
    /// `vibrato_pair` when authored (`""` = none), else the opposite
    /// vibrato side within the same kind + sordino family. `None` when the
    /// library declares no vibrato controller.
    #[must_use]
    pub fn vibrato_counterpart(&self, artic_id: &str) -> Option<String> {
        self.dynamics.vibrato_controller.as_deref()?;
        let a = self.articulation(artic_id);
        if let Some(pair) = a.and_then(|a| a.vibrato_pair.clone()) {
            return (!pair.is_empty() && self.articulation(&pair).is_some()).then_some(pair);
        }
        // Inferred: same kind (Sustain↔Sustain, Legato↔Legato), same sordino
        // family, opposite vibrato side.
        let a = a?;
        let (want_kind, is_sord, is_vib) = (a.kind.clone(), a.is_sordino(), a.is_vibrato());
        self.articulations
            .iter()
            .filter(|c| c.id != artic_id)
            .filter(|c| c.kind == want_kind)
            .filter(|c| matches!(c.kind, ArticulationKind::Sustain | ArticulationKind::Legato))
            .find(|c| c.is_sordino() == is_sord && c.is_vibrato() != is_vib)
            .map(|c| c.id.clone())
    }

    /// The legato-engine configuration, falling back to an all-defaults
    /// [`LegatoEngineSpec`] when the library declares none — so engine code
    /// can read timing/crossfade policy unconditionally (the defaults are
    /// the historical hardcoded values).
    #[must_use]
    pub fn legato_cfg(&self) -> &LegatoEngineSpec {
        static DEFAULT: std::sync::OnceLock<LegatoEngineSpec> = std::sync::OnceLock::new();
        self.legato_engine
            .as_ref()
            .unwrap_or_else(|| DEFAULT.get_or_init(LegatoEngineSpec::default))
    }

    /// Look up a section by its `id` field.
    #[must_use]
    pub fn section(&self, id: &str) -> Option<&SectionSpec> {
        self.sections.iter().find(|s| s.id == id)
    }

    /// Look up a mic by its `id` field.
    #[must_use]
    pub fn mic(&self, id: &str) -> Option<&MicSpec> {
        self.mics.iter().find(|m| m.id == id)
    }

    /// Median heard-arrival (ms) across the legato transition zones that
    /// match a `from → to` move — the document scheduler prefires the
    /// transition by exactly this so the pitch change lands on the
    /// destination tick. Per zone the MEASURED `arrival_ms` (destination-
    /// pitch settle, measured from the sample audio) wins; `lead_in_ms`
    /// (library metadata) is only a fallback for unmeasured zones.
    ///
    /// Matching mirrors the engine's transition selection: direction from
    /// `sign(to - from)`, named note `min(from, to)`, interval
    /// `|to - from|` clamped to an octave (CSS samples nothing wider; the
    /// engine octave-clamps and pitch-shifts so the destination stays
    /// exact). Zones are matched at the nearest recorded root (whole-tone
    /// grid). For a same-pitch re-bow: the median measured onset of the
    /// re-trigger (`Legzero`) zones at the nearest root, or `Some(0)` when
    /// they are unmeasured (the historical claim — no lead-in). `None` when
    /// the library has no measured transition zones at all (caller falls
    /// back to the velocity curve).
    #[must_use]
    pub fn legato_lead_ms(&self, from: u8, to: u8) -> Option<u32> {
        if from == to {
            if !self.has_measured_legato() {
                // Legacy libraries keep the velocity-curve behaviour.
                return None;
            }
            // Same-pitch re-bow: the re-trigger (Legzero) samples' measured
            // perceptual onset — the bow re-attack is NOT instantaneous.
            // Unmeasured re-trigger zones keep the historical zero-lead
            // claim.
            let retrig = |id: &str| {
                self.articulations.iter().any(|a| {
                    a.kind == ArticulationKind::Legato
                        && a.resolve_legato_role() == LegatoRole::Retrigger
                        && a.id.eq_ignore_ascii_case(id)
                })
            };
            let candidates = || {
                self.zones
                    .iter()
                    .filter(|z| z.arrival_ms > 0.0 && retrig(&z.articulation))
            };
            let Some(best_dist) = candidates().map(|z| z.root_key.abs_diff(from)).min() else {
                return Some(0);
            };
            // MAX over the nearest-root group (all RRs / mics / dynamic
            // layers): the re-bow cannot be skipped into (its front IS the
            // re-attack), so the engine can only HOLD a voice back — the
            // schedule lead must be an upper bound over every zone the
            // dispatch could pick, or a zone with a later onset lands late.
            return candidates()
                .filter(|z| z.root_key.abs_diff(from) == best_dist)
                .map(|z| z.arrival_ms)
                .max_by(f32::total_cmp)
                .map(|ms| ms.ceil() as u32);
        }
        let direction = if to > from { "up" } else { "down" };
        let named = from.min(to);
        let interval = u32::from(from.abs_diff(to)).min(12);
        let is_legato_artic = |id: &str| {
            self.articulations
                .iter()
                .any(|a| a.kind == ArticulationKind::Legato && a.id.eq_ignore_ascii_case(id))
        };
        let candidates = || {
            self.zones.iter().filter(|z| {
                z.interval == interval
                    && z.direction.eq_ignore_ascii_case(direction)
                    && z.transition_arrival_ms() > 0.0
                    && is_legato_artic(&z.articulation)
            })
        };
        // Nearest recorded root first, then the median arrival of that group.
        let best_dist = candidates().map(|z| z.root_key.abs_diff(named)).min()?;
        let mut leads: Vec<f32> = candidates()
            .filter(|z| z.root_key.abs_diff(named) == best_dist)
            .map(ZoneSpec::transition_arrival_ms)
            .collect();
        leads.sort_by(f32::total_cmp);
        Some(leads[leads.len() / 2].round() as u32)
    }

    /// The largest measured heard-arrival (ms) over the attack zones any of
    /// `artic_ids` could fire at `pitch` — the document scheduler pre-rolls
    /// a fresh trigger by exactly this upper bound, and each spawned voice is
    /// held back by `lead − its own zone's arrival` so the heard arrival
    /// lands ON the tick regardless of which round-robin / mic / dynamic
    /// layer fires. `None` when no matching zone carries a measurement
    /// (caller falls back to `pre_delay_ms` for shorts / trigger-time for
    /// sustains — the historical behaviour).
    #[must_use]
    pub fn max_attack_arrival_ms(&self, artic_ids: &[&str], pitch: u8) -> Option<f32> {
        let tol = self.performance.zone_pitch_tolerance;
        self.zones
            .iter()
            .filter(|z| {
                z.arrival_ms > 0.0
                    && (z.trigger_mode.is_empty() || z.trigger_mode.eq_ignore_ascii_case("attack"))
                    && artic_ids
                        .iter()
                        .any(|id| z.articulation.eq_ignore_ascii_case(id))
                    && ((pitch >= z.key_min && pitch <= z.key_max)
                        || (z.root_key as i32 - pitch as i32).unsigned_abs() as u8 <= tol)
            })
            .map(|z| z.arrival_ms)
            .max_by(f32::total_cmp)
    }

    /// Whether any zone carries a measured legato transition (interval +
    /// lead-in written by the sample-collector generator). Gates the
    /// measured-lead prefire alignment; without it the engine and scheduler
    /// keep the legacy velocity-curve behaviour.
    #[must_use]
    pub fn has_measured_legato(&self) -> bool {
        self.zones
            .iter()
            .any(|z| z.interval > 0 && z.transition_arrival_ms() > 0.0)
    }

    /// The resolved latched-CC articulation selector, when the pack enables
    /// one (`selector uacc`). `None` = no selector configured — the engine
    /// behaves exactly as before (defaults leave existing packs untouched).
    ///
    /// Resolution per articulation, first match wins per code:
    /// 1. explicit `uacc <code>` field (any articulation kind — the author
    ///    knows best),
    /// 2. the published standard table by id / label / aliases
    ///    ([`standard_uacc_code`]) — skipped for `Legato` transition and
    ///    `Release` articulations, which are engine-internal sample sets,
    ///    not selectable playing styles.
    // r[impl signal.sampling.articulation.select]
    #[must_use]
    pub fn latched_cc_selector(&self) -> Option<LatchedCcSelector> {
        if !self.selector.eq_ignore_ascii_case("uacc") {
            return None;
        }
        let cc = if self.selector_cc == 0 {
            UACC_DEFAULT_CC
        } else {
            self.selector_cc
        };
        let mut map: Vec<(u8, String)> = Vec::new();
        let mut push = |code: u8, id: &str| {
            if code > 0 && !map.iter().any(|(c, _)| *c == code) {
                map.push((code, id.to_string()));
            }
        };
        // Explicit codes first — they always beat table inference.
        for a in &self.articulations {
            if a.uacc > 0 {
                push(a.uacc, &a.id);
            }
        }
        // Standard-table defaults for the rest.
        for a in &self.articulations {
            if a.uacc > 0 || matches!(a.kind, ArticulationKind::Legato | ArticulationKind::Release)
            {
                continue;
            }
            let code = standard_uacc_code(&a.id)
                .or_else(|| standard_uacc_code(&a.label))
                .or_else(|| a.aliases.iter().find_map(|al| standard_uacc_code(al)));
            if let Some(code) = code {
                push(code, &a.id);
            }
        }
        map.sort_by_key(|(c, _)| *c);
        Some(LatchedCcSelector { cc, map })
    }

    /// Materialize a [`TagSet`] from the flat `tags` vector.
    ///
    /// The collection browser consumes `TagSet`; the spec stores tags as a
    /// `Vec` purely for facet-styx round-trip ergonomics.
    #[must_use]
    pub fn tag_set(&self) -> TagSet {
        let mut set = TagSet::new();
        for t in &self.tags {
            set.insert(t.clone());
        }
        set
    }
}

// ── Performance model ─────────────────────────────────────────────────────────

/// Library-wide playback-policy numbers — the POLICY the engine reads from
/// data (the engine keeps only the MECHANISM: zone resolution, scheduling,
/// crossfading).
///
/// Every default equals the value the engine hardcoded before this block
/// existed, so a spec that omits `performance` plays identically.
///
/// The non-zero defaults were decoded from Cinematic Studio Strings (KSP
/// persistent values / `GroupList` envelopes) but apply harmlessly to any
/// zoned library; a library that needs different numbers writes them here.
// r[impl signal.soundsource.declarative]
#[derive(Debug, Clone, Facet)]
pub struct PerformanceSpec {
    /// Held-sustain note-off overlap fade (ms) for zoned sustains (CSS
    /// `$tukcw` = 400): on key-up the looping sustain fades over this window.
    #[facet(default = 400)]
    pub sustain_noteoff_ms: u32,
    /// A script fade over the key-up release (ms; 0 = none): a held body's
    /// release envelope is multiplied by a linear fade to silence over this
    /// time, and the voice ends there — Kontakt's `fade_out` at the note-off
    /// on top of the group's amp release (CSB `$qxeb4` 200, CSW `$tukcw` 100).
    #[facet(default)]
    pub sustain_noteoff_cut_ms: f32,
    /// Output makeup (dB) applied to looping sustain-layer voices — the flat
    /// level offset between a looped-plateau playback and the vendor
    /// instrument's rendered level (CSS: +6 dB, see the A/B calibration).
    /// Default 0: a library plays at the level it was mastered at unless it
    /// says otherwise.
    ///
    /// This used to default to 6.0 — CSS's looped-plateau→Kontakt-render
    /// offset — which meant every library that does not author a
    /// `performance` block inherited one library's A/B calibration. Generated
    /// packs (the NI pianos, the Keyscape extractions) write no such block, so
    /// they were all running +6 dB into the pack-level normalisation that is
    /// supposed to be the single place level is decided. Every spec that
    /// actually cares sets its own value.
    #[facet(default)]
    pub sustain_makeup_db: f32,
    /// Level the host instrument adds to EVERY voice (dB). Kontakt plays a
    /// CSS zone at +2.55 dB over the raw file when every group curve reads
    /// 1.0 (null-tested). Unlike `sustain_makeup_db` it is not a loop or
    /// legato compensation and applies to shorts and releases too.
    #[facet(default)]
    pub instrument_gain_db: f32,
    /// Balance the host instrument applies to EVERY voice, −1..1 (stereo:
    /// the far channel × (1 − |pan|), as [`Voice::with_pan`]). CSB Solo
    /// Trombone's buses carry a balance stage at 0.22 (Kontakt effect kind
    /// 9): its left channel plays 2.2 dB under its right.
    ///
    /// [`Voice::with_pan`]: crate::engine::voice::Voice::with_pan
    #[facet(default)]
    pub instrument_pan: f32,
    /// A script's ensemble of players (see [`PlayersSpec`]); `None` = one
    /// player, nothing of it.
    #[facet(default)]
    pub players: Option<PlayersSpec>,
    /// Controllers that read a saved value until MIDI first sets them
    /// (Kontakt keeps a start value with a controller's modulation:
    /// MOJO 2's transitions read CC113 at 100 until it moves).
    #[facet(default)]
    pub start_ccs: Vec<FixedCcSpec>,
    /// Controllers a script copies to others on every event (MOJO 2:
    /// `set_controller(113, %CC[1])`, its "CC1 to transition volume").
    #[facet(default)]
    pub mirror_ccs: Vec<CcMirrorSpec>,
    /// How a note between sampled keys is pitched. `true`: RESAMPLE (rate
    /// change, the note runs shorter/longer) — what Kontakt does, null-tested
    /// on CSS (+100 c at key 72 from the B3 zone, −52.7 dB). `false`/unset:
    /// the Cinematic-family default, a time-preserving pitch shift (a delay
    /// line, which also adds an echo-like double) — kept for libraries not
    /// yet null-tested.
    #[facet(default)]
    pub resample_transpose: bool,
    /// Curvature `k` of the sustain body's note-off release (0 = linear;
    /// see `Voice::release_k`). CSS in Kontakt: 2.6 over `release_ms` 580.
    #[facet(default)]
    pub release_curve_k: f32,
    /// Pan a mono sample with the balance law stereo samples use (unity in
    /// both channels at centre) instead of equal power (−3 dB each at
    /// centre). Kontakt plays a centred mono zone at unity: a mono DI
    /// guitar nulls 2.97 dB quieter without it. Default off.
    #[facet(default)]
    pub mono_balance_pan: bool,
    /// Strikes of one key kept ringing when it is struck again (older ones
    /// fade over 100 ms). `None` = 2 (a pedalled piano's guard); `Some(0)` =
    /// every strike rings on — Kontakt's CSS (null test: shorts re-struck
    /// every 300 ms all ring).
    pub max_repeat_strikes: Option<u32>,
    /// A key no zone of the articulation covers plays nothing (as Kontakt),
    /// instead of the nearest recorded pitch.
    #[facet(default)]
    pub strict_key_ranges: bool,
    /// Per-note round robins (`ArticulationSpec::rr_per_note`) count per
    /// KEY instead of per zone root. CSS keys them by the KSP's note-class
    /// table `%dtnvu`: the violin patches pair each even key from 60 up with
    /// the key below (= the zone root), the other sections keep every key
    /// apart (Cellos Sfz C2/C#2: per key).
    #[facet(default)]
    pub rr_per_key: bool,
    /// Trills from key pairs (CSS script slot 0): with a trill articulation
    /// selected, two keys a semitone / tone apart struck within
    /// `window_ms` play ONE note — `half` / `whole` — on the lower key,
    /// `delay_ms` after the first key. A lone key plays nothing.
    pub trill_pair: Option<TrillPairSpec>,
    /// Release-sample level by held time (KSP `%ru5pa`): gain =
    /// `release_held_db × table[clamp(held_ms/10 − 1, 0, len−1)] / 100`. Empty
    /// = the legacy −6 dB base + linear held curve.
    #[facet(default)]
    pub release_held_table: Vec<u8>,
    #[facet(default)]
    pub release_held_db: f32,
    /// Delay from note-off to the release sample (ms). CSS: 2 (KSP `wait(2000)`).
    #[facet(default)]
    pub release_delay_ms: f32,
    /// The sustain pedal (CC64) does not hold key-ups: the instrument's
    /// script reads the pedal itself (MOJO 2's octave legato), and its
    /// release callbacks run at the key-ups (KONTRA trace).
    #[facet(default)]
    pub pedal_ignored: bool,
    /// The random round robin avoids the last pick of ANY articulation and
    /// key (one script global, MOJO 2's `UpdateRoundRobinIndex`), not the
    /// key's own history.
    #[facet(default)]
    pub rr_random_global: bool,
    /// The first note of another articulation fades every sounding voice
    /// over this many ms (MOJO 2's articulation cutoff, `DoArticulationCutoff`:
    /// `fade_out($ALL_EVENTS, 100 ms)`). 0 = none.
    #[facet(default)]
    pub artic_change_fade_ms: f32,
    /// A script's time-stretch parameter per whole speed percent (index =
    /// percent), as the script computes it (MOJO 2's integer
    /// `_Speed_to_ep`): the engine plays it through Kontakt's speed law,
    /// `(7^(u³+1) − 1)/6` with `u = 2·ep/1e6 − 1` (`UnityToNat_Speed`
    /// 0x412bf8, in f32). Empty = percent / 100.
    #[facet(default)]
    pub stretch_speed_ep: Vec<i32>,
    /// How voices read between sample frames: `"kontakt_normal"` plays
    /// through Kontakt 8's Normal-quality resampler (a block-formed f32
    /// step, its cache-relative f32 cursor, the 4-point Lagrange; see
    /// `engine::resample`), for a library whose groups ship Normal quality
    /// (MOJO 2). Empty / `"linear"`: the engine's linear read.
    #[facet(default)]
    pub interpolation: String,
    /// Global master tune in cents, applied on top of the per-note transpose.
    /// CSS ships `tune=1.00521` ≈ +9.0 cents on every playable group; other
    /// libraries stay at 0.
    #[facet(default = 0.0f32)]
    pub master_tune_cents: f32,
    /// Seamless loop-crossfade length (ms) for held/looped bodies.
    #[facet(default = 150)]
    pub loop_xfade_ms: u32,
    /// Max semitones to pitch-shift from the nearest recorded zone when no
    /// zone spans a note (CSS whole-tone grid → 2).
    #[facet(default = 2)]
    pub zone_pitch_tolerance: u8,
    /// Linear gain for recorded release-tail voices (the release `ENV_FLEX`
    /// does the shaping; CSS release groups ship 0 dB static → 1.0).
    #[facet(default = 1.0f32)]
    pub release_gain: f32,
    /// Default amp attack (ms) applied to sustain-layer voices at load.
    /// `None` keeps the engine's default (no attack bloom). CSS "Arco
    /// attack" ships `$mmirg = 30/127` ≈ 198 ms under Kontakt's cubic law.
    pub attack_ms: Option<u32>,
    /// Default note-off release (ms) applied at load. `None` keeps the
    /// engine default. CSS: 400 (`$tukcw`).
    pub release_ms: Option<u32>,
    /// Grid placement policy for FRESH sustain attacks (phrase starts) in
    /// document scheduling:
    ///
    /// * `"start_at_tick"` — the sample STARTS on the grid tick and speaks
    ///   naturally after it. Nothing sounds before the click the note starts
    ///   on; the perceptual onset lands `arrival_ms` late by the recording's
    ///   own nature (exactly what the vendor instrument does live).
    /// * `"arrive_at_tick"` (default / empty) — the trigger pre-rolls by the
    ///   measured perceptual-onset bound so the note SPEAKS on the tick
    ///   (audio from the sample's own bloom is audible before the click).
    ///
    /// Applies to fresh sustain-family attacks only. Legato transitions and
    /// re-bows always arrive-at-tick (their pre-click content is the
    /// PREVIOUS note continuing — correct musical behaviour), and shorts
    /// always arrive-at-tick (their recorded pre-roll is the attack noise
    /// before the rhythmic peak).
    #[facet(default)]
    pub attack_placement: String,
    /// Note-off, release and declick timings every voice follows. See
    /// [`VoiceTimingSpec`].
    #[facet(default)]
    pub voice_timing: VoiceTimingSpec,
    /// The Con Sordino lowpass applied when sordino is on and the library
    /// has no muted recordings to switch to.
    #[facet(default)]
    pub sordino_filter: SordinoFilterSpec,
}

/// Note-off, release and declick timings (the engine's mechanism reads
/// every number from here). The defaults are the values the engine used to
/// hardcode; a library sets its own in `performance { voice_timing { … } }`.
// r[impl signal.soundsource.declarative]
#[derive(Debug, Clone, Facet)]
pub struct VoiceTimingSpec {
    /// Note-off fade (ms) of a sustain voice when `performance.release_ms`
    /// is unset: the damper-down time, a short fade that reads as a cutoff.
    #[facet(default = 60)]
    pub noteoff_fade_ms: u32,
    /// The note-off fade's multiplier at the top of the half-pedal range.
    #[facet(default = 4.0f32)]
    pub half_pedal_release_mult: f32,
    /// Note-off fade (ms) when a release articulation supplies the natural
    /// tail (the body only needs a declick).
    #[facet(default = 80)]
    pub key_up_declick_ms: u32,
    /// Longest a release-tail voice may ring (ms) before it is retired.
    #[facet(default = 2000)]
    pub release_max_ms: u32,
    /// Fade-in (ms) of a looped sustain that starts mid-sample at full level.
    #[facet(default = 12)]
    pub sustain_declick_ms: u32,
    /// Fade-in (ms) of a fresh sustain entered mid-sample (a start offset).
    #[facet(default = 20)]
    pub mid_sample_declick_ms: u32,
    /// Fade-in (ms) of a voice that starts mid-sample (transitions, release
    /// tails, sample-start offsets).
    #[facet(default = 6)]
    pub onset_declick_ms: u32,
    /// Body peak (linear) below which a note-off fires no release tail.
    #[facet(default = 0.002f32)]
    pub release_body_floor: f32,
    /// Lowest note-off velocity that plays a release sample.
    #[facet(default = 1)]
    pub release_min_velocity: u8,
    /// Fade (ms) of the previous strike when the same key is struck again.
    #[facet(default = 90)]
    pub retrigger_fade_ms: u32,
    /// Smoothing (ms) of a CC1/CC11 level change (Kontakt's modulator lag).
    #[facet(default = 120)]
    pub cc1_lag_ms: u32,
    /// Smoothing (ms) of a CC2 vibrato crossfade.
    #[facet(default = 1000)]
    pub cc2_lag_ms: u32,
}

impl Default for VoiceTimingSpec {
    fn default() -> Self {
        Self {
            noteoff_fade_ms: 60,
            half_pedal_release_mult: 4.0,
            key_up_declick_ms: 80,
            release_max_ms: 2000,
            sustain_declick_ms: 12,
            mid_sample_declick_ms: 20,
            onset_declick_ms: 6,
            release_body_floor: 0.002,
            release_min_velocity: 1,
            retrigger_fade_ms: 90,
            cc1_lag_ms: 120,
            cc2_lag_ms: 1000,
        }
    }
}

/// The Con Sordino lowpass (a placeholder until a measured muted response
/// replaces it).
#[derive(Debug, Clone, Facet)]
pub struct SordinoFilterSpec {
    #[facet(default = 2500.0f32)]
    pub cutoff_hz: f32,
    #[facet(default = 0.9f32)]
    pub q: f32,
}

impl Default for SordinoFilterSpec {
    fn default() -> Self {
        Self {
            cutoff_hz: 2500.0,
            q: 0.9,
        }
    }
}

impl Default for PerformanceSpec {
    fn default() -> Self {
        Self {
            sustain_noteoff_ms: 400,
            sustain_noteoff_cut_ms: 0.0,
            sustain_makeup_db: 0.0,
            instrument_gain_db: 0.0,
            instrument_pan: 0.0,
            players: None,
            start_ccs: Vec::new(),
            mirror_ccs: Vec::new(),
            resample_transpose: false,
            release_curve_k: 0.0,
            mono_balance_pan: false,
            max_repeat_strikes: None,
            strict_key_ranges: false,
            rr_per_key: false,
            trill_pair: None,
            release_held_table: Vec::new(),
            release_held_db: 0.0,
            release_delay_ms: 0.0,
            pedal_ignored: false,
            rr_random_global: false,
            artic_change_fade_ms: 0.0,
            stretch_speed_ep: Vec::new(),
            interpolation: String::new(),
            master_tune_cents: 0.0,
            loop_xfade_ms: 150,
            zone_pitch_tolerance: 2,
            release_gain: 1.0,
            attack_ms: None,
            attack_placement: String::new(),
            release_ms: None,
            voice_timing: VoiceTimingSpec::default(),
            sordino_filter: SordinoFilterSpec::default(),
        }
    }
}

/// One decoded amp-envelope segment `(time_ms, level, curve)` — the literal
/// `ENV_FLEX` representation (segment 0 is the attack). See
/// [`ArticulationSpec::amp_env`].
#[derive(Debug, Clone, Copy, PartialEq, Facet)]
pub struct EnvSegmentSpec {
    /// Segment duration in ms.
    pub time_ms: f32,
    /// Target level at the end of the segment (0..=1).
    pub level: f32,
    /// Curve shape parameter (0.5 = linear-ish; matches the decoded tables).
    pub curve: f32,
}

/// A group's volume `ENV_AHDSR` as Kontakt runs it, at its control rate
/// (`engine::kontakt_ahdsr`): times in ms, the sustain level 0..=1, the
/// attack curve −1..=1, and the AHD-only switch (the key-up ignored, the
/// decay falling to silence).
#[derive(Debug, Clone, Copy, PartialEq, Facet)]
pub struct AmpAhdsrSpec {
    pub attack_ms: f32,
    #[facet(default)]
    pub hold_ms: f32,
    #[facet(default)]
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
    #[facet(default)]
    pub curve: f32,
    #[facet(default)]
    pub ahd_only: bool,
}

/// A CSS legato transition as Kontakt plays it (KSP §1.2, every number
/// null-tested). At the transition (after the overlap delay):
/// - the old body fades out linearly over `old_body_fade_ms`;
/// - the previous transition voice fades out over 250 → 400 ms by IOI
///   (low latency; expressive flat 400);
/// - the old note's in-phrase release plays at `in_phrase_release_db`;
/// - the new body enters silent, waits `body_wait_ms`, then fades in
///   linearly over `body_fade_ms`;
/// - the transition sample of the LOWER note (interval-coded) enters at the
///   mode/zone offset + the overlap delay, under the envelope `lt_env`, its
///   first node at `(CC66·CC67)/127²` reached in `lt_seg0_ms`.
/// Intervals above an octave play no transition sample: the body enters at
/// `big_leap_body_offset_ms`, fading in over `big_leap_new_fade_ms` while
/// the old body fades over `big_leap_old_fade_ms`.
#[derive(Debug, Clone, Facet)]
pub struct KontaktTransitionSpec {
    pub old_body_fade_ms: f32,
    pub body_wait_ms: f32,
    pub body_fade_ms: f32,
    pub big_leap_body_offset_ms: f32,
    pub big_leap_old_fade_ms: f32,
    pub big_leap_new_fade_ms: f32,
    pub in_phrase_release_db: f32,
    /// The in-phrase release starts this long after the transition (on top
    /// of `release_delay_ms`); 0 = with it. Null-tested on CSB Solo Trumpet:
    /// Kontakt's starts 5.7 ms after ours did.
    #[facet(default)]
    pub in_phrase_release_delay_ms: f32,
    /// First envelope segment's length: low latency (CC68 = 108), expressive (127).
    pub lt_seg0_ms_low_latency: f32,
    pub lt_seg0_ms_expressive: f32,
    /// The transition envelope after its first node (FlexEnv segments).
    pub lt_env: Vec<EnvSegmentSpec>,
    /// The level of `lt_env`'s first segment is `1 − depth·(1 − CC66/127)`
    /// (measured: 0.90 at CC66 = 100, 0.83 at 82, either mode). 0 = use the
    /// spec level as is.
    #[facet(default)]
    pub lt_node1_cc66_depth: f32,
    /// Phrase-start dip on the transition voice: within `phrase_dip_window_ms`
    /// of the phrase anchor it starts `phrase_dip_db` down and recovers over
    /// `phrase_dip_window_ms`.
    pub phrase_dip_db: f32,
    pub phrase_dip_window_ms: f32,
    /// The dip's measured recovery, as a gain envelope starting at
    /// `phrase_dip_db` (Kontakt's ramp is slower than the script's nominal
    /// 0.12 dB / 10 ms: −1.4 dB at 250 ms, 0 dB near 1 s). Empty = a linear
    /// ramp back over `phrase_dip_window_ms`.
    #[facet(default)]
    pub phrase_dip_curve: Vec<EnvSegmentSpec>,
    /// The transition envelope's first node level, `(CC66 · CC67) / 127²`,
    /// each CC by IOI per legato mode (CSS KSP 7186-7425). None = 1.0.
    #[facet(default)]
    pub lt_node: Option<LtNodeSpec>,
    /// How the previous transition voice retires on the next transition:
    /// ms by IOI per legato mode, or by attack class. None = at once.
    #[facet(default)]
    pub lt_retire: Option<LtRetireSpec>,
    /// The destination body's handoff per dynamic layer (CSB `%k1aqh`):
    /// each layer waits and fades in on its own clock, by velocity zone.
    /// Empty = `body_wait_ms` / `body_fade_ms` for every layer.
    #[facet(default)]
    pub body_handoff: Vec<BodyHandoffSpec>,
    /// The same for a re-strike of the sounding key (CSB `%wu5b4`).
    #[facet(default)]
    pub retrigger_body_handoff: Vec<BodyHandoffSpec>,
    /// A transition touching a key at or above `min_key` hands every layer
    /// over with this wait / fade instead (CSB `$vv4qt`).
    #[facet(default)]
    pub high_key_handoff: Option<HighKeyHandoffSpec>,
    /// The old body's fade by velocity zone (1-3); empty = `old_body_fade_ms`.
    #[facet(default)]
    pub old_body_fade_by_zone_ms: Vec<f32>,
    /// The transition voice fades out along this curve (dB, evenly spaced
    /// points) over its layer's body fade, starting when the body starts
    /// fading in, then stops (CSB `%jygjv`). Empty = no such fade.
    #[facet(default)]
    pub lt_fade_curve_db: Vec<f32>,
    /// The transition starts this many dB down by IOI and recovers linearly
    /// over `recover_ms` (CSB `$dwujq`, recovered over `$pgocp`).
    #[facet(default)]
    pub lt_ioi_dip: Option<LtIoiDipSpec>,
}

/// See [`KontaktTransitionSpec::lt_node`].
#[derive(Debug, Clone, Facet)]
pub struct LtNodeSpec {
    pub expressive: LtNodeModeSpec,
    pub low_latency: LtNodeModeSpec,
}

/// The two CCs of one legato mode by IOI; `cc67_loud` from velocity zone 2.
#[derive(Debug, Clone, Facet)]
pub struct LtNodeModeSpec {
    pub cc66: IoiCurveSpec,
    pub cc67: IoiCurveSpec,
    #[facet(default)]
    pub cc67_loud: Option<IoiCurveSpec>,
}

impl LtNodeSpec {
    /// `(CC66, CC67)` for a transition (IOI floored to whole ms, as the KSP).
    #[must_use]
    pub fn ccs(&self, expressive: bool, zone: u8, ioi_ms: f32) -> (f32, f32) {
        let m = if expressive {
            &self.expressive
        } else {
            &self.low_latency
        };
        let ioi = ioi_ms.max(0.0).floor();
        let cc67 = match &m.cc67_loud {
            Some(loud) if zone >= 2 => loud,
            _ => &m.cc67,
        };
        (m.cc66.value_at(ioi), cc67.value_at(ioi))
    }
}

/// How the previous transition voice retires (ms by IOI).
#[derive(Debug, Clone, Facet)]
pub struct LtRetireSpec {
    pub low_latency: IoiCurveSpec,
    pub expressive: IoiCurveSpec,
    /// From velocity zone `loud_from_zone` up, this curve instead (CSB: the
    /// loud attack class).
    #[facet(default)]
    pub loud: Option<IoiCurveSpec>,
    #[facet(default = 2)]
    pub loud_from_zone: u8,
}

/// One dynamic layer's body handoff on a connected note.
#[derive(Debug, Clone, Facet)]
pub struct BodyHandoffSpec {
    /// Only this articulation's layers (CSW: the vibsus and nonvib slots of
    /// one dynamic wait differently); empty = every articulation's.
    #[facet(default)]
    pub articulation: String,
    pub dynamic: String,
    /// Wait before fading in (ms), by velocity zone 1-3.
    pub wait_ms: Vec<f32>,
    /// Fade-in (ms), by velocity zone 1-3.
    pub fade_ms: Vec<f32>,
    /// Beyond an octave: no wait, this fade.
    pub big_fade_ms: f32,
    /// How deep the layer enters the recording (ms) by velocity zone 1-3,
    /// expressive legato (CSW 2-player nonvib "NV Delay", `$wj5ti`/`$bycsx`/
    /// `$req1g`: no handoff wait, the body starts at once this far in).
    /// Empty = from the start.
    #[facet(default)]
    pub offset_ms: Vec<f32>,
    /// The same in low-latency legato (`$wrchu`/`$kb2nd`/`$zhqzl`); empty =
    /// `offset_ms`.
    #[facet(default)]
    pub offset_ms_low_latency: Vec<f32>,
    /// Beyond an octave (`$t5wpo` / `$auf1d`; one value per mode, expressive
    /// first). Empty = none.
    #[facet(default)]
    pub big_offset_ms: Vec<f32>,
}

/// See [`KontaktTransitionSpec::high_key_handoff`].
#[derive(Debug, Clone, Facet)]
pub struct HighKeyHandoffSpec {
    pub min_key: u8,
    pub wait_ms: f32,
    pub fade_ms: f32,
}

/// See [`KontaktTransitionSpec::lt_ioi_dip`].
#[derive(Debug, Clone, Facet)]
pub struct LtIoiDipSpec {
    /// Starting level (dB) by IOI.
    pub db: IoiCurveSpec,
    pub recover_ms: f32,
}

impl BodyHandoffSpec {
    /// (wait, fade) in ms for velocity zone `zone` (1-3), or beyond an octave.
    #[must_use]
    pub fn for_zone(&self, zone: u8, big_leap: bool) -> (f32, f32) {
        if big_leap {
            return (0.0, self.big_fade_ms);
        }
        let i = usize::from(zone.clamp(1, 3) - 1);
        let pick = |v: &[f32]| v.get(i).or_else(|| v.last()).copied().unwrap_or(0.0);
        (pick(&self.wait_ms), pick(&self.fade_ms))
    }

    /// The layer's entry offset (ms) for velocity zone `zone` (1-3) in the
    /// given legato mode, or beyond an octave.
    #[must_use]
    pub fn offset_for(&self, zone: u8, big_leap: bool, expressive: bool) -> f32 {
        if big_leap {
            let i = usize::from(!expressive);
            return self
                .big_offset_ms
                .get(i)
                .or_else(|| self.big_offset_ms.first())
                .copied()
                .unwrap_or(0.0);
        }
        let v = if expressive || self.offset_ms_low_latency.is_empty() {
            &self.offset_ms
        } else {
            &self.offset_ms_low_latency
        };
        let i = usize::from(zone.clamp(1, 3) - 1);
        v.get(i).or_else(|| v.last()).copied().unwrap_or(0.0)
    }
}

/// A CSS MARCATO transition as Kontakt plays it (`$ocjln=6`, KSP 18114-20402,
/// null-tested against Kontakt 8 renders of CSS 1st Violins: −34 to −43 dB
/// on every probed transition). No overlap delay; at the note-on:
/// - "hard" = velocity > 75 and IOI > 50 ms (`$kbqnb`);
/// - the old body fades out linearly over `xfade_ms` (cut at its end),
///   bending 0 → +glide cents toward the new note, and — CC19 leaving 127
///   drops its `CC19=127` step mod — sinking toward `old_sink_level` from
///   `old_sink_start_ms` over `old_sink_ms`;
/// - the previous transition voice retires over `lt_retire_ms` by IOI;
/// - the old note's in-phrase release plays at `in_phrase_release_db`;
/// - the new body (next round-robin) enters `body_offset_ms` deep, muted,
///   bending from −glide back to 0, fading in in two stages (stage 1 at the
///   rate of `xfade·split/node_divisor`, until `xfade·split/100` ticks; stage
///   2 to full over `xfade·(100 − split)/100`), under the body envelopes
///   `body_env_cc19` / `body_env_cc18` whose negative levels stand for the
///   CC value the script sets ÷ 127;
/// - the marc-leg transition sample (the lower note's interval-coded zone of
///   `lt_articulation`, entered at its `marcato_start` + `lt_offset_ms`)
///   plays per marcato dynamic layer at that zone's marcato level × `lt_db`
///   × `lt_group_db` under `lt_env`.
/// Every IOI table is the script's integer lerp, flat outside its ends.
#[derive(Debug, Clone, Facet)]
pub struct MarcatoTransitionSpec {
    /// The connected body waits this long (ms) after the note-on — the
    /// transition sample does not (CSW: `wait($5aowl)` = 40 ms between them,
    /// KSP 12813). 0 = at once.
    #[facet(default)]
    pub body_delay_ms: f32,
    /// The script's crossfade loop ticks this often (ms, `$0cqdq`); a note
    /// that arrives while the previous transition's loop still runs stops
    /// it and waits for its next tick, so its whole transition starts one
    /// tick late (CSS/CSW/CSB `while ($kxgro=1) wait($0cqdq)`, CSB 15338-
    /// 15355). The loop lasts the XTime, or the bend if longer. 0 = no
    /// interlock.
    #[facet(default)]
    pub busy_tick_ms: f32,
    /// The controller the script writes its CC19 value to on a connected
    /// note (`set_controller(19, …)`), so routes that read it follow; 0 =
    /// not written.
    #[facet(default)]
    pub cc19_controller: u8,
    /// A transition is HARD (`$kbqnb`) above this velocity (`$qm4n3`)…
    #[facet(default = 75)]
    pub hard_velocity: u8,
    /// … and IOI in ms (`$hx3nl`: 50, Cellos 100).
    #[facet(default = 50.0f32)]
    pub hard_ioi_ms: f32,
    /// XTime `$a3zg3`, ms.
    pub xfade_ms: f32,
    /// Atk-fade split `$igmiu` %: (soft hard) within an octave; beyond it.
    pub split: [f32; 2],
    pub split_big: f32,
    /// Node-volume divisor `$x444h` by IOI.
    pub node_divisor: Vec<[f32; 2]>,
    /// Body entry `$1fvjk`, ms: (soft hard) within an octave / beyond it.
    pub body_offset_ms: [f32; 2],
    pub body_offset_big_ms: [f32; 2],
    /// A random extra body entry, ms: `random(0, n)` on every connection
    /// (CSSS `$yp2sq + random(0, $a4bsp)`, CSSS 11357 / 12016). 0 = none.
    #[facet(default)]
    pub body_offset_random_ms: f32,
    /// CC19 (`$313em`): soft by IOI; (hard, beyond-octave soft, beyond hard).
    pub cc19_soft: Vec<[f32; 2]>,
    pub cc19_other: [f32; 3],
    /// CC18 (`$eaghu`): soft by IOI for a step (≤ 2 semitones) and a leap;
    /// (hard, beyond-octave soft, beyond hard).
    pub cc18_soft_step: Vec<[f32; 2]>,
    pub cc18_soft_leap: Vec<[f32; 2]>,
    pub cc18_other: [f32; 3],
    /// The body's CC-driven volume envelopes (level < 0 = CC ÷ 127).
    pub body_env_cc19: Vec<EnvSegmentSpec>,
    pub body_env_cc18: Vec<EnvSegmentSpec>,
    /// The outgoing body's sink once CC19 leaves 127.
    pub old_sink_level: f32,
    pub old_sink_start_ms: f32,
    pub old_sink_ms: f32,
    /// Glide depth `$ruv02`, cents by IOI: semitone, tone, wider; over `glide_ms`.
    pub glide_semitone: Vec<[f32; 2]>,
    pub glide_tone: Vec<[f32; 2]>,
    pub glide_wide: Vec<[f32; 2]>,
    pub glide_ms: f32,
    /// The transition-sample articulation and its layers: `(marcato dynamic,
    /// transition dynamic, index into the zone's marcato_gains_db)`.
    pub lt_articulation: String,
    pub lt_layers: Vec<MarcLegLayerSpec>,
    /// LT entry past the zone's marcato start, ms, by IOI.
    pub lt_offset_ms: Vec<[f32; 2]>,
    /// By attack class `(soft, hard)` instead of by IOI (CSB `$5ibvn` /
    /// `$cblef`; level `$b23jt` / `$rlbmz` dB; retire `$pprfd` / `$orqo1`).
    #[facet(default)]
    pub lt_offset_ms_by_class: Option<[f32; 2]>,
    #[facet(default)]
    pub lt_db_by_class: Option<[f32; 2]>,
    #[facet(default)]
    pub lt_retire_ms_by_class: Option<[f32; 2]>,
    /// LT level "LT Vol" dB by IOI: step (≤ 2) / leap; plus the group volume.
    pub lt_db_step: Vec<[f32; 2]>,
    pub lt_db_leap: Vec<[f32; 2]>,
    pub lt_group_db: f32,
    /// The LT's velocity law, dB at velocity 0: `(127 − vel)` of it ÷ 127
    /// (CSSS `(127-$EVENT_VELOCITY)*%lwgt4[51]/10000` from `$0kd5m`,
    /// CSSS 11771). 0 = none.
    #[facet(default)]
    pub lt_vel_db: f32,
    /// The LT's phrase ramp, deci-dB / ms as the script ships them
    /// (`$4lqhx`, `$ee3a4`, `$xu41m`, `$c2hkn`; CSSS 11757-11768), see
    /// [`ksp_phrase_ramp_ddb`]. None = no ramp.
    #[facet(default)]
    pub lt_ramp: Option<[i32; 4]>,
    pub lt_env: Vec<EnvSegmentSpec>,
    /// The previous transition voice's retire fade, ms by IOI.
    pub lt_retire_ms: Vec<[f32; 2]>,
    /// The in-phrase release's level against the zone (it replaces the
    /// release articulation's held-time level; Kontakt: −8 dB).
    pub in_phrase_release_db: f32,
    /// At the note-off of a note reached by a transition its transition
    /// voice fades over `lt_noteoff_ms` by that transition's IOI (20831-20859);
    /// the body keeps the articulation's held-time fade (measured — not the
    /// decode's 225 ms phrase-note fade).
    pub lt_noteoff_ms: Vec<[f32; 2]>,
}

/// A connected note on CSS tremolo / trills / harmonics (KSP `$ocjln` 2-4),
/// null-tested against Kontakt 8: no overlap delay; the old body fades out
/// linearly over `old_fade_ms[zone]`, the new body enters from its sample
/// start fading in over `new_fade_ms[zone]` (beyond an octave both use
/// `big_fade_ms`); the old note's in-phrase release plays at
/// `in_phrase_release_db`; with `lt_articulation` set, the lower note's
/// interval-coded transition zone of it enters `lt_offset_ms` deep at
/// `lt_db` under `lt_env` × a linear fade-out over `lt_fade_out_ms`
/// (measured: silent 120 ms, then 680 ms up to full).
#[derive(Debug, Clone, Facet)]
pub struct SimpleTransitionSpec {
    pub old_fade_ms: [f32; 3],
    pub new_fade_ms: [f32; 3],
    pub big_fade_ms: [f32; 2],
    pub in_phrase_release_db: f32,
    #[facet(default)]
    pub lt_articulation: String,
    #[facet(default)]
    pub lt_offset_ms: f32,
    #[facet(default)]
    pub lt_db: f32,
    #[facet(default)]
    pub lt_env: Vec<EnvSegmentSpec>,
    #[facet(default)]
    pub lt_fade_out_ms: f32,
}

/// A loud first note's attack voice (CSS KSP 17415-17450): in velocity
/// zone 2 / 3 the first note of a phrase also plays, in every vibsus dynamic
/// group, the zone at a drawn velocity — `random(5,7)` (Staccato
/// recordings) / `random(8,9)` (Sfz) — from the sample start, at that
/// group's own CC1/CC2 curve level (so with vibrato down it is silent).
/// Null-tested in Kontakt: no delay, no extra trim, the zone's level.
#[derive(Debug, Clone, Facet)]
pub struct FirstNoteAttackSpec {
    /// The zones' articulation (written by `zones sync-tsv`; their
    /// `rr_index` holds the Kontakt velocity).
    pub articulation: String,
    /// The articulation whose dynamic layers carry it (the vibsus side).
    pub layer_articulation: String,
    /// Velocity ranges `[lo, hi]` for zone 2 and zone 3.
    pub velocities: Vec<[u8; 2]>,
}

/// A held key that plays a timed phrase of one-shot hits instead of one
/// short (CSB Taka in its "Hold" mode, KSP 10365-10805). `delay_ms` after
/// the note-on the key plays one of `first_rr` (not the previous phrase's
/// first); every `tick_ms` while any phrase key is down it plays the next
/// hit — first one of `second_rr`, then that RR + 1, wrapping to
/// `step_wrap[0]` past `step_wrap[1]` — fading the key's previous hit over
/// `fade_ms`; at the first tick after every key is up each key plays one of
/// `end_rr` (a key released while others hold plays it at its next tick).
/// RRs are 1-based within the velocity layer. The hits ring to their ends:
/// note-offs do not release them.
#[derive(Debug, Clone, Facet)]
pub struct PhraseSpec {
    pub delay_ms: f32,
    pub tick_ms: f32,
    pub fade_ms: f32,
    pub first_rr: Vec<u8>,
    pub second_rr: Vec<u8>,
    pub step_wrap: [u8; 2],
    pub end_rr: Vec<u8>,
}

/// A marcato phrase's first-note overlay (KSP 17620-17855): with the
/// overlay on (CC58 71-75) the first note also plays a short of
/// `articulation` at velocity `vel · cc1_scale[CC1] / 100`, through that
/// short's own layer/level law and RR sequence, untuned.
#[derive(Debug, Clone, Facet)]
pub struct OverlaySpec {
    pub articulation: String,
    pub cc1_scale: Vec<u8>,
    /// A first note after a HARD transition plays no overlay (CSS
    /// `$kbqnb`). CSB plays it on every first note: false.
    #[facet(default = true)]
    pub skip_after_hard_transition: bool,
    /// Connected notes at or above this velocity play the overlay too
    /// (CSB: 64, `$evk4m`). None = first notes only (CSS).
    #[facet(default)]
    pub connected_from_velocity: Option<u8>,
}

/// A section's decoded numbers over the shared library config (CSS: each
/// section's NKI ships its own velocity tables, RR counts, group CC curves
/// and envelopes). Written by `features/rigs/orchestra/tests/css_section_spec.py`
/// as `section.styx` beside the section's `library.styx`; `load_merged`
/// applies it by articulation id, field by field.
#[derive(Debug, Clone, Default, Facet)]
pub struct SectionOverlay {
    #[facet(default)]
    pub articulations: Vec<ArticulationOverlay>,
    /// The section's legato timing knobs (see [`LegatoOverlay`]).
    pub legato: Option<LegatoOverlay>,
    /// See [`PerformanceSpec::rr_per_key`].
    pub rr_per_key: Option<bool>,
    /// See [`PerformanceSpec::instrument_gain_db`]: an instrument whose
    /// level differs from its library's (CSW / CSB patches each set their
    /// own instrument volume).
    pub instrument_gain_db: Option<f32>,
    /// See [`PerformanceSpec::instrument_pan`].
    pub instrument_pan: Option<f32>,
    /// See [`PerformanceSpec::players`].
    pub players: Option<PlayersSpec>,
    /// See [`PerformanceSpec::start_ccs`].
    pub start_ccs: Option<Vec<FixedCcSpec>>,
    /// See [`PerformanceSpec::mirror_ccs`].
    pub mirror_ccs: Option<Vec<CcMirrorSpec>>,
    /// See [`DynamicsSpec::vibrato_controller`]: an instrument whose vibrato
    /// control differs from its library's. `""` = none (the CSW clarinets'
    /// `$vkgua = -1`: CC2 never leaves the vibsus slot).
    pub vibrato_controller: Option<String>,
    /// See [`LibrarySpec::fretboard`]: the instrument's strings and its
    /// shipped placement and round-robin data, which replace the library's.
    pub fretboard: Option<FretboardSpec>,
    /// See [`DynamicsSpec::fixed_ccs`]: the instrument's shipped knobs on
    /// controllers, which replace the library's.
    pub fixed_ccs: Option<Vec<FixedCcSpec>>,
}

/// The overridable legato-engine fields of a section: the CSS editor knobs
/// whose shipped values differ between sections.
#[derive(Debug, Clone, Default, Facet)]
pub struct LegatoOverlay {
    pub overlap_delay: Option<OverlapDelaySpec>,
    pub first_note_variant_init: Option<u8>,
    pub first_note_fade_ms: Option<Vec<f32>>,
    /// An instrument's whole transition model (every CSB number is
    /// per instrument).
    pub kontakt_transition: Option<KontaktTransitionSpec>,
    /// Just the per-layer body handoff of the library's
    /// `kontakt_transition` (CSW: the `%k1aqh` waits differ per instrument).
    pub body_handoff: Option<Vec<BodyHandoffSpec>>,
    pub lt_offset_expressive: Option<Vec<u32>>,
    pub lt_offset_low_latency: Option<Vec<u32>>,
}

/// A marcato transition's phrase ramp (CSSS `$1z3x0`) at `elapsed_ms` past the phrase anchor, in
/// deci-dB, and whether the anchor moves to `now − window` — the
/// script's integer arithmetic line for line (CSSS 11757-11768):
///
/// ```text
/// if (elapsed <= $xu41m)          ; A: start + |mid| over the window
///     $1z3x0 := $4lqhx*100 - ($ee3a4*100*elapsed/$xu41m)
/// else
///     if (elapsed <= $c2hkn)      ; B: mid back toward 0
///         $1z3x0 := $ee3a4*100 + elapsed*(abs($ee3a4)*100/$c2hkn)
///     end if                      ; C: 0
///     $0nind := $ENGINE_UPTIME - $xu41m
/// end if
/// $1z3x0 := $1z3x0/100
/// ```
#[must_use]
pub fn ksp_phrase_ramp_ddb(ramp: Option<[i32; 4]>, elapsed_ms: i64) -> (i32, bool) {
    let Some([start, mid, window, span]) = ramp else {
        return (0, false);
    };
    let (start, mid, window, span) = (
        i64::from(start),
        i64::from(mid),
        i64::from(window),
        i64::from(span),
    );
    let (v, moved) = if elapsed_ms <= window {
        (start * 100 - mid * 100 * elapsed_ms / window.max(1), false)
    } else if elapsed_ms <= span {
        (
            mid * 100 + elapsed_ms * (mid.abs() * 100 / span.max(1)),
            true,
        )
    } else {
        (0, true)
    };
    ((v / 100) as i32, moved)
}

/// The overridable [`MarcatoTransitionSpec`] fields of a section.
#[derive(Debug, Clone, Default, Facet)]
pub struct MarcatoOverlay {
    /// The swell's XTime (`$a3zg3`; CSB differs per instrument).
    pub xfade_ms: Option<f32>,
    pub lt_offset_ms_by_class: Option<[f32; 2]>,
    pub lt_db_by_class: Option<[f32; 2]>,
    pub lt_retire_ms_by_class: Option<[f32; 2]>,
    pub hard_velocity: Option<u8>,
    pub hard_ioi_ms: Option<f32>,
    pub body_offset_ms: Option<[f32; 2]>,
    /// The body offset past an octave (CSW / CSB: the same `$yp2sq` as
    /// any connection; CSS has its own `$dztsp` / `$fswjh`).
    pub body_offset_big_ms: Option<[f32; 2]>,
    /// See [`MarcatoTransitionSpec::body_offset_random_ms`].
    pub body_offset_random_ms: Option<f32>,
    /// See [`MarcatoTransitionSpec::lt_vel_db`].
    pub lt_vel_db: Option<f32>,
    /// See [`MarcatoTransitionSpec::lt_ramp`].
    pub lt_ramp: Option<[i32; 4]>,
    pub lt_offset_ms: Option<Vec<[f32; 2]>>,
    pub lt_db_step: Option<Vec<[f32; 2]>>,
    pub lt_db_leap: Option<Vec<[f32; 2]>>,
    pub cc18_other: Option<[f32; 3]>,
    /// The instrument's own marcato-layer → transition-layer pairs (CSB:
    /// each `marc leg` group plays with the `marcato mod` group of its
    /// dynamic code, holding that instrument's own recordings).
    pub lt_layers: Option<Vec<MarcLegLayerSpec>>,
}

/// A pitch envelope whose depth and decay are drawn per strike (CSW marcato,
/// KSP 9220-9240): the depth is `±max_cents · |u|^taper` for `u` uniform in
/// −1..1 (the script draws the raw modulation intensity uniformly; Kontakt's
/// pitch intensity is its square: taper 2), the decay segment's length
/// uniform in `decay_ms`. A key struck within `gate_ms` draws nothing and
/// keeps the last draw (the script leaves the engine parameter set).
#[derive(Debug, Clone, Facet)]
pub struct RandomPitchEnvSpec {
    pub max_cents: f32,
    pub taper: f32,
    pub decay_ms: [f32; 2],
    #[facet(default)]
    pub gate_ms: f32,
}

/// The overridable fields of one articulation (see [`SectionOverlay`]).
#[derive(Debug, Clone, Facet)]
pub struct ArticulationOverlay {
    pub id: String,
    pub dynamics: Option<Vec<String>>,
    pub rr: Option<usize>,
    pub vel_thresholds: Option<Vec<u8>>,
    pub vel_layer_db: Option<Vec<f32>>,
    pub rr_sequence: Option<Vec<u8>>,
    pub rr_random_avoid_last: Option<u8>,
    pub random_tune_cents: Option<u8>,
    pub random_tune_gate_ms: Option<f32>,
    pub held_random_tune_cents: Option<u8>,
    pub release_by_held: Option<ReleaseByHeldSpec>,
    pub rr_per_note: Option<bool>,
    pub group_curves: Option<Vec<GroupCurveSpec>>,
    pub group_fx: Option<Vec<GroupFxSpec>>,
    pub key_pitch_cents: Option<Vec<f32>>,
    pub pitch_env: Option<Vec<EnvSegmentSpec>>,
    pub pitch_env_cents: Option<f32>,
    pub pitch_env_random: Option<RandomPitchEnvSpec>,
    pub amp_env: Option<Vec<EnvSegmentSpec>>,
    /// See [`ArticulationSpec::amp_env2`].
    pub amp_env2: Option<Vec<EnvSegmentSpec>>,
    pub amp_ahdsr: Option<AmpAhdsrSpec>,
    pub amp_env_hold: Option<bool>,
    pub amp_env_release: Option<u32>,
    /// See [`ArticulationSpec::release_ms_by_held`].
    pub release_ms_by_held: Option<Vec<[f32; 2]>>,
    pub level_db: Option<f32>,
    pub script_start_ms: Option<f32>,
    pub vel_gain_db: Option<Vec<f32>>,
    pub key_range: Option<Vec<u8>>,
    pub release_sample: Option<ReleaseSampleSpec>,
    pub release_types: Option<ReleaseTypesSpec>,
    pub fade_releases_ms: Option<f32>,
    pub script_legato: Option<ScriptLegatoSpec>,
    pub keynoise: Option<KeynoiseSpec>,
    pub timed_length: Option<TimedLengthSpec>,
    pub amp_env_depth: Option<f32>,
    pub amp_env_depths: Option<Vec<DynDepthSpec>>,
    pub phrase: Option<PhraseSpec>,
    /// The first-note overlay's velocity scale per CC1 (KSP `%fizyd`).
    pub overlay_cc1_scale: Option<Vec<u8>>,
    pub marcato_transition: Option<MarcatoOverlay>,
}

impl SectionOverlay {
    /// Parse a `section.styx`.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn from_file(path: &Path) -> Result<Self, SamplerError> {
        let text = std::fs::read_to_string(path).map_err(SamplerError::Io)?;
        facet_styx::from_str(&text).map_err(|e| SamplerError::SpecParse(e.to_string()))
    }

    /// Apply to `spec`'s articulations (unknown ids are ignored).
    pub fn apply(&self, spec: &mut LibrarySpec) {
        for o in &self.articulations {
            let Some(a) = spec
                .articulations
                .iter_mut()
                .find(|a| a.id.eq_ignore_ascii_case(&o.id))
            else {
                continue;
            };
            if let Some(v) = &o.dynamics {
                a.dynamics.clone_from(v);
            }
            if let Some(v) = o.rr {
                a.rr = v;
            }
            if let Some(v) = &o.vel_thresholds {
                a.vel_thresholds.clone_from(v);
            }
            if let Some(v) = &o.vel_layer_db {
                a.vel_layer_db.clone_from(v);
            }
            if let Some(v) = &o.rr_sequence {
                a.rr_sequence.clone_from(v);
            }
            if let Some(v) = o.random_tune_cents {
                a.random_tune_cents = v;
            }
            if let Some(v) = o.held_random_tune_cents {
                a.held_random_tune_cents = v;
            }
            if let Some(v) = o.random_tune_gate_ms {
                a.random_tune_gate_ms = v;
            }
            if let Some(v) = &o.release_by_held {
                a.release_by_held = Some(v.clone());
            }
            if let Some(v) = o.rr_random_avoid_last {
                a.rr_random_avoid_last = v;
            }
            if let Some(v) = o.rr_per_note {
                a.rr_per_note = v;
            }
            if let Some(v) = &o.group_curves {
                a.group_curves.clone_from(v);
            }
            if let Some(v) = &o.group_fx {
                a.group_fx.clone_from(v);
            }
            if let Some(v) = &o.key_pitch_cents {
                a.key_pitch_cents.clone_from(v);
            }
            if let Some(v) = &o.pitch_env {
                a.pitch_env.clone_from(v);
            }
            if let Some(v) = o.pitch_env_cents {
                a.pitch_env_cents = v;
            }
            if let Some(v) = &o.pitch_env_random {
                a.pitch_env_random = Some(v.clone());
            }
            if let Some(v) = &o.amp_env {
                a.amp_env.clone_from(v);
            }
            if let Some(v) = o.amp_ahdsr {
                a.amp_ahdsr = Some(v);
            }
            if let Some(v) = o.amp_env_hold {
                a.amp_env_hold = Some(v);
            }
            if let Some(v) = o.amp_env2.clone() {
                a.amp_env2 = v;
            }
            if let Some(v) = o.amp_env_release {
                a.amp_env_release = Some(v);
            }
            if let Some(v) = &o.release_ms_by_held {
                a.release_ms_by_held.clone_from(v);
            }
            if let Some(v) = o.level_db {
                a.level_db = v;
            }
            if let Some(v) = o.script_start_ms {
                a.script_start_ms = v;
            }
            if let Some(v) = &o.vel_gain_db {
                a.vel_gain_db.clone_from(v);
            }
            if let Some(v) = &o.key_range {
                a.key_range.clone_from(v);
            }
            if let Some(v) = &o.release_sample {
                a.release_sample = Some(v.clone());
            }
            if let Some(v) = &o.release_types {
                a.release_types = Some(v.clone());
            }
            if let Some(v) = o.fade_releases_ms {
                a.fade_releases_ms = v;
            }
            if let Some(v) = &o.script_legato {
                a.script_legato = Some(v.clone());
            }
            if let Some(v) = &o.keynoise {
                a.keynoise = Some(v.clone());
            }
            if let Some(v) = &o.timed_length {
                a.timed_length = Some(v.clone());
            }
            if let Some(v) = o.amp_env_depth {
                a.amp_env_depth = Some(v);
            }
            if let Some(v) = &o.amp_env_depths {
                a.amp_env_depths.clone_from(v);
            }
            if let Some(v) = &o.phrase {
                a.phrase = Some(v.clone());
            }
            if let (Some(v), Some(ov)) = (&o.overlay_cc1_scale, a.overlay.as_mut()) {
                ov.cc1_scale.clone_from(v);
            }
            if let (Some(m), Some(t)) = (&o.marcato_transition, a.marcato_transition.as_mut()) {
                m.apply(t);
            }
        }
        if let Some(v) = self.rr_per_key {
            spec.performance.rr_per_key = v;
        }
        if let Some(v) = self.instrument_gain_db {
            spec.performance.instrument_gain_db = v;
        }
        if let Some(v) = self.instrument_pan {
            spec.performance.instrument_pan = v;
        }
        if let Some(v) = &self.players {
            spec.performance.players = Some(v.clone());
        }
        if let Some(v) = &self.start_ccs {
            spec.performance.start_ccs.clone_from(v);
        }
        if let Some(v) = &self.mirror_ccs {
            spec.performance.mirror_ccs.clone_from(v);
        }
        if let Some(v) = &self.vibrato_controller {
            spec.dynamics.vibrato_controller = (!v.is_empty()).then(|| v.clone());
        }
        if let Some(v) = &self.fretboard {
            spec.fretboard = Some(v.clone());
        }
        if let Some(v) = &self.fixed_ccs {
            spec.dynamics.fixed_ccs.clone_from(v);
        }
        if let (Some(l), Some(cfg)) = (&self.legato, spec.legato_engine.as_mut()) {
            if let Some(v) = &l.overlap_delay {
                cfg.overlap_delay = Some(v.clone());
            }
            if let Some(v) = l.first_note_variant_init {
                cfg.first_note_variant_init = v;
            }
            if let Some(v) = &l.first_note_fade_ms {
                cfg.first_note_fade_ms.clone_from(v);
            }
            if let Some(v) = &l.kontakt_transition {
                cfg.kontakt_transition = Some(v.clone());
            }
            if let (Some(v), Some(kt)) = (&l.body_handoff, cfg.kontakt_transition.as_mut()) {
                kt.body_handoff.clone_from(v);
            }
            if let Some(v) = &l.lt_offset_expressive {
                cfg.lt_offset_expressive.clone_from(v);
            }
            if let Some(v) = &l.lt_offset_low_latency {
                cfg.lt_offset_low_latency.clone_from(v);
            }
        }
    }
}

impl MarcatoOverlay {
    fn apply(&self, t: &mut MarcatoTransitionSpec) {
        if let Some(v) = self.xfade_ms {
            t.xfade_ms = v;
        }
        if self.lt_offset_ms_by_class.is_some() {
            t.lt_offset_ms_by_class = self.lt_offset_ms_by_class;
        }
        if self.lt_db_by_class.is_some() {
            t.lt_db_by_class = self.lt_db_by_class;
        }
        if self.lt_retire_ms_by_class.is_some() {
            t.lt_retire_ms_by_class = self.lt_retire_ms_by_class;
        }
        if let Some(v) = self.hard_velocity {
            t.hard_velocity = v;
        }
        if let Some(v) = self.hard_ioi_ms {
            t.hard_ioi_ms = v;
        }
        if let Some(v) = self.body_offset_ms {
            t.body_offset_ms = v;
        }
        if let Some(v) = self.body_offset_big_ms {
            t.body_offset_big_ms = v;
        }
        if let Some(v) = self.body_offset_random_ms {
            t.body_offset_random_ms = v;
        }
        if let Some(v) = self.lt_vel_db {
            t.lt_vel_db = v;
        }
        if self.lt_ramp.is_some() {
            t.lt_ramp = self.lt_ramp;
        }
        if let Some(v) = &self.lt_offset_ms {
            t.lt_offset_ms.clone_from(v);
        }
        if let Some(v) = &self.lt_db_step {
            t.lt_db_step.clone_from(v);
        }
        if let Some(v) = &self.lt_db_leap {
            t.lt_db_leap.clone_from(v);
        }
        if let Some(v) = &self.lt_layers {
            t.lt_layers.clone_from(v);
        }
        if let Some(v) = self.cc18_other {
            t.cc18_other = v;
        }
    }
}

/// See [`PerformanceSpec::trill_pair`].
#[derive(Debug, Clone, Facet)]
pub struct TrillPairSpec {
    pub half: String,
    pub whole: String,
    pub window_ms: f32,
    pub delay_ms: f32,
}

/// One marcato dynamic layer's transition sample (see
/// [`MarcatoTransitionSpec::lt_layers`]).
#[derive(Debug, Clone, Facet)]
pub struct MarcLegLayerSpec {
    pub dynamic: String,
    pub lt_dynamic: String,
    pub gain_index: u32,
}

/// The KSP's integer lerp over IOI breakpoints (`$5agkf*($l022q*10000/$5yyiz)
/// /10000+base`): the first value below the first breakpoint, the last at or
/// past the last, truncated toward zero in between.
#[must_use]
pub fn ksp_lerp(pts: &[[f32; 2]], x_ms: i64) -> i64 {
    let Some(first) = pts.first() else { return 0 };
    if x_ms < first[0] as i64 {
        return first[1] as i64;
    }
    for w in pts.windows(2) {
        let (x0, y0, x1, y1) = (
            w[0][0] as i64,
            w[0][1] as i64,
            w[1][0] as i64,
            w[1][1] as i64,
        );
        if x_ms < x1 {
            let span = (x1 - x0).max(1);
            return y0 + (y1 - y0) * ((x_ms - x0) * 10000 / span) / 10000;
        }
    }
    pts[pts.len() - 1][1] as i64
}

/// Piecewise-linear `(x, y)` table lookup, clamped at both ends.
#[must_use]
pub fn lerp_table(pts: &[[f32; 2]], x: f32) -> f32 {
    let Some(first) = pts.first() else { return 0.0 };
    if x <= first[0] {
        return first[1];
    }
    for w in pts.windows(2) {
        let ([x0, y0], [x1, y1]) = (w[0], w[1]);
        if x <= x1 {
            return if x1 > x0 {
                y0 + (y1 - y0) * (x - x0) / (x1 - x0)
            } else {
                y1
            };
        }
    }
    pts[pts.len() - 1][1]
}

/// A piecewise-linear curve over the inter-onset interval (IOI, ms): below
/// `thresholds_ms[0]` → `anchors_ms[0]`, above the last threshold → the last
/// anchor, linear between.
///
/// Used for the legato Overlap-Delay and the transition sample-start offset.
#[derive(Debug, Clone, Default, PartialEq, Facet)]
pub struct IoiCurveSpec {
    /// Ascending IOI breakpoints (ms).
    pub thresholds_ms: Vec<f32>,
    /// Hold each anchor until the next breakpoint instead of interpolating
    /// (an integer step table, e.g. a KSP value computed with floor / ceil).
    #[facet(default)]
    pub step: bool,
    /// Anchor values (ms) at each breakpoint; same length as `thresholds_ms`.
    pub anchors_ms: Vec<f32>,
}

impl IoiCurveSpec {
    /// Piecewise-linear interpolation of `ioi_ms` across the breakpoints.
    #[must_use]
    pub fn value_at(&self, ioi_ms: f32) -> f32 {
        let n = self.thresholds_ms.len().min(self.anchors_ms.len());
        if n == 0 {
            return 0.0;
        }
        if self.step {
            let i = self.thresholds_ms[..n]
                .iter()
                .rposition(|t| *t <= ioi_ms)
                .unwrap_or(0);
            return self.anchors_ms[i];
        }
        if ioi_ms <= self.thresholds_ms[0] {
            return self.anchors_ms[0];
        }
        for k in 0..n - 1 {
            if ioi_ms < self.thresholds_ms[k + 1] {
                let span = (self.thresholds_ms[k + 1] - self.thresholds_ms[k]).max(1e-6);
                let t = (ioi_ms - self.thresholds_ms[k]) / span;
                return self.anchors_ms[k] + (self.anchors_ms[k + 1] - self.anchors_ms[k]) * t;
            }
        }
        self.anchors_ms[n - 1]
    }
}

/// A body's release level (dB) by how long the note was held (ms), per
/// release context. Each curve is linear between its points and flat past
/// either end ([`IoiCurveSpec::value_at`] over the held time). CSB: `%ciooq`
/// / `%54rxx` (detached and first), `%x1kwp` / `%5gm34` (in phrase),
/// `%e1dvv` / `%fxjmy` (phrase end), deci-dB in the script.
#[derive(Debug, Clone, Default, PartialEq, Facet)]
pub struct ReleaseByHeldSpec {
    /// Legato off: the note was played detached.
    pub detached: IoiCurveSpec,
    /// Added to `detached` (CSB `$b5zq1`, −3 dB).
    #[facet(default)]
    pub detached_db: f32,
    /// Legato on, a phrase's only note released on its own.
    pub first: IoiCurveSpec,
    /// The old note of a legato connection (or a retrigger).
    pub in_phrase: IoiCurveSpec,
    /// Held longer than this, a connection's old note plays no release
    /// (CSB `%nuvn3`). `None` = always.
    #[facet(default)]
    pub in_phrase_cutoff_ms: Option<f32>,
    /// The last note of a legato phrase.
    pub phrase_end: IoiCurveSpec,
}

/// Which release a note-off or a connection plays (see [`ReleaseByHeldSpec`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseContext {
    Detached,
    First,
    InPhrase,
    PhraseEnd,
}

impl ReleaseByHeldSpec {
    /// The release level (dB) in `ctx` after `held_ms`, or `None` when this
    /// context plays no release.
    #[must_use]
    pub fn db(&self, ctx: ReleaseContext, held_ms: f32) -> Option<f32> {
        Some(match ctx {
            ReleaseContext::Detached => self.detached.value_at(held_ms) + self.detached_db,
            ReleaseContext::First => self.first.value_at(held_ms),
            ReleaseContext::InPhrase => {
                if self.in_phrase_cutoff_ms.is_some_and(|c| held_ms > c) {
                    return None;
                }
                self.in_phrase.value_at(held_ms)
            }
            ReleaseContext::PhraseEnd => self.phrase_end.value_at(held_ms),
        })
    }
}

/// Overlap-Delay curves for one legato mode: how long the engine waits after
/// a note-on before firing the transition, interpolated over the IOI.
///
/// `soft` applies to attack-velocity range 1 (≤ the first
/// [`LegatoEngineSpec::velocity_splits`] split), `loud` to ranges 2+.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct OverlapDelayCurveSpec {
    pub soft: IoiCurveSpec,
    pub loud: IoiCurveSpec,
}

/// Overlap-Delay configuration (CSS `legtrans_OD` / `$b0n3s`), per legato
/// mode. Defaults are the decoded CSS persistent values — near-zero
/// everywhere except soft+fast playing.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct OverlapDelaySpec {
    pub low_latency: OverlapDelayCurveSpec,
    pub expressive: OverlapDelayCurveSpec,
}

impl Default for OverlapDelaySpec {
    /// The decoded CSS persistent values (`CSS 1st Violins.nki` `BParScript`
    /// store): LL thresholds `$deey3/$fxiox/$jystg/$zvaet`, EX thresholds
    /// `$g45yq/$bwkdm/$waq1e/$whtm2`; soft anchors `$nbkqa…` / `$kadcz…`;
    /// loud anchors all-zero.
    fn default() -> Self {
        let z4 = vec![0.0, 0.0, 0.0, 0.0];
        Self {
            low_latency: OverlapDelayCurveSpec {
                soft: IoiCurveSpec {
                    step: false,
                    thresholds_ms: vec![75.0, 100.0, 800.0, 1100.0],
                    anchors_ms: vec![77.0, 0.0, 0.0, 0.0],
                },
                loud: IoiCurveSpec {
                    step: false,
                    thresholds_ms: vec![75.0, 100.0, 800.0, 1100.0],
                    anchors_ms: z4.clone(),
                },
            },
            expressive: OverlapDelayCurveSpec {
                soft: IoiCurveSpec {
                    step: false,
                    thresholds_ms: vec![200.0, 300.0, 800.0, 800.0],
                    anchors_ms: vec![83.0, 0.0, 0.0, 0.0],
                },
                loud: IoiCurveSpec {
                    step: false,
                    thresholds_ms: vec![200.0, 300.0, 800.0, 800.0],
                    anchors_ms: z4,
                },
            },
        }
    }
}

/// The default transition sample-start offset curve — the decoded CSS
/// `$1fvjk` IOI curve (`$ocjln = 6`): flat 177 ms up to a 150 ms IOI, linear
/// 177 → 117 ms across 150…500 ms, flat 117 ms above. Fast lines start
/// DEEPER into the transition recording (less audible pre-bow).
fn default_start_offset_curve() -> IoiCurveSpec {
    IoiCurveSpec {
        step: false,
        thresholds_ms: vec![100.0, 150.0, 500.0],
        anchors_ms: vec![177.0, 177.0, 117.0],
    }
}

/// CC1 → loudness expression curve for CC1-crossfaded sustains: 0 dB at and
/// above `knee`, linear to `floor_db` at CC1=0.
///
/// The per-layer crossfade handles TIMBRE (≈flat total level); this supplies
/// only the gentle bottom rolloff. Defaults calibrated on the CSS reference
/// render (CC1=20 → −3.0 dB, flat from CC1≈45).
#[derive(Debug, Clone, Copy, PartialEq, Facet)]
pub struct Cc1ExpressionSpec {
    #[facet(default = 45)]
    pub knee: u8,
    #[facet(default = -5.4f32)]
    pub floor_db: f32,
    /// Curve exponent on the normalised distance below the knee. 1.0 is the
    /// original straight line. Above 1.0 the attenuation concentrates near
    /// CC1=0, which is the shape the Kontakt reference actually has: measured
    /// on the S14 CC1 sweep, the reference falls ~44 dB from CC1=127 to CC1=1
    /// but is already within a few dB of its knee level by CC1≈30, so a
    /// straight ramp cannot be steep enough at the bottom without being far
    /// too steep in the middle.
    #[facet(default = 1.0f32)]
    pub shape: f32,
}

impl Default for Cc1ExpressionSpec {
    fn default() -> Self {
        Self {
            knee: 45,
            floor_db: -5.4,
            shape: 1.0,
        }
    }
}

impl DynamicsSpec {
    /// CC1 → linear loudness gain from [`DynamicsSpec::cc1_expression`]
    /// (falling back to the CSS-calibrated defaults): 0 dB at/above the knee,
    /// falling to `floor_db` at CC1=0 along `shape` (1.0 = straight line).
    #[must_use]
    pub fn cc1_expression_gain(&self, cc1: u8) -> f32 {
        let c = self.cc1_expression.unwrap_or_default();
        let db = if cc1 >= c.knee || c.knee == 0 {
            0.0
        } else {
            let x = (c.knee - cc1) as f32 / c.knee as f32;
            let x = if c.shape > 0.0 && c.shape != 1.0 {
                x.powf(c.shape)
            } else {
                x
            };
            c.floor_db * x
        };
        10f32.powf(db / 20.0)
    }
}

// ── Default (family) amp envelopes ───────────────────────────────────────────
//
// The decoded CSS ENV_FLEX amp envelopes (GroupList 0x33; literal shipped
// values, Main mic) — retained here as the FAMILY DEFAULTS an articulation
// falls back to when its spec carries no `amp_env`. A pack can override any
// of them per articulation; libraries that never matched these families
// (non-zoned, plain synth) are unaffected.

/// Sustain family: fast bake attack, hold, 20 s decay-to-0. Held via the
/// engine's sustain-hold freeze.
pub const AMP_ENV_SUSTAIN: &[EnvSegmentSpec] = &[
    seg(4.0, 1.0, 0.505),
    seg(1000.0, 1.0, 0.9),
    seg(20000.0, 0.0, 0.05),
];
/// Legato / legato-zero transition body.
pub const AMP_ENV_LEGATO: &[EnvSegmentSpec] = &[
    seg(80.0, 1.0, 0.499),
    seg(480.0, 1.0, 0.72),
    seg(442.3, 1.0, 0.5),
    seg(1002.3, 0.0, 0.33),
    seg(152.0, 0.0, 0.5),
    seg(342.0, 0.0, 0.75),
];
/// Portamento glide.
pub const AMP_ENV_PORTAMENTO: &[EnvSegmentSpec] = &[
    seg(88.0, 0.466, 0.499),
    seg(472.0, 1.0, 0.8),
    seg(1240.0, 0.0, 0.5),
    seg(152.0, 0.0, 0.5),
    seg(342.0, 0.0, 0.75),
];
/// Marcato-legato / marc-port.
pub const AMP_ENV_MARC_LEG: &[EnvSegmentSpec] = &[
    seg(68.0, 0.493, 0.499),
    seg(492.0, 1.0, 0.72),
    seg(1440.0, 0.0, 0.33),
    seg(156.6, 0.0, 0.5),
    seg(342.0, 0.0, 0.75),
];
/// Marcato-mod overlay.
pub const AMP_ENV_MARCATO_MOD: &[EnvSegmentSpec] = &[
    seg(1.0, 1.0, 0.685),
    seg(1499.0, 1.0, 0.5),
    seg(104.0, 1.0, 0.45),
    seg(1000.0, 0.0, 0.63),
];
/// Short family: one-shot, natural end shaped by the 8/604/7381 decay.
pub const AMP_ENV_SHORT: &[EnvSegmentSpec] = &[
    seg(8.0, 1.0, 0.505),
    seg(604.0, 1.0, 0.45),
    seg(7381.0, 0.0, 0.65),
];
/// Release tails.
pub const AMP_ENV_RELEASE: &[EnvSegmentSpec] = &[
    seg(1.0, 0.986, 0.125),
    seg(4007.0, 1.0, 0.9),
    seg(1250.0, 0.0, 0.7),
];

const fn seg(time_ms: f32, level: f32, curve: f32) -> EnvSegmentSpec {
    EnvSegmentSpec {
        time_ms,
        level,
        curve,
    }
}

/// The voice role the engine resolved from its `VoiceKind` — the MECHANISM
/// side of amp-envelope selection. The POLICY (which family table applies)
/// lives here in the spec layer, in [`default_amp_env`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmpEnvRole {
    Release,
    Short,
    Legato,
    SustainLayer,
    Other,
}

/// Family-default amp envelope `(segments, hold)` for an articulation id +
/// voice role. `None` = no envelope (flat unity — legacy behaviour for
/// families outside the decoded set).
#[must_use]
pub fn default_amp_env(
    artic_id: &str,
    role: AmpEnvRole,
) -> Option<(&'static [EnvSegmentSpec], bool)> {
    let id = artic_id.to_ascii_lowercase();
    if role == AmpEnvRole::Release || id.contains("rel") {
        Some((AMP_ENV_RELEASE, false))
    } else if role == AmpEnvRole::Short {
        Some((AMP_ENV_SHORT, false))
    } else if id.contains("port") {
        Some((AMP_ENV_PORTAMENTO, false))
    } else if id.contains("marc") && id.contains("leg") {
        Some((AMP_ENV_MARC_LEG, false))
    } else if id.contains("marcato") && id.contains("mod") {
        Some((AMP_ENV_MARCATO_MOD, false))
    } else if role == AmpEnvRole::Legato || id.contains("legato") {
        Some((AMP_ENV_LEGATO, false))
    } else if role == AmpEnvRole::SustainLayer {
        Some((AMP_ENV_SUSTAIN, true))
    } else {
        None
    }
}

/// Read the `STINFO` sustain-loop tag from a FLAC's Vorbis comment:
/// `STINFO=<enabled> <loop_start> <loop_end> <xfade>` (frames). Returns
/// `(loop_start, loop_end)` only when enabled and non-empty. Metadata only —
/// walks the FLAC metadata blocks and never decodes audio.
fn read_flac_stinfo_loop(path: &Path) -> Option<(u32, u32)> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let mut magic = [0u8; 4];
    f.read_exact(&mut magic).ok()?;
    if &magic != b"fLaC" {
        return None;
    }
    loop {
        let mut hdr = [0u8; 4];
        f.read_exact(&mut hdr).ok()?;
        let is_last = hdr[0] & 0x80 != 0;
        let block_type = hdr[0] & 0x7f;
        let len = u32::from_be_bytes([0, hdr[1], hdr[2], hdr[3]]) as usize;
        if block_type == 4 {
            // VORBIS_COMMENT
            let mut body = vec![0u8; len];
            f.read_exact(&mut body).ok()?;
            return parse_stinfo_from_vorbis(&body);
        }
        f.seek(SeekFrom::Current(len as i64)).ok()?;
        if is_last {
            return None;
        }
    }
}

/// Find + parse a `STINFO=` entry inside a FLAC `VORBIS_COMMENT` block body
/// (`[vendor_len u32le][vendor][count u32le]([len u32le][KEY=val])*`).
fn parse_stinfo_from_vorbis(body: &[u8]) -> Option<(u32, u32)> {
    let rd = |o: usize| -> Option<u32> {
        body.get(o..o + 4)
            .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    };
    let mut o = 0usize;
    o += 4 + rd(o)? as usize; // skip vendor string
    let count = rd(o)? as usize;
    o += 4;
    for _ in 0..count {
        let clen = rd(o)? as usize;
        o += 4;
        let c = body.get(o..o + clen)?;
        o += clen;
        let Ok(s) = std::str::from_utf8(c) else {
            continue;
        };
        // Vorbis comment keys are case-insensitive.
        if s.len() >= 7 && s[..7].eq_ignore_ascii_case("STINFO=") {
            let parts: Vec<&str> = s[7..].split_whitespace().collect();
            if parts.len() >= 3 {
                let enabled: u32 = parts[0].parse().ok()?;
                let ls: u32 = parts[1].parse().ok()?;
                let le: u32 = parts[2].parse().ok()?;
                if enabled != 0 && le > ls {
                    return Some((ls, le));
                }
            }
            return None;
        }
    }
    None
}

fn parse_sfz_zones(s: &str) -> Result<Vec<ZoneSpec>, SamplerError> {
    let mut current_group: HashMap<String, String> = HashMap::new();
    let mut current_region: HashMap<String, String> = HashMap::new();
    let mut zones = Vec::new();
    let mut in_region = false;

    for token in sfz_tokens(s) {
        match token.as_str() {
            "<group>" => {
                if current_region.contains_key("sample") {
                    zones.push(zone_from_sfz(&current_group, &current_region)?);
                    current_region.clear();
                }
                current_group.clear();
                in_region = false;
            }
            "<region>" => {
                if current_region.contains_key("sample") {
                    zones.push(zone_from_sfz(&current_group, &current_region)?);
                    current_region.clear();
                }
                in_region = true;
            }
            _ => {
                if let Some((key, value)) = token.split_once('=') {
                    let target = if in_region {
                        &mut current_region
                    } else {
                        &mut current_group
                    };
                    target.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
                }
            }
        }
    }
    if current_region.contains_key("sample") {
        zones.push(zone_from_sfz(&current_group, &current_region)?);
    }
    Ok(zones)
}

fn sfz_tokens(s: &str) -> Vec<String> {
    s.lines()
        .flat_map(|line| {
            let line = line.split("//").next().unwrap_or("").trim();
            line.split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn sfz_value<'a>(
    group: &'a HashMap<String, String>,
    region: &'a HashMap<String, String>,
    key: &str,
) -> Option<&'a str> {
    region
        .get(key)
        .or_else(|| group.get(key))
        .map(String::as_str)
}

fn sfz_u8(
    group: &HashMap<String, String>,
    region: &HashMap<String, String>,
    key: &str,
) -> Option<u8> {
    sfz_value(group, region, key).and_then(parse_sfz_note_or_u8)
}

fn sfz_u32(
    group: &HashMap<String, String>,
    region: &HashMap<String, String>,
    key: &str,
) -> Option<u32> {
    sfz_value(group, region, key).and_then(|value| value.parse().ok())
}

fn sfz_f32(
    group: &HashMap<String, String>,
    region: &HashMap<String, String>,
    key: &str,
) -> Option<f32> {
    sfz_value(group, region, key).and_then(|value| value.parse().ok())
}

fn parse_sfz_note_or_u8(value: &str) -> Option<u8> {
    value
        .parse()
        .ok()
        .or_else(|| crate::midi::note_name_to_midi(value).ok())
}

fn zone_from_sfz(
    group: &HashMap<String, String>,
    region: &HashMap<String, String>,
) -> Result<ZoneSpec, SamplerError> {
    let file = sfz_value(group, region, "sample")
        .ok_or_else(|| SamplerError::SpecParse("SFZ region missing sample".to_string()))?
        .replace('\\', "/");
    let key = sfz_u8(group, region, "key");
    let key_min = key.or_else(|| sfz_u8(group, region, "lokey")).unwrap_or(0);
    let key_max = key
        .or_else(|| sfz_u8(group, region, "hikey"))
        .unwrap_or(127);
    let root_key = sfz_u8(group, region, "pitch_keycenter")
        .or(key)
        .unwrap_or(key_min);
    let transpose = sfz_f32(group, region, "transpose").unwrap_or(0.0) * 100.0;
    let tune = sfz_f32(group, region, "tune").unwrap_or(0.0);
    let seq_position = sfz_u32(group, region, "seq_position").unwrap_or(1);
    let seq_length = sfz_u32(group, region, "seq_length").unwrap_or(0);
    let group_id = sfz_value(group, region, "group").unwrap_or("").to_string();
    let off_by = sfz_value(group, region, "off_by")
        .map(|value| vec![value.to_string()])
        .unwrap_or_default();
    let trigger_mode = sfz_value(group, region, "trigger")
        .map(normalize_sfz_trigger)
        .unwrap_or_default();

    Ok(ZoneSpec {
        file,
        key_min,
        key_max,
        root_key,
        vel_min: sfz_u8(group, region, "lovel").unwrap_or(0),
        vel_max: sfz_u8(group, region, "hivel").unwrap_or(127),
        rr_index: seq_position.saturating_sub(1),
        rr_mode: if seq_length > 0 {
            "cycle".to_string()
        } else {
            Default::default()
        },
        gain_db: sfz_f32(group, region, "volume").unwrap_or(0.0),
        pan: (sfz_f32(group, region, "pan").unwrap_or(0.0) / 100.0).clamp(-1.0, 1.0),
        tune_cents: tune + transpose,
        sample_start: sfz_u32(group, region, "offset").unwrap_or(0),
        start_mod: None,
        tm2_block: 0,
        fixed_pitch: false,
        variant_starts: Vec::new(),
        disabled: false,
        borrowed: false,
        marcato_start: 0,
        marcato_gains_db: Vec::new(),
        sample_end: sfz_u32(group, region, "end").unwrap_or(0),
        loop_start: sfz_u32(group, region, "loop_start").unwrap_or(0),
        loop_end: sfz_u32(group, region, "loop_end").unwrap_or(0),
        loop_xfade: 0,
        fade_in: 0,
        release_start: 0,
        playback_mode: String::new(),
        trigger_mode,
        trigger_cc: sfz_u8(group, region, "on_locc").unwrap_or(0),
        trigger_value_min: sfz_u8(group, region, "on_locc").unwrap_or(0),
        trigger_value_max: sfz_u8(group, region, "on_hicc").unwrap_or(0),
        pedal_state: String::new(),
        mic: String::new(),
        articulation: String::new(),
        dynamic: String::new(),
        direction: String::new(),
        interval: 0,
        string: 0,
        stroke: String::new(),
        lead_in_ms: 0.0,
        arrival_ms: 0.0,
        group: group_id.clone(),
        group_polyphony: 0,
        choke_group: group_id,
        off_by,
        section: String::new(),
        variant: String::new(),
    })
}

fn normalize_sfz_trigger(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "release" => "release".to_string(),
        "first" => "first-note".to_string(),
        "legato" => "legato".to_string(),
        "attack" => String::new(),
        other => other.to_string(),
    }
}

// ── Section ───────────────────────────────────────────────────────────────────

/// One instrument section (violin, viola, cello, etc.).
#[derive(Debug, Clone, Facet)]
pub struct SectionSpec {
    /// Short identifier used in filenames: `"1v"`, `"Va"`, `"Ce"`, `"Ba"`.
    pub id: String,
    /// Human-readable label.
    pub label: String,

    /// Pitch classes that were sampled (e.g. `["G","A","B","C#","D#","F"]`).
    /// Every 2 semitones; sampler pitch-shifts to fill the gaps.
    #[facet(default)]
    pub note_grid: Vec<String>,

    /// Lowest sampled MIDI note as a name ("G2").
    pub lowest_note: String,
    /// Highest sampled MIDI note as a name ("C#6").
    pub highest_note: String,
}

// ── Mic ───────────────────────────────────────────────────────────────────────

/// One microphone / output bus position.
#[derive(Debug, Clone, Facet)]
pub struct MicSpec {
    /// Short identifier: `"Mix"`, `"Main"`, `"Room"`, `"Spot1"`, `"Spot2"`.
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// `"blended"` (pre-mixed stereo bus) or `"separate"` (individual channel).
    #[facet(default)]
    pub kind: String,
    /// Whether this mic loads automatically when the library is opened.
    /// At most one mic per library should be marked `default`. Other mics
    /// stay unloaded until explicitly requested — saves RAM in libraries
    /// with many mic positions (CSS = 5, MM2 = 7).
    #[facet(default)]
    pub default: bool,
}

// ── Dynamics ─────────────────────────────────────────────────────────────────

/// A controller copied to another on every event (see
/// [`PerformanceSpec::mirror_ccs`]).
#[derive(Debug, Clone, Facet)]
pub struct CcMirrorSpec {
    pub from: u8,
    pub to: u8,
}

/// A controller held at one value (see [`DynamicsSpec::fixed_ccs`]).
#[derive(Debug, Clone, Default, Facet)]
pub struct FixedCcSpec {
    pub cc: u8,
    pub value: u8,
}

/// Dynamic control model for the library.
#[derive(Debug, Clone, Default, Facet)]
pub struct DynamicsSpec {
    /// Controller for long-note dynamics. `"CC1"` for most libraries.
    pub sustain_controller: Option<String>,
    /// Controller for vibrato crossfade. `"CC2"` for strings.
    pub vibrato_controller: Option<String>,
    /// `"crossfade"` or `"on_off"` (solo strings: vibrato is binary).
    pub vibrato_mode: Option<String>,
    /// Controllers the instrument holds at a value of its own, ignoring MIDI
    /// for them: CSB's script ignores CC2 and sets it to its saved vibrato
    /// knob (`$takdp` = 126), which its release groups' CC2 curves read.
    #[facet(default)]
    pub fixed_ccs: Vec<FixedCcSpec>,
    /// Controller for short-note dynamics. `"velocity"` for most libraries.
    pub short_note_controller: Option<String>,

    /// CC1 ranges that select short-note type.
    #[facet(default)]
    pub short_note_cc1_map: HashMap<String, String>,
    /// CC1 ranges for pizzicato sub-types.
    #[facet(default)]
    pub pizzicato_cc1_map: HashMap<String, String>,
    /// Velocity ranges for sustain attack character (winds: normal / accented).
    #[facet(default)]
    pub sustain_attack_velocity: HashMap<String, String>,

    /// Two-layer CC1 crossfade zones.
    #[facet(default)]
    pub cc1_layers_2: Vec<Cc1Layer>,
    /// Three-layer CC1 crossfade zones.
    #[facet(default)]
    pub cc1_layers_3: Vec<Cc1Layer>,
    /// Four-layer CC1 crossfade zones.
    #[facet(default)]
    pub cc1_layers_4: Vec<Cc1Layer>,
    /// Five-layer CC1 crossfade zones.
    #[facet(default)]
    pub cc1_layers_5: Vec<Cc1Layer>,
    /// Six-layer CC1 crossfade zones (piano has 6 dynamics).
    #[facet(default)]
    pub cc1_layers_6: Vec<Cc1Layer>,
    /// Half-pedal damping curve for CC64 values 1..63. Empty/`"linear"` =
    /// straight response, `"squared"` keeps low values closer to normal
    /// damping, `"sqrt"` makes low values more resonant.
    #[facet(default)]
    pub half_pedal_curve: String,
    /// Release-time multiplier at CC64=63. 0 means the engine default.
    #[facet(default)]
    pub half_pedal_max_release_multiplier: f32,
    /// Enable the KSP-confirmed short-note VELOCITY → dynamic-LAYER selection
    /// (the per-articulation `%g1qri` thresholds in `vel_thresholds`) instead of
    /// an even velocity split. This is the `VeloIDX` band selection from
    /// `script_1.ksp`; the short TYPE stays on CC1/CC58. ON for CSS (validated
    /// against the reference render — timbre holds, the collapsed Staccato ladder
    /// lands on the same recorded dynamics). See `SampleEngine::short_band`.
    #[facet(default)]
    pub enable_velocity_layers: bool,
    /// Apply the decoded intra-band velocity→volume trim (`$arhiq`, the KSP
    /// `%bcez1` per-band deltas in `vel_layer_db`) ON TOP of the layer selection.
    /// OFF by default and OFF for CSS: the layer-selection math is validated, but
    /// the currently-decoded `%bcez1` values over-attenuate vs the reference
    /// render (they regress mean|level| ~0.6 dB / MATCH 38→31 on the A/B while
    /// timbre holds), so `$arhiq` awaits re-derivation of the true `%bcez1`
    /// magnitudes from the KSP persistent. `short_velocity_volume_db` still
    /// computes the curve; this flag only gates whether it is applied.
    #[facet(default)]
    pub apply_short_velvol: bool,
    /// Continuous CC1 → loudness curve on top of the layer crossfade (the
    /// gentle bottom rolloff). `None` = the CSS-calibrated defaults
    /// (knee 45, floor −5.4 dB → −3.0 dB at CC1=20).
    pub cc1_expression: Option<Cc1ExpressionSpec>,
}

/// One CC1 dynamic layer with its crossfade range.
#[derive(Debug, Clone, Facet)]
pub struct Cc1Layer {
    /// Dynamic label: `"p"`, `"mf"`, `"ff"`, etc.
    pub label: String,
    /// `[lo, hi]` inclusive CC1 range for this layer (with crossfade on both edges).
    pub cc_range: [u8; 2],
}

/// One Kontakt `CC_VOLUME` table on one dynamic layer's group.
#[derive(Debug, Clone, Facet)]
pub struct GroupCurveSpec {
    /// The layer it scales (zone `dynamic` tag: `"p"`, `"mf"`, `"ff"`, …).
    pub dynamic: String,
    /// Controller number (1 dynamics, 2 vibrato, …).
    pub cc: u8,
    /// Kontakt's table, verbatim: `y0 c0 (x y c)…` — the first point sits at
    /// x = 0; each point's `c` bends the segment that STARTS at it (see
    /// [`kontakt_table_value`]).
    pub points: Vec<f32>,
}

/// One dynamic layer's group insert chain: Kontakt's filter/EQ slots that
/// are not bypassed, in slot order, and the controller routes onto them.
#[derive(Debug, Clone, Facet)]
pub struct GroupFxSpec {
    /// The layer it filters (zone `dynamic` tag).
    pub dynamic: String,
    pub stages: Vec<FxStageSpec>,
    #[facet(default)]
    pub routes: Vec<FxRouteSpec>,
}

/// One filter or EQ stage. `kind`: `legacy_lp1`, `legacy_lp2` (Kontakt's
/// legacy filters, types 2 / 6), `sv_lp1`, `sv_lp2`, `sv_lp4` (the
/// state-variable family, 50 / 52 / 55) or `eq` (1–3 bands, 22–24).
#[derive(Debug, Clone, Facet)]
pub struct FxStageSpec {
    pub kind: String,
    /// Cutoff knob, 0..1 (the kind's law maps it to Hz).
    #[facet(default)]
    pub cutoff: f32,
    /// Resonance knob, 0..1.
    #[facet(default)]
    pub resonance: f32,
    /// EQ bands, flat: `hz octaves db` per band.
    #[facet(default)]
    pub bands: Vec<f32>,
    /// `saturation` stage: Kontakt's Saturation insert, Enhanced mode — the
    /// saved shape (−1..=1) picks the drive/makeup/wet table position
    /// (`engine::kontakt_saturation`).
    #[facet(default)]
    pub shape: f32,
    /// The stage's output gain, dB (Kontakt's per-slot output level; MOJO 2's
    /// saturation slots run at −3 dB). 0 = unity.
    #[facet(default)]
    pub output_db: f32,
    /// `width` stage: the side signal is scaled by `1 + width` (mid kept).
    /// Kontakt's kind-9 group insert, measured on MOJO 2 Trumpet Stabs:
    /// −0.3 leaves the side at 0.70.
    #[facet(default)]
    pub width: f32,
    /// `limiter`: Kontakt's classic Limiter's input gain (dB) and release
    /// (ms).
    #[facet(default)]
    pub input_db: f32,
    #[facet(default)]
    pub release_ms: f32,
}

/// A controller route onto one stage's knob: the knob moves by
/// `intensity × table(cc / 127)` (no table: the controller itself), smoothed
/// by `lag_ms` (Kontakt's lag: the time to settle within 1 %).
#[derive(Debug, Clone, Facet)]
pub struct FxRouteSpec {
    /// Index into the layer's `stages`.
    pub stage: u8,
    /// `cutoff`, `resonance`, `eq_gain1`, `eq_gain2`, `eq_gain3`, or
    /// `volume`: the voice's level at its amplifier (after the chain's
    /// saturation, as its pan), by Kontakt's controller volume law
    /// `max(0, 1 − (1 − x)·intensity)`, `stage` ignored.
    pub param: String,
    pub cc: u8,
    #[facet(default)]
    pub intensity: f32,
    #[facet(default)]
    pub lag_ms: f32,
    /// Kontakt's breakpoint table (`y0 c0 (x y c)…`); empty = none.
    #[facet(default)]
    pub points: Vec<f32>,
    /// An envelope as the source instead of `cc` (a flex envelope swept
    /// across the stage, e.g. CSB marcato's cutoff): its segments, a level
    /// of −1 meaning `cc`'s value ÷ 127 at the note's start. The route moves
    /// the knob by `intensity` × the envelope's level.
    #[facet(default)]
    pub env: Vec<EnvSegmentSpec>,
    /// `velocity`: the note's velocity is the source instead of `cc`, read
    /// once at the note's start (a script setting a stage per note, e.g.
    /// MOJO 2's one-layer lowpass); `key`: the note's key (a Key Position
    /// modulator). Empty = `cc`.
    #[facet(default)]
    pub source: String,
    /// The knob offset at each source value 0..=127, in the param's units,
    /// replacing `intensity × points` (a law a script computes, baked by
    /// the generator). Empty = `intensity × points`.
    #[facet(default)]
    pub table: Vec<f32>,
}

/// Evaluate a Kontakt modulation table (`y0 c0 (x y c)…`) at `x` in `[0, 1]`.
///
/// Measured against Kontakt 8 renders of CSS (sweeps of CC1 and CC2, every
/// value within ±0.002): between two points the curve follows
/// `(e^(k·t) − 1) / (e^k − 1)` with `k = 16·|c|`, where `c` is the curvature
/// of the segment's START point; `c > 0` bends the segment BELOW its chord,
/// `c < 0` above it, `c = 0` is a straight line.
#[must_use]
pub fn kontakt_table_value(points: &[f32], x: f32) -> f32 {
    if points.len() < 2 {
        return 1.0;
    }
    let x = x.clamp(0.0, 1.0);
    // (x, y, c) triples, the implicit first point at x = 0.
    let mut pts: Vec<(f32, f32, f32)> = vec![(0.0, points[0], points[1])];
    pts.extend(points[2..].chunks_exact(3).map(|p| (p[0], p[1], p[2])));
    let Some(seg) = pts.windows(2).find(|w| x <= w[1].0) else {
        return pts.last().map_or(1.0, |p| p.1);
    };
    let ((x0, y0, c), (x1, y1, _)) = (seg[0], seg[1]);
    if x1 <= x0 {
        return y1;
    }
    let t = (x - x0) / (x1 - x0);
    let k = 16.0 * c.abs();
    if k < 1e-6 {
        return y0 + (y1 - y0) * t;
    }
    let g = |u: f32| ((k * u).exp() - 1.0) / (k.exp() - 1.0);
    // The two curvature shapes; pick the one on the side `c` asks for.
    let (a, b) = (y0 + (y1 - y0) * g(t), y0 + (y1 - y0) * (1.0 - g(1.0 - t)));
    let chord = y0 + (y1 - y0) * t;
    let (below, above) = if a <= chord { (a, b) } else { (b, a) };
    if c > 0.0 { below } else { above }
}

#[cfg(test)]
mod kontakt_table_tests {
    use super::kontakt_table_value as v;

    // Tables and expected values: CSS 1st Violins Mix groups, measured from
    // Kontakt 8 renders (orchestra/tests/curve_fit.py).
    const NV_P: &[f32] = &[
        0.0,
        0.067_567_55,
        0.194_881_89,
        0.418_918_9,
        -1.862_645_1e-8,
        0.360_236_23,
        1.0,
        -0.168_918_9,
        0.480_314_97,
        0.0,
        0.0,
        1.0,
        0.0,
        0.0,
    ];
    const NV_FF: &[f32] = &[
        0.0,
        0.0,
        0.421_259_85,
        0.0,
        0.067_567_57,
        0.795_275_57,
        0.716_216_2,
        -0.152_027_01,
        1.0,
        1.0,
        0.0,
    ];
    const NV_CC2: &[f32] = &[1.0, -0.084_459_47, 1.0, 0.0, 0.0];
    const VS_CC2: &[f32] = &[0.0, -0.084_459_46, 1.0, 1.0, 0.0];
    const ZVOL_P: f32 = 1.2599; // the p zones' +2 dB zone volume rides on the measurement

    fn near(a: f32, b: f32, tol: f32) {
        assert!((a - b).abs() <= tol, "{a} vs {b}");
    }

    #[test]
    fn convex_rising_segment() {
        near(v(NV_P, 4.0 / 127.0) * ZVOL_P, 0.0508, 0.002);
        near(v(NV_P, 16.0 / 127.0) * ZVOL_P, 0.2734, 0.003);
    }

    #[test]
    fn straight_segment() {
        near(v(NV_P, 32.0 / 127.0) * ZVOL_P, 0.7805, 0.002);
    }

    #[test]
    fn concave_falling_segment() {
        near(v(NV_P, 48.0 / 127.0) * ZVOL_P, 1.2154, 0.003);
        near(v(NV_P, 52.0 / 127.0) * ZVOL_P, 1.0763, 0.003);
    }

    #[test]
    fn concave_rising_segment_and_ends() {
        near(v(NV_FF, 104.0 / 127.0), 0.7930, 0.002);
        near(v(NV_FF, 0.0), 0.0, 1e-6);
        near(v(NV_FF, 1.0), 1.0, 1e-6);
    }

    #[test]
    fn vibrato_crossfade() {
        near(v(NV_CC2, 16.0 / 127.0), 0.9363, 0.002);
        near(v(VS_CC2, 16.0 / 127.0), 0.2094, 0.002);
        near(v(NV_CC2, 64.0 / 127.0), 0.6594, 0.003);
    }
}

// ── Articulation ─────────────────────────────────────────────────────────────

/// One playing technique in the library.
#[derive(Debug, Clone, Facet)]
pub struct ArticulationSpec {
    /// Token used in WAV filenames: `"Vibsus"`, `"Leg"`, `"Staccato"`, etc.
    pub id: String,
    /// Human-readable name.
    pub label: String,

    /// Playback category.
    pub kind: ArticulationKind,

    /// Sampled dynamic layers (soft → loud): `["p", "mf", "ff"]`.
    #[facet(default)]
    pub dynamics: Vec<String>,
    /// Round-robin count per note per dynamic layer.
    #[facet(default)]
    pub rr: usize,
    /// How dynamics are controlled: `"cc1"`, `"velocity"`, or `"fixed"`.
    #[facet(default)]
    pub dyn_ctrl: String,

    /// ID of the release articulation to trigger on note-off, if any.
    pub release_artic: Option<String>,
    /// ID of a one-shot ATTACK articulation layered on a fresh note-on of
    /// this sustain (Pacific atk+sus pairing). The attack voice plays at the
    /// same CC1 dynamics blend and is independent of legato transitions.
    pub attack_artic: Option<String>,
    /// Whether separate up/down transition samples exist (legato only).
    pub directional: Option<bool>,
    /// `"full"` = full section range; `"short"` = reduced range.
    pub notes: Option<String>,
    /// If set, this articulation only exists for these section ids.
    #[facet(default)]
    pub instrument_filter: Vec<String>,

    /// Alternative filename tokens to try if the primary `id` is not found
    /// in the sample map for a given section.
    #[facet(default)]
    pub aliases: Vec<String>,

    /// UACC code — the latched-CC selector value (see `LibrarySpec.selector`)
    /// that selects this articulation. 0 = unset → resolved from the
    /// published standard table ([`UACC_STANDARD_TABLE`]) by matching
    /// id/label/aliases. An explicit code always wins over the table.
    #[facet(default)]
    pub uacc: u8,

    /// Short-note velocity-layer boundaries (CSS KSP `%g1qri`, the thresholds
    /// AFTER the implicit floor of 1). A note's velocity picks the band it falls
    /// in — `[t0,t1,…]` gives bands `[1,t0) [t0,t1) …`, plus a top band above the
    /// last boundary < 127. Empty = fall back to the even velocity split.
    #[facet(default)]
    pub vel_thresholds: Vec<u8>,
    /// Per-band intra-layer velocity→volume deltas in dB (CSS KSP `%bcez1`, the
    /// recorded adjacent-layer level differences that `$arhiq` ramps across each
    /// band so a note's level tracks velocity CONTINUOUSLY, not just the discrete
    /// recorded layer). One entry per band; applied via the decoded KSP law
    /// `dB = (band_top − vel)·delta / (band_span − 1)`.
    #[facet(default)]
    pub vel_layer_db: Vec<f32>,

    /// Fixed pitch transpose (semitones) applied to every voice of this
    /// articulation ON TOP OF the per-note `note − root_key` shift. Zone
    /// selection still keys off the played note; only the playback rate moves.
    /// CSS Harmonics need `-12`: the shipped natural-harmonic zones are mapped
    /// an octave above the pitch CSS actually sounds (verified by the reference
    /// render — key 67/G4 sounds G5 in CSS but G6 from the raw sample), so the
    /// engine drops them an octave to match. Default 0 = no shift.
    #[facet(default)]
    pub transpose: i8,

    /// Decoded per-articulation amp envelope (`ENV_FLEX` segments; segment 0
    /// is the attack). Empty = fall back to the built-in family defaults
    /// ([`default_amp_env`], keyed by articulation kind/id), which preserve
    /// the historical behaviour for libraries that don't author envelopes.
    // r[impl signal.soundsource.declarative]
    #[facet(default)]
    pub amp_env: Vec<EnvSegmentSpec>,
    /// The volume envelope as Kontakt's control-rate AHDSR
    /// ([`AmpAhdsrSpec`]); it replaces `amp_env` when set, with
    /// `amp_env_depth(s)` as before.
    #[facet(default)]
    pub amp_ahdsr: Option<AmpAhdsrSpec>,
    /// Kontakt per-group CC volume tables, decoded from the instrument
    /// (`groups.json` `CC_VOLUME` mods). A dynamic layer's gain is the
    /// PRODUCT of its curves at the current CC values — this replaces the
    /// equal-power CC1/CC2 crossfades and `cc1_expression` for any
    /// articulation that declares it. Empty = the legacy crossfade.
    #[facet(default)]
    pub group_curves: Vec<GroupCurveSpec>,
    /// Kontakt group insert filters and EQs per dynamic layer, decoded from
    /// the instrument (`group_fx.json`), with the controller routes that
    /// move them. Each voice of the layer runs its own chain. Empty = none.
    #[facet(default)]
    pub group_fx: Vec<GroupFxSpec>,
    /// A fixed tuning per played key, cents (128 entries; empty = none):
    /// Kontakt's key-position → pitch table on the articulation's group (CSB
    /// shorts nudge single keys by up to ~20 cents).
    #[facet(default)]
    pub key_pitch_cents: Vec<f32>,
    /// A second amplitude envelope multiplied with `amp_env`, always one-shot
    /// (runs through, no hold) — Kontakt's group ENV_AHDSR running alongside
    /// its ENV_FLEX. CSS release groups: 250 ms attack, 25 s decay to 0.25.
    #[facet(default)]
    pub amp_env2: Vec<EnvSegmentSpec>,
    /// A pitch envelope, one-shot from the note's start: each level (0..1)
    /// times `pitch_env_cents` detunes the voice — Kontakt's group
    /// ENV_AHDSR routed to pitch (attack, hold, decay to sustain as
    /// segments). CSW marcato: +~10 c (intensity 0.008 × 12 st), held
    /// 333 ms, decaying 333 ms to 0. Empty = none.
    #[facet(default)]
    pub pitch_env: Vec<EnvSegmentSpec>,
    /// Cents at the pitch envelope's level 1 (see `pitch_env`).
    #[facet(default)]
    pub pitch_env_cents: f32,
    /// Draw the pitch envelope's depth and decay per strike instead (the
    /// last `pitch_env` segment is the decay). `None` = fixed.
    pub pitch_env_random: Option<RandomPitchEnvSpec>,
    /// Round-robin counted per NOTE-CLASS (notes sharing a sample zone share
    /// a counter), not globally — CSS marcato alternates RR 1/2 per class
    /// (`%fbn4o`, null-tested). CC59 resets every class counter.
    #[facet(default)]
    pub rr_per_note: bool,
    /// This articulation's body fade at note-off, by how long the note was
    /// held: `(held_ms, release_ms)` pairs, linear between, clamped at the
    /// ends. Empty = the instrument's `performance.release_ms`. CSS marcato:
    /// 80 ms (held < 75 ms) → 500 ms (held > 175 ms) (KSP 21723-21751).
    #[facet(default)]
    pub release_ms_by_held: Vec<[f32; 2]>,
    /// Curvature of this articulation's note-off fade (see
    /// `PerformanceSpec::release_curve_k`); `None` = the instrument's.
    pub release_curve_k: Option<f32>,
    /// Level offset (dB) on every voice of this articulation — e.g. CSS
    /// marcato releases sound 6 dB under their zone volume (null-tested).
    #[facet(default)]
    pub level_db: f32,
    /// Kontakt's short round-robin ORDER (CSS `%5y220`, `%okqzw`, …): a fixed
    /// sequence of 1-based RR numbers walked by one counter per velocity
    /// band, reset by CC59 to `CC59 mod 16`. A pick equal to the last RR that
    /// note-class played in that band advances once more. Empty = the
    /// engine's own rotation.
    #[facet(default)]
    pub rr_sequence: Vec<u8>,
    /// Random short round robins: each strike draws an RR at random and
    /// draws again while it equals one of the last `N` this KEY played (the
    /// history is per key, shared by every short type). 0 = off. CS
    /// Woodwinds / Brass: 2 (KSP `random(1,n)` against `%h5aha`/`%z0q4q`).
    #[facet(default)]
    pub rr_random_avoid_last: u8,
    /// The release sample's level by how long the note was held, per
    /// context (CSB `on release`, 17361-17849): replaces the library-wide
    /// `release_held_db` / `release_held_table` for this body's releases.
    /// `None` = the library-wide curve.
    #[facet(default)]
    pub release_by_held: Option<ReleaseByHeldSpec>,
    /// The script's start offset (ms) for this articulation's fresh note
    /// (CSW nonvib sustains: `$0kimq := 333`), which each zone's
    /// `start_mod` limits (ignored where the zone allows none). 0 = none.
    #[facet(default)]
    pub script_start_ms: f32,
    /// A short note's level by MIDI velocity, dB (128 entries): what the
    /// script sets the played group's volume to (MOJO 2: `VelGainMin +
    /// (knob − VelGainMin)·(0.99^(v−1) − 1)/(0.99^126 − 1)` in engine
    /// units, baked to dB by the generator). Replaces the velocity² and
    /// intra-band laws. Empty = those laws. On a sustained articulation it
    /// levels the body by its note's velocity (CSSS marcato:
    /// `(127 − vel) · $0kd5m / 126`).
    #[facet(default)]
    pub vel_gain_db: Vec<f32>,
    /// The keys this articulation plays, `[lo, hi]` inclusive: a note
    /// outside them plays nothing (MOJO 2's per-articulation range,
    /// `%ArtRange`). Empty = every key with a zone.
    #[facet(default)]
    pub key_range: Vec<u8>,
    /// The volume envelope's modulation intensity (Kontakt's ENV → volume
    /// route below 1: the gain is `1 − depth·(1 − env)`), and the voice ends
    /// when the envelope does. `None` = the plain envelope, voice ends as
    /// before (MOJO 2 sustain groups: 0.043).
    #[facet(default)]
    pub amp_env_depth: Option<f32>,
    /// Per dynamic layer, its own `amp_env_depth` (a library whose layers'
    /// groups carry different ENV → volume intensities: MOJO 2 Alto Sax's
    /// sustains are 1, 1, 0, 1). A dynamic not listed takes `amp_env_depth`.
    #[facet(default)]
    pub amp_env_depths: Vec<DynDepthSpec>,
    /// A script-played release sample (see [`ReleaseSampleSpec`]): at the
    /// note-off the note's voices fade out and a zone of `artic` starts in
    /// the note-on's layer. `None` = the library's release path.
    #[facet(default)]
    pub release_sample: Option<ReleaseSampleSpec>,
    /// Release types (see [`ReleaseTypesSpec`]); `release_sample` is then
    /// unused.
    #[facet(default)]
    pub release_types: Option<ReleaseTypesSpec>,
    /// At this articulation's note-on, every sounding release-sample voice
    /// fades out over this many ms (MOJO 2: `fade_out(by_marks($MARK_3),
    /// 75 ms)` on each sustain note). 0 = none.
    #[facet(default)]
    pub fade_releases_ms: f32,
    /// A script's monophonic legato (see [`ScriptLegatoSpec`]).
    #[facet(default)]
    pub script_legato: Option<ScriptLegatoSpec>,
    /// A script's key noise on this articulation's notes (see
    /// [`KeynoiseSpec`]).
    #[facet(default)]
    pub keynoise: Option<KeynoiseSpec>,
    /// A tempo-synced length (see [`TimedLengthSpec`]).
    #[facet(default)]
    pub timed_length: Option<TimedLengthSpec>,
    /// Per velocity band, `(compare, store)`: which band's last-RR memory the
    /// no-repeat rule reads and which it writes. Empty = each band its own.
    /// CSS pizzicato reads an unwritten table in bands 2 and 4 and writes
    /// band 1's (KSP 15139-15241, kept as shipped).
    #[facet(default)]
    pub rr_last_slots: Vec<[u8; 2]>,
    /// Random detune: a uniform integer −n..=n cents per note, unless the
    /// same key was played within `random_tune_gate_ms` (then 0). CSS
    /// shorts: 5 cents, 500 ms.
    #[facet(default)]
    pub random_tune_cents: u8,
    #[facet(default)]
    pub random_tune_gate_ms: f32,
    /// A body struck while another key is down detunes by a uniform integer
    /// −n..=n cents, every layer alike (CSB `$q4ous` on, `$yqdot` cents:
    /// `change_tune` when more than one key is held, KSP 11050-11071).
    /// 0 = never.
    #[facet(default)]
    pub held_random_tune_cents: u8,
    /// Controllers the script sets on a fresh note of this articulation,
    /// before its voices start (`set_controller`; CSB/CSW marcato: CC19 :=
    /// 127, KSP 11018 / 9090), as `(cc, value)`.
    #[facet(default)]
    pub note_on_cc: Vec<[u8; 2]>,
    /// A CSS marcato transition: when set, this articulation plays legato
    /// (mono line) with Kontakt's marcato two-stage transition.
    pub marcato_transition: Option<MarcatoTransitionSpec>,
    /// Tremolo / trills / harmonics legato: see [`SimpleTransitionSpec`].
    pub simple_transition: Option<SimpleTransitionSpec>,
    /// Marcato first-note overlay: see [`OverlaySpec`].
    pub overlay: Option<OverlaySpec>,
    /// A held key plays a timed phrase: see [`PhraseSpec`].
    pub phrase: Option<PhraseSpec>,
    /// Whether the envelope freezes at its hold level while the note is held
    /// (sustains) or plays through one-shot (shorts, releases, transitions).
    /// `None` = held for Sustain/Looped/Trill kinds, one-shot otherwise.
    pub amp_env_hold: Option<bool>,
    /// The `amp_env` segment a note-off jumps to (Kontakt's release stage,
    /// after the flex sustain loop). `None` = the family default (shorts:
    /// 2, their group flex `[8 ms, 604 ms hold, 7381 ms release]`).
    pub amp_env_release: Option<u32>,

    /// Which side of the CC2 vibrato crossfade this articulation belongs to.
    /// `None` = infer from the id (CSS convention: `NV`/`Nonvib` in the name
    /// = non-vibrato side; everything else vibrato).
    pub vibrato: Option<bool>,
    /// The CC2 vibrato-crossfade counterpart articulation id (e.g. `Nonvib`
    /// → `Vibsus`, `NVLeg` → `Leg`). `Some("")` = explicitly no pair;
    /// `None` = infer by name (same kind + sordino family, opposite side).
    pub vibrato_pair: Option<String>,
    /// Role of a `kind @Legato` articulation in transition selection:
    /// `"transition"` (interval move), `"retrigger"` (same-note re-bow, CSS
    /// `*zero`), or `"portamento"` (glide). Empty = infer from the id
    /// (`zero` → retrigger, `port` → portamento, else transition).
    // r[impl signal.soundsource.legato]
    #[facet(default)]
    pub legato_role: String,
    /// Whether this articulation is the Con Sordino (muted) variant.
    /// `None` = infer from the id (`Sord` prefix).
    pub sordino: Option<bool>,
    /// The Con Sordino counterpart id (both directions are looked up).
    /// `Some("")` = explicitly none; `None` = infer by the `Sord` prefix.
    pub sordino_pair: Option<String>,
}

/// Role of a legato-kind articulation in transition selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegatoRole {
    /// Interval transition (`Leg`, `NVLeg`).
    Transition,
    /// Same-note re-trigger / re-bow (`Legzero`, `NVLegzero`).
    Retrigger,
    /// Portamento glide (`Port`).
    Portamento,
}

impl ArticulationSpec {
    /// Sordino-family membership: the explicit `sordino` flag, else the CSS
    /// `Sord` id-prefix convention.
    #[must_use]
    pub fn is_sordino(&self) -> bool {
        self.sordino.unwrap_or_else(|| self.id.starts_with("Sord"))
    }

    /// Vibrato-side membership for the CC2 crossfade: the explicit
    /// `vibrato` flag, else the CSS convention (`nv`/`nonvib` in the id =
    /// non-vibrato side).
    #[must_use]
    pub fn is_vibrato(&self) -> bool {
        self.vibrato.unwrap_or_else(|| {
            let id = self.id.to_lowercase();
            !(id.contains("nv") || id.contains("nonvib"))
        })
    }

    /// Transition-selection role: the explicit `legato_role`, else inferred
    /// from the id (CSS convention: `port` → portamento, `zero` →
    /// retrigger).
    #[must_use]
    pub fn resolve_legato_role(&self) -> LegatoRole {
        match self.legato_role.to_ascii_lowercase().as_str() {
            "retrigger" => LegatoRole::Retrigger,
            "portamento" => LegatoRole::Portamento,
            "transition" => LegatoRole::Transition,
            _ => {
                let id = self.id.to_lowercase();
                if id.contains("port") {
                    LegatoRole::Portamento
                } else if id.contains("zero") {
                    LegatoRole::Retrigger
                } else {
                    LegatoRole::Transition
                }
            }
        }
    }
}

/// A release sample a script plays at the note-off (MOJO 2's sustain
/// release callback, script_1.ksp 36445-37013): the note's voices fade out
/// linearly over `fade_ms`, and a zone of `artic` — the dynamic the note-on
/// played, the next random RR — starts `start_ms` into its sample (limited by
/// the zone's start range) at `gain_db[note-on velocity]`. Its fade-in and
/// later fade-out are `artic`'s own `amp_env`.
#[derive(Debug, Clone, Facet)]
pub struct ReleaseSampleSpec {
    /// The release articulation (zones tagged with the same dynamics).
    pub artic: String,
    #[facet(default)]
    pub start_ms: f32,
    #[facet(default)]
    pub fade_ms: f32,
    /// Level by the note-on's MIDI velocity, dB (128 entries; empty = 0 dB).
    #[facet(default)]
    pub gain_db: Vec<f32>,
    /// The release voice's own envelope instead of `artic`'s `amp_env`:
    /// fade in over `fade_ms`, then fade out over this (MOJO 2: 9950 ms,
    /// `wait(RelFade)` + `fade_out`). 0 = the articulation's envelope.
    #[facet(default)]
    pub tail_ms: f32,
    /// The release plays in the note-on's dynamic (MOJO 2's normal
    /// release); otherwise in `variants`' dynamic, or any.
    #[facet(default)]
    pub layered: bool,
    /// The dynamic (zone group) by a controller: `variants[i]` from the
    /// value's place among `variant_edges`; `variant_default` until the
    /// controller is sent (MOJO 2: fall length by CC18, trill interval by
    /// CC42).
    #[facet(default)]
    pub variant_cc: Option<u8>,
    #[facet(default)]
    pub variant_edges: Vec<u8>,
    #[facet(default)]
    pub variants: Vec<String>,
    #[facet(default)]
    pub variant_default: u8,
    /// Per variant, its own `start_ms` / `fade_ms` (empty = the type's).
    #[facet(default)]
    pub variant_start_ms: Vec<f32>,
    #[facet(default)]
    pub variant_fade_ms: Vec<f32>,
    /// Keys this type plays on; outside them the first type plays (MOJO 2:
    /// the type's articulation range). Empty = every key.
    #[facet(default)]
    pub key_range: Vec<u8>,
}

/// A script's release types (MOJO 2, decode-release-types.md): which
/// release sample a note-off plays.
#[derive(Debug, Clone, Facet)]
pub struct ReleaseTypesSpec {
    /// The types, by the script's type number (`types[0]` is the normal
    /// release and the fallback).
    pub types: Vec<ReleaseSampleSpec>,
    /// `keys[i]` latches type `i` (any velocity, any articulation; it
    /// sounds nothing).
    #[facet(default)]
    pub keys: Vec<u8>,
    /// The shipped latched type.
    #[facet(default)]
    pub latched: u8,
    /// `(key, type)` pairs: a key struck at `override_velocity` or harder
    /// while notes of this articulation are held releases them all, lowest
    /// first, the first with this type, the rest with the latched one; the
    /// articulation does not change.
    #[facet(default)]
    pub overrides: Vec<u8>,
    #[facet(default)]
    pub override_velocity: u8,
    /// The release sample is silent (its round robin still drawn) when
    /// another articulation is selected at the key-up (MOJO 2's
    /// articulation cutoff).
    #[facet(default)]
    pub cutoff: bool,
}

/// One dynamic layer's envelope depth (`ArticulationSpec::amp_env_depths`).
#[derive(Debug, Clone, Facet)]
pub struct DynDepthSpec {
    pub dynamic: String,
    pub depth: f32,
}

/// A tempo-synced one-shot (MOJO 2's swells and crescendos,
/// decode-swells-crescendos.md §1): the note wants `beats` at the host
/// tempo, `desired = ⌊(60e6 / bpm_int)·beats / 1000⌋` ms with `bpm_int =
/// 60e6 / µs-per-quarter` (integers); the take whose recorded length is
/// nearest plays (ties to the longer; one longer if its speed would be
/// under `min_speed_pct`), time-stretched at `⌊rec/desired·100⌋` % so it
/// lasts the desired length at its own pitch.
#[derive(Debug, Clone, Facet)]
pub struct TimedLengthSpec {
    /// The takes' dynamics (zone groups), shortest first, and their
    /// recorded lengths in ms.
    pub takes: Vec<String>,
    pub rec_ms: Vec<i32>,
    pub min_speed_pct: i32,
    /// Beats from a controller, `cc·15/127 + 1`; `beats` until it is sent.
    pub beats_cc: u8,
    pub beats: i32,
}

/// A script's key noise (MOJO 2's keyclicks, script_1.ksp 25520-25557): on
/// a note that is not the last one it tried, it draws `random(1, 100)`
/// and, at or under `chance_pct`, plays `artic` at `key` with velocity
/// `random(1, 127)` (the zone by its velocity band), at `gain_db[note-on
/// velocity]`, fading out linearly over `fade_ms` from its start.
#[derive(Debug, Clone, Facet)]
pub struct KeynoiseSpec {
    pub artic: String,
    pub key: u8,
    pub chance_pct: i32,
    #[facet(default)]
    pub gain_db: Vec<f32>,
    pub fade_ms: f32,
    /// While this key is down (it sounds nothing), every note tries and
    /// every try clicks (the chance is still drawn): MOJO 2's `$erctx`.
    #[facet(default)]
    pub force_key: Option<u8>,
}

/// A script's ensemble: one key played by up to ten players
/// (`engine/players.rs`; MOJO 2's Ensemble, `_notes/decode-players-divisi.md`).
///
/// - `count_cc` sets the player count in bands of `count_band` (0..=band →
///   1, then one more player per band, the last band to 127); `divisi_cc`
///   switches Auto Divisi (≥ 64 on). Every message of either ends the divisi
///   voices (`reset_fade_ms`).
/// - Stacked (divisi off, or a legato on): every note plays one voice per
///   player. A second voice on one key in one callback is moved to a
///   neighbouring key inside the articulation's `key_range` (pitched back)
///   and takes the next round robin.
/// - Humanize: player p ≥ 1 has a detune (m-cents), volume (mdB) and start
///   offset (ms) drawn once, uniform in ±`*_max`, scaled by
///   `humanize`/10000 and truncated at each note.
/// - Pan: player p at `∓((p/2 + 1)·spread)/(count/2)` (even left), ‰.
/// - Every voice of the instrument: `−attenuation_mdb·(count − 1)` mdB
///   beyond the one-player level the articulations' own levels carry.
/// - Auto Divisi: notes arriving within `buffer_ms` (1 ms ticks of a counter
///   every waiting note advances) are spread over the free players, lowest
///   note first, cycling; with every player busy the newest note takes the
///   closest note's player(s) (`steal_fade_ms`).
#[derive(Debug, Clone, Facet)]
pub struct PlayersSpec {
    pub count_cc: u8,
    pub divisi_cc: u8,
    #[facet(default = 12)]
    pub count_band: u8,
    /// The shipped count and Auto Divisi switch.
    #[facet(default = 1)]
    pub count: u8,
    #[facet(default)]
    pub divisi: bool,
    /// 0..=10000 (100 %).
    pub humanize: i32,
    /// 0..=1000 (‰ of full pan).
    pub spread: i32,
    pub detune_max_mcents: i32,
    pub volume_max_mdb: i32,
    pub offset_max_ms: i32,
    pub attenuation_mdb: i32,
    pub buffer_ms: u32,
    pub steal_fade_ms: f32,
    pub reset_fade_ms: f32,
}

/// A script's monophonic legato over a one-layer sustain (MOJO 2's legato
/// engine, script_1.ksp 25570-28257 and the release callback 33710-37579;
/// `_notes/decode-legato.md`). With it on, a note within `max_interval` of
/// the line's last note plays a transition instead of a fresh attack:
///
/// - the line's voices fade out over `fade_in_ms`;
/// - a zone of `up`/`down` (direction from the last note; zone by
///   destination key and `interval`, dynamic by `layer_thresholds`) starts
///   `trans_offset_ms` in, fading in over `fade_in_ms / 2`, at
///   `gain_db[velocity]`, time-stretched by the speed law;
/// - after the hold it fades over `fade_out_ms` and, if the key is still
///   down, the sustain starts `target_offset_ms` in, fading in over
///   `fade_out_ms`.
///
/// A fresh note (no last note, or a leap past `max_interval`) also fades the
/// line's voices over `fade_in_ms` first: the line is monophonic. Releasing
/// the last note plays the sustain's release sample (and clears it); a key
/// released while the one before it is held legatos back to it.
#[derive(Debug, Clone, Facet)]
pub struct ScriptLegatoSpec {
    /// The controller that switches it (on above 63), and its shipped state.
    pub enable_cc: u8,
    #[facet(default)]
    pub on: bool,
    /// Transition articulations by direction (a repeated key goes down).
    pub up: String,
    pub down: String,
    /// The widest interval that connects (MOJO 2: 12).
    pub max_interval: u8,
    /// Curved-velocity edges of the transition's dynamics (`up`'s dynamics
    /// list, in order): below the first edge plays the first dynamic.
    #[facet(default)]
    pub layer_thresholds: Vec<u8>,
    /// The transition's level by MIDI velocity, dB (128 entries).
    #[facet(default)]
    pub gain_db: Vec<f32>,
    pub timing: ScriptLegatoTiming,
    /// A note connects only within this many ms of the last connection
    /// (MOJO 2's stab "transition bypass"); 0 = always.
    #[facet(default)]
    pub bypass_ms: f32,
    /// At the last note's key-up, the target voices fade over this many ms
    /// and the note releases as usual (stabs, 37596-37612), instead of the
    /// articulation's release sample. 0 = the release sample.
    #[facet(default)]
    pub release_fade_ms: f32,
    /// The transitions' dynamic is always this one (its index in the
    /// transition articulation's dynamics) instead of by velocity (MOJO 2's
    /// crescendo legato: `CrescLegTransDynLayer`).
    #[facet(default)]
    pub fixed_layer: Option<u8>,
    /// Octave legato: with the pedal down, a leap of more than three keys
    /// (or a fresh note) takes this set and these transitions (25595).
    #[facet(default)]
    pub octave: Option<ScriptLegatoOctave>,
    /// Natural legato, switched by a latched key (see its doc).
    #[facet(default)]
    pub natural: Option<ScriptLegatoNatural>,
}

/// Octave legato (`ScriptLegatoSpec::octave`).
#[derive(Debug, Clone, Facet)]
pub struct ScriptLegatoOctave {
    pub up: String,
    pub down: String,
    /// The pedal controller (down at 64 and above).
    pub pedal_cc: u8,
    /// Octave when `|note − last| >` this (MOJO 2: 3).
    pub min_leap: u8,
    /// Curved-velocity edges of its transitions' dynamics; empty = the first.
    #[facet(default)]
    pub layer_thresholds: Vec<u8>,
    pub timing: ScriptLegatoTiming,
}

/// Natural legato (`ScriptLegatoSpec::natural`, 26088-26100, 26460,
/// 27112-28249): the transition plays at its recorded speed and IS the
/// note (no target), at `gain_db`; after `length_ms`, the key still down
/// and the note still the line's last, it ends with the release sample
/// and the next note is fresh. With octave legato it hands over to a
/// unison tail (`tail`, offset 0, the sustain's level) instead of the
/// target, which ends the same way after `tail_ms`.
#[derive(Debug, Clone, Facet)]
pub struct ScriptLegatoNatural {
    /// The key that toggles it (it sounds nothing), and its shipped state.
    pub key: u8,
    #[facet(default)]
    pub on: bool,
    pub length_ms: f32,
    /// The transition's level by MIDI velocity, dB (128 entries).
    #[facet(default)]
    pub gain_db: Vec<f32>,
    pub tail: String,
    #[facet(default)]
    pub tail_layer_thresholds: Vec<u8>,
    pub tail_ms: f32,
}

/// The legato timing set of one kind of connection (`ScriptLegatoSpec`).
#[derive(Debug, Clone, Facet)]
pub struct ScriptLegatoTiming {
    pub fade_in_ms: f32,
    pub trans_offset_ms: f32,
    /// The hold before the target, at interval 1 and 12 (linear between, in
    /// whole ms, as the script's integer maths), scaled by 100 / speed %.
    pub hold_small_ms: f32,
    pub hold_large_ms: f32,
    pub fade_out_ms: f32,
    /// Where the target sustain enters its sample (the script's attack
    /// offset plus its target offset).
    pub target_offset_ms: f32,
    /// The transition's speed in percent at the slowest and fastest playing,
    /// for interval 1 (`small`) and 12 (`large`), linear between.
    pub slow_small_pct: i32,
    pub fast_small_pct: i32,
    pub slow_large_pct: i32,
    pub fast_large_pct: i32,
    /// Speed by the time between the line's note-ons: at or above
    /// `slowest_ms` the slow speed, at or below `fastest_ms` the fast one.
    pub slowest_ms: i32,
    pub fastest_ms: i32,
    /// Instead of that time, a controller sets the speed index (`127 − cc`);
    /// `cc_default` until it is sent.
    #[facet(default)]
    pub speed_cc: Option<u8>,
    #[facet(default)]
    pub speed_cc_default: u8,
    /// Instead, the note's velocity is the speed index.
    #[facet(default)]
    pub speed_by_velocity: bool,
}

/// High-level category for an articulation's playback behaviour.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(C)]
pub enum ArticulationKind {
    /// Held note with CC1-driven dynamics (sustain, tremolo, harmonics).
    Sustain,
    /// Short one-shot note with velocity-driven dynamics.
    Short,
    /// Legato transition sample (played when a second note is held).
    Legato,
    /// Triggered on note-off after a sustain.
    Release,
    /// Half-tone or whole-tone trill (two simultaneous notes).
    Trill,
    /// Library-specific special use (FX, col legno looped, etc.).
    Special,
    /// One-shot playback — no note-off.
    OneShot,
    /// Looped sample with CC1-driven x-fade.
    Looped,
}

// ── Legato engine ─────────────────────────────────────────────────────────────

/// Full legato engine specification.
#[derive(Debug, Clone, Facet)]
pub struct LegatoEngineSpec {
    /// Legato model. `@Css` (default): transition-over-sustain with
    /// velocity-zone pre-delays, OD curves, per-velocity retire fades, −6 dB
    /// connected-sustain trim + bloom. `@Pacific` (Performance Samples):
    /// IMMEDIATE interval-addressed transition (no delay machinery at all),
    /// destination sustain crossfades in at full level over
    /// [`destination_fade_ms`](Self::destination_fade_ms), the outgoing pair
    /// fades over [`outgoing_fade_ms`](Self::outgoing_fade_ms), and an
    /// optional [`release_overlap`](Self::release_overlap) blip of the
    /// departed note plays under each transition.
    #[facet(default)]
    pub style: LegatoStyle,
    /// Pacific: fade (ms) applied to BOTH the previous transition and the
    /// previous sustain when a new transition fires (KSP `$thaw1` = 115).
    #[facet(default = 115)]
    pub outgoing_fade_ms: u32,
    /// Pacific: crossfade-in (ms) of the destination sustain under the
    /// transition sample (KSP fade envelopes ≈ 500 ms).
    #[facet(default = 500)]
    pub destination_fade_ms: u32,
    /// Pacific: play the departed note's release articulation under each
    /// transition, faded out over `fade_ms` (KSP `legrel` groups, `$amble`
    /// = 1500 ms). `None` disables the layer.
    pub release_overlap: Option<ReleaseOverlapSpec>,
    /// Flat zones for libraries with a single legato mode (e.g. brass).
    /// When populated, `expressive` and `low_latency` are typically absent.
    #[facet(default)]
    pub zones: Vec<LegatoZoneSpec>,
    /// Expressive mode: 3 velocity zones with longer pre-delays.
    pub expressive: Option<LegatoModeSpec>,
    /// Low-latency mode: 2 velocity zones with shorter pre-delays.
    pub low_latency: Option<LegatoModeSpec>,
    /// Portamento slide configuration.
    pub portamento: Option<PortamentoSpec>,
    /// Same-note re-trigger (Legzero) configuration.
    pub retrigger: Option<RetriggerSpec>,

    /// Attack-velocity range splits (CSS `$eluxs`/`$0uhls`): velocities ≤
    /// `splits[0]` are range 1 (soft), ≤ `splits[1]` range 2, above range 3.
    /// Empty = the decoded CSS defaults `[64, 100]`.
    // r[impl signal.soundsource.legato.velocity-zones]
    #[facet(default)]
    pub velocity_splits: Vec<u8>,
    /// Body variant per loud (zone 2/3) first note, cycled by a counter
    /// (KSP `%v2aqt[$2ekvq]`): the zone velocity whose `variant_starts`
    /// entry the body starts at. CC59 sets the counter to `value mod len`.
    /// Empty = the body starts at its own `sample_start`.
    #[facet(default)]
    pub first_note_variants: Vec<u8>,
    /// Initial counter for `first_note_variants` (CSS ships `$2ekvq` = 9).
    #[facet(default)]
    pub first_note_variant_init: u8,
    /// Draw the first note's variant at random from `first_note_variants`'
    /// values instead of walking the list (CSSS: `random(2,4)`, KSP 8637-8642;
    /// CSS walks `%v2aqt`). A forced draw (`set_forced_variants`) comes first.
    #[facet(default)]
    pub first_note_variant_random: bool,
    /// Whether a loud first note layers the attack-ornament voice. CSS: no —
    /// its attack velocities (5–9) have no vibsus zones, so Kontakt plays
    /// nothing there (null test: silent for the first 38 ms).
    pub first_note_attack_voice: Option<bool>,
    /// The loud first note's attack voice (see [`FirstNoteAttackSpec`]).
    pub first_note_attack: Option<FirstNoteAttackSpec>,
    /// Silence before a loud first note's body fades in, per velocity zone
    /// `[zone 2, zone 3]` in ms. Empty = 40 ms (the KSP `wait`). Measured in
    /// Kontakt: 38.61 / 37.34 ms (the wait lands on a processing block).
    #[facet(default)]
    pub first_note_mute_ms: Vec<f32>,
    /// How long a loud first note's body takes to fade in after the mute,
    /// per velocity zone `[zone 2, zone 3]` in ms (KSP `$otlef` / `$yqhyd`,
    /// per section: 1st Violins 230 / 180, the other CSS sections 180 / 180).
    /// Empty = 230 / 180.
    #[facet(default)]
    pub first_note_fade_ms: Vec<f32>,
    /// The legato transition exactly as Kontakt runs CSS's script, measured
    /// against Kontakt renders (null −41 dB per transition). When set, a
    /// mono-legato transition on a zoned patch uses it instead of the older
    /// fitted model. See [`KontaktTransitionSpec`].
    pub kontakt_transition: Option<KontaktTransitionSpec>,
    /// Overlap-Delay curves (`legtrans_OD` / `$b0n3s`) — the wait between a
    /// live note-on and the transition firing, IOI-interpolated per legato
    /// mode and velocity range. `None` = the decoded CSS defaults.
    // r[impl signal.soundsource.legato.live]
    pub overlap_delay: Option<OverlapDelaySpec>,
    /// Transition sample-start offset curve (`$1fvjk`): how far INTO the
    /// transition recording playback begins, IOI-interpolated. `None` = the
    /// decoded CSS defaults (177 → 117 ms).
    // r[impl signal.soundsource.legato.offline]
    pub start_offset: Option<IoiCurveSpec>,
    /// Default legato crossfade (ms) — the old voice ramps out over this.
    #[facet(default = 30)]
    pub transition_fade_ms: u32,
    /// Legato-retire crossfade (ms) for the PREVIOUS transition voice,
    /// indexed by attack-velocity range (1..=3). Empty = the decoded CSS
    /// defaults `[150, 281, 281]` (`$fjtlu`/`$hbi2j`/`$2ebzd`).
    #[facet(default)]
    pub retire_transition_ms: Vec<u32>,
    /// Legato-retire crossfade (ms) for the PREVIOUS sustain voice, indexed
    /// by attack-velocity range. Empty = the decoded CSS defaults
    /// `[550, 500, 500]` (`$tdjzq`/`$3ivkj`/`$u0t23`).
    #[facet(default)]
    pub retire_sustain_ms: Vec<u32>,
    /// Trim (dB) on the legato-connected held SUSTAIN voice (CSS `$3tsb0` =
    /// −6 dB `change_vol` on `%grhcg`) — a connected note sits this far below
    /// a fresh first note.
    #[facet(default = -6.0f32)]
    pub sustain_trim_db: f32,
    /// Velocity used for the transition back to a held note when the
    /// sounding note is released (no real release-velocity exists).
    #[facet(default = 80)]
    pub fallback_velocity: u8,
    /// Declick fade (ms) for a transition voice that starts DEEP inside the
    /// recording via the start-offset curve (a Low-Latency prefire skips
    /// ~300 ms in and begins partway up the bow-change swell).
    #[facet(default = 25)]
    pub skip_declick_ms: u32,
    /// Standard-legato transition START-OFFSET bases (ms) per attack-velocity
    /// range (1..=3, split by `velocity_splits`), EXPRESSIVE mode — the real
    /// CSS `$1fvjk` selection for `$ocjln = 0` (KSP: `$fjf3c/$2p1wl/$ywj0r`,
    /// shipped `0/83/177` for 1st Violins). The soft+fast IOI boost
    /// (`legtrans_OD`, added to the OFFSET — it is not a wait) comes from
    /// [`Self::overlap_delay_ms`]. Empty = fall back to the legacy
    /// IOI-interpolated [`Self::start_offset`] curve — which the decode shows
    /// is actually CSS's MARCATO transition table (`$ocjln = 6`:
    /// `$ggt00/$v0rbb/$5exar` = 177/177→117 over IOI 100/150/500).
    #[facet(default)]
    pub lt_offset_expressive: Vec<u32>,
    /// The overlap delay deepens the transition entry only in velocity
    /// zone 1 (CSW: `$1fvjk := base + $b0n3s` in zone 1 alone); false =
    /// every zone (CSS).
    #[facet(default)]
    pub overlap_offset_zone1_only: bool,
    /// [`Self::lt_offset_expressive`] for LOW-LATENCY mode (KSP:
    /// `$ak2j4/$ixzi1/$cltif`, shipped `100/148/177`).
    #[facet(default)]
    pub lt_offset_low_latency: Vec<u32>,
    /// Slow secondary bloom on the legato-connected held sustain (CSS
    /// `%1wcdh`, `$foyeb`/`$g4dbu` ≈ 1 s): the −6 dB `sustain_trim_db`
    /// handoff level ramps back to full over this window, so a LONG
    /// connected note recovers the body of a fresh note instead of sitting
    /// 6 dB down until the next join — without it the held note's carrier
    /// has decayed by the time the next transition's pre-bow plays at slow
    /// tempi, and the incoming destination pitch reads (and sounds) early.
    /// `0` disables the bloom (the pre-model behaviour).
    #[facet(default = 1000)]
    pub sustain_bloom_ms: u32,
    /// The shape of a transition the library does not measure with
    /// `kontakt_transition`: the destination swell, the low-latency fade,
    /// the transition's start skip, portamento, the AB dip and the old
    /// body's fade. See [`TransitionShapeSpec`].
    #[facet(default)]
    pub shape: TransitionShapeSpec,
}

/// How a legato transition is shaped when the library has no
/// `kontakt_transition` (and for portamento, which never uses it). The
/// defaults are the Cinematic Studio Strings KSP values the engine used to
/// hardcode; `cinematic-strings.styx` writes them out with their KSP names.
// r[impl signal.soundsource.declarative]
#[derive(Debug, Clone, Facet)]
pub struct TransitionShapeSpec {
    /// Expressive-mode destination swell length (ms), capped at the lead.
    #[facet(default = 225)]
    pub swell_ms: u32,
    /// Share of the swell (%) in its fast first stage: a soft attack, a
    /// hard attack, and a leap over 12 semitones (whatever the attack).
    #[facet(default = 50)]
    pub swell_split_pct: u32,
    #[facet(default = 60)]
    pub swell_split_pct_hard: u32,
    #[facet(default = 50)]
    pub swell_split_pct_leap: u32,
    /// First-stage divisor by IOI (smaller = a more gradual first stage).
    #[facet(default)]
    pub swell_divisor: IoiCurveSpec,
    /// Low-latency mode's single-stage fade (ms).
    #[facet(default = 80)]
    pub low_latency_fade_ms: u32,
    /// A velocity-zone-3 transition counts as a hard attack only above this
    /// IOI (ms).
    #[facet(default = 50.0f32)]
    pub hard_ioi_ms: f32,
    /// Least skip (ms) into a transition recording, by attack and interval:
    /// `(soft ≤ 12 st, soft > 12, hard ≤ 12, hard > 12)`.
    #[facet(default)]
    pub transition_min_skip_ms: Vec<u32>,
    /// Portamento glide time (ms).
    #[facet(default = 60)]
    pub portamento_glide_ms: u32,
    /// Portamento bend depth (cents) by IOI, one curve per interval class:
    /// unison/semitone, whole tone, wider.
    #[facet(default)]
    pub portamento_bend_cents: Vec<IoiCurveSpec>,
    /// AB dip: a note within `ab_window_ms` of the last dips by `ab_dip_db`;
    /// a velocity-zone-3 attack takes `ab_zone3_db` instead.
    #[facet(default = 250)]
    pub ab_window_ms: u32,
    #[facet(default = -3.0f32)]
    pub ab_dip_db: f32,
    #[facet(default)]
    pub ab_zone3_db: f32,
    /// Fade (ms) of the outgoing body when a new body starts.
    #[facet(default = 250)]
    pub old_body_fade_ms: u32,
}

impl Default for TransitionShapeSpec {
    fn default() -> Self {
        Self {
            swell_ms: 225,
            swell_split_pct: 50,
            swell_split_pct_hard: 60,
            swell_split_pct_leap: 50,
            swell_divisor: IoiCurveSpec::default(),
            low_latency_fade_ms: 80,
            hard_ioi_ms: 50.0,
            transition_min_skip_ms: Vec::new(),
            portamento_glide_ms: 60,
            portamento_bend_cents: Vec::new(),
            ab_window_ms: 250,
            ab_dip_db: -3.0,
            ab_zone3_db: 0.0,
            old_body_fade_ms: 250,
        }
    }
}

impl TransitionShapeSpec {
    /// The swell's first-stage divisor at `ioi_ms` (100 = no extra
    /// lengthening when the library gives no curve).
    #[must_use]
    pub fn swell_divisor_at(&self, ioi_ms: f32) -> u32 {
        if self.swell_divisor.thresholds_ms.is_empty() {
            return 100;
        }
        self.swell_divisor.value_at(ioi_ms).round().max(1.0) as u32
    }

    /// Least skip into a transition recording (0 when the library gives none).
    #[must_use]
    pub fn transition_min_skip(&self, hard: bool, interval: u32) -> u32 {
        let i = usize::from(hard) * 2 + usize::from(interval > 12);
        self.transition_min_skip_ms.get(i).copied().unwrap_or(0)
    }

    /// Portamento bend depth (cents, unsigned) for an interval and IOI (0
    /// when the library gives no curves).
    #[must_use]
    pub fn portamento_bend(&self, interval: u8, ioi_ms: f32) -> f32 {
        let class = match interval {
            0 | 1 => 0,
            2 => 1,
            _ => 2,
        };
        self.portamento_bend_cents
            .get(class)
            .or_else(|| self.portamento_bend_cents.last())
            .map_or(0.0, |c| c.value_at(ioi_ms))
    }
}

impl Default for LegatoEngineSpec {
    fn default() -> Self {
        Self {
            style: LegatoStyle::default(),
            outgoing_fade_ms: 115,
            destination_fade_ms: 500,
            release_overlap: None,
            zones: Vec::new(),
            expressive: None,
            low_latency: None,
            portamento: None,
            retrigger: None,
            velocity_splits: Vec::new(),
            first_note_variants: Vec::new(),
            first_note_variant_random: false,
            first_note_variant_init: 0,
            first_note_attack_voice: None,
            first_note_attack: None,
            first_note_mute_ms: Vec::new(),
            first_note_fade_ms: Vec::new(),
            kontakt_transition: None,
            overlap_delay: None,
            start_offset: None,
            transition_fade_ms: 30,
            retire_transition_ms: Vec::new(),
            retire_sustain_ms: Vec::new(),
            sustain_trim_db: -6.0,
            fallback_velocity: 80,
            skip_declick_ms: 25,
            sustain_bloom_ms: 1000,
            shape: TransitionShapeSpec::default(),
            lt_offset_expressive: Vec::new(),
            lt_offset_low_latency: Vec::new(),
            overlap_offset_zone1_only: false,
        }
    }
}

impl LegatoEngineSpec {
    /// Get the flat mode (for single-mode libraries like brass), or fall back
    /// to the expressive mode if flat zones are absent.
    #[must_use]
    pub fn primary_mode(&self) -> Option<LegatoModeSpec> {
        if !self.zones.is_empty() {
            Some(LegatoModeSpec {
                enabled_cc58_range: None,
                zones: self.zones.clone(),
            })
        } else {
            self.expressive.clone()
        }
    }

    /// Attack-velocity range (1..=3) from [`Self::velocity_splits`]
    /// (defaults `[64, 100]` — CSS `$eluxs`/`$0uhls`).
    #[must_use]
    pub fn velocity_range(&self, vel: u8) -> u8 {
        let (s1, s2) = match self.velocity_splits.as_slice() {
            [] => (64, 100),
            [a] => (*a, 127),
            [a, b, ..] => (*a, *b),
        };
        if vel <= s1 {
            1
        } else if vel <= s2 {
            2
        } else {
            3
        }
    }

    /// Overlap-Delay (ms) before a reactive legato transition fires —
    /// IOI-interpolated per mode + velocity range from
    /// [`Self::overlap_delay`] (defaults = the decoded CSS persistent
    /// values: near-zero except soft+fast playing).
    // r[impl signal.soundsource.legato.live]
    #[must_use]
    pub fn overlap_delay_ms(&self, ioi_ms: f32, velocity: u8, expressive: bool) -> u32 {
        let default;
        let od = if let Some(od) = &self.overlap_delay {
            od
        } else {
            default = OverlapDelaySpec::default();
            &default
        };
        let mode = if expressive {
            &od.expressive
        } else {
            &od.low_latency
        };
        let curve = if self.velocity_range(velocity) == 1 {
            &mode.soft
        } else {
            &mode.loud
        };
        curve.value_at(ioi_ms).round().max(0.0) as u32
    }

    /// Transition sample-start offset (ms) — how far INTO the transition
    /// recording playback begins (`$1fvjk`), IOI-interpolated from
    /// [`Self::start_offset`] (defaults 177 → 117 ms).
    // r[impl signal.soundsource.legato.offline]
    #[must_use]
    pub fn start_offset_ms(&self, ioi_ms: f32) -> f32 {
        match &self.start_offset {
            Some(c) => c.value_at(ioi_ms),
            None => default_start_offset_curve().value_at(ioi_ms),
        }
    }

    /// STANDARD-legato transition start offset (ms) — the real CSS `$1fvjk`
    /// for `$ocjln = 0`: a base per attack-VELOCITY range and mode
    /// ([`Self::lt_offset_expressive`] / [`Self::lt_offset_low_latency`])
    /// plus the `legtrans_OD` soft+fast IOI boost, which the shipped script
    /// ADDS TO THE OFFSET (`$1fvjk := base + $b0n3s` — it is not a wait).
    /// Libraries without authored bases keep the legacy IOI curve
    /// ([`Self::start_offset_ms`]).
    #[must_use]
    pub fn lt_offset_ms(&self, ioi_ms: f32, velocity: u8, expressive: bool) -> f32 {
        let bases = if expressive {
            &self.lt_offset_expressive
        } else {
            &self.lt_offset_low_latency
        };
        if bases.is_empty() {
            return self.start_offset_ms(ioi_ms);
        }
        let vr = (self.velocity_range(velocity) - 1) as usize;
        let base = bases.get(vr.min(bases.len() - 1)).copied().unwrap_or(0) as f32;
        base + self.overlap_delay_ms(ioi_ms, velocity, expressive) as f32
    }

    /// Retire crossfades `(transition_ms, sustain_ms)` for the PREVIOUS
    /// legato pair, indexed by the attack-velocity range of the NEW note.
    #[must_use]
    pub fn retire_fades_ms(&self, velocity: u8) -> (u32, u32) {
        let vr = (self.velocity_range(velocity) - 1) as usize;
        let pick = |v: &[u32], defaults: [u32; 3]| -> u32 {
            if v.is_empty() {
                defaults[vr.min(2)]
            } else {
                v[vr.min(v.len() - 1)]
            }
        };
        (
            pick(&self.retire_transition_ms, [150, 281, 281]),
            pick(&self.retire_sustain_ms, [550, 500, 500]),
        )
    }
}

/// One legato mode (expressive or low-latency) with its velocity zones.
#[derive(Debug, Clone, Facet)]
pub struct LegatoModeSpec {
    /// CC58 range that enables this mode (e.g. `"0-5"` or `"6-10"`).
    pub enabled_cc58_range: Option<String>,
    /// Velocity → pre-delay mapping.
    #[facet(default)]
    pub zones: Vec<LegatoZoneSpec>,
}

/// One velocity zone within a legato mode.
#[derive(Debug, Clone, Facet)]
pub struct LegatoZoneSpec {
    /// `[lo, hi]` inclusive velocity range.
    pub vel_range: [u8; 2],
    /// Human label: `"slow"`, `"medium"`, `"fast"`.
    pub label: String,
    /// Pre-delay in milliseconds before the transition sample plays.
    pub delay_ms: u32,
}

impl LegatoModeSpec {
    /// Look up the pre-delay for a given MIDI velocity.
    #[must_use]
    pub fn delay_for_velocity(&self, vel: u8) -> Option<u32> {
        self.zones
            .iter()
            .find(|z| vel >= z.vel_range[0] && vel <= z.vel_range[1])
            .map(|z| z.delay_ms)
    }
}

/// Portamento slide configuration.
#[derive(Debug, Clone, Facet)]
pub struct PortamentoSpec {
    /// Maximum velocity at which portamento triggers (default 20).
    pub trigger_vel_max: u8,
    /// CC controller for portamento volume (default "CC5").
    pub volume_controller: String,
}

/// Which legato model drives transitions. See [`LegatoEngineSpec::style`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum LegatoStyle {
    /// CSS-family transition-over-sustain with velocity-zone pre-delays.
    #[default]
    Css,
    /// Performance Samples: immediate interval-addressed transitions with a
    /// full-level destination sustain and optional release-overlap layer.
    Pacific,
}

/// Pacific release-overlap layer — the departed note's release articulation
/// plays under each transition, faded out over `fade_ms`.
#[derive(Debug, Clone, Facet)]
pub struct ReleaseOverlapSpec {
    /// Fade-out (ms) of the overlap voice (KSP `$amble` = 1500).
    #[facet(default = 1500)]
    pub fade_ms: u32,
}

/// Same-note re-trigger (re-bowing / re-tonguing) configuration.
#[derive(Debug, Clone, Facet)]
pub struct RetriggerSpec {
    /// How re-trigger is activated. `"sustain_pedal_held"` = CC64 must be on.
    pub trigger: String,
    /// Number of round robins for re-trigger samples.
    pub rr: usize,
}

// ── Short note timing ─────────────────────────────────────────────────────────

/// Pre-delay compensation for short note samples.
#[derive(Debug, Clone, Facet)]
pub struct ShortNoteTimingSpec {
    /// All short-note samples start this many ms before their "rhythmic peak."
    /// Apply a negative track delay of this amount when sequencing short notes.
    pub pre_delay_ms: u32,
}

// ── Keyswitch ─────────────────────────────────────────────────────────────────

/// Keyswitch and CC58 articulation switching configuration.
#[derive(Debug, Clone, Facet)]
pub struct KeyswitchSpec {
    /// Whether keyswitches are velocity-sensitive.
    #[facet(default)]
    pub velocity_sensitive: bool,
    /// Whether keyswitch assignments are user-configurable in the GUI.
    #[facet(default)]
    pub user_configurable: bool,

    /// CC58 value range → what that value does, written like a keyswitch
    /// note's value: `+`-joined tokens, each an `@mode` token
    /// (`@legato-low`, `@legato-expressive`, `@legato-on/off`,
    /// `@sordino-on/off`, `@overlay-on/off`, `@novib`, `@ignore`) or an
    /// articulation / keyswitch-group name (`Spiccato`, `Trills`).
    /// `"Nonvib+@legato-low"` selects Nonvib in low-latency legato. Keys are
    /// range strings like `"0-5"`, `"6-10"`, etc.
    #[facet(default)]
    pub cc58_map: HashMap<String, String>,

    /// Velocity-sensitive keyswitch NOTES (how CSS switches from a keyboard):
    /// playing one of these low notes selects an articulation / mode instead of
    /// sounding. Each note carries its own velocity → value map.
    #[facet(default)]
    pub notes: Vec<KeyswitchNote>,
}

/// One velocity-sensitive keyswitch note.
///
/// Playing `note` at a given velocity applies the mapped value — a zone
/// articulation tag (e.g. `Spiccato`) or an `@`-prefixed mode token
/// (`@legato-on`, `@sordino-off`, `@legato-expressive`).
///
/// `+` joins several, e.g. `"Leg+@legato-expressive"` (select Leg AND set).
///
/// Expressive legato.
#[derive(Debug, Clone, Facet)]
pub struct KeyswitchNote {
    /// MIDI note name (e.g. `"C0"`, `"A#0"`) — convention C0 = MIDI 12.
    pub note: String,
    /// Human-readable group name for UI (e.g. `"Sustain"`, `"Shorts"`).
    #[facet(default)]
    pub label: String,
    /// Velocity range (`"0-64"`) → value applied when struck in that range.
    pub vel_map: HashMap<String, String>,
}

impl KeyswitchNote {
    /// The value mapped to `velocity` on this keyswitch, if any.
    #[must_use]
    pub fn value_for(&self, velocity: u8) -> Option<&str> {
        for (range_str, value) in &self.vel_map {
            if let Some((lo, hi)) = parse_range(range_str) {
                if velocity >= lo && velocity <= hi {
                    return Some(value);
                }
            }
        }
        None
    }
}

impl KeyswitchSpec {
    /// Look up the function name for a given CC58 value.
    #[must_use]
    pub fn cc58_function(&self, value: u8) -> Option<&str> {
        for (range_str, function) in &self.cc58_map {
            if let Some((lo, hi)) = parse_range(range_str) {
                if value >= lo && value <= hi {
                    return Some(function);
                }
            }
        }
        None
    }

    /// The articulation / keyswitch-group token of a CC58 value (its one
    /// non-`@` token), if it names one.
    #[must_use]
    pub fn cc58_target(&self, value: u8) -> Option<&str> {
        self.cc58_function(value)?
            .split('+')
            .map(str::trim)
            .find(|t| !t.is_empty() && !t.starts_with('@'))
    }
}

// ── Latched-CC articulation selector (UACC) ──────────────────────────────────

/// Default controller for the `"uacc"` selector source: CC32, per Spitfire's
/// Universal Articulation Controller Channel convention.
pub const UACC_DEFAULT_CC: u8 = 32;

/// The published UACC standard code table (core codes), as
/// `(code, canonical name, match keywords)`.
///
/// Provenance: Spitfire's UACC v2 specification. Codes cross-checked against
/// the Spitfire support articles ("Long = 1", "Staccato = 40",
/// "26 = Legato - Muted", "52 = Short Marcato", "56 = Pizzicato",
/// "40 Generic / 41 Alternative / 42 Very short (spicc)") and the
/// Reaticulate factory banks for the Spitfire libraries (which encode the
/// same table as `cc:32,<code>` outputs). This is DATA, not law: the full
/// v2 table has more rows (FX, run/flurry families, 90+) — extend here as
/// needed, and any pack can override per articulation with an explicit
/// `uacc <code>` field. Do not renumber existing entries.
///
/// The keyword lists drive [`standard_uacc_code`]: an articulation whose
/// normalized id/label/alias equals one of the keywords gets the code.
pub const UACC_STANDARD_TABLE: &[(u8, &str, &[&str])] = &[
    (1, "Long", &["long", "sustain", "sus", "vibsus", "arco"]),
    (3, "Long Octave", &["longoctave"]),
    (6, "Long Flutter", &["flutter", "fluttertongue"]),
    (7, "Long Con Sordino", &["longconsord", "longmuted"]),
    (8, "Long Flautando", &["flautando"]),
    (9, "Long Marcato", &["longmarcato"]),
    (10, "Long Harmonics", &["harmonics", "harmonic", "harm"]),
    (11, "Long Tremolo", &["tremolo", "trem"]),
    (
        12,
        "Long Tremolo Con Sordino",
        &["tremoloconsord", "tremolomuted"],
    ),
    (13, "Long Tremolo Sul Pont", &["tremolosulpont"]),
    (17, "Long Sul Tasto", &["sultasto"]),
    (18, "Long Sul Pont", &["sulpont"]),
    (20, "Legato", &["legato", "leg"]),
    (26, "Legato Con Sordino", &["legatoconsord", "legatomuted"]),
    (31, "Legato Portamento", &["portamento", "port"]),
    (32, "Legato Fast", &["legatofast"]),
    (33, "Legato Runs", &["legatoruns", "runs"]),
    (40, "Short", &["staccato", "stac", "short"]),
    (41, "Short Alternative", &["staccatissimo", "staccatiss"]),
    (42, "Very Short", &["spiccato", "spicc"]),
    (
        47,
        "Short Con Sordino",
        &["shortconsord", "staccatoconsord", "shortmuted"],
    ),
    (
        48,
        "Short Brushed",
        &["spiccatofeathered", "feathered", "brushed"],
    ),
    (52, "Short Marcato", &["marcato", "marc"]),
    (54, "Short Sforzando", &["sforzando", "sfz"]),
    (55, "Short Bells Up", &["bellsup"]),
    (56, "Pizzicato", &["pizzicato", "pizz"]),
    (
        57,
        "Bartok Pizzicato",
        &["bartokpizz", "bartokpizzicato", "snappizzicato", "snappizz"],
    ),
    (58, "Col Legno", &["collegno", "clegno"]),
    (
        70,
        "Trill (Minor 2nd)",
        &[
            "trillminor2nd",
            "trillm2",
            "htrills",
            "halftonetrill",
            "trill",
        ],
    ),
    (
        71,
        "Trill (Major 2nd)",
        &["trillmajor2nd", "trillmaj2", "wtrills", "wholetonetrill"],
    ),
    (72, "Trill (Minor 3rd)", &["trillminor3rd", "trillm3"]),
    (73, "Trill (Major 3rd)", &["trillmajor3rd", "trillmaj3"]),
    (74, "Trill (Perfect 4th)", &["trillperfect4th", "trillp4"]),
    (
        81,
        "Tremolo Measured 150",
        &["tremolomeasured", "meastrem", "measuredtremolo"],
    ),
    (90, "FX", &["fx"]),
];

/// Normalize an articulation name for standard-table matching: lowercase,
/// alphanumerics only (`"Bartok Pizz."` → `"bartokpizz"`).
fn normalize_artic_name(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}

/// The standard UACC code for a conventionally-named articulation, matching
/// the normalized name against [`UACC_STANDARD_TABLE`] keywords.
///
/// `None` = not a standard name (the pack must author `uacc <code>` explicitly).
#[must_use]
pub fn standard_uacc_code(name: &str) -> Option<u8> {
    let n = normalize_artic_name(name);
    if n.is_empty() {
        return None;
    }
    UACC_STANDARD_TABLE
        .iter()
        .find(|(_, _, keys)| keys.contains(&n.as_str()))
        .map(|&(code, _, _)| code)
}

/// A resolved latched-CC articulation selector: the controller number and
/// the code → articulation-id map, ready for the engine (which treats it as
/// a pure mechanism — no convention names in engine code).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatchedCcSelector {
    /// Controller number carrying the selector value.
    pub cc: u8,
    /// `(code, articulation id)` — sorted by code, unique codes.
    pub map: Vec<(u8, String)>,
}

impl LatchedCcSelector {
    /// The articulation id selected by CC `value`, if any. Unknown codes
    /// select nothing (the previous latch stays).
    #[must_use]
    pub fn artic_for(&self, value: u8) -> Option<&str> {
        self.map
            .iter()
            .find(|(code, _)| *code == value)
            .map(|(_, id)| id.as_str())
    }
}

// ── Zones ────────────────────────────────────────────────────────────────────

/// One sample placed at a specific (key range × velocity range × RR slot).
///
/// Multiple zones may match the same `(note, velocity)`; the engine treats
/// them as a round-robin group and cycles by `rr_index`.
#[derive(Debug, Clone, Facet)]
pub struct ZoneSpec {
    /// Sample file path, relative to the library's `samples_root`.
    pub file: String,
    /// Lowest MIDI note in the zone (inclusive).
    pub key_min: u8,
    /// Highest MIDI note in the zone (inclusive).
    pub key_max: u8,
    /// Root MIDI note: pitch at which the sample plays back unchanged.
    pub root_key: u8,
    /// Lowest MIDI velocity in the zone (inclusive). Default 0.
    #[facet(default)]
    pub vel_min: u8,
    /// Highest MIDI velocity in the zone (inclusive). Default 127.
    #[facet(default)]
    pub vel_max: u8,
    /// Round-robin slot index (0-based). Zones with the same key/vel range
    /// but different `rr_index` form one round-robin group.
    #[facet(default)]
    pub rr_index: u32,
    /// Round-robin mode for this zone group. Empty/`"cycle"` = sequential,
    /// `"random"` = seeded pseudo-random, `"no-repeat-random"` avoids the
    /// previous slot when more than one RR slot is available.
    #[facet(default)]
    pub rr_mode: String,
    /// Per-zone gain in dB. Default 0.
    #[facet(default)]
    pub gain_db: f32,
    /// Equal-power stereo pan. -1 = left, 0 = center, +1 = right.
    #[facet(default)]
    pub pan: f32,
    /// Pitch fine-tune in cents. Default 0.
    #[facet(default)]
    pub tune_cents: f32,
    /// First sample frame to play from this zone. 0 = sample start.
    #[facet(default)]
    pub sample_start: u32,
    /// How far (frames) a script's start offset may move this zone's start
    /// (Kontakt's sample-start modulation range, `start_mod` in the decode):
    /// `-1` = not at all, the offset is ignored. `None` = no limit.
    #[facet(default)]
    pub start_mod: Option<i64>,
    /// The zone's group plays through Kontakt's Time Machine II (source
    /// mode 5, `engine::tm2`): its sample is cut at pitch marks found with
    /// this analysis block (frames: 512, 1024 or 2048, by the source
    /// record's analysis setting 1-2, 3, 4), and every voice of it plays
    /// grain by grain at its speed, unity included. 0 = not a Time Machine
    /// II zone.
    #[facet(default)]
    pub tm2_block: u32,
    /// Kontakt's source-module Tracking off: the zone plays at its own
    /// pitch on every key it spans (Shreddage's rake noise). Default false.
    #[facet(default)]
    pub fixed_pitch: bool,
    /// Kontakt's velocity-2.. zones over this same file (index 0 = velocity
    /// 2): the scripted body VARIANTS — the same recording entered deeper.
    /// CSS plays one for a loud first note (`%v2aqt`), see
    /// `LegatoEngineSpec::first_note_variants`. Empty = no variants.
    #[facet(default)]
    pub variant_starts: Vec<u32>,
    /// A zone the library ships but the instrument never maps (CSS Cellos
    /// spiccato C1…: Kontakt plays its A#1… zones there). Never played.
    #[facet(default)]
    pub disabled: bool,
    /// A zone the NKI maps into ANOTHER articulation's group (CSS Violas'
    /// spiccato bands play Staccatissimo recordings; 1st Violins layers a
    /// quiet Staccatissimo under its top Spiccato notes). Written by
    /// `zones sync-tsv` from the decoded group column, regenerated each sync.
    #[facet(default)]
    pub borrowed: bool,
    /// A transition zone's MARCATO entry (CSS `marc leg` zones over this same
    /// file): the sample frame those zones start at (0 = none) …
    #[facet(default)]
    pub marcato_start: u32,
    /// … and their levels in zone order, dB: `(f fff)` on a Leg ff file (the
    /// f and fff marcato layers both play it), `(mf)` on a Leg mf file.
    #[facet(default)]
    pub marcato_gains_db: Vec<f32>,
    /// One-past-last sample frame to play from this zone. 0 = sample end.
    #[facet(default)]
    pub sample_end: u32,
    /// Forward sustain-loop start frame. 0 with `loop_end == 0` = no loop.
    #[facet(default)]
    pub loop_start: u32,
    /// Forward sustain-loop end frame (one-past-last). 0 = no loop.
    #[facet(default)]
    pub loop_end: u32,
    /// Loop crossfade length in frames — the tail before `loop_end` blended
    /// into the region before `loop_start` for a seamless wrap. 0 = hard loop.
    #[facet(default)]
    pub loop_xfade: u32,
    /// Attack fade-in length in frames from `sample_start`. 0 = no fade.
    #[facet(default)]
    pub fade_in: u32,
    /// Release-portion start frame — where the note's release tail begins,
    /// for one-shot / legato release modelling and CSS-style release samples.
    /// 0 = no distinct release point.
    #[facet(default)]
    pub release_start: u32,
    /// Playback mode. Empty or `"forward"` = normal playback, `"reverse"` =
    /// play the zone window backwards, `"alternate"`/`"alternating"` =
    /// ping-pong between loop points.
    #[facet(default)]
    pub playback_mode: String,
    /// Trigger mode. Empty/`"attack"` = held until note-off; `"one-shot"`
    /// ignores note-off and plays to the end/window; `"release"`/`"key-up"`
    /// fires on note-off; `"pedal-down"` / `"pedal-up"` fires on CC64
    /// threshold crossings; `"cc"` / `"cc-threshold"` fires when `trigger_cc`
    /// crosses into `trigger_value_min..=trigger_value_max`; `"aftertouch"`
    /// fires when channel or poly aftertouch crosses into that same value range.
    #[facet(default)]
    pub trigger_mode: String,
    /// MIDI CC number for CC-threshold trigger zones. 0 disables CC matching.
    #[facet(default)]
    pub trigger_cc: u8,
    /// Lowest CC value that fires a CC-threshold trigger. Defaults to 64.
    #[facet(default)]
    pub trigger_value_min: u8,
    /// Highest CC value that fires a CC-threshold trigger. 0 means 127.
    #[facet(default)]
    pub trigger_value_max: u8,
    /// The sustain-pedal state the zone sounds in: `"up"` / `"down"`, empty
    /// for either. A piano sampled with the dampers down and lifted (Keyscape's
    /// `Pedal Up` / `Pedal Down` layers) plays one or the other at note-on.
    #[facet(default)]
    pub pedal_state: String,
    /// Microphone / output-bus identifier — references a `MicSpec.id` in the
    /// containing `LibrarySpec.mics`. Empty string means the zone is
    /// mic-agnostic (single-mic libraries / synth zones).
    ///
    /// Multi-mic libraries (drum kits, multi-position orchestral) declare
    /// each mic in `LibrarySpec.mics` and tag each zone with the matching
    /// `mic`. The engine fires the matching zone for **every active mic**
    /// at note-on; each mic is routed to its own output bus.
    ///
    /// Many zones will share the same `(key_min, key_max, vel_min, vel_max,
    /// rr_index)` and differ only by `mic` — these form a "multi-mic group".
    #[facet(default)]
    pub mic: String,
    /// Articulation identifier for percussion / multi-articulation libraries
    /// (e.g. drum kit "Hit" / "Sidestick" / "Flam"). Empty = no articulation
    /// distinction. Articulation switching is a Layer-level concern; this
    /// field just tags the source so the importer / UI can group zones.
    #[facet(default)]
    pub articulation: String,
    /// Dynamic layer label for CC1-crossfade sustains (`"ppp"`, `"p"`, `"mf"`,
    /// `"ff"`, `"fff"` …). Empty = velocity drives dynamics directly (drums,
    /// short notes).
    ///
    /// CSS / Cinematic Studio sustains use CC1 to crossfade between sampled
    /// layers; multiple zones share the same `(key, vel_range, rr, mic,
    /// articulation)` and differ only by `dynamic`. The engine uses the
    /// library's `DynamicsSpec.cc1_layers_*` ranges to pick which 1–2 zones
    /// play simultaneously and at what gain, based on current CC1.
    #[facet(default)]
    pub dynamic: String,
    /// Direction for legato transitions (`"up"` / `"down"`). Empty = not a
    /// directional transition. Maps to the `direction` discriminator the
    /// existing filename-based scanner already uses.
    #[facet(default)]
    pub direction: String,
    /// Legato transition interval in semitones (1..=12). CSS names its
    /// transition samples `<dyn>_<dir>_<NOTE>_<N>`: `NOTE` is the LOWER pitch
    /// of the pair (stored in `root_key` as sounding MIDI), `dir` says which
    /// end is the source (`up` = named→named+N, `down` = named+N→named), and
    /// `N` is this interval. 0 = not a transition (or a `Legzero` re-trigger).
    #[facet(default)]
    pub interval: u32,
    /// The string a fretted instrument's zone was recorded on, 1 = the
    /// highest-pitched string (a guitarist's numbering). 0 = not
    /// string-bound: every zone of a non-fretted library, and a fretted
    /// library's noises. The fretboard stage (`LibrarySpec::fretboard`)
    /// plays a note on one string and picks only that string's zones.
    #[facet(default)]
    pub string: u8,
    /// The pick stroke a fretted zone was recorded with: `"down"` / `"up"`.
    /// Empty = either (a sample recorded with one stroke only, or not
    /// picked). The fretboard stage alternates strokes as the script does.
    #[facet(default)]
    pub stroke: String,
    /// Measured lead-in of a legato transition sample (ms): time from sample
    /// start until the pitch leaves the source note. The document scheduler
    /// prefires the transition by this much so the pitch change lands on the
    /// destination tick; the old sounding note crossfades out underneath it.
    /// Measured per sample by the generator (median ~330 ms, spread wide) —
    /// never assumed. 0 for non-transition zones.
    #[facet(default)]
    pub lead_in_ms: f32,
    /// MEASURED heard-arrival marker (ms of sample time from `sample_start`):
    /// when this zone's note is actually HEARD after playback begins, measured
    /// from the sample audio itself (see the `measure_arrivals` tool) — never
    /// assumed, never copied from library metadata. Semantics per zone class:
    ///
    /// * legato transition (`interval > 0`): destination-pitch settle — the
    ///   moment the destination pitch takes over from the source (Goertzel
    ///   harmonic-share crossing). Supersedes `lead_in_ms` where present
    ///   (the CSS pack's `lead_in_ms` values came from library metadata that
    ///   the audio contradicts).
    /// * short / one-shot: the rhythmic peak of the attack (spectral-flux
    ///   peak) — the per-round-robin replacement for the single global
    ///   `short_note_timing.pre_delay_ms`.
    /// * re-trigger (Legzero) / fresh sustain: perceptual onset — where the
    ///   note starts speaking (spectral-flux leading edge).
    ///
    /// `0` = unmeasured (or a release zone, where arrival is meaningless):
    /// consumers fall back to `lead_in_ms` / `pre_delay_ms` / trigger-time.
    #[facet(default)]
    pub arrival_ms: f32,
    /// Logical zone group id. Used for group-level editing and for choke
    /// relationships when `off_by` references another group.
    #[facet(default)]
    pub group: String,
    /// Maximum simultaneous hits for this zone group. 0 = unlimited. This is
    /// enforced at the group/choke id level, so multi-mic zones for one hit
    /// are treated as one practical group event.
    #[facet(default)]
    pub group_polyphony: u32,
    /// Choke/exclusive group id. A zone with this set silences currently
    /// playing voices in the same choke group before it starts.
    #[facet(default)]
    pub choke_group: String,
    /// Group/choke ids this zone silences on note-on. This maps directly to
    /// DecentSampler-style `off_by` behavior for hi-hats and other mutually
    /// exclusive one-shots.
    #[facet(default)]
    pub off_by: Vec<String>,
    /// Section identifier for multi-section libraries (orchestral with
    /// 1v/2v/Va/Ce/Ba; CS Brass with French Horn/Trombone/etc; Pacific
    /// with Cello/Violin/Viola/etc). References a `SectionSpec.id`. Empty
    /// = single-section library (most synths, drums where pieces are
    /// already split per file).
    ///
    /// Multi-section libraries fold all sections into one styx, with each
    /// zone tagged. The Layer determines which section's zones to play.
    #[facet(default)]
    pub section: String,
    /// Variant identifier for libraries that ship multiple processed copies
    /// of the same drum / sample set — typically `"Mixed"` (pre-bounced /
    /// EQ'd) vs `"Unmixed"` (raw multi-mic stems).
    ///
    /// Empty = single-variant library. When non-empty, multiple zones
    /// share the same `(key, vel, RR, mic, articulation)` and differ only
    /// by `variant`; the engine picks one at patch load. Used by
    /// `GetGoodDrums` Luke Holland and Thomas Pridgen kits.
    #[facet(default)]
    pub variant: String,
}

// ── Grooves (Stylus RMX-style loops with slices) ─────────────────────────────

/// One groove loop at a fixed BPM with optional slice markers.
///
/// Maps to one `<LOOP>` in a Stylus RMX `data.xml` sidecar (one audio
/// file). The companion `<COMBOCHILD>` virtual stems can either be folded
/// into separate `GrooveSpec`s or modeled as views over the same audio
/// (engine-side decision; not represented at the spec layer).
#[derive(Debug, Clone, Facet)]
pub struct GrooveSpec {
    /// Loop file path, relative to `samples_root`. Currently AIFF for
    /// Stylus RMX; format-agnostic in principle.
    pub file: String,
    /// Native tempo in BPM. The engine time-stretches by `host_bpm /
    /// bpm` to match the host.
    pub bpm: f32,
    /// Phrase length in bars. Empty/zero = unknown; engine falls back
    /// to the audio file duration.
    #[facet(default)]
    pub bars: u8,
    /// Time signature numerator (default 4).
    #[facet(default)]
    pub time_sig_num: u8,
    /// Time signature denominator (default 4).
    #[facet(default)]
    pub time_sig_den: u8,
    /// Display name (often differs from `file` — Stylus uses spaces and
    /// stem labels in the loop name, while the audio file may use a
    /// terser stem identifier).
    #[facet(default)]
    pub label: String,
    /// Original-tempo sample positions of every slice. Each slice maps
    /// to one MIDI key starting at `slice_base_note` (default C2 = 36).
    #[facet(default)]
    pub slices: Vec<SliceMarker>,
    /// MIDI note that the first slice (`slices[0]`) maps to. Defaults
    /// to 36 (C2) — Stylus RMX's standard slice base.
    #[facet(default)]
    pub slice_base_note: u8,
    /// Optional category / mood tags.
    #[facet(default)]
    pub tags: Vec<String>,
    /// Stem name within a multi-stem suite (e.g. `"Combo"`, `"Beat"`,
    /// `"Scuba"`). Empty for single-stem grooves.
    #[facet(default)]
    pub stem: String,
    /// Suite name — groups stems together. Empty for un-grouped loops.
    #[facet(default)]
    pub suite: String,
}

/// One slice marker inside a [`GrooveSpec`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct SliceMarker {
    /// Inclusive start sample offset in original-tempo frames.
    pub begin: u32,
    /// Exclusive end sample offset in original-tempo frames.
    pub end: u32,
    /// Slice "class" — Spectrasonics's classification (0 = unmapped /
    /// generic; nonzero = authored kick/snare/etc class).
    #[facet(default)]
    pub class: u8,
    /// Spectrasonics-specific modifier flag. Treat as opaque for now.
    #[facet(default)]
    pub mod_flag: u8,
}

// ── Wavetables ───────────────────────────────────────────────────────────────

/// One wavetable file in the library — a single-cycle waveform morphing
/// across `frame_count` frames of `cycle_length` samples each.
///
/// Sample data lives in the file referenced by `file`, in raw little-endian
/// IEEE 754 32-bit float (mono) — the same format Spectrasonics' `.stmwf`
/// files and standard wavetable WAVs (Serum / Vital `clm `-tagged) use after
/// stripping their RIFF header.
#[derive(Debug, Clone, Facet)]
pub struct WavetableSpec {
    /// Wavetable file path, relative to the library's `samples_root`.
    /// Either a raw `.stmwf` (no header) or a 32-bit float WAV with a
    /// Serum-style `clm ` chunk declaring the cycle length.
    pub file: String,
    /// Number of frames (single-cycle waveforms) in the bank.
    pub frame_count: u32,
    /// Samples per frame. Almost always 2048 (Spectrasonics, Serum, Vital).
    pub cycle_length: u32,
    /// Optional human label (e.g. `"Waldorf R30 00"`).
    #[facet(default)]
    pub label: String,
    /// Optional category (e.g. `"Classic Waveforms"`, `"Analog Timbres"`).
    #[facet(default)]
    pub category: String,
    /// Per-wavetable gain in dB. Default 0.
    #[facet(default)]
    pub gain_db: f32,
}

impl ZoneSpec {
    /// The heard-arrival marker of a legato TRANSITION zone (ms of sample
    /// time): the MEASURED `arrival_ms` (destination-pitch settle from the
    /// audio itself) when present, else the metadata `lead_in_ms`. `0.0` =
    /// no marker at all (legacy zone).
    #[must_use]
    pub fn transition_arrival_ms(&self) -> f32 {
        if self.arrival_ms > 0.0 {
            self.arrival_ms
        } else {
            self.lead_in_ms
        }
    }

    /// Whether this zone contains the given `(note, velocity)`.
    #[must_use]
    pub fn contains(&self, note: u8, velocity: u8) -> bool {
        note >= self.key_min
            && note <= self.key_max
            && velocity >= self.vel_min
            && velocity <= self.vel_max
    }
}

/// Parse a range string like `"0-5"` into `(lo, hi)`.
#[must_use]
pub fn parse_range(s: &str) -> Option<(u8, u8)> {
    let (a, b) = s.split_once('-')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

/// A fretted instrument: its strings and how a note finds one.
///
/// The placement is a score per string, as Impact Soundworks' Shreddage
/// engine does it (`str___get_best_string`): every string the note fits on
/// scores `1`, plus a weight for each thing in its favour — the fret is
/// under the hand, nothing rings on the string, it is next to or the same
/// as the last string played, its last fret is this fret. The highest score
/// wins, the higher string on a tie. A winner not under the hand gives way
/// to the string whose fret is nearest the last fret played.
#[derive(Debug, Clone, Default, Facet)]
pub struct FretboardSpec {
    /// Open-string MIDI notes, string 1 (the highest) first.
    pub strings: Vec<u8>,
    /// The highest fret a note may be placed on.
    pub frets: u8,
    /// The hand's fret (a fixed hand position): the window under the hand
    /// starts here.
    #[facet(default)]
    pub hand_fret: u8,
    /// How many frets above `hand_fret` each string's hand window reaches,
    /// string 1 first.
    pub hand_widths: Vec<u8>,
    /// The placement weights.
    pub weights: FretWeightsSpec,
    /// A string's remembered last fret before it is first played (Kontakt
    /// leaves the array unset: 0). Default 0.
    #[facet(default)]
    pub fresh_last_fret: i8,
    /// Round-robin orders: the RR to play for the n-th note on one
    /// (articulation, string, fret), per RR count. An articulation with no
    /// order for its count cycles.
    #[facet(default)]
    pub rr_orders: Vec<RrOrderSpec>,
    /// When set, a position's RR counter is the slot `stride·string + fret`
    /// (0-based string), so a fret past the stride shares the next
    /// string's counter: Shreddage indexes `%s3___string_rr_matrix` as
    /// `24·string + fret` with frets 0..=24, and fret 24 of one string
    /// counts with fret 0 of the string below. 0 = one counter per
    /// (string, fret).
    #[facet(default)]
    pub rr_slot_stride: u8,
    /// Every fretted voice starts this long after its note (µs): Kontakt's
    /// Shreddage defers each note through a 1 µs phantom note and a 50 µs
    /// poly-input window. Floored to whole frames. Default 0.
    #[facet(default)]
    pub onset_delay_us: f32,
    /// Anti-repetition: sometimes play the recording two frets away,
    /// retuned. `None` = never.
    pub borrow: Option<FretBorrowSpec>,
    /// A note struck again on its own ringing string fades the old voice
    /// over this long (ms); another note on that string releases it.
    #[facet(default)]
    pub retrigger_fade_ms: f32,
    /// Articulations that open with another recording at high velocity.
    #[facet(default)]
    pub lead_ins: Vec<FretLeadInSpec>,
    /// The recording a released note leaves behind (Shreddage's pitched
    /// release). `None` = none.
    pub release_layer: Option<FretReleaseSpec>,
    /// Hammer-ons and pull-offs. `None` = none.
    pub hammer: Option<FretHammerSpec>,
    /// Slides. `None` = none.
    pub slide: Option<FretSlideSpec>,
    /// Tapping. `None` = none.
    pub tap: Option<FretTapSpec>,
    /// Articulations played by key, not placed on a string, without the
    /// poly window (Shreddage's FX: `play_note` straight from `on note`).
    #[facet(default)]
    pub free_articulations: Vec<String>,
    /// Pick strokes for picked articulations. `None` = no strokes.
    pub picking: Option<FretPickingSpec>,
    /// Doubled takes: every fretted play sounds once per track. Empty = one
    /// track, centred.
    #[facet(default)]
    pub tracks: Vec<FretTrackSpec>,
    /// Where each articulation's fretted plays start in their recordings.
    #[facet(default)]
    pub start_offsets: Vec<FretOffsetSpec>,
    /// A pick attack under every plain note. `None` = none.
    pub extra_pick: Option<FretExtraPickSpec>,
    /// A noise a let-go note leaves, besides its release recording.
    /// `None` = none.
    pub noise_release: Option<FretNoiseReleaseSpec>,
    /// The hand resets after the board falls idle. `None` = never.
    pub idle_reset: Option<FretIdleResetSpec>,
    /// Articulations chosen by the note's velocity under a keyswitch's
    /// articulation (TACT's KEY + VEL rules).
    #[facet(default)]
    pub velocity_splits: Vec<FretVelocitySplitSpec>,
    /// Random detune and start shifts on the tracks past the first.
    /// `None` = none.
    pub jitter: Option<FretJitterSpec>,
    /// A detune per position for one articulation. `None` = none.
    pub tune_table: Option<FretTuneTableSpec>,
    /// Articulations placed on only some strings, or below a fret.
    #[facet(default)]
    pub limits: Vec<FretLimitSpec>,
    /// Keys that play a side recording instead of a note.
    #[facet(default)]
    pub side_keys: Vec<FretSideKeySpec>,
}

/// A side key (see [`FretboardSpec::side_keys`]), Shreddage's fret noise
/// and wide chokes (Argent L25828-25885): a press of `key_lo..=key_hi`
/// plays, after the onset delay, once per track, `articulation`'s zone at
/// key `(counter + track) mod cycle` (`cycle` 0: the press's key −
/// `key_lo`), at its velocity gain for the press's velocity, to the end of
/// its recording. With `rr` > 0 the zone's round robin is `(counter[dir] +
/// track) mod rr`; with `alternate` its direction alternates per press
/// (down first) and keeps its own counter. `jitter` takes the second
/// track's KSP random() detune and shift. Counters step once per press.
#[derive(Debug, Clone, Default, Facet)]
pub struct FretSideKeySpec {
    pub key_lo: u8,
    pub key_hi: u8,
    pub articulation: String,
    #[facet(default)]
    pub cycle: u8,
    #[facet(default)]
    pub rr: u8,
    #[facet(default)]
    pub alternate: bool,
    #[facet(default)]
    pub jitter: bool,
    /// Each key of the range keeps its own counter (Darkwall's key FX).
    #[facet(default)]
    pub counter_per_key: bool,
}

/// A placement limit (see [`FretboardSpec::limits`]): a note of
/// `articulation` is placed only on `strings` (0-based; empty = all), at
/// `max_fret` or below; one that fits nowhere is not played (Shreddage's
/// `meta[36]` string mask and `meta[41]` fret limit, L16190).
#[derive(Debug, Clone, Default, Facet)]
pub struct FretLimitSpec {
    pub articulation: String,
    #[facet(default)]
    pub strings: Vec<u8>,
    pub max_fret: u8,
}

/// A detune table (see [`FretboardSpec::tune_table`]), Shreddage's
/// `%s3___mute_tuning` (Argent L7591, Darkwall L7474): every voice of
/// `articulation` is detuned by `values[string_stride·string +
/// fret_stride·fret + rr_stride·rr + layer_stride·layer + stroke]`
/// thousandths of a cent, with its own round robin, its velocity layer
/// (0-based) and the note's pick stroke (0 down, 1 up; counted even when
/// the articulation is not picked). Past the table: 0.
#[derive(Debug, Clone, Default, Facet)]
pub struct FretTuneTableSpec {
    pub articulation: String,
    pub string_stride: u32,
    pub fret_stride: u32,
    pub rr_stride: u32,
    pub layer_stride: u32,
    pub values: Vec<i32>,
}

/// A velocity split (see [`FretboardSpec::velocity_splits`]): a note
/// selected as `articulation` struck at `min_velocity..=max_velocity` plays
/// `plays` instead.
#[derive(Debug, Clone, Default, Facet)]
pub struct FretVelocitySplitSpec {
    pub articulation: String,
    pub min_velocity: u8,
    pub max_velocity: u8,
    pub plays: String,
}

/// Track jitter (see [`FretboardSpec::jitter`]), Shreddage's multitracking
/// (`s3___play_event`, Argent L7209-7229, L7557-7570): track `t` past the
/// first draws KSP `random(lo, hi)` for its detune (1/1000 cent), then
/// again for its start shift (µs), which accumulates over the tracks and
/// wraps at `offset_wrap_us`. Plays of `articulations` and every lead-in's
/// lead are jittered; a range of `0..=0` draws nothing.
#[derive(Debug, Clone, Default, Facet)]
pub struct FretJitterSpec {
    pub articulations: Vec<String>,
    /// Articulations whose plays draw every track's range, enabled or not
    /// (Shreddage's trigger types 3 and 4, L7374-7389).
    #[facet(default)]
    pub all_tracks: Vec<String>,
    /// The draw range per track, the first track first.
    pub ranges: Vec<FretJitterRangeSpec>,
    pub offset_wrap_us: i32,
}

/// One track's jitter range (see [`FretJitterSpec`]).
#[derive(Debug, Clone, Default, Facet)]
pub struct FretJitterRangeSpec {
    pub lo: i32,
    pub hi: i32,
}

/// The idle hand reset (see [`FretboardSpec::idle_reset`]), Shreddage's
/// `s3__engine__reset_hand` from its timer listener: once a key-up leaves no
/// note held, the first timer tick (every `tick_ms` from the start) at least
/// `after_ms` later forgets the last string and fret and every string's last
/// fret (−1); the pick stroke's own memory stays.
#[derive(Debug, Clone, Default, Facet)]
pub struct FretIdleResetSpec {
    pub after_ms: f32,
    pub tick_ms: f32,
}

/// Pick strokes (see [`FretboardSpec::picking`]), economy picking as
/// Shreddage 3.5 plays it (`$ncb__pick_dir`, Argent script_0.ksp
/// L16196-16219): the first note after a reset is a down stroke; a note on
/// a lower string (higher index) than the last note is an up stroke, on a
/// higher string a down stroke, on the same string the stroke after the
/// last one. Every placed note moves the stroke on, picked or not. A picked
/// articulation plays the zones of its stroke (`ZoneSpec::stroke`) and
/// counts its round robins per stroke.
#[derive(Debug, Clone, Default, Facet)]
pub struct FretPickingSpec {
    /// The picked articulations.
    pub articulations: Vec<String>,
}

/// One take of a doubled part (see [`FretboardSpec::tracks`]): track `t`
/// plays the round robin `t` places further along the position's order,
/// at `pan` (−1 left … 1 right), and its own key ahead in the counters of
/// the side recordings (extra pick, noise release).
#[derive(Debug, Clone, Default, Facet)]
pub struct FretTrackSpec {
    pub pan: f32,
}

/// A start offset (see [`FretboardSpec::start_offsets`]).
#[derive(Debug, Clone, Default, Facet)]
pub struct FretOffsetSpec {
    pub articulation: String,
    pub ms: f32,
}

/// A pick attack (see [`FretboardSpec::extra_pick`]): every plain note
/// (not a hammer, tap or slide) first plays `articulation` at the key the
/// RR order for `rr` gives at the pick counter (+ the track), at its own
/// level, detuned `cents_per_string` per string past `from_string`
/// (0-based); the counter steps once per note.
#[derive(Debug, Clone, Default, Facet)]
pub struct FretExtraPickSpec {
    pub articulation: String,
    pub rr: u8,
    #[facet(default)]
    pub cents_per_string: f32,
    #[facet(default)]
    pub from_string: u8,
}

/// A release noise (see [`FretboardSpec::noise_release`]): a let-go note
/// not of `excluded` held `min_held_ms..=max_held_ms` plays
/// `articulation`'s zone at key `(counter + track) mod keys`, at
/// `gain_scale` × its velocity gain (dB) and its pan; the counter steps
/// once per let-go that played one.
#[derive(Debug, Clone, Default, Facet)]
pub struct FretNoiseReleaseSpec {
    pub articulation: String,
    #[facet(default)]
    pub excluded: Vec<String>,
    #[facet(default)]
    pub min_held_ms: f32,
    pub max_held_ms: f32,
    pub keys: u8,
    pub gain_scale: f32,
}

/// Tapping (see [`FretboardSpec::tap`]): every note of `articulation` is a
/// hammer — up or down by its interval from the note ringing on its string,
/// or up from an idle string — played from its own zones at its own level;
/// a tap on an idle open string plays `open_as` instead (L17065-17144,
/// L17426).
#[derive(Debug, Clone, Default, Facet)]
pub struct FretTapSpec {
    /// The tapping articulation.
    pub articulation: String,
    /// What a tap on an idle open string plays.
    pub open_as: String,
}

/// Slides (see [`FretboardSpec::slide`]): while `key` is held, a note of
/// one of `from` that finds a string ringing a note of one of `sources`
/// within `max_interval` (the nearest such string) slides there: the
/// ringing note fades over `source_fade_ms`, `articulation`'s transition
/// zone (by `direction` and `interval`) plays at the new key, fading in
/// over `transition_fade_in_ms`, and after the interval's `time_us` it
/// fades over `transition_fade_out_ms` while the new note's own
/// articulation enters `body_offset_ms` into its recording, fading in over
/// `body_fade_in_ms`, at the slide's origin's velocity layer. Both are
/// levelled by the new note's velocity gain plus `age_db_per_s` × the
/// origin's age (s, truncated to whole mdB) × the slide's place in its
/// chain. The transition takes its play's borrow draw but never borrows;
/// its round robin is its position's next without stepping the counter.
#[derive(Debug, Clone, Default, Facet)]
pub struct FretSlideSpec {
    /// The key held to slide (it sounds nothing).
    pub key: u8,
    /// The transition articulation.
    pub articulation: String,
    /// The articulations a slide can leave.
    pub sources: Vec<String>,
    /// The articulations a new note may be selected as to slide.
    pub from: Vec<String>,
    /// The widest slide (semitones).
    pub max_interval: u8,
    /// The transition's length by interval: index 0 = 1 semitone (µs).
    pub time_us: Vec<u32>,
    pub source_fade_ms: f32,
    pub transition_fade_in_ms: f32,
    pub transition_fade_out_ms: f32,
    pub body_offset_ms: f32,
    pub body_fade_in_ms: f32,
    /// Level change per second of the origin's age, per chain step (dB).
    #[facet(default)]
    pub age_db_per_s: f32,
    /// A slide as a chain of one-fret stitches instead of one transition
    /// (Shreddage 3.5 Argent/Darkwall). `None` = one transition.
    pub stitched: Option<FretStitchSpec>,
}

/// A stitched slide (see [`FretSlideSpec::stitched`]), Shreddage 3.5's
/// `sti` engine (Argent L16677-17168): after the source fades, a squeak
/// per track, then for each fret on the way (the destination last) the
/// transition articulation's one-fret zone into it, `direction` up or down,
/// at the slide's level plus the interval's `gain_db`; on the last step the
/// destination enters too, `dest_offset_ms` into its recording, at the
/// slide's level plus `dest_gain_db`, fading in over the interval's
/// `dest_fade_in_us`. Each step lasts the interval's `wait_us`, then its
/// stitch is released; after the last the squeak stops. A key let go or a
/// string taken mid-slide stops the squeak and the chain.
#[derive(Debug, Clone, Default, Facet)]
pub struct FretStitchSpec {
    /// Per interval (index 0 = 1 semitone).
    pub steps: Vec<FretStitchStepSpec>,
    /// The destination's start: `dest_offset_ms` plus its articulation's
    /// start offset, at most `dest_offset_max_ms` (Kontakt clamps a
    /// `play_note` offset below `meta[6]·100` µs).
    pub dest_offset_ms: f32,
    pub dest_offset_max_ms: f32,
    pub dest_gain_db: f32,
    /// The squeak at the slide's start. `None` = none.
    pub squeak: Option<FretSqueakSpec>,
    /// The last stitch is the destination: no destination play, and the
    /// stitch rings on until the note is let go (Darkwall, whose slide
    /// recordings are full-length sustains; engine-model §6).
    #[facet(default)]
    pub keep_last: bool,
    /// The slide-in key (Shreddage's "Slide In/Out" note): while it is held,
    /// a note of a slide source on an idle string above fret 0 slides up
    /// from the open string, one stitch per fret, and always ends on the
    /// destination (L17301-17769). `None` = no slide-in.
    pub slide_in_key: Option<u8>,
    /// Pressed while notes ring, the slide-in key slides each string down
    /// by its fret, at most `slide_out_max` frets (a stitched slide whose
    /// key stays the one that played it); a string at fret 0 is cut over
    /// `open_cut_ms` (Darkwall L23548-23660). 0 = no slide-out.
    #[facet(default)]
    pub slide_out_max: u8,
    #[facet(default)]
    pub open_cut_ms: f32,
}

/// One interval's stitch timing and levels (see [`FretStitchSpec`]).
#[derive(Debug, Clone, Default, Facet)]
pub struct FretStitchStepSpec {
    pub wait_us: u32,
    pub gain_db: f32,
    pub dest_fade_in_us: u32,
}

/// A slide's fret squeak (see [`FretStitchSpec::squeak`]): `articulation`'s
/// zone on the string at key `(counter + track) mod keys` (a counter per
/// string), at `gain_db` plus `vel_gain_from`'s velocity gain at the
/// chain's first velocity, plus the slide's age change.
#[derive(Debug, Clone, Default, Facet)]
pub struct FretSqueakSpec {
    pub articulation: String,
    pub keys: u8,
    pub gain_db: f32,
    pub vel_gain_from: String,
}

/// A release recording (see [`FretboardSpec::release_layer`]): when a
/// note of one of `sources` is let go (its key comes up, or another note
/// takes its string) after `min_held_ms..=max_held_ms`, `articulation`
/// plays on its string at the key the note's recording played, at the
/// note's velocity gain. Its round robin is its position's next (the RR
/// orders), and the position's counter steps only on a fretted string
/// unless `open_string_counts` (Shreddage: `global_var__pitched_rel_fret > 0`).
#[derive(Debug, Clone, Default, Facet)]
pub struct FretReleaseSpec {
    /// The release articulation.
    pub articulation: String,
    /// The articulations whose notes leave it.
    pub sources: Vec<String>,
    /// The shortest held time that leaves it (ms).
    #[facet(default)]
    pub min_held_ms: f32,
    /// The longest held time that leaves it (ms).
    pub max_held_ms: f32,
    /// Whether an open string's release steps its position's counter.
    #[facet(default)]
    pub open_string_counts: bool,
    /// Sources that leave another release recording instead. It reads its
    /// own position counter, which never steps: the step always goes to
    /// `articulation`'s (Shreddage's power chords, L5189-5192, L5550).
    #[facet(default)]
    pub mapped: Vec<FretReleaseMapSpec>,
}

/// A release mapping (see [`FretReleaseSpec::mapped`]).
#[derive(Debug, Clone, Default, Facet)]
pub struct FretReleaseMapSpec {
    pub sources: Vec<String>,
    pub articulation: String,
}

/// Hammer-ons and pull-offs (see [`FretboardSpec::hammer`]): a note of one
/// of `from` at `max_velocity` or below that lands on a string still
/// ringing a note of one of `sources`, within `range` frets of it and not
/// the same note, plays `articulation` instead (`direction` up for a
/// hammer-on, down for a pull-off), `gain_db` under its velocity gain; the
/// ringing note is let go.
#[derive(Debug, Clone, Default, Facet)]
pub struct FretHammerSpec {
    /// The hammer/pull articulation.
    pub articulation: String,
    /// The articulations a hammer can follow.
    pub sources: Vec<String>,
    /// The articulations a new note may be selected as to become a hammer.
    pub from: Vec<String>,
    /// The loudest velocity that hammers.
    pub max_velocity: u8,
    /// The widest interval (frets) a hammer spans.
    pub range: u8,
    /// Level offset on a hammer (dB).
    #[facet(default)]
    pub gain_db: f32,
    /// Force same string (Shreddage `[28]`): a lone note whose string is
    /// idle moves to the ringing string within `range` it fits on (the
    /// nearest; a tie to the higher string) and plays there — a hammer
    /// when what rings is a source, else a fresh note that releases it.
    #[facet(default)]
    pub force: bool,
}

/// A lead-in (see [`FretboardSpec::lead_ins`]): `articulation` played at
/// `min_velocity` or above first plays `lead` (Shreddage's pop); after
/// `body_delay_ms`, if the key still holds its string, the lead fades over
/// `lead_fade_ms` and `articulation` enters `body_offset_ms` into its
/// recording. Released sooner, the lead alone sounds. The lead's round
/// robin is a borrow-generator draw mod its RR count, taken after a borrow
/// draw the lead never uses; with `draw_modulus`, the draw is first taken
/// mod it, and track `t` plays `(draw + t) mod` its RR count (Argent's
/// rake: `(draw mod 10 + t) mod 4`, L7207-7216). Each track draws its own.
#[derive(Debug, Clone, Default, Facet)]
pub struct FretLeadInSpec {
    /// The articulation that opens with a lead.
    pub articulation: String,
    /// The lowest velocity that plays the lead.
    pub min_velocity: u8,
    /// The loudest velocity that plays the lead (`None` = 127).
    #[facet(default)]
    pub max_velocity: Option<u8>,
    /// The lead articulation.
    pub lead: String,
    /// The articulation the body plays (`None` = `articulation`).
    #[facet(default)]
    pub body: Option<String>,
    /// On a string past this (0-based) the note plays `body` plain, with no
    /// lead (Argent L16193). `None` = every string leads.
    #[facet(default)]
    pub max_string: Option<u8>,
    /// From the lead's start to the body's (ms).
    pub body_delay_ms: f32,
    /// The lead's fade when the body enters (ms; `None` = it rings on).
    #[facet(default)]
    pub lead_fade_ms: Option<f32>,
    /// Where in its recording the body starts (ms).
    pub body_offset_ms: f32,
    /// The lead's draw is taken mod this first (0 = not).
    #[facet(default)]
    pub draw_modulus: u32,
    /// The body is levelled by the lead's velocity gain, not its own
    /// (Argent's rake passes Rake's Vel→Vol to the Sustain, L17343).
    #[facet(default)]
    pub body_lead_gain: bool,
}

/// [`FretboardSpec`] placement weights (each added to a string's score).
#[derive(Debug, Clone, Default, Facet)]
pub struct FretWeightsSpec {
    /// The fret is inside the hand window.
    pub in_hand: u32,
    /// Nothing is ringing on the string.
    pub free: u32,
    /// The string is next to the last string played.
    pub adjacent: u32,
    /// The string is the last string played.
    pub same_string: u32,
    /// The string's last fret is this fret.
    pub same_fret: u32,
}

/// One round-robin order (see [`FretboardSpec::rr_orders`]).
#[derive(Debug, Clone, Default, Facet)]
pub struct RrOrderSpec {
    /// The articulation's RR count this order serves.
    pub rr: u8,
    /// The RR index to play, by how many notes the position has played.
    pub order: Vec<u8>,
}

/// Anti-repetition by a neighbouring recording (see
/// [`FretboardSpec::borrow`]). A 32-bit LCG `s = a·s + 1` draws
/// `((s >> 8) & 0xFFFFFF) mod 1000 + 1` once per note; a draw at or under
/// `chance_permille` plays the zone `frets` higher on the same string (lower
/// when that is past the neck), retuned to the played pitch.
#[derive(Debug, Clone, Default, Facet)]
pub struct FretBorrowSpec {
    /// Per-mille chance a note borrows.
    pub chance_permille: u32,
    /// The generator's seed (its state when the instrument loads).
    pub seed: u32,
    /// The generator's multiplier.
    pub multiplier: u32,
    /// How far away the borrowed recording is.
    pub frets: u8,
}
