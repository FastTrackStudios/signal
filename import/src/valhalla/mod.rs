//! Valhalla → FTS reverb translation.
//!
//! Valhalla's VST3 plugins store their whole patch as a plain XML element
//! inside the host chunk, with every parameter already normalized to `0..1`:
//!
//! ```xml
//! <ValhallaVintageVerb pluginVersion="4.0.5" presetName="Kick Room"
//!   Mix="1.0" PreDelay="0.148…" Decay="0.197…" … />
//! ```
//!
//! So there is no binary decode here — find the element, read the attributes,
//! de-normalize into real units, and map onto `builtin::NativeReverb`.
//!
//! Two wrinkles the format hides:
//!
//! - **Case differs per plugin.** `VintageVerb` writes `Mix`/`Decay`, Room writes
//!   `mix`/`decay`. Attribute lookup is case-insensitive throughout.
//! - **Enums are fractions.** `ReverbMode`, `ColorMode`, `type` and `space` are
//!   selectors stored as `index / (count - 1)`, so they must be rounded back to
//!   a step rather than read as continuous values.
//!
//! See `spec/project-state-formats.md`.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;

/// Which Valhalla plugin a chunk came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValhallaPlugin {
    VintageVerb,
    Room,
}

impl ValhallaPlugin {
    /// Match the XML element name.
    fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "ValhallaVintageVerb" => Some(Self::VintageVerb),
            "ValhallaRoom" => Some(Self::Room),
            _ => None,
        }
    }

    /// The plugin's algorithm menu: `(stored value, name)`, in menu order.
    ///
    /// Measured from the shipping plugins, not inferred — the render bridge
    /// (`signal-analyzer`'s `reverb_match --enumerate`) sweeps the parameter
    /// and reads back each display name. Entries sit ~1/24 apart for
    /// `VintageVerb` and ~1/12 for Room.
    ///
    /// Values are exact 24ths (`VintageVerb`) / 12ths (Room): the sweep reports
    /// the first step at or past each boundary, so the measurements are
    /// snapped back to those fractions. Both plugins report their slot 0 and
    /// slot 1 under one name, which is why the tables skip index 1.
    ///
    /// Matching is by nearest stored value rather than by a computed index,
    /// because the exact index formula is not pinned down (both plugins
    /// report their first two slots under one name) and nearest-value
    /// matching does not depend on it.
    // The tables spell every entry as `index / (count - 1)` so the encoding is
    // readable down the column; that makes the last row `n / n`, which is the
    // value 1.0 written the same way as its neighbours rather than a mistake.
    #[allow(clippy::eq_op)]
    #[must_use]
    pub fn modes(self) -> &'static [(f64, &'static str)] {
        match self {
            Self::VintageVerb => &[
                (0.0 / 24.0, "Concert Hall"),
                (1.0 / 24.0, "Concert Hall"),
                (2.0 / 24.0, "Plate"),
                (3.0 / 24.0, "Room"),
                (4.0 / 24.0, "Chamber"),
                (5.0 / 24.0, "Random Space"),
                (6.0 / 24.0, "Chorus Space"),
                (7.0 / 24.0, "Ambience"),
                (8.0 / 24.0, "Bright Hall"),
                (9.0 / 24.0, "Sanctuary"),
                (10.0 / 24.0, "Dirty Hall"),
                (11.0 / 24.0, "Dirty Plate"),
                (12.0 / 24.0, "Smooth Plate"),
                (13.0 / 24.0, "Smooth Room"),
                (14.0 / 24.0, "Smooth Random"),
                (15.0 / 24.0, "Nonlin"),
                (16.0 / 24.0, "Chaotic Chamber"),
                (17.0 / 24.0, "Chaotic Hall"),
                (18.0 / 24.0, "Chaotic Neutral"),
                (19.0 / 24.0, "Cathedral"),
                (20.0 / 24.0, "Palace"),
                (21.0 / 24.0, "Chamber1979"),
                (22.0 / 24.0, "Hall1984"),
                (23.0 / 24.0, "Concert Hall"),
                (24.0 / 24.0, "Concert Hall"),
            ],
            Self::Room => &[
                (0.0 / 12.0, "Large Room"),
                (1.0 / 12.0, "Large Room"),
                (2.0 / 12.0, "Medium Room"),
                (3.0 / 12.0, "Bright Room"),
                (4.0 / 12.0, "Large Chamber"),
                (5.0 / 12.0, "Dark Room"),
                (6.0 / 12.0, "Dark Chamber"),
                (7.0 / 12.0, "Dark Space"),
                (8.0 / 12.0, "Nostromo"),
                (9.0 / 12.0, "Narcissus"),
                (10.0 / 12.0, "Sulaco"),
                (11.0 / 12.0, "LV-426"),
                (12.0 / 12.0, "Dense Room"),
            ],
        }
    }
}

