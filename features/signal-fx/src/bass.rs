//! `FTS Bass Rig`: signal's bass chain on a DI.
//!
//! The chain signal's bass rig builds from a preset
//! (`signal-bass`'s `build_profile`): a drive capture (optional), the amp
//! capture, a cab IR (optional), with a trim either side. Each capture is
//! a NAM model ([`signal_sampler::nam::NamProcessor`]), the cab a
//! convolution ([`signal_sampler::convolver::Convolver`]) — signal's own
//! processors, the same the rig runs. A preamp captured direct (a DI
//! preamp pedal) wants no cab.
//!
//! What it plays is its state, JSON: `{"amp": "…/Amp.nam", "drive":
//! "…/Drive.nam", "cab": "…/Cab.wav", "input_db": 0, "output_db": 0}`.
//! Loaded at `prepare` (off the audio thread); a model that will not load
//! leaves the rig a straight wire, said once in the log.

use std::path::PathBuf;

use daw_standalone::plugin::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};
use signal_sampler::convolver::Convolver;
use signal_sampler::nam::NamProcessor;

/// What the rig plays, as saved.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BassState {
    /// The amp (or preamp) capture, a `.nam` file.
    #[serde(default)]
    pub amp: Option<PathBuf>,
    /// A drive capture before the amp.
    #[serde(default)]
    pub drive: Option<PathBuf>,
    /// A cab impulse response after it.
    #[serde(default)]
    pub cab: Option<PathBuf>,
    /// Into the chain, dB.
    #[serde(default)]
    pub input_db: f64,
    /// Out of it, dB.
    #[serde(default)]
    pub output_db: f64,
}

/// The trims' range, dB either way.
const TRIM_DB: f64 = 24.0;

/// Parameter ids.
const INPUT: u32 = 0;
const OUTPUT: u32 = 1;

/// The rig.
pub struct BassRig {
    state: BassState,
    sample_rate: f64,
    prepared: bool,
    stages: Vec<Box<dyn PluginInstance>>,
    /// Two scratch buffers the stages hand the audio between.
    a: [Vec<f32>; 2],
    b: [Vec<f32>; 2],
}

impl BassRig {
    #[must_use]
    pub const fn new(sample_rate: f64) -> Self {
        Self {
            state: BassState {
                amp: None,
                drive: None,
                cab: None,
                input_db: 0.0,
                output_db: 0.0,
            },
            sample_rate,
            prepared: false,
            stages: Vec::new(),
            a: [Vec::new(), Vec::new()],
            b: [Vec::new(), Vec::new()],
        }
    }

    /// The stages the state names, loaded: drive, amp, cab.
    fn load(&self, block: u32) -> Vec<Box<dyn PluginInstance>> {
        let mut stages: Vec<Box<dyn PluginInstance>> = Vec::new();
        let block_frames = usize::try_from(block).unwrap_or(4_096);
        for model in [&self.state.drive, &self.state.amp].into_iter().flatten() {
            match NamProcessor::load(model, self.sample_rate, block_frames) {
                Ok(nam) => stages.push(Box::new(nam)),
                Err(why) => {
                    tracing::warn!(rig.model = %model.display(), why, "bass rig: a capture would not load");
                }
            }
        }
        if let Some(cab) = &self.state.cab {
            match Convolver::load(cab) {
                Ok(mut ir) => match ir.prepare(self.sample_rate, block) {
                    Ok(()) => stages.push(Box::new(ir)),
                    Err(why) => {
                        tracing::warn!(rig.cab = %cab.display(), why = %why, "bass rig: the cab would not prepare");
                    }
                },
                Err(why) => {
                    tracing::warn!(rig.cab = %cab.display(), why, "bass rig: the cab would not load");
                }
            }
        }
        stages
    }
}

fn db(value: f64) -> f32 {
    num_traits::ToPrimitive::to_f32(&10_f64.powf(value / 20.0)).unwrap_or(1.0)
}

