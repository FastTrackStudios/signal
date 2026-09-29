//! The NI pianos' release-sample law — which release layer plays, and how
//! loud, when a key comes up.
//!
//! Kontakt never plays these pianos' release samples at the key's velocity.
//! Each library's release script picks the layer (the velocity the release
//! is played at) and its volume from the struck velocity (after Color) and
//! how long the key was held, adds a per-key term and a fade-in, and plays no
//! release at all past a per-key held time. Playing the release at the key's
//! own velocity and full level instead — what a plain release trigger does —
//! puts a loud thump on every key-up, ~28 dB over the dying note on a long
//! Grandeur hold.
//!
//! The tables in `data/piano_release/` are that law, tabulated by running
//! each library's `script_2.ksp` through the `ksp` evaluator (nkx-extract:
//! `ksp release-law <script_2.ksp> --instrument N`; Grandeur 1, Gentleman 3,
//! Maverick 0, The Giant its own script). Inside a band the script is linear
//! in held time, so a band stores its start value and slope.

use std::sync::OnceLock;

/// Which NI piano's release script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NiPiano {
    Grandeur,
    Gentleman,
    Maverick,
    Giant,
}

/// Which saved state of the instrument a piano plays with: the library's
/// shipped defaults, or a gig's snapshot of its controls (Color, Dynamic
/// Range, noise levels — read from the gig's Kontakt multi; see
/// `data/piano_release/<piano>.worship.persistent.tsv`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Snapshot {
    #[default]
    Factory,
    /// Worship Gig 3's NI Pianos multi.
    Worship,
}

impl Snapshot {
    /// By name (`"factory"`, `"worship"`); `None` for anything else.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "factory" | "" => Some(Self::Factory),
            "worship" => Some(Self::Worship),
            _ => None,
        }
    }
}

/// A snapshot's levels for the noise layers and its Dynamic Range knob.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapshotLevels {
    /// `$Ana_sliRelVol` — the Release group's volume (Kontakt units).
    pub release_volume: i32,
    /// `$Ana_sliHammerVol` when hammer noise is on (`$Ana_swiHammerOnOff`).
    pub hammer_volume: Option<i32>,
    /// `$Mas_sliAnaDyn` — the Dynamic Range knob.
    pub dynamics: i32,
    /// The instrument's own volume in the multi (dB) — every voice of it.
    pub instrument_db: f32,
}

/// The struck notes' group volume: every one of these libraries ships its
/// DRY (per-key body) groups at 0.5 = −6.02 dB (a few Gentleman keys a
/// little lower), which the packs' zones do not carry. The noise groups'
/// volumes are the snapshot's sliders instead.
pub const DRY_GROUP_DB: f32 = -6.02;

impl SnapshotLevels {
    /// From each library's `persistent_0.tsv` (factory) and the gig's
    /// multi (worship).
    #[must_use]
    pub fn of(piano: NiPiano, snapshot: Snapshot) -> Self {
        // Worship: the gig multi's per-instrument persistent block and its
        // program volume (0.4854 / 0.4915 / 0.3004 / 0.3968 linear).
        let (release_volume, hammer_volume, dynamics, instrument_db) = match (piano, snapshot) {
            (NiPiano::Giant, Snapshot::Factory) => (340_000, None, 0, 0.0),
            (_, Snapshot::Factory) => (500_000, None, 0, 0.0),
            (NiPiano::Maverick, Snapshot::Worship) => (454_475, Some(269_971), 0, -6.28),
            (NiPiano::Gentleman, Snapshot::Worship) => (359_630, Some(305_093), 0, -6.17),
            (NiPiano::Grandeur, Snapshot::Worship) => (393_774, Some(249_672), 0, -10.45),
            (NiPiano::Giant, Snapshot::Worship) => (342_419, Some(305_018), -20, -8.03),
        };
        Self {
            release_volume,
            hammer_volume,
            dynamics,
            instrument_db,
        }
    }
}

/// One release event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Release {
    /// Velocity the release layer is played at — selects the layer.
    pub layer_velocity: u8,
    /// Level (dB) from the script — the Release group's volume (a
    /// snapshot setting, [`SnapshotLevels::release_volume`]) is not in it.
    pub gain_db: f32,
    /// Fade-in (ms; 0 = none).
    pub fade_in_ms: f32,
}

#[derive(Debug, Clone, Copy)]
struct Band {
    t0: i32,
    t1: i32,
    layer_vel: u8,
    mdb: f32,
    mdb_per_ms: f32,
    fade_us: i32,
}

/// A piano's tabulated law.
#[derive(Debug)]
pub struct ReleaseLaw {
    /// Bands per struck velocity (index 1‥127), ordered by `t0`.
    bands: Vec<Vec<Band>>,
    key_mdb: [i32; 128],
    no_release_after_ms: [i32; 128],
}

