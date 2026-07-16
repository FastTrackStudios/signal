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
    pub fn from_styx(s: &str) -> Result<Self, SamplerError> {
        facet_styx::from_str(s).map_err(|e| SamplerError::SpecParse(e.to_string()))
    }

    /// Parse from TOML format.
    pub fn from_toml(s: &str) -> Result<Self, SamplerError> {
        facet_toml::from_str(s).map_err(|e| SamplerError::SpecParse(e.to_string()))
    }

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
    pub fn articulation(&self, id: &str) -> Option<&ArticulationSpec> {
        self.articulations.iter().find(|a| a.id == id)
    }

    /// The Con Sordino counterpart of `artic_id` when toggling sordino
    /// `active`: the articulation's explicit `sordino_pair` when authored
    /// (`""` = none), else the CSS `Sord` id-prefix convention — in both
    /// cases only if the counterpart exists in the spec.
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
            .filter(|c| {
                matches!(
                    c.kind,
                    ArticulationKind::Sustain | ArticulationKind::Legato
                )
            })
            .find(|c| c.is_sordino() == is_sord && c.is_vibrato() != is_vib)
            .map(|c| c.id.clone())
    }

    /// The legato-engine configuration, falling back to an all-defaults
    /// [`LegatoEngineSpec`] when the library declares none — so engine code
    /// can read timing/crossfade policy unconditionally (the defaults are
    /// the historical hardcoded values).
    pub fn legato_cfg(&self) -> &LegatoEngineSpec {
        static DEFAULT: std::sync::OnceLock<LegatoEngineSpec> = std::sync::OnceLock::new();
        self.legato_engine
            .as_ref()
            .unwrap_or_else(|| DEFAULT.get_or_init(LegatoEngineSpec::default))
    }

    /// Look up a section by its `id` field.
    pub fn section(&self, id: &str) -> Option<&SectionSpec> {
        self.sections.iter().find(|s| s.id == id)
    }

    /// Look up a mic by its `id` field.
    pub fn mic(&self, id: &str) -> Option<&MicSpec> {
        self.mics.iter().find(|m| m.id == id)
    }

    /// Median measured lead-in (ms) across the legato transition zones that
    /// match a `from → to` move — the document scheduler prefires the
    /// transition by exactly this so the pitch change lands on the
    /// destination tick.
    ///
    /// Matching mirrors the engine's transition selection: direction from
    /// `sign(to - from)`, named note `min(from, to)`, interval
    /// `|to - from|` clamped to an octave (CSS samples nothing wider; the
    /// engine octave-clamps and pitch-shifts so the destination stays
    /// exact). Zones are matched at the nearest recorded root (whole-tone
    /// grid). `Some(0)` for a same-pitch re-bow — the `Legzero` re-trigger
    /// samples have no lead-in. `None` when the library has no measured
    /// transition zones (caller falls back to the velocity curve).
    pub fn legato_lead_ms(&self, from: u8, to: u8) -> Option<u32> {
        if from == to {
            // Same-pitch re-bow: the Legzero re-trigger samples have no
            // lead-in — but only claim that for libraries that actually
            // carry measured transitions; legacy libraries keep the
            // velocity-curve behaviour.
            return self.has_measured_legato().then_some(0);
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
                    && z.lead_in_ms > 0.0
                    && is_legato_artic(&z.articulation)
            })
        };
        // Nearest recorded root first, then the median lead of that group.
        let best_dist = candidates().map(|z| z.root_key.abs_diff(named)).min()?;
        let mut leads: Vec<f32> = candidates()
            .filter(|z| z.root_key.abs_diff(named) == best_dist)
            .map(|z| z.lead_in_ms)
            .collect();
        leads.sort_by(|a, b| a.total_cmp(b));
        Some(leads[leads.len() / 2].round() as u32)
    }

    /// Whether any zone carries a measured legato transition (interval +
    /// lead-in written by the sample-collector generator). Gates the
    /// measured-lead prefire alignment; without it the engine and scheduler
    /// keep the legacy velocity-curve behaviour.
    pub fn has_measured_legato(&self) -> bool {
        self.zones
            .iter()
            .any(|z| z.interval > 0 && z.lead_in_ms > 0.0)
    }

    /// Materialize a [`TagSet`] from the flat `tags` vector.
    ///
    /// The collection browser consumes `TagSet`; the spec stores tags as a
    /// `Vec` purely for facet-styx round-trip ergonomics.
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
/// crossfading). Every default equals the value the engine hardcoded before
/// this block existed, so a spec that omits `performance` plays identically.
///
/// The non-zero defaults were decoded from Cinematic Studio Strings (KSP
/// persistent values / GroupList envelopes) but apply harmlessly to any
/// zoned library; a library that needs different numbers writes them here.
// r[impl signal.soundsource.declarative]
#[derive(Debug, Clone, Facet)]
pub struct PerformanceSpec {
    /// Held-sustain note-off overlap fade (ms) for zoned sustains (CSS
    /// `$tukcw` = 400): on key-up the looping sustain fades over this window.
    #[facet(default = 400)]
    pub sustain_noteoff_ms: u32,
    /// Output makeup (dB) applied to looping sustain-layer voices — the flat
    /// level offset between a looped-plateau playback and the vendor
    /// instrument's rendered level (CSS: +6 dB, see the A/B calibration).
    #[facet(default = 6.0f32)]
    pub sustain_makeup_db: f32,
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
    /// Linear gain for recorded release-tail voices (the release ENV_FLEX
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
}

