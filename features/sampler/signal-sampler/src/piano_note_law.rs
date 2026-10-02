//! The NI pianos' note-on law — the volume and tuning each library's main
//! script puts on every struck note, at the snapshot's default controls.
//!
//! Kontakt plays these pianos' raw samples and then shapes them in script:
//! a velocity volume table (with the Dynamic Range baseline folded in) that
//! lifts soft notes ~9 dB relative to hard ones, a per-key volume term, and
//! a per-key tuning table. Our packs hold the same raw samples, so without
//! this the velocity response is far too steep and the tuning is off by the
//! table's key-to-key variation.
//!
//! The tables in `data/piano_release/<piano>_attack.tsv` are tabulated from
//! each library's `script_0.ksp` with its `persistent_0.tsv` loaded (the
//! `ksp` evaluator in nkx-extract: `ksp attack-law script_0.ksp --persistent
//! persistent_0.tsv --group "1_DRY_A#-1=1"`, the group telling the shared
//! Essential Pianos script which piano it is in: 0 Maverick, 1 Grandeur,
//! 3 Gentleman). The knobs' *changes* from those defaults stay with
//! [`crate::piano_voice::PianoVoice::apply`].

use std::sync::OnceLock;

use crate::piano_release::{NiPiano, Snapshot};

/// A piano's note-on law.
#[derive(Debug)]
pub struct NoteOnLaw {
    /// Per struck velocity (1‥127): the velocity the note is played at (the
    /// snapshot's Color moves it) …
    vel_played: [u8; 128],
    /// … and its volume at key 60, mdB.
    vel_mdb: [i32; 128],
    /// Per key: volume over key 60's (mdB) and tune (millicents).
    key_mdb: [i32; 128],
    key_millicents: [i32; 128],
}

impl NoteOnLaw {
    fn parse(tsv: &str) -> Self {
        let mut law = Self {
            vel_played: std::array::from_fn(|v| v as u8),
            vel_mdb: [0; 128],
            key_mdb: [0; 128],
            key_millicents: [0; 128],
        };
        for line in tsv.lines().filter(|l| !l.starts_with('#')) {
            let f: Vec<&str> = line.split('\t').collect();
            let int = |i: usize| f.get(i).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
            let i = int(1).clamp(0, 127) as usize;
            match f.first() {
                Some(&"vel") => {
                    law.vel_played[i] = int(2).clamp(1, 127) as u8;
                    law.vel_mdb[i] = int(3);
                }
                Some(&"key") => {
                    law.key_mdb[i] = int(2);
                    law.key_millicents[i] = int(3);
                }
                _ => {}
            }
        }
        law
    }

    /// The law for `piano` in `snapshot` (parsed once).
    #[must_use]
    pub fn of(piano: NiPiano, snapshot: Snapshot) -> &'static Self {
        static LAWS: [OnceLock<NoteOnLaw>; 8] = [const { OnceLock::new() }; 8];
        let (i, tsv) = match (piano, snapshot) {
            (NiPiano::Grandeur, Snapshot::Factory) => (0, include_str!("../data/piano_release/grandeur_attack.tsv")),
            (NiPiano::Gentleman, Snapshot::Factory) => (1, include_str!("../data/piano_release/gentleman_attack.tsv")),
            (NiPiano::Maverick, Snapshot::Factory) => (2, include_str!("../data/piano_release/maverick_attack.tsv")),
            (NiPiano::Giant, Snapshot::Factory) => (3, include_str!("../data/piano_release/giant_attack.tsv")),
            (NiPiano::Grandeur, Snapshot::Worship) => (4, include_str!("../data/piano_release/grandeur_attack.worship.tsv")),
            (NiPiano::Gentleman, Snapshot::Worship) => (5, include_str!("../data/piano_release/gentleman_attack.worship.tsv")),
            (NiPiano::Maverick, Snapshot::Worship) => (6, include_str!("../data/piano_release/maverick_attack.worship.tsv")),
            (NiPiano::Giant, Snapshot::Worship) => (7, include_str!("../data/piano_release/giant_attack.worship.tsv")),
        };
        LAWS[i].get_or_init(|| Self::parse(tsv))
    }

    /// `note` struck at `velocity`: the velocity it plays at and the volume
    /// (dB) the script puts on it.
    #[must_use]
    pub fn play(&self, note: u8, velocity: u8) -> (u8, f32) {
        let v = usize::from(velocity.clamp(1, 127));
        let db = (self.vel_mdb[v] + self.key_mdb[usize::from(note.min(127))]) as f32 / 1000.0;
        (self.vel_played[v], db)
    }

    /// The volume (dB) alone — see [`Self::play`].
    #[must_use]
    pub fn gain_db(&self, note: u8, velocity: u8) -> f32 {
        self.play(note, velocity).1
    }

    /// The tuning (cents) the script puts on `note`.
    #[must_use]
    pub fn tune_cents(&self, note: u8) -> f32 {
        self.key_millicents[usize::from(note.min(127))] as f32 / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grandeur_lifts_soft_notes_and_cuts_the_hardest() {
        let law = NoteOnLaw::of(NiPiano::Grandeur, Snapshot::Factory);
        // From script_0 + persistent_0 (ksp attack-law): velocity mode 4
        // (the shipped default) plays a 40 at 56, +5.08 dB; −2.14 dB at 120.
        assert_eq!(law.play(60, 40), (56, 5.08));
        assert!((law.gain_db(60, 120) + 2.14).abs() < 0.01);
        // Stretch table (−2 c) + basic pitch 440 (0) + the Grandeur's pitch
        // mod (−5.892 c) — what Kontakt plays C4 at, measured.
        assert!((law.tune_cents(60) + 7.892).abs() < 0.001);
        // C5 sits 5 cents above C4 in the tuning table.
        assert!((law.tune_cents(72) - law.tune_cents(60) - 5.0).abs() < 0.001);
    }

    #[test]
    fn every_piano_has_a_law() {
        for p in [NiPiano::Grandeur, NiPiano::Gentleman, NiPiano::Maverick, NiPiano::Giant] {
            for s in [Snapshot::Factory, Snapshot::Worship] {
                let law = NoteOnLaw::of(p, s);
                assert!((1..=127).any(|v| law.gain_db(60, v) != 0.0), "{p:?} {s:?}");
            }
        }
    }

    #[test]
    fn the_worship_snapshot_plays_brighter_layers() {
        // The gig's Color +14 moves every strike further up the layers.
        let law = NoteOnLaw::of(NiPiano::Grandeur, Snapshot::Worship);
        assert_eq!(law.play(60, 40).0, 70);
        assert_eq!(NoteOnLaw::of(NiPiano::Grandeur, Snapshot::Factory).play(60, 40).0, 56);
    }
}
