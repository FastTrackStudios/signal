//! Persisting measurements, so conclusions can come later.
//!
//! Separation is expensive and the questions change. Once a song has
//! been measured, the answer to a question nobody had asked yet should
//! be a query, not another hour of GPU — so what gets written is the
//! **raw measurement**, never a verdict.
//!
//! # What "raw" means here, concretely
//!
//! - A kick with no beater click records its actual `-62 dB`, not a
//!   `has_click: false`. Some kicks genuinely have no click, and that is
//!   a finding rather than a gap; a boolean would throw away the number
//!   that lets you say *how* clickless, and lets you change your mind
//!   later about where the threshold sits.
//! - A snare's `fundamental_hz` stays `null`. It is not given 0, and not
//!   given the loudest bin — a snare has no stable pitch, and inventing
//!   one would be indistinguishable from a real 40 Hz reading once it is
//!   averaged with others.
//! - The whole sixth-octave curve is stored, not only the named
//!   regions. Regions are derivable from the curve; the curve is not
//!   recoverable from the regions, and a region definition that looked
//!   right today is exactly the sort of thing that gets revised.
//!
//! # Schema versioning
//!
//! [`SCHEMA`] is written into every record. Measurement definitions will
//! change — the gate threshold, the band spacing, a region's edges — and
//! a file that does not say which definitions produced it cannot safely
//! be compared with a newer one.

use serde::{Deserialize, Serialize};

use crate::elements::{ElementProfile, Fullness, Region, region_balance};

/// Bumped whenever a stored measurement changes meaning.
pub const SCHEMA: u32 = 1;

/// Everything measured about one stem, as stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StemMeasurement {
    /// `kick`, `lead`, `bass` — the role, not the filename.
    pub stem: String,
    /// Where the audio came from, for tracing a number back.
    pub source: Option<String>,
    pub sample_rate: f64,
    pub loudness_lufs: f64,
    pub crest_db: f64,
    /// `None` where the element has no stable pitch. Never fabricated.
    pub fundamental_hz: Option<f64>,
    pub fullness: Fullness,
    /// The full sixth-octave curve, `(centre_hz, dB)`.
    ///
    /// Stored in full because named regions are derived from it and it
    /// cannot be reconstructed from them.
    pub profile: Vec<(f64, f64)>,
    /// Named regions, as computed at write time — a convenience, since
    /// the curve above is the authority.
    pub regions: Vec<(String, f64)>,
}

impl StemMeasurement {
    /// Record a measured stem under a set of named regions.
    pub fn new(
        stem: impl Into<String>,
        source: Option<String>,
        sample_rate: f64,
        p: &ElementProfile,
        regions: &[Region],
    ) -> StemMeasurement {
        StemMeasurement {
            stem: stem.into(),
            source,
            sample_rate,
            loudness_lufs: p.loudness_lufs,
            crest_db: p.crest_db,
            fundamental_hz: p.fundamental_hz,
            fullness: p.fullness,
            regions: region_balance(&p.profile, regions)
                .into_iter()
                .map(|(n, v)| (n.to_string(), v))
                .collect(),
            profile: p.profile.clone(),
        }
    }

    /// Recompute region balances under a *different* set of regions.
    ///
    /// The reason the full curve is stored: region edges get revised,
    /// and revising them must not mean re-separating the corpus.
    pub fn rebalance(&self, regions: &[Region]) -> Vec<(String, f64)> {
        region_balance(&self.profile, regions)
            .into_iter()
            .map(|(n, v)| (n.to_string(), v))
            .collect()
    }
}

/// Every stem measured for one song, with the provenance needed to
/// compare it against another song fairly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SongMeasurement {
    pub schema: u32,
    /// Whatever identifies the song upstream — a corpus id, a title.
    pub song: String,
    /// Seconds since the Unix epoch.
    pub measured_at: u64,
    /// Which separation models produced these stems.
    ///
    /// Two songs measured under different models are not directly
    /// comparable, and without this recorded there is no way to notice.
    pub models: Vec<String>,
    pub stems: Vec<StemMeasurement>,
}

impl SongMeasurement {
    pub fn new(song: impl Into<String>, models: Vec<String>) -> SongMeasurement {
        SongMeasurement {
            schema: SCHEMA,
            song: song.into(),
            measured_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            models,
            stems: Vec::new(),
        }
    }