/// A parsed Valhalla patch: the raw normalized attributes plus identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValhallaState {
    pub plugin: ValhallaPlugin,
    pub plugin_version: Option<String>,
    pub preset_name: Option<String>,
    /// Every attribute, in file order, as written (values still normalized).
    pub attributes: Vec<(String, String)>,
}

impl ValhallaState {
    /// Attribute lookup, case-insensitive.
    #[must_use]
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    /// Numeric attribute lookup, case-insensitive.
    #[must_use]
    pub fn num(&self, key: &str) -> Option<f64> {
        self.attr(key).and_then(|v| v.parse().ok())
    }

    /// First numeric attribute present among `keys` — lets one call cover both
    /// plugins' spellings (`Decay` vs `decay`, `Size` vs `lateSize`).
    fn num_any(&self, keys: &[&str]) -> Option<f64> {
        keys.iter().find_map(|k| self.num(k))
    }

    /// The raw stored algorithm selector.
    #[must_use]
    pub fn mode_value(&self) -> Option<f64> {
        let key = match self.plugin {
            ValhallaPlugin::VintageVerb => "ReverbMode",
            ValhallaPlugin::Room => "type",
        };
        self.num(key)
    }

    /// The selected algorithm's menu position and name.
    #[must_use]
    pub fn mode(&self) -> Option<(usize, &'static str)> {
        let v = self.mode_value()?;
        let modes = self.plugin.modes();
        let (idx, &(_, name)) = modes.iter().enumerate().min_by(|(_, a), (_, b)| {
            (a.0 - v)
                .abs()
                .partial_cmp(&(b.0 - v).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;
        Some((idx, name))
    }

    /// The selected algorithm's name.
    #[must_use]
    pub fn mode_name(&self) -> Option<&'static str> {
        self.mode().map(|(_, n)| n)
    }
}

/// Map a `0..1` selector onto an index in `0..count`.
///
/// Valhalla stores enum selectors as `index / (count - 1)`, so recovering the
/// index is a scale-and-round. Values outside `0..1` clamp.
// Exercised by this module's tests but not yet called by the importer
// itself — the encoding it documents is still being wired in. Kept
// rather than deleted so the decoding knowledge and its coverage stay.
#[allow(dead_code)]
fn nearest_step(v: f64, count: usize) -> usize {
    if count <= 1 {
        return 0;
    }
    let idx = (v.clamp(0.0, 1.0) * (count - 1) as f64).round() as usize;
    idx.min(count - 1)
}

/// Extract the Valhalla XML element from a decoded plugin chunk.
///
/// The element is plain UTF-8 inside otherwise-binary chunk data, so the
/// surrounding bytes are scanned rather than parsed.
#[must_use]
pub fn extract_xml(chunk: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(chunk);
    let start = text.find("<Valhalla")?;
    let end = text[start..].find("/>")? + start + 2;
    Some(text[start..end].to_string())
}

/// Decode the base64 body of a REAPER `<VST …>` block into raw chunk bytes.
///
/// VST3 chunks are several base64 segments, **each encoded independently** and
/// each starting on a fresh line, so the whole body cannot be decoded as one
/// base64 string. Lines are decoded individually and concatenated; a line that
/// fails to decode is skipped rather than aborting, since only the segment
/// holding the XML matters here.
pub fn decode_vst_chunk_lines<'a>(lines: impl IntoIterator<Item = &'a str>) -> Vec<u8> {
    let mut out = Vec::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Segments are wrapped at 128 chars, so a line may carry no padding of
        // its own — supply it before decoding.
        let mut padded = line.to_string();
        while padded.len() % 4 != 0 {
            padded.push('=');
        }
        if let Ok(bytes) = B64.decode(&padded) {
            out.extend_from_slice(&bytes);
        }
    }
    out
}