impl PluginInstance for BassRig {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "fts.signal.bass_rig".into(),
            name: crate::BASS_RIG.into(),
            vendor: "FastTrackStudio".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }

    fn params(&mut self) -> Vec<PluginParamInfo> {
        vec![
            PluginParamInfo {
                id: INPUT,
                name: "Input".into(),
                min: -TRIM_DB,
                max: TRIM_DB,
                default: 0.0,
            },
            PluginParamInfo {
                id: OUTPUT,
                name: "Output".into(),
                min: -TRIM_DB,
                max: TRIM_DB,
                default: 0.0,
            },
        ]
    }

    fn param_value(&mut self, id: u32) -> Option<f64> {
        match id {
            INPUT => Some(self.state.input_db),
            OUTPUT => Some(self.state.output_db),
            _ => None,
        }
    }

    fn value_to_text(&mut self, id: u32, value: f64) -> Option<String> {
        (id <= OUTPUT).then(|| format!("{value:.1} dB"))
    }

    fn text_to_value(&mut self, id: u32, text: &str) -> Option<f64> {
        let number = text
            .trim()
            .trim_end_matches("dB")
            .trim()
            .parse::<f64>()
            .ok()?;
        (id <= OUTPUT).then(|| number.clamp(-TRIM_DB, TRIM_DB))
    }

    fn latency(&mut self) -> u32 {
        self.stages.iter_mut().map(|s| s.latency()).sum()
    }

    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.sample_rate = sample_rate;
        self.stages = self.load(block_size);
        let frames = usize::try_from(block_size).unwrap_or(4_096);
        for buffer in self.a.iter_mut().chain(self.b.iter_mut()) {
            buffer.resize(frames, 0.0);
        }
        self.prepared = true;
        Ok(())
    }

    fn is_prepared(&self) -> bool {
        self.prepared
    }

    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        for &(id, value) in events.params {
            match id {
                INPUT => self.state.input_db = value.clamp(-TRIM_DB, TRIM_DB),
                OUTPUT => self.state.output_db = value.clamp(-TRIM_DB, TRIM_DB),
                _ => {}
            }
        }
        let frames = out_l.len().min(out_r.len()).min(in_l.len()).min(in_r.len());
        if self.a[0].len() < frames {
            return Err(PluginError::BlockTooLarge);
        }
        let gain_in = db(self.state.input_db);
        let [al, ar] = &mut self.a;
        for ((l, r), (il, ir)) in al
            .iter_mut()
            .zip(ar.iter_mut())
            .zip(in_l.iter().zip(in_r))
            .take(frames)
        {
            *l = il * gain_in;
            *r = ir * gain_in;
        }
        // Each stage from one buffer into the other, and back.
        let mut flipped = false;
        for stage in &mut self.stages {
            let (from, to) = if flipped {
                (&self.b, &mut self.a)
            } else {
                (&self.a, &mut self.b)
            };
            let [tl, tr] = to;
            let [fl, fr] = from;
            let (Some(fl), Some(fr), Some(tl), Some(tr)) = (
                fl.get(..frames),
                fr.get(..frames),
                tl.get_mut(..frames),
                tr.get_mut(..frames),
            ) else {
                return Err(PluginError::BlockTooLarge);
            };
            stage.process_block(fl, fr, tl, tr, &PluginEvents::EMPTY)?;
            flipped = !flipped;
        }
        let last = if flipped { &self.b } else { &self.a };
        let gain_out = db(self.state.output_db);
        for (o, s) in out_l.iter_mut().zip(&last[0]).take(frames) {
            *o = s * gain_out;
        }
        for (o, s) in out_r.iter_mut().zip(&last[1]).take(frames) {
            *o = s * gain_out;
        }
        Ok(())
    }

    fn load_state(&mut self, state: &[u8]) -> Result<(), PluginError> {
        self.state = serde_json::from_slice(state)
            .map_err(|e| PluginError::LoadFailed(format!("bass rig state: {e}")))?;
        // The chain the new state names, loaded here — on the caller's
        // thread, never the audio's — at the size it was last prepared
        // for; not yet prepared, at the next prepare.
        if self.prepared {
            let block = u32::try_from(self.a[0].len()).unwrap_or(4_096);
            self.stages = self.load(block);
        }
        Ok(())
    }

    fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        serde_json::to_vec(&self.state).map_err(|e| PluginError::LoadFailed(e.to_string()))
    }

    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No capture named: a straight wire, through the trims.
    #[test]
    fn with_nothing_loaded_it_is_a_wire_with_trims() {
        let mut rig = BassRig::new(48_000.0);
        rig.load_state(br#"{"output_db": -6.0206}"#).expect("state");
        rig.prepare(48_000.0, 64).expect("prepare");
        let input = vec![0.5_f32; 64];
        let (mut l, mut r) = (vec![0.0_f32; 64], vec![0.0_f32; 64]);
        rig.process_block(&input, &input, &mut l, &mut r, &PluginEvents::EMPTY)
            .expect("process");
        assert!((l[10] - 0.25).abs() < 1e-3, "{}", l[10]);
        assert!((r[63] - 0.25).abs() < 1e-3);
    }

    /// What it plays survives a save and a load.
    #[test]
    fn its_state_round_trips() {
        let mut rig = BassRig::new(48_000.0);
        let state = br#"{"amp":"/x/amp.nam","cab":"/x/cab.wav","input_db":3.0}"#;
        rig.load_state(state).expect("state");
        let saved = rig.save_state().expect("save");
        let back: BassState = serde_json::from_slice(&saved).expect("json");
        assert_eq!(
            back.amp.as_deref(),
            Some(std::path::Path::new("/x/amp.nam"))
        );
        assert!((back.input_db - 3.0).abs() < 1e-12);
    }
}