    pub fn stem(&self, name: &str) -> Option<&StemMeasurement> {
        self.stems.iter().find(|s| s.stem == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elements::{self, KICK_REGIONS, VOCAL_REGIONS};

    fn tone(freq: f64, amp: f32) -> Vec<f32> {
        let n = 48_000 * 3;
        (0..n)
            .map(|i| amp * (2.0 * std::f64::consts::PI * freq * i as f64 / 48_000.0).sin() as f32)
            .collect()
    }

    fn measured(freq: f64) -> StemMeasurement {
        let p = elements::profile(&tone(freq, 0.5), 48_000.0).unwrap();
        StemMeasurement::new("kick", None, 48_000.0, &p, KICK_REGIONS)
    }

    /// A stored record must come back meaning the same thing.
    ///
    /// Compared with a tolerance rather than for bit-equality:
    /// `serde_json` writes the shortest decimal that identifies a float,
    /// which can differ from the original by an ULP
    /// (`28.284271247461902` returning as `...906`). That is far below
    /// any measurement here — these are decibels and hertz — but it does
    /// mean JSON is not a bit-exact container, and a test demanding
    /// exact equality fails for reasons that have nothing to do with the
    /// data being right.
    #[test]
    fn a_record_survives_a_round_trip() {
        let m = measured(55.0);
        let json = serde_json::to_string(&m).unwrap();
        let back: StemMeasurement = serde_json::from_str(&json).unwrap();

        assert_eq!(m.stem, back.stem);
        assert_eq!(m.profile.len(), back.profile.len());
        assert_eq!(m.regions.len(), back.regions.len());

        let close = |a: f64, b: f64, what: &str| {
            assert!(
                (a - b).abs() <= 1e-9 * a.abs().max(1.0),
                "{what}: {a} != {b}"
            );
        };
        close(m.loudness_lufs, back.loudness_lufs, "loudness");
        close(m.crest_db, back.crest_db, "crest");
        close(
            m.fundamental_hz.unwrap(),
            back.fundamental_hz.unwrap(),
            "fundamental",
        );
        close(m.fullness.centroid_hz, back.fullness.centroid_hz, "centroid");
        for ((fa, da), (fb, db)) in m.profile.iter().zip(&back.profile) {
            close(*fa, *fb, "band centre");
            close(*da, *db, "band level");
        }
        for ((na, va), (nb, vb)) in m.regions.iter().zip(&back.regions) {
            assert_eq!(na, nb);
            close(*va, *vb, "region");
        }
    }

    /// The whole point: regions can be redefined later without touching
    /// the audio again.
    #[test]
    fn regions_can_be_recomputed_from_a_stored_curve() {
        let m = measured(3000.0);
        let wider = &[Region { name: "everything", lo_hz: 20.0, hi_hz: 20_000.0 }];
        let r = m.rebalance(wider);
        assert_eq!(r.len(), 1);
        // All the energy is inside one region spanning the spectrum.
        assert!(r[0].1 > -1.0, "got {:?}", r[0]);

        // And an entirely different scheme still works off the same file.
        assert_eq!(m.rebalance(VOCAL_REGIONS).len(), VOCAL_REGIONS.len());
    }

    #[test]
    fn an_absent_feature_stores_its_number_not_a_flag() {
        // A kick with no click: the click region is near-empty, and the
        // stored value says HOW empty rather than merely that it is.
        let m = measured(55.0);
        let click = m.regions.iter().find(|(n, _)| n == "click").unwrap().1;
        assert!(click < -40.0, "expected a very low click, got {click}");
        assert!(click.is_finite(), "an absent region must still be a number");
    }

    #[test]
    fn a_pitchless_element_stores_null_rather_than_a_guess() {
        // Noise, which has no fundamental. A fabricated one would be
        // indistinguishable from a real reading after averaging.
        let mut state = 0x1234_5678_u32;
        let noise: Vec<f32> = (0..48_000 * 3)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((state >> 8) as f32 / 8_388_608.0 - 1.0) * 0.3
            })
            .collect();
        let p = elements::profile(&noise, 48_000.0).unwrap();
        let m = StemMeasurement::new("snare", None, 48_000.0, &p, KICK_REGIONS);
        assert!(m.fundamental_hz.is_none());

        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"fundamental_hz\":null"), "{json}");
    }

    #[test]
    fn the_full_curve_is_stored_not_only_the_regions() {
        let m = measured(1000.0);
        assert!(m.profile.len() > 50, "curve has {} bands", m.profile.len());
        assert!(m.regions.len() < m.profile.len());
    }

    #[test]
    fn a_song_records_which_models_produced_it() {
        // Stems from different models are not comparable, and without
        // this there is no way to notice that they were mixed.
        let mut s = SongMeasurement::new("test-song", vec!["htdemucs_ft".into()]);
        s.stems.push(measured(55.0));
        let json = serde_json::to_string(&s).unwrap();
        let back: SongMeasurement = serde_json::from_str(&json).unwrap();
        assert_eq!(back.schema, SCHEMA);
        assert_eq!(back.models, ["htdemucs_ft"]);
        assert!(back.stem("kick").is_some());
        assert!(back.stem("snare").is_none());
    }
}