/// Parse a Valhalla XML element.
///
/// Hand-parsed rather than pulled through an XML crate: the payload is always
/// one self-closing element with plain `key="value"` attributes.
pub fn parse_xml(xml: &str) -> Option<ValhallaState> {
    let inner = xml.trim().strip_prefix('<')?.strip_suffix("/>")?;
    let (tag, rest) = inner.split_once(char::is_whitespace)?;
    let plugin = ValhallaPlugin::from_tag(tag)?;

    let mut attributes = Vec::new();
    let mut chars = rest.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c.is_whitespace() || c == '=' || c == '"' {
            continue;
        }
        // Read key up to '='.
        let key_start = i;
        let Some(eq) = rest[key_start..].find('=') else {
            break;
        };
        let key = rest[key_start..key_start + eq].trim().to_string();

        // Value is the quoted run after '='.
        let after = key_start + eq + 1;
        let Some(open) = rest[after..].find('"') else {
            break;
        };
        let vstart = after + open + 1;
        let Some(close) = rest[vstart..].find('"') else {
            break;
        };
        let value = rest[vstart..vstart + close].to_string();
        attributes.push((key, value));

        // Resume scanning past the closing quote.
        while let Some(&(j, _)) = chars.peek() {
            if j <= vstart + close {
                chars.next();
            } else {
                break;
            }
        }
    }

    let find = |k: &str| {
        attributes
            .iter()
            .find(|(a, _)| a.eq_ignore_ascii_case(k))
            .map(|(_, v)| v.clone())
    };

    Some(ValhallaState {
        plugin,
        plugin_version: find("pluginVersion"),
        preset_name: find("presetName").filter(|s| !s.is_empty()),
        attributes,
    })
}

/// Convenience: decode a REAPER `<VST>` body straight to a parsed patch.
pub fn parse_vst_chunk_lines<'a>(
    lines: impl IntoIterator<Item = &'a str>,
) -> Option<ValhallaState> {
    let chunk = decode_vst_chunk_lines(lines);
    parse_xml(&extract_xml(&chunk)?)
}

/// `VintageVerb`'s pre-delay range, in milliseconds, at `PreDelay = 1.0`.
const VVV_PREDELAY_MAX_MS: f64 = 500.0;
/// `ValhallaRoom`'s pre-delay range, in milliseconds, at `predelay = 1.0`.
const ROOM_PREDELAY_MAX_MS: f64 = 1000.0;
/// `NativeReverb`'s `predelay` ceiling — the translation clamps to it.
const NATIVE_PREDELAY_MAX_MS: f64 = 200.0;