/// Kontakt's volume parameter (0‥1 000 000) in dB: `(v/10⁶)³ · 4` as gain,
/// so 630 000 ≈ 0 dB and 1 000 000 = +12 dB.
#[must_use]
pub fn kontakt_volume_db(v: i32) -> f32 {
    60.0 * (v.max(1) as f32 / 1_000_000.0).log10() + 12.041
}

impl ReleaseLaw {
    fn parse(tsv: &str) -> Self {
        let mut bands = vec![Vec::new(); 128];
        let mut key_mdb = [0; 128];
        let mut no_release_after_ms = [0; 128];
        for line in tsv.lines().filter(|l| !l.starts_with('#')) {
            let f: Vec<&str> = line.split('\t').collect();
            let int = |i: usize| f.get(i).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
            match f.first() {
                Some(&"band") => {
                    let vel = int(1).clamp(0, 127) as usize;
                    bands[vel].push(Band {
                        t0: int(2),
                        t1: int(3),
                        layer_vel: int(4).clamp(1, 127) as u8,
                        mdb: int(5) as f32,
                        mdb_per_ms: f.get(6).and_then(|v| v.parse().ok()).unwrap_or(0.0),
                        fade_us: int(7),
                    });
                }
                Some(&"key") => {
                    let n = int(1).clamp(0, 127) as usize;
                    key_mdb[n] = int(2);
                    no_release_after_ms[n] = int(3);
                }
                _ => {}
            }
        }
        Self {
            bands,
            key_mdb,
            no_release_after_ms,
        }
    }

    /// The law for `piano` (parsed once).
    #[must_use]
    pub fn of(piano: NiPiano) -> &'static Self {
        static LAWS: [OnceLock<ReleaseLaw>; 4] = [const { OnceLock::new() }; 4];
        let (i, tsv) = match piano {
            NiPiano::Grandeur => (0, include_str!("../data/piano_release/grandeur.tsv")),
            NiPiano::Gentleman => (1, include_str!("../data/piano_release/gentleman.tsv")),
            NiPiano::Maverick => (2, include_str!("../data/piano_release/maverick.tsv")),
            NiPiano::Giant => (3, include_str!("../data/piano_release/giant.tsv")),
        };
        LAWS[i].get_or_init(|| Self::parse(tsv))
    }

    /// The release for `note` struck at `velocity` (after Color) and held
    /// `held_ms`; `None` when the script plays none.
    #[must_use]
    pub fn release(&self, note: u8, velocity: u8, held_ms: f32) -> Option<Release> {
        let n = usize::from(note.min(127));
        let limit = self.no_release_after_ms[n];
        if limit > 0 && held_ms > limit as f32 {
            return None;
        }
        let t = held_ms.max(0.0);
        let bands = &self.bands[usize::from(velocity.clamp(1, 127))];
        // The band holding `t` (the last one runs on past its end).
        let b = bands
            .iter()
            .find(|b| t <= b.t1 as f32 + 0.999)
            .or_else(|| bands.last())?;
        let mdb = b.mdb + b.mdb_per_ms * (t - b.t0 as f32) + self.key_mdb[n] as f32;
        Some(Release {
            layer_velocity: b.layer_vel,
            gain_db: mdb / 1000.0,
            fade_in_ms: b.fade_us as f32 / 1000.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kontakt_volume_law() {
        assert!(kontakt_volume_db(630_000).abs() < 0.05);
        assert!((kontakt_volume_db(1_000_000) - 12.04).abs() < 0.01);
        assert!((kontakt_volume_db(500_000) + 6.02).abs() < 0.05);
    }

    #[test]
    fn grandeur_matches_the_script_by_hand() {
        // Grandeur, vel 100, held 50 ms (script_2 case 92‥109, 0‥100 ms):
        // VelVol (−2 + 100 − 107)·400 − 12000 = −15600, layer 8 → vel 99,
        // key C4's position term +5500 → −10100 mdB.
        let r = ReleaseLaw::of(NiPiano::Grandeur).release(60, 100, 50.0).unwrap();
        assert_eq!(r.layer_velocity, 99);
        assert!((r.gain_db + 10.1).abs() < 0.05, "{r:?}");
        assert!((r.fade_in_ms - 120.0).abs() < 1e-3);
    }

    #[test]
    fn longer_holds_pick_softer_layers() {
        let law = ReleaseLaw::of(NiPiano::Grandeur);
        let short = law.release(60, 100, 50.0).unwrap();
        let long = law.release(60, 100, 20_000.0).unwrap();
        assert!(long.layer_velocity < short.layer_velocity);
    }

    #[test]
    fn every_piano_parses_with_a_band_at_every_velocity() {
        for p in [NiPiano::Grandeur, NiPiano::Gentleman, NiPiano::Maverick, NiPiano::Giant] {
            let law = ReleaseLaw::of(p);
            for v in 1..=127u8 {
                assert!(law.release(60, v, 500.0).is_some(), "{p:?} vel {v}");
            }
        }
    }
}
