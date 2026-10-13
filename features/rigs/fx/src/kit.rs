//! `FTS Kit Mic`: a kit's sampled voices for some of its pieces and mics,
//! played from the track's own MIDI.
//!
//! A signal kit is a `.signalpreset`: pieces (`kick`, `snare-a`, `rtom-a`
//! …), each an engine over a pack of multi-mic recordings — one file per
//! mic per hit (`In 1`, `In 2`, `Sub`, `Overhead`, `Room Close` …) — and a
//! note map. signal plays one [`SampleEngine`] per (piece, mic), each
//! solo'd to its mic, and keeps a piece's mics together by giving every
//! one the same notes: the round robin is chosen the same way from the
//! same sequence.
//!
//! This does the same on a session's own tracks. Which pieces and mics it
//! plays is its state, so the kit lands where the recorded one is:
//!
//! - a piece's **close** mics on that piece's Sample track,
//! - every piece's **overhead** mics on a sample track into the kit's
//!   overheads, its **room** mics on one into the rooms —
//!
//! each fed the same notes for a piece, so the hits agree across them.
//!
//! With `split`, one Kit Mic plays a piece's every mic on its Sample
//! track: the close mics on the main pair, the overheads on 3/4 and the
//! rooms on 5/6 (`PluginInstance::aux_out`), which the track sends into
//! the recorded overheads and rooms — no track of their own.
//!
//! State, JSON: `{"preset": "…/Heavy Thwacky.signalpreset", "pieces":
//! ["kick"], "mics": ["close"], "gain_db": 0, "split": false}`. `pieces` absent: every
//! piece. A mic is a group — `close` (any not a bus mic), `overhead`,
//! `room` — or a mic's own name.

use std::path::{Path, PathBuf};

use daw_standalone::plugin::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};
use signal_sampler::SampleEngine;
use signal_sampler::kit_tracks::KitRouting;

/// What the voices play, as saved.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct KitState {
    /// The kit, a `.signalpreset`.
    #[serde(default)]
    pub preset: Option<PathBuf>,
    /// Which of its pieces, by the preset's ids; none named, all of them.
    #[serde(default)]
    pub pieces: Option<Vec<String>>,
    /// Which mics: `close`, `overhead`, `room`, or a mic's name.
    #[serde(default)]
    pub mics: Vec<String>,
    /// Level, dB.
    #[serde(default)]
    pub gain_db: f64,
    /// Close mics on the main pair, overheads on 3/4, rooms on 5/6 —
    /// rather than every mic on the main pair.
    #[serde(default)]
    pub split: bool,
}

/// Which output a mic plays on when split: 0 the main pair (a close
/// mic), 1 the 3/4 pair (an overhead), 2 the 5/6 pair (a room).
#[must_use]
pub fn output_of(id: &str) -> usize {
    if wants(&["overhead".to_owned()], id) {
        1
    } else if wants(&["room".to_owned()], id) {
        2
    } else {
        0
    }
}

/// Whether the mic `id` is one `wanted` names.
#[must_use]
pub fn wants(wanted: &[String], id: &str) -> bool {
    let bus = signal_sampler::mixer::mic_is_bus(id);
    let lower = id.to_ascii_lowercase();
    wanted
        .iter()
        .any(|w| match w.trim().to_ascii_lowercase().as_str() {
            "close" => !bus,
            "overhead" | "oh" => bus && (lower == "oh" || lower.contains("overhead")),
            "room" => bus && (lower.starts_with("room") || lower.contains("ambien")),
            exact => exact == lower,
        })
}

/// The piece the kit `preset` plays for `note`, by its id (`kick`,
/// `rtom-a`).
///
/// As its note map says — the first of them, for a layered note. `None`
/// for a preset that will not read, or a note it maps to nothing.
#[must_use]
pub fn piece_for_note(preset: &Path, note: u8) -> Option<String> {
    let spec = signal_sampler::preset_spec::PresetSpec::from_file(preset).ok()?;
    spec.note_routing
        .iter()
        .find(|r| r.note == note)
        .and_then(|r| r.targets.first().cloned())
}