/// `VintageVerb`'s `Decay` control, in seconds.
///
/// Measured off the shipping plugin by stepping the parameter and reading its
/// display text (`reverb_match --enumerate Decay --slots 20`): 0.2 s at 0
/// through 70 s at 1.0, on a power curve. A least-squares read of the probe
/// points gives an exponent of 3.36, which reproduces every measured point to
/// better than 1.5%.
///
/// **This is the control's own displayed time, not the tail the plugin
/// actually produces** — `Size` scales the space on top of it, and at
/// `Size = 1.0` a preset reading 3.8 s here measured 2.47 s in the render
/// bridge. So treat this as a starting estimate; the authoritative number is
/// the measured RT60 of a real render.
#[must_use]
pub fn vintageverb_decay_seconds(decay: f64) -> f64 {
    const MIN_S: f64 = 0.2;
    const SPAN_S: f64 = 69.8;
    const EXPONENT: f64 = 3.36;
    MIN_S + SPAN_S * decay.clamp(0.0, 1.0).powf(EXPONENT)
}

/// Interpolate a measured control curve at `x` (0–1), geometrically.
///
/// The tables below are read off the shipping plugin at evenly spaced probe
/// points. Interpolating in the log domain is right for every one of them —
/// frequencies and decay multipliers are both perceived and specified
/// ratiometrically.
fn interp_log(table: &[f64], x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    let last = table.len() - 1;
    let pos = x * last as f64;
    let i = (pos.floor() as usize).min(last.saturating_sub(1));
    let frac = pos - i as f64;
    let (a, b) = (table[i].max(1e-9), table[i + 1].max(1e-9));
    a * (b / a).powf(frac)
}

/// `BassMult` → decay-time multiplier. 0.25x to 4.0x — exactly the range an
/// FTS `DecayBand` rate spans.
const VVV_BASS_MULT: [f64; 9] = [0.25, 0.28, 0.40, 0.63, 1.00, 1.51, 2.17, 3.00, 4.00];
/// `BassXover` → Hz.
const VVV_BASS_XOVER_HZ: [f64; 9] = [
    100.0, 100.0, 140.0, 290.0, 700.0, 1580.0, 3190.0, 5870.0, 10000.0,
];
/// `HighFreq` → Hz.
const VVV_HIGH_FREQ_HZ: [f64; 9] = [
    100.0, 620.0, 1850.0, 3660.0, 6000.0, 8830.0, 12110.0, 15840.0, 20000.0,
];

/// `HighShelf` → decay-time multiplier.
///
/// The control is a shelving **gain** in the reverb's feedback path, linear
/// from −24 dB at 0 to 0 dB at 1.0 (measured). A per-pass cut of `g` dB
/// shortens that band's decay, so it maps onto a rate multiplier: 0 dB leaves
/// the tail alone, and the full −24 dB takes it to a quarter — the bottom of
/// the `DecayBand` range.
// Exercised by this module's tests but not yet called by the importer
// itself — the encoding it documents is still being wired in. Kept
// rather than deleted so the decoding knowledge and its coverage stay.
#[allow(dead_code)]
fn vintageverb_high_shelf_rate(high_shelf: f64) -> f64 {
    let gain_db = -24.0 + 24.0 * high_shelf.clamp(0.0, 1.0);
    4.0f64.powf(gain_db / 24.0)
}

/// `BassMult` as a decay-time multiplier.
#[must_use]
pub fn vintageverb_bass_mult(bass_mult: f64) -> f64 {
    interp_log(&VVV_BASS_MULT, bass_mult)
}

/// `BassXover` in Hz.
#[must_use]
pub fn vintageverb_bass_xover_hz(bass_xover: f64) -> f64 {
    interp_log(&VVV_BASS_XOVER_HZ, bass_xover)
}

/// `HighFreq` in Hz.
#[must_use]
pub fn vintageverb_high_freq_hz(high_freq: f64) -> f64 {
    interp_log(&VVV_HIGH_FREQ_HZ, high_freq)
}

