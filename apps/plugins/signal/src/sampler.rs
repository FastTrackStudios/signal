//! The sampler as a Signal block (desktop): a `.signalpack` played from
//! MIDI, over `sampler::SamplerBank`. What the v0 plugin did, now one
//! block in a lane (an instrument lane: no audio in).
//!
//! It lives here, not in signal-core, because the bank streams its samples
//! from disk on threads of its own: a desktop engine. The compiled-in
//! sampler for iOS and the browser is the sampler engine's core behind the
//! same block (signal#26, with the drum submixer).

use std::path::Path;

use signal_core::block::{
    Backend, BlockFactory, BlockKind, BlockProcessor, BuildContext, Built, Midi, ModSourceSpec,
    Unavailable,
};
use signal_core::host::AssetRef;
use signal_core::param::{ParamSpec, Unit};
use signal_core::platform::Platforms;
use signal_core::state::{BlockSpec, LaneSpec, Preset};

use crate::config::{INSTRUMENT_ID, RigConfig};

const PARAMS: &[ParamSpec] = &[
    ParamSpec::new("attack", "Attack", Unit::Ms, 0.0, 2000.0, 5.0),
    ParamSpec::new("release", "Release", Unit::Ms, 1.0, 5000.0, 200.0),
    ParamSpec::new("level", "Level", Unit::Db, -60.0, 12.0, 0.0),
    // Read when the pack is loaded.
    ParamSpec::new("cache_mb", "Cache", Unit::Plain(0), 64.0, 65_536.0, 8192.0),
];

pub struct Sampler {
    bank: sampler::SamplerBank,
    scratch: Vec<f32>,
    level: f32,
}

impl BlockProcessor for Sampler {
    fn params(&self) -> &'static [ParamSpec] {
        PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let ms = value.round().max(0.0) as u32;
        match index {
            0 => self.bank.set_attack_ms(INSTRUMENT_ID, ms),
            1 => self.bank.set_release_ms(INSTRUMENT_ID, ms),
            2 => self.level = 10f32.powf(value / 20.0),
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.bank.all_notes_off(INSTRUMENT_ID);
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len()).min(self.scratch.len() / 2);
        let Some(scratch) = self.scratch.get_mut(..n * 2) else {
            return;
        };
        scratch.fill(0.0);
        // `render` adds into an interleaved stereo buffer.
        self.bank.render(scratch);
        for ((frame, l), r) in scratch
            .chunks_exact(2)
            .zip(left.iter_mut())
            .zip(right.iter_mut())
        {
            *l += frame[0] * self.level;
            *r += frame[1] * self.level;
        }
    }

    fn midi(&mut self, event: Midi) {
        match event {
            Midi::NoteOn { note, velocity, .. } => {
                self.bank.note_on(INSTRUMENT_ID, note, velocity.max(1))
            }
            Midi::NoteOff { note, .. } => self.bank.note_off(INSTRUMENT_ID, note),
            Midi::Cc { cc, value, .. } => self.bank.cc(INSTRUMENT_ID, cc, value),
            Midi::PitchBend { .. } => {}
        }
    }

    fn mod_sources(&self) -> &'static [ModSourceSpec] {
        &[]
    }
}

/// Makes the `sampler` block, on the desktop.
pub struct SamplerFactory;

impl BlockFactory for SamplerFactory {
    fn runs_on(&self, kind: &BlockKind) -> Option<Platforms> {
        matches!(kind, BlockKind::Signal(name) if name == "sampler").then_some(Platforms::DESKTOP)
    }

    fn build(&self, spec: &BlockSpec, cx: &BuildContext<'_>) -> Result<Built, Unavailable> {
        let pack = spec.asset.as_ref().ok_or_else(|| Unavailable::Missing {
            what: format!("{}: no pack named", spec.id),
        })?;
        let cache_mb = spec
            .params
            .get("cache_mb")
            .copied()
            .unwrap_or(8192.0)
            .max(64.0) as usize;
        let sr = cx.sample_rate.round() as u32;
        let mut bank = sampler::SamplerBank::with_cache_budget(sr, Some(cache_mb * 1024 * 1024));
        // The bank streams the pack from disk itself (on its own preload
        // thread), so it takes the path, not the host's bytes.
        bank.load_pack(INSTRUMENT_ID, Path::new(&pack.path))
            .map_err(|e| Unavailable::Failed {
                why: format!("{}: {e}", pack.path),
            })?;
        tracing::info!(pack = %pack.path, sr, "sampler block: pack loaded");
        let block = Sampler {
            bank,
            scratch: vec![0.0; cx.max_block * 2],
            level: 1.0,
        };
        Ok(Built {
            backend: Backend::Linked(Box::new(block)),
            params: PARAMS,
            sources: &[],
        })
    }
}

/// The v0 configuration (`FTS_SIGNAL_RIG`) as a preset: the pack in an
/// instrument lane.
#[must_use]
pub fn preset_for(cfg: &RigConfig) -> Preset {
    let mut block = BlockSpec::signal("sampler", "sampler").asset(AssetRef::path(cfg.pack.clone()));
    if let Some(ms) = cfg.attack_ms {
        block = block.param("attack", ms as f32);
    }
    if let Some(ms) = cfg.release_ms {
        block = block.param("release", ms as f32);
    }
    if let Some(mb) = cfg.cache_budget_mb {
        block = block.param("cache_mb", mb as f32);
    }
    Preset {
        name: "Sampler".into(),
        inputs: 2,
        outputs: 2,
        lanes: vec![
            // The host's audio through, as v0 summed it under the rig.
            LaneSpec {
                name: "Through".into(),
                port: None,
                input: vec![0, 1],
                output: 0,
                gain_db: 0.0,
                mute: false,
                blocks: vec![],
                ..LaneSpec::default()
            },
            LaneSpec {
                name: "Sampler".into(),
                port: None,
                input: vec![],
                output: 0,
                gain_db: 0.0,
                mute: false,
                blocks: vec![block],
                ..LaneSpec::default()
            },
        ],
        macros: vec![],
    }
}