/// One piece's voices: its id, its preset settings, an engine per mic.
struct Piece {
    id: String,
    transpose: i8,
    gain: f32,
    /// Each mic's engine and the output it plays on ([`output_of`]).
    engines: Vec<(SampleEngine, usize)>,
}

/// The kit's voices.
pub struct KitMic {
    state: KitState,
    sample_rate: f64,
    /// The rate the voices were built at.
    built_at: f64,
    prepared: bool,
    routing: KitRouting,
    pieces: Vec<Piece>,
    /// One engine's block, interleaved stereo.
    scratch: Vec<f32>,
    /// The 3/4 and 5/6 outputs' last block, left and right.
    aux: [(Vec<f32>, Vec<f32>); 2],
    /// How long the last block was.
    frames: usize,
}

fn resolve(path: &str, base: &Path) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_absolute() { p } else { base.join(p) }
}

fn gain(db: f64) -> f32 {
    num_traits::ToPrimitive::to_f32(&10_f64.powf(db / 20.0)).unwrap_or(1.0)
}

impl KitMic {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        Self {
            state: KitState::default(),
            sample_rate,
            built_at: 0.0,
            prepared: false,
            routing: KitRouting::default(),
            pieces: Vec::new(),
            scratch: Vec::new(),
            aux: Default::default(),
            frames: 0,
        }
    }

    /// The voices the state names, at `sample_rate`, their samples
    /// loaded. Heavy — packs read, samples decoded — so never on the
    /// audio thread.
    fn build(&mut self) {
        self.pieces.clear();
        self.routing = KitRouting::default();
        self.built_at = self.sample_rate;
        let Some(preset_path) = self.state.preset.clone() else {
            return;
        };
        let preset = match signal_sampler::preset_spec::PresetSpec::from_file(&preset_path) {
            Ok(p) => p,
            Err(why) => {
                tracing::warn!(kit.preset = %preset_path.display(), why = %why, "kit mic: the preset would not load");
                return;
            }
        };
        let dir = preset_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf();
        let rate = num_traits::ToPrimitive::to_u32(&self.sample_rate.round()).unwrap_or(48_000);
        for er in &preset.engines {
            if self
                .state
                .pieces
                .as_ref()
                .is_some_and(|only| !only.iter().any(|p| p.eq_ignore_ascii_case(&er.id)))
            {
                continue;
            }
            match self.piece(er, &dir, rate) {
                Ok(piece) if !piece.engines.is_empty() => self.pieces.push(piece),
                Ok(_) => {}
                Err(why) => {
                    tracing::warn!(kit.piece = %er.id, why, "kit mic: a piece would not load");
                }
            }
        }
        self.routing = KitRouting::from_preset(&preset);
        tracing::info!(
            kit.preset = %preset_path.display(),
            kit.pieces = self.pieces.len(),
            kit.engines = self.pieces.iter().map(|p| p.engines.len()).sum::<usize>(),
            "kit mic: voices loaded"
        );
    }

    /// One piece's engines, one per wanted mic, each preloaded with its
    /// mic's samples only.
    fn piece(
        &self,
        er: &signal_sampler::preset_spec::PresetEngineRef,
        dir: &Path,
        rate: u32,
    ) -> Result<Piece, String> {
        let engine_path = resolve(&er.engine, dir);
        let spec = signal_sampler::engine_spec::EngineSpec::from_file(&engine_path)
            .map_err(|e| e.to_string())?;
        let pack = resolve(
            &spec.block.pack,
            engine_path.parent().unwrap_or_else(|| Path::new("")),
        );
        let patch = signal_sampler::PlayerPatch::from_pack(&pack).map_err(|e| e.to_string())?;
        let section = patch
            .spec
            .sections
            .first()
            .map(|s| s.id.clone())
            .unwrap_or_default();
        let mut gain_db = 0.0_f64;
        for ov in spec.block.overrides.iter().chain(er.overrides.iter()) {
            if matches!(ov.param.as_str(), "gain_db" | "gain") {
                gain_db += f64::from(ov.value);
            }
        }
        let mut engines = Vec::new();
        for mic in patch.spec.mics.iter().map(|m| m.id.clone()) {
            if !wants(&self.state.mics, &mic) {
                continue;
            }
            let mut engine = SampleEngine::new(patch.clone(), rate, section.clone(), mic.clone());
            engine.set_solo_mic(Some(mic.clone()));
            if !er.articulation.is_empty() {
                engine.pin_articulation(Some(er.articulation.clone()));
            }
            if !er.choke_group.is_empty() {
                engine.set_choke_group(Some(&er.choke_group), &er.choke_on);
            }
            let paths: Vec<PathBuf> = patch
                .spec
                .zones
                .iter()
                .zip(patch.zone_paths.iter())
                .filter(|(z, _)| z.mic == mic)
                .map(|(_, p)| p.clone())
                .collect();
            let _ = engine
                .cache_handle()
                .preload(paths.iter().map(PathBuf::as_path));
            let output = if self.state.split { output_of(&mic) } else { 0 };
            engines.push((engine, output));
        }
        Ok(Piece {
            id: er.id.clone(),
            transpose: er.transpose,
            gain: gain(gain_db),
            engines,
        })
    }

    /// One note to the pieces it routes to.
    fn note(&mut self, note: u8, velocity: u8, on: bool) {
        let targets: Vec<(String, Option<String>)> = match self.routing.targets(note) {
            Some(targets) => targets
                .iter()
                .map(|t| {
                    (
                        t.piece.clone(),
                        (!t.articulation.is_empty()).then(|| t.articulation.clone()),
                    )
                })
                .collect(),
            None if self.routing.is_empty() => {
                self.pieces.iter().map(|p| (p.id.clone(), None)).collect()
            }
            None => Vec::new(),
        };
        for (id, articulation) in targets {
            let Some(piece) = self.pieces.iter_mut().find(|p| p.id == id) else {
                continue;
            };
            let key = u8::try_from(
                i16::from(note)
                    .saturating_add(i16::from(piece.transpose))
                    .clamp(0, 127),
            )
            .unwrap_or(note);
            for (engine, _) in &mut piece.engines {
                if on {
                    engine.note_on_articulated(key, velocity, articulation.as_deref());
                } else {
                    engine.note_off(key);
                }
            }
        }
    }

    /// Every voice's next `frames`, summed into its output from `at`: the
    /// main pair `out`, or the aux pairs.
    fn render(&mut self, out_l: &mut [f32], out_r: &mut [f32], (at, frames): (usize, usize)) {
        let level = gain(self.state.gain_db);
        let want = frames.saturating_mul(2);
        let Self {
            pieces,
            scratch,
            aux,
            ..
        } = self;
        for piece in pieces.iter_mut() {
            for (engine, output) in &mut piece.engines {
                let Some(scratch) = scratch.get_mut(..want) else {
                    return;
                };
                scratch.fill(0.0);
                engine.render(scratch);
                let g = piece.gain * level;
                let (to_l, to_r): (&mut [f32], &mut [f32]) = match output.checked_sub(1) {
                    None => (&mut *out_l, &mut *out_r),
                    Some(pair) => match aux.get_mut(pair) {
                        Some((l, r)) => (l.as_mut_slice(), r.as_mut_slice()),
                        None => continue,
                    },
                };
                let (Some(ls), Some(rs)) = (to_l.get_mut(at..), to_r.get_mut(at..)) else {
                    return;
                };
                for ((l, r), pair) in ls
                    .iter_mut()
                    .zip(rs.iter_mut())
                    .zip(scratch.chunks_exact(2))
                {
                    if let [left, right] = pair {
                        *l += left * g;
                        *r += right * g;
                    }
                }
            }
        }
    }
}