/// Map a Valhalla algorithm name onto an FTS `(algorithm, variant)` pair.
///
/// Algorithm indices follow `AlgorithmType::ALL`: 0 Room, 1 Hall, 2 Plate,
/// 3 Spring, 4 Cloud, 5 Bloom, 6 Shimmer, 7 Chorale, 8 Magneto, 9 `NonLinear`,
/// 10 Swell, 11 Reflections, 12 Velvet, 13 `FreeVerb`, 14 Convolution.
///
/// The **variant** matters as much as the algorithm: several FTS engines ship
/// more than one tuning, and two of them are exactly what Valhalla's biggest
/// preset clusters need —
///
/// - `Room` variant 1 is `room_chamber`, which covers Valhalla's `Chamber`,
///   `Chaotic Chamber`, `Chamber1979`, `Large Chamber` and `Dark Chamber`.
///   Between `VintageVerb` and Room those account for roughly a quarter of the
///   425 factory presets.
/// - `Hall` variant 1 is `hall_cathedral`, for `Cathedral` and `Sanctuary`.
/// - `Hall` variant 2 is `hall_arena`, the largest hall tuning — the closest
///   fit for `Palace`, `VintageVerb`'s single biggest cluster (50 presets).
///
/// The remaining genuine gap is Palace itself: arena is the nearest existing
/// space, not the same one. See `spec/project-state-formats.md`.
// Only the tests call the no-substitution form today; the importer always
// goes through `algorithm_and_variant_for`.
#[allow(dead_code)]
fn algorithm_and_variant(mode: &str) -> (f64, f64) {
    algorithm_and_variant_for(mode, None)
}

/// As [`algorithm_and_variant`], but allowed to substitute a smaller space
/// when the named one cannot ring as briefly as the preset asks.
///
/// A mode name describes a character, not a length, and Valhalla's factory
/// library is full of big-sounding names on short settings — "PALACE-1982
/// Room Mics" wants a 0.29 s tail. Routing that to `hall_arena`, whose
/// shortest reachable decay is around a second, does not merely sound wrong:
/// the request clamps at the floor and the render has no fittable decay at
/// all, so the comparison cannot even measure it.
///
/// When the requested time falls below the mapped engine's floor, fall back to
/// the Room family, which reaches down to 0.08 s. Character is worth less than
/// being the right length.
fn algorithm_and_variant_for(mode: &str, want_seconds: Option<f64>) -> (f64, f64) {
    let (algorithm, variant) = algorithm_and_variant_by_name(mode);
    let Some(want) = want_seconds else {
        return (algorithm, variant);
    };
    let algo = reverb::algorithm::AlgorithmType::from_index(algorithm as usize);
    match algo.t60_range(variant as usize) {
        // Comfortably reachable, or an engine with no time model to consult.
        Some((floor, _)) if want < floor => {
            // Room variants, shortest first: studio (0.08 s), medium, chamber.
            (0.0, 2.0)
        }
        _ => (algorithm, variant),
    }
}

fn algorithm_and_variant_by_name(mode: &str) -> (f64, f64) {
    let m = mode.to_ascii_lowercase();
    // Order matters. A mode name carries both a space ("Chamber", "Hall") and
    // a character ("Chaotic", "Smooth"); where it names a space, the space
    // wins, because that is what the reverb *is*. Only the modes with no
    // space in their name — Random Space, Smooth Random, Chaotic Neutral,
    // Chorus Space — fall through to Random, whose defining feature is the
    // motion rather than the geometry.
    if m.contains("nonlin") {
        (9.0, 0.0) // NonLinear
    } else if m.contains("chamber") {
        (0.0, 1.0) // Room / room_chamber — incl. Chaotic Chamber, Chamber1979
    } else if m.contains("cathedral") || m.contains("sanctuary") {
        (1.0, 1.0) // Hall / hall_cathedral
    } else if m.contains("palace") {
        (1.0, 2.0) // Hall / hall_arena — nearest large space
    } else if m.contains("plate") {
        (2.0, 0.0) // Plate
    } else if m.contains("ambience") {
        // Room / room_studio, NOT Reflections.
        //
        // Reflections is early-reflections only: it produces no measurable
        // decay at any setting, so it has no `t60_range`, `decay_time` is a
        // no-op on it, and a fit has nothing to steer. Valhalla's Ambience is
        // not that — "Small Drum Room" is an Ambience preset with a real if
        // brief 0.17 s tail. A tight studio room is what it actually is.
        (0.0, 2.0)
    } else if m.contains("hall") {
        (1.0, 0.0) // Hall — incl. Chaotic Hall, Hall1984
    } else if m.contains("dense room") {
        (0.0, 2.0) // Room / room_studio — tight and dense
    } else if m.contains("room") {
        (0.0, 0.0) // Room — incl. Smooth Room, Dark Room
    } else {
        // Random Space, Smooth Random, Chaotic Neutral, Chorus Space, Dark
        // Space, and Room's Alien-themed modes (Nostromo, Narcissus, Sulaco,
        // LV-426) — diffuse, moving spaces with no geometric name.
        (15.0, 0.0) // Random
    }
}