impl Default for PerformanceSpec {
    fn default() -> Self {
        Self {
            sustain_noteoff_ms: 400,
            sustain_makeup_db: 6.0,
            master_tune_cents: 0.0,
            loop_xfade_ms: 150,
            zone_pitch_tolerance: 2,
            release_gain: 1.0,
            attack_ms: None,
            release_ms: None,
        }
    }
}

/// One decoded amp-envelope segment `(time_ms, level, curve)` — the literal
/// ENV_FLEX representation (segment 0 is the attack). See
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

/// A piecewise-linear curve over the inter-onset interval (IOI, ms): below
/// `thresholds_ms[0]` → `anchors_ms[0]`, above the last threshold → the last
/// anchor, linear between. Used for the legato Overlap-Delay and the
/// transition sample-start offset.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct IoiCurveSpec {
    /// Ascending IOI breakpoints (ms).
    pub thresholds_ms: Vec<f32>,
    /// Anchor values (ms) at each breakpoint; same length as `thresholds_ms`.
    pub anchors_ms: Vec<f32>,
}

impl IoiCurveSpec {
    /// Piecewise-linear interpolation of `ioi_ms` across the breakpoints.
    pub fn value_at(&self, ioi_ms: f32) -> f32 {
        let n = self.thresholds_ms.len().min(self.anchors_ms.len());
        if n == 0 {
            return 0.0;
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

/// Overlap-Delay curves for one legato mode: how long the engine waits after
/// a note-on before firing the transition, interpolated over the IOI. `soft`
/// applies to attack-velocity range 1 (≤ the first
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
    /// The decoded CSS persistent values (`CSS 1st Violins.nki` BParScript
    /// store): LL thresholds `$deey3/$fxiox/$jystg/$zvaet`, EX thresholds
    /// `$g45yq/$bwkdm/$waq1e/$whtm2`; soft anchors `$nbkqa…` / `$kadcz…`;
    /// loud anchors all-zero.
    fn default() -> Self {
        let z4 = vec![0.0, 0.0, 0.0, 0.0];
        Self {
            low_latency: OverlapDelayCurveSpec {
                soft: IoiCurveSpec {
                    thresholds_ms: vec![75.0, 100.0, 800.0, 1100.0],
                    anchors_ms: vec![77.0, 0.0, 0.0, 0.0],
                },
                loud: IoiCurveSpec {
                    thresholds_ms: vec![75.0, 100.0, 800.0, 1100.0],
                    anchors_ms: z4.clone(),
                },
            },
            expressive: OverlapDelayCurveSpec {
                soft: IoiCurveSpec {
                    thresholds_ms: vec![200.0, 300.0, 800.0, 800.0],
                    anchors_ms: vec![83.0, 0.0, 0.0, 0.0],
                },
                loud: IoiCurveSpec {
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
        thresholds_ms: vec![100.0, 150.0, 500.0],
        anchors_ms: vec![177.0, 177.0, 117.0],
    }
}

/// CC1 → loudness expression curve for CC1-crossfaded sustains: 0 dB at and
/// above `knee`, linear to `floor_db` at CC1=0. The per-layer crossfade
/// handles TIMBRE (≈flat total level); this supplies only the gentle bottom
/// rolloff. Defaults calibrated on the CSS reference render (CC1=20 →
/// −3.0 dB, flat from CC1≈45).
#[derive(Debug, Clone, Copy, PartialEq, Facet)]
pub struct Cc1ExpressionSpec {
    #[facet(default = 45)]
    pub knee: u8,
    #[facet(default = -5.4f32)]
    pub floor_db: f32,
}

impl Default for Cc1ExpressionSpec {
    fn default() -> Self {
        Self {
            knee: 45,
            floor_db: -5.4,
        }
    }
}

impl DynamicsSpec {
    /// CC1 → linear loudness gain from [`DynamicsSpec::cc1_expression`]
    /// (falling back to the CSS-calibrated defaults): 0 dB at/above the knee,
    /// linear to `floor_db` at CC1=0.
    pub fn cc1_expression_gain(&self, cc1: u8) -> f32 {
        let c = self.cc1_expression.unwrap_or_default();
        let db = if cc1 >= c.knee || c.knee == 0 {
            0.0
        } else {
            c.floor_db * (c.knee - cc1) as f32 / c.knee as f32
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
pub fn default_amp_env(artic_id: &str, role: AmpEnvRole) -> Option<(&'static [EnvSegmentSpec], bool)> {
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

/// Find + parse a `STINFO=` entry inside a FLAC VORBIS_COMMENT block body
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
        let Ok(s) = std::str::from_utf8(c) else { continue };
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
        rr_mode: (seq_length > 0)
            .then(|| "cycle".to_string())
            .unwrap_or_default(),
        gain_db: sfz_f32(group, region, "volume").unwrap_or(0.0),
        pan: (sfz_f32(group, region, "pan").unwrap_or(0.0) / 100.0).clamp(-1.0, 1.0),
        tune_cents: tune + transpose,
        sample_start: sfz_u32(group, region, "offset").unwrap_or(0),
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
        mic: String::new(),
        articulation: String::new(),
        dynamic: String::new(),
        direction: String::new(),
        interval: 0,
        lead_in_ms: 0.0,
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

/// Dynamic control model for the library.
#[derive(Debug, Clone, Default, Facet)]
pub struct DynamicsSpec {
    /// Controller for long-note dynamics. `"CC1"` for most libraries.
    pub sustain_controller: Option<String>,
    /// Controller for vibrato crossfade. `"CC2"` for strings.
    pub vibrato_controller: Option<String>,
    /// `"crossfade"` or `"on_off"` (solo strings: vibrato is binary).
    pub vibrato_mode: Option<String>,
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

    /// Decoded per-articulation amp envelope (ENV_FLEX segments; segment 0
    /// is the attack). Empty = fall back to the built-in family defaults
    /// ([`default_amp_env`], keyed by articulation kind/id), which preserve
    /// the historical behaviour for libraries that don't author envelopes.
    // r[impl signal.soundsource.declarative]
    #[facet(default)]
    pub amp_env: Vec<EnvSegmentSpec>,
    /// Whether the envelope freezes at its hold level while the note is held
    /// (sustains) or plays through one-shot (shorts, releases, transitions).
    /// `None` = held for Sustain/Looped/Trill kinds, one-shot otherwise.
    pub amp_env_hold: Option<bool>,

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
    pub fn is_sordino(&self) -> bool {
        self.sordino.unwrap_or_else(|| self.id.starts_with("Sord"))
    }

    /// Vibrato-side membership for the CC2 crossfade: the explicit
    /// `vibrato` flag, else the CSS convention (`nv`/`nonvib` in the id =
    /// non-vibrato side).
    pub fn is_vibrato(&self) -> bool {
        self.vibrato.unwrap_or_else(|| {
            let id = self.id.to_lowercase();
            !(id.contains("nv") || id.contains("nonvib"))
        })
    }

    /// Transition-selection role: the explicit `legato_role`, else inferred
    /// from the id (CSS convention: `port` → portamento, `zero` →
    /// retrigger).
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
}

impl Default for LegatoEngineSpec {
    fn default() -> Self {
        Self {
            zones: Vec::new(),
            expressive: None,
            low_latency: None,
            portamento: None,
            retrigger: None,
            velocity_splits: Vec::new(),
            overlap_delay: None,
            start_offset: None,
            transition_fade_ms: 30,
            retire_transition_ms: Vec::new(),
            retire_sustain_ms: Vec::new(),
            sustain_trim_db: -6.0,
            fallback_velocity: 80,
            skip_declick_ms: 25,
        }
    }
}

impl LegatoEngineSpec {
    /// Get the flat mode (for single-mode libraries like brass), or fall back
    /// to the expressive mode if flat zones are absent.
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
    pub fn overlap_delay_ms(&self, ioi_ms: f32, velocity: u8, expressive: bool) -> u32 {
        let default;
        let od = match &self.overlap_delay {
            Some(od) => od,
            None => {
                default = OverlapDelaySpec::default();
                &default
            }
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
    pub fn start_offset_ms(&self, ioi_ms: f32) -> f32 {
        match &self.start_offset {
            Some(c) => c.value_at(ioi_ms),
            None => default_start_offset_curve().value_at(ioi_ms),
        }
    }

    /// Retire crossfades `(transition_ms, sustain_ms)` for the PREVIOUS
    /// legato pair, indexed by the attack-velocity range of the NEW note.
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

    /// CC58 value range → articulation/function label.
    /// Keys are range strings like `"0-5"`, `"6-10"`, etc.
    #[facet(default)]
    pub cc58_map: HashMap<String, String>,

    /// Velocity-sensitive keyswitch NOTES (how CSS switches from a keyboard):
    /// playing one of these low notes selects an articulation / mode instead of
    /// sounding. Each note carries its own velocity → value map.
    #[facet(default)]
    pub notes: Vec<KeyswitchNote>,
}

/// One velocity-sensitive keyswitch note: playing `note` at a given velocity
/// applies the mapped value — a zone articulation tag (e.g. `Spiccato`) or an
/// `@`-prefixed mode token (`@legato-on`, `@sordino-off`, `@legato-expressive`).
/// `+` joins several, e.g. `"Leg+@legato-expressive"` (select Leg AND set
/// expressive legato).
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
    /// Measured lead-in of a legato transition sample (ms): time from sample
    /// start until the pitch leaves the source note. The document scheduler
    /// prefires the transition by this much so the pitch change lands on the
    /// destination tick; the old sounding note crossfades out underneath it.
    /// Measured per sample by the generator (median ~330 ms, spread wide) —
    /// never assumed. 0 for non-transition zones.
    #[facet(default)]
    pub lead_in_ms: f32,
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
    /// GetGoodDrums Luke Holland and Thomas Pridgen kits.
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
    /// MIDI note that the first slice (slices[0]) maps to. Defaults
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
#[derive(Debug, Clone, Copy, PartialEq, Facet)]
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
    /// Whether this zone contains the given `(note, velocity)`.
    pub fn contains(&self, note: u8, velocity: u8) -> bool {
        note >= self.key_min
            && note <= self.key_max
            && velocity >= self.vel_min
            && velocity <= self.vel_max
    }
}

/// Parse a range string like `"0-5"` into `(lo, hi)`.
pub fn parse_range(s: &str) -> Option<(u8, u8)> {
    let (a, b) = s.split_once('-')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs_dir() -> std::path::PathBuf {
        // The CSS soundpack definition is owned by the orchestra rig crate.
        let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        std::path::Path::new(&manifest).join("../../rigs/orchestra/specs")
    }

    #[test]
    fn test_parse_range() {
        assert_eq!(parse_range("0-5"), Some((0, 5)));
        assert_eq!(parse_range("76-80"), Some((76, 80)));
        assert_eq!(parse_range("bad"), None);
    }

    #[test]
    fn load_css_spec_styx() {
        let path = specs_dir().join("cinematic-strings.styx");
        assert!(path.exists(), "CSS soundpack definition missing: {path:?}");
        let spec = LibrarySpec::from_file(&path).expect("parse CSS styx spec");
        assert_eq!(spec.sections.len(), 5);
        assert!(spec.articulations.len() > 10);
        let le = spec.legato_engine.as_ref().unwrap();
        assert_eq!(
            le.expressive.as_ref().unwrap().delay_for_velocity(30),
            Some(333)
        );
        assert_eq!(
            le.expressive.as_ref().unwrap().delay_for_velocity(80),
            Some(250)
        );
        assert_eq!(
            le.expressive.as_ref().unwrap().delay_for_velocity(110),
            Some(100)
        );
        let ks = spec.keyswitch.as_ref().unwrap();
        assert_eq!(ks.cc58_function(0), Some("Sustain: Low Latency Legato"));
        assert_eq!(ks.cc58_function(88), Some("Con Sordino On"));

        // The performance / timing model is carried by the pack, and its
        // values equal the engine's compiled-in defaults (so an old spec
        // without the blocks plays identically).
        assert_eq!(spec.performance.master_tune_cents, 9.0);
        assert_eq!(spec.performance.attack_ms, Some(198));
        assert_eq!(spec.performance.release_ms, Some(400));
        assert_eq!(spec.performance.sustain_noteoff_ms, 400);
        assert_eq!(le.velocity_splits, vec![64, 100]);
        assert_eq!(le.overlap_delay_ms(50.0, 30, false), 77); // soft+fast LL
        assert_eq!(le.overlap_delay_ms(50.0, 30, true), 83); // soft+fast EX
        assert_eq!(le.overlap_delay_ms(400.0, 30, false), 0); // slow line
        assert_eq!(le.overlap_delay_ms(50.0, 110, true), 0); // loud
        assert_eq!(le.start_offset_ms(100.0), 177.0);
        assert_eq!(le.start_offset_ms(500.0), 117.0);
        assert_eq!(le.retire_fades_ms(30), (150, 550));
        assert_eq!(le.retire_fades_ms(110), (281, 500));
        assert_eq!(le.sustain_trim_db, -6.0);
        // Defaults (spec omits nothing) equal an unset spec's resolution.
        let default = LegatoEngineSpec::default();
        assert_eq!(
            default.overlap_delay_ms(50.0, 30, false),
            le.overlap_delay_ms(50.0, 30, false)
        );
        assert_eq!(default.start_offset_ms(300.0), le.start_offset_ms(300.0));

        // Transition selection is data: roles + vibrato pairing.
        let leg = spec.articulation("Leg").unwrap();
        assert_eq!(leg.resolve_legato_role(), LegatoRole::Transition);
        assert!(leg.is_vibrato());
        assert_eq!(
            spec.vibrato_counterpart("Leg").as_deref(),
            Some("NVLeg")
        );
        assert_eq!(
            spec.articulation("Legzero").unwrap().resolve_legato_role(),
            LegatoRole::Retrigger
        );
        assert_eq!(
            spec.articulation("Port").unwrap().resolve_legato_role(),
            LegatoRole::Portamento
        );
        // Tremolo/Harmonics explicitly opt out of the CC2 pair (fixes the
        // inferred Tremolo↔Nonvib mispairing).
        assert_eq!(spec.vibrato_counterpart("Tremolo"), None);
        assert_eq!(spec.vibrato_counterpart("Harm"), None);
        assert_eq!(
            spec.vibrato_counterpart("Nonvib").as_deref(),
            Some("Vibsus")
        );

        // Amp envelopes are authored per articulation for the long families
        // and equal the decoded family defaults.
        let sus_env = &spec.articulation("Vibsus").unwrap().amp_env;
        assert_eq!(sus_env.as_slice(), AMP_ENV_SUSTAIN);
        assert_eq!(
            spec.articulation("Leg").unwrap().amp_env.as_slice(),
            AMP_ENV_LEGATO
        );
        assert_eq!(
            spec.articulation("NVrel").unwrap().amp_env.as_slice(),
            AMP_ENV_RELEASE
        );
    }

    #[test]
    fn parse_sfz_subset_to_zones() {
        let sfz = r#"
            <group> lokey=C2 hikey=C5 group=1 off_by=2
            <region> sample=Samples\Kick.wav key=C3 lovel=1 hivel=90 pitch_keycenter=C3 volume=-3 pan=-50 seq_position=2 seq_length=4 trigger=release loop_start=10 loop_end=20
            <region> sample=snare.wav lokey=60 hikey=62 transpose=1 tune=-5 offset=4 end=128
        "#;

        let spec = LibrarySpec::from_sfz(sfz).expect("parse sfz");

        assert_eq!(spec.zones.len(), 2);
        let kick = &spec.zones[0];
        assert_eq!(kick.file, "Samples/Kick.wav");
        assert_eq!(kick.key_min, 48);
        assert_eq!(kick.key_max, 48);
        assert_eq!(kick.root_key, 48);
        assert_eq!(kick.vel_min, 1);
        assert_eq!(kick.vel_max, 90);
        assert_eq!(kick.rr_index, 1);
        assert_eq!(kick.rr_mode, "cycle");
        assert_eq!(kick.trigger_mode, "release");
        assert_eq!(kick.group, "1");
        assert_eq!(kick.off_by, vec!["2"]);
        assert_eq!(kick.loop_start, 10);
        assert_eq!(kick.loop_end, 20);
        assert!((kick.pan + 0.5).abs() < f32::EPSILON);

        let snare = &spec.zones[1];
        assert_eq!(snare.key_min, 60);
        assert_eq!(snare.key_max, 62);
        assert_eq!(snare.sample_start, 4);
        assert_eq!(snare.sample_end, 128);
        assert_eq!(snare.tune_cents, 95.0);
    }
}