impl PluginInstance for KitMic {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "fts.signal.kit_mic".into(),
            name: crate::KIT_MIC.into(),
            vendor: "FastTrackStudio".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }

    fn params(&mut self) -> Vec<PluginParamInfo> {
        Vec::new()
    }

    fn param_value(&mut self, _id: u32) -> Option<f64> {
        None
    }

    fn value_to_text(&mut self, _id: u32, _value: f64) -> Option<String> {
        None
    }

    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }

    fn latency(&mut self) -> u32 {
        0
    }

    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.sample_rate = sample_rate;
        if (self.built_at - sample_rate).abs() > f64::EPSILON {
            self.build();
        }
        let frames = usize::try_from(block_size).unwrap_or(4_096);
        self.scratch.resize(frames.saturating_mul(2), 0.0);
        for (l, r) in &mut self.aux {
            l.resize(frames, 0.0);
            r.resize(frames, 0.0);
        }
        self.prepared = true;
        Ok(())
    }

    fn is_prepared(&self) -> bool {
        self.prepared
    }

    fn process_block(
        &mut self,
        _in_l: &[f32],
        _in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        use daw_proto::MidiEvent;
        let frames = out_l.len().min(out_r.len());
        out_l.fill(0.0);
        out_r.fill(0.0);
        self.frames = frames;
        for (l, r) in &mut self.aux {
            if l.len() < frames || r.len() < frames {
                // A block longer than prepared: grown once, not per block.
                l.resize(frames, 0.0);
                r.resize(frames, 0.0);
            }
            l.fill(0.0);
            r.fill(0.0);
        }
        // Each note where it falls in the block: the voices rendered up to
        // it, then the note.
        let mut at = 0_usize;
        for event in events.midi {
            let offset = usize::try_from(event.offset).unwrap_or(0).min(frames);
            if offset > at {
                self.render(out_l, out_r, (at, offset.saturating_sub(at)));
                at = offset;
            }
            match &event.message {
                MidiEvent::NoteOn { key, velocity, .. } => {
                    let on = velocity.get() > 0;
                    self.note(key.get(), velocity.get(), on);
                }
                MidiEvent::NoteOff { key, .. } => self.note(key.get(), 0, false),
                _ => {}
            }
        }
        if frames > at {
            self.render(out_l, out_r, (at, frames.saturating_sub(at)));
        }
        Ok(())
    }

    fn load_state(&mut self, state: &[u8]) -> Result<(), PluginError> {
        self.state = serde_json::from_slice(state)
            .map_err(|e| PluginError::LoadFailed(format!("kit mic state: {e}")))?;
        self.build();
        Ok(())
    }

    fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        serde_json::to_vec(&self.state).map_err(|e| PluginError::LoadFailed(e.to_string()))
    }

    fn deactivate(&mut self) {
        self.prepared = false;
    }

    fn aux_out(&self, pair: usize) -> Option<(&[f32], &[f32])> {
        if !self.state.split {
            return None;
        }
        let (l, r) = self.aux.get(pair)?;
        Some((l.get(..self.frames)?, r.get(..self.frames)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Split, a close mic plays on the main pair, an overhead on 3/4, a
    /// room on 5/6.
    #[test]
    fn a_mic_plays_on_its_groups_output() {
        assert_eq!(output_of("In 1"), 0);
        assert_eq!(output_of("Sub"), 0);
        assert_eq!(output_of("Overhead"), 1);
        assert_eq!(output_of("OH"), 1);
        assert_eq!(output_of("Room Close"), 2);
        assert_eq!(output_of("Room Far"), 2);
    }

    #[test]
    fn mic_groups_pick_the_mics_they_name() {
        let close = ["close".to_owned()];
        let oh = ["overhead".to_owned()];
        let room = ["room".to_owned()];
        assert!(wants(&close, "In 1") && wants(&close, "Sub"));
        assert!(!wants(&close, "Overhead") && !wants(&close, "Room Far"));
        assert!(wants(&oh, "Overhead") && !wants(&oh, "Room Close"));
        assert!(wants(&room, "Room Close") && wants(&room, "Room Mono") && !wants(&room, "In 2"));
        assert!(wants(&["in 2".to_owned()], "In 2") && !wants(&["in 2".to_owned()], "In 1"));
    }

    /// With no kit named it is silence, and asks nothing of the disk.
    #[test]
    fn with_no_kit_it_is_silent() {
        let mut kit = KitMic::new(48_000.0);
        kit.prepare(48_000.0, 32).expect("prepare");
        let (mut l, mut r) = (vec![1.0_f32; 32], vec![1.0_f32; 32]);
        kit.process_block(&[0.0; 32], &[0.0; 32], &mut l, &mut r, &PluginEvents::EMPTY)
            .expect("process");
        assert!(l.iter().chain(&r).all(|s| *s == 0.0));
    }
}