/// Translate a Valhalla patch into `NativeReverb` parameters.
///
/// Returns `(param_name, value)` pairs for `NativeReverb::set_named`. Only
/// parameters the source actually carries are emitted, so the rest keep the
/// `NativeReverb` defaults.
///
/// Not everything survives: see `spec/project-state-formats.md` §3 for the
/// parameters with no FTS equivalent yet (frequency-dependent decay
/// multipliers, separate early/late sections, `VintageVerb`'s `ColorMode`).
#[must_use]
pub fn to_native_reverb_params(v: &ValhallaState) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = Vec::new();
    let mut set = |k: &str, val: f64| out.push((k.to_string(), val));

    // Algorithm and variant FIRST: both rebuild the reverb chain, so any
    // value written before them is discarded. Emitting them last silently
    // reverted every other parameter to its default.
    let mut chosen: Option<reverb::algorithm::AlgorithmType> = None;
    if let Some(mode) = v.mode_name() {
        // The decay estimate feeds engine selection, so a short preset is not
        // handed to an engine that cannot ring that briefly.
        let want = (v.plugin == ValhallaPlugin::VintageVerb)
            .then(|| v.num("Decay").map(vintageverb_decay_seconds))
            .flatten();
        let (algorithm, variant) = algorithm_and_variant_for(mode, want);
        set("algorithm", algorithm);
        set("variant", variant);
        chosen = Some(reverb::algorithm::AlgorithmType::from_index(
            algorithm as usize,
        ));
    }

    if let Some(mix) = v.num_any(&["Mix", "mix"]) {
        set("mix", mix.clamp(0.0, 1.0));
    }
    if let Some(decay) = v.num_any(&["Decay", "decay"]) {
        set("decay", decay.clamp(0.0, 1.0));
        // A time target as well as the raw control: `decay` spans a
        // different number of seconds in every FTS engine, so only
        // `decay_time` can carry "this preset rings for N seconds".
        if v.plugin == ValhallaPlugin::VintageVerb {
            set("decay_time", vintageverb_decay_seconds(decay));
        }
    }
    // VintageVerb has one `Size`; Room splits early/late and only the late
    // size has a NativeReverb counterpart.
    if let Some(size) = v.num_any(&["Size", "lateSize"]) {
        set("size", size.clamp(0.0, 1.0));
    }

    let predelay_max = match v.plugin {
        ValhallaPlugin::VintageVerb => VVV_PREDELAY_MAX_MS,
        ValhallaPlugin::Room => ROOM_PREDELAY_MAX_MS,
    };
    // Two FTS engines do not have a pre-delay: Magneto and NonLinear remap
    // that knob to the engine's own regeneration feedback and disengage the
    // chain's delay line (`ReverbChain::effective_nonlinear`). Passing the
    // source's pre-delay through would not be a near miss — it would ask for
    // repeats the preset never had, on a control that reads a completely
    // different quantity. The four shipped VintageVerb "NL-" presets carried a
    // 125 ms pre-delay, which arrived as 0.625 regeneration and measured +71 LU
    // against the reference. Dropping it loses the pre-delay, which that engine
    // cannot express either way; sending it loses the whole preset.
    let remaps_predelay = matches!(
        chosen,
        Some(
            reverb::algorithm::AlgorithmType::Magneto | reverb::algorithm::AlgorithmType::NonLinear
        )
    );
    if let Some(pd) = v.num_any(&["PreDelay", "predelay"]) {
        if !remaps_predelay {
            set(
                "predelay",
                (pd.clamp(0.0, 1.0) * predelay_max).min(NATIVE_PREDELAY_MAX_MS),
            );
        }
    }

    // Diffusion: VintageVerb exposes early and late separately, Room has one
    // control. Average the pair so a single knob represents both.
    match (
        v.num("EarlyDiffusion"),
        v.num("LateDiffusion"),
        v.num("diffusion"),
    ) {
        (Some(e), Some(l), _) => set("diffusion", ((e + l) / 2.0).clamp(0.0, 1.0)),
        (_, _, Some(d)) => set("diffusion", d.clamp(0.0, 1.0)),
        (Some(e), None, _) => set("diffusion", e.clamp(0.0, 1.0)),
        (None, Some(l), _) => set("diffusion", l.clamp(0.0, 1.0)),
        _ => {}
    }

    // Bass decay multiplier → `low_end` (both are 0.5-neutral). Coarse, and
    // kept for engines that read it; the Decay Rate EQ below is the precise
    // path.
    if let Some(b) = v.num_any(&["BassMult", "RTBassMultiply"]) {
        set("low_end", b.clamp(0.0, 1.0));
    }

    // NOT mapped to the Decay Rate EQ, deliberately.
    //
    // It is tempting to route `BassMult` / `HighShelf` straight onto decay
    // bands — they are named and displayed as decay multipliers, and their
    // 0.25x-4.0x range is exactly a `DecayBand` rate. Measurement says no.
    // On "79 Acoustic Chamber" (`BassMult` 0.0 = "0.25 X", `HighShelf` 0.0 =
    // -24 dB) the reference still decays for 2.47 s, with per-band ratios of
    // only ~0.73 low and ~0.6 high against its own midband. Translating the
    // displayed multipliers literally gave two stacked quarter-rate shelves
    // and collapsed our tail to 0.46 s.
    //
    // So these controls interact with Valhalla's own damping and geometry in
    // ways the displayed number does not capture, and no static mapping from
    // them is currently defensible. `signal-analyzer`'s `reverb_match --tune`
    // fits the decay bands from the reference's *measured* per-band ratios
    // instead, which needs no model of what the controls mean.
    //
    // The control curves themselves are measured and correct — see
    // `vintageverb_bass_mult` and friends — and are kept for when a mapping
    // can be grounded.

    // High cut → `tone`, NOT `damping`.
    //
    // `damping` models absorption: it shortens the high-frequency decay, by up
    // to 6.7x at the limit. Valhalla's High Cut is a filter on the tail, and
    // measurement says it does not shorten the decay — on "300 Large Hall" the
    // reference decays 2.45 s at 2 kHz and 2.17 s at 8 kHz, essentially flat,
    // while mapping High Cut onto damping made our top decay so much faster
    // that no `decay_time` below freeze could reach the reference at all.
    //
    // Frequency-dependent decay now has a proper home in the Decay Rate EQ, so
    // this control belongs on tone, where it colours without shortening.
    if let Some(hc) = v.num_any(&["HighCut", "HiCut"]) {
        set("tone", (hc.clamp(0.0, 1.0) - 0.5) * 2.0);
    }

    // Modulation: VintageVerb has one rate/depth pair; Room has separate early
    // and late pairs, of which the late one carries the tail character.
    if let Some(depth) = v.num_any(&["ModDepth", "lateModDepth"]) {
        set("modulation", depth.clamp(0.0, 1.0));
    }

    out
}
