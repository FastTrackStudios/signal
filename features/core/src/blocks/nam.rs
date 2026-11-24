//! A NAM capture: the amp. `neural-amp-modeler`, pure Rust (valverig).
//!
//! Captures are mono in and out and run on `f64`; the lane is stereo, so
//! the input is folded to mono and the output fed to both sides (stereo
//! width comes after the cab). Moved from `sampler::nam::NamProcessor`,
//! without its file loading (the host loads the bytes) or its DI
//! calibration cache (the rig's, desktop-only).

use alloc::vec;
use alloc::vec::Vec;

use neural_amp_modeler::NamModel;

use crate::block::{BlockProcessor, Unavailable};
use crate::param::{ParamSpec, Unit};

const PARAMS: &[ParamSpec] = &[
    ParamSpec::new("input", "Input", Unit::Db, -24.0, 24.0, 0.0),
    ParamSpec::new("output", "Output", Unit::Db, -24.0, 24.0, 0.0),
];

pub struct Nam {
    model: NamModel,
    in_mono: Vec<f64>,
    out_mono: Vec<f64>,
    input: f64,
    output: f64,
}

impl Nam {
    /// A model from a `.nam` file's bytes, prepared for `sample_rate`.
    ///
    /// # Errors
    ///
    /// When the bytes aren't a model this engine runs.
    pub fn from_bytes(
        bytes: &[u8],
        sample_rate: f32,
        max_block: usize,
    ) -> Result<Self, Unavailable> {
        let mut model = NamModel::from_bytes(bytes).map_err(|why| Unavailable::Failed { why })?;
        let sr = f64::from(sample_rate);
        model.reset(sr, max_block);
        if let Some(expected) = model.expected_sample_rate() {
            if (expected - sr).abs() > 1.0 {
                tracing::warn!(
                    nam.expected_sample_rate = expected,
                    nam.sample_rate = sr,
                    "NAM capture trained at another rate: its voicing will be off"
                );
            }
        }
        Ok(Self {
            model,
            in_mono: vec![0.0; max_block],
            out_mono: vec![0.0; max_block],
            input: 1.0,
            output: 1.0,
        })
    }
}

impl BlockProcessor for Nam {
    fn params(&self) -> &'static [ParamSpec] {
        PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let g = f64::from(crate::util::db_to_gain(value));
        match index {
            0 => self.input = g,
            1 => self.output = g,
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.in_mono.iter_mut().for_each(|s| *s = 0.0);
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len()).min(self.in_mono.len());
        let (Some(inp), Some(out)) = (self.in_mono.get_mut(..n), self.out_mono.get_mut(..n)) else {
            return;
        };
        for ((m, l), r) in inp.iter_mut().zip(left.iter()).zip(right.iter()) {
            *m = (f64::from(*l) + f64::from(*r)) * 0.5 * self.input;
        }
        self.model.process(inp, out);
        for ((y, l), r) in out.iter().zip(left.iter_mut()).zip(right.iter_mut()) {
            let v = signal_dsp_core::num::narrow(*y * self.output);
            *l = v;
            *r = v;
        }
    }
}
