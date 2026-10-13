//! FTS Mic — CLAP/VST3 dual-capsule microphone modeller.
//!
//! Input is a Sphere-style capsule pair (left = front capsule, right =
//! rear); output is the modelled mic, mono on both channels — or, in 180
//! mode, a stereo pair: mic 1 forward on the left, mic 2 facing back on the
//! right (its own measured models, `<rate>[-LX]-180/`), balanced by Mic Pan
//! and crossfed by Stereo Width. The engine is
//! [`mic_dsp`]; this crate is the host shell: parameters, latency, and
//! loading the models on a loader thread so the audio thread never touches
//! the disk or the allocator.
//!
//! Models are `.micm` files in `$FTS_MIC_MODELS/<sample rate>[-LX]/`
//! (default `~/.local/share/fts/mic-models`, a checkout of the private
//! `mic-models` repository: `just mic-models-install`), one per mic, named as
//! the mic list with spaces as underscores. The default mic (LD-47K) is also
//! built in, and stands in for any mic that is not installed.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_channel::{Receiver, Sender, bounded};
use mic_dsp::{DualMic, DualSettings, MicChain, Settings, Solo, StereoMic, StereoSettings, latency};
use nice_plug::prelude::*;
use nice_plug_dioxus::{DioxusState, create_dioxus_editor_with_state};
pub use mic_ui::params::MicParams;
use mic_ui::params::MicUiState;

const PLUGIN_NAME: &str = "FTS Mic";

#[path = "common.rs"]
mod common;
pub use common::MICS;
use common::{index, load_model_or_default};


/// Which models a chain needs, and at what rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Want {
    mic1: usize,
    mic2: usize,
    dual: bool,
    /// The 180 pair (mic 2's backward model) rather than a dual mix.
    stereo: bool,
    source: usize,
    sample_rate: u32,
}

/// What the loader hands the audio thread.
enum Loaded {
    Single(Box<MicChain>),
    Dual(Box<DualMic>),
    Stereo(Box<StereoMic>),
}

/// The loader thread: builds chains off the audio thread, and drops the
/// ones the audio thread retires.
fn loader(requests: &Receiver<Want>, ready: &Sender<(Want, Loaded)>, trash: &Receiver<Loaded>, alive: &AtomicBool) {
    while alive.load(Ordering::Relaxed) {
        while trash.try_recv().is_ok() {}
        let Ok(mut want) = requests.recv_timeout(std::time::Duration::from_millis(50)) else { continue };
        // Only the newest request matters.
        while let Ok(newer) = requests.try_recv() {
            want = newer;
        }
        let Some(m1) = load_model_or_default(want.mic1, want.source, want.sample_rate, "") else { continue };
        let loaded = if want.stereo {
            let Some(m2) = load_model_or_default(want.mic2, want.source, want.sample_rate, "-180") else { continue };
            Loaded::Stereo(Box::new(StereoMic::new(MicChain::new(m1), MicChain::new(m2))))
        } else if want.dual {
            let Some(m2) = load_model_or_default(want.mic2, want.source, want.sample_rate, "") else { continue };
            Loaded::Dual(Box::new(DualMic::new(MicChain::new(m1), MicChain::new(m2))))
        } else {
            Loaded::Single(Box::new(MicChain::new(m1)))
        };
        if ready.send((want, loaded)).is_err() {
            break;
        }
    }
}

pub struct FtsMic {
    params: Arc<MicParams>,
    ui_state: Arc<MicUiState>,
    editor_state: Arc<DioxusState>,
    engine: Option<Loaded>,
    have: Option<Want>,
    asked: Option<Want>,
    sample_rate: u32,
    requests: Sender<Want>,
    ready: Receiver<(Want, Loaded)>,
    trash: Sender<Loaded>,
    alive: Arc<AtomicBool>,
}

impl Default for FtsMic {
    fn default() -> Self {
        let (req_tx, req_rx) = bounded(16);
        let (ready_tx, ready_rx) = bounded(2);
        let (trash_tx, trash_rx) = bounded(4);
        let alive = Arc::new(AtomicBool::new(true));
        let flag = alive.clone();
        // The thread lives as long as the plugin; a failed spawn leaves the
        // plugin silent rather than crashing the host.
        let _ = std::thread::Builder::new().name("fts-mic-loader".into()).spawn(move || loader(&req_rx, &ready_tx, &trash_rx, &flag));
        let params = Arc::new(MicParams::default());
        Self {
            params: params.clone(),
            ui_state: Arc::new(MicUiState::new(params)),
            editor_state: DioxusState::new(|| (mic_ui::view::EDITOR_W, mic_ui::view::EDITOR_H)).with_resize_hint(mic_ui::view::resize_hint()),
            engine: None,
            have: None,
            asked: None,
            sample_rate: 48_000,
            requests: req_tx,
            ready: ready_rx,
            trash: trash_tx,
            alive,
        }
    }
}

impl Drop for FtsMic {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}

impl FtsMic {
    /// 180 with Link: mic 2 is mic 1's type, pattern, filter and axis.
    fn linked(&self) -> bool {
        self.params.stereo180.value() && self.params.link.value()
    }

    fn want(&self) -> Want {
        Want {
            mic1: index(&self.params.type1),
            mic2: if self.linked() { index(&self.params.type1) } else { index(&self.params.type2) },
            dual: self.params.dual.value(),
            stereo: self.params.stereo180.value(),
            source: index(&self.params.source),
            sample_rate: self.sample_rate,
        }
    }

    fn settings(&self, second: bool) -> Settings {
        let p = &self.params;
        let (pattern, filter, axis) = if second && !self.linked() { (&p.pattern2, &p.filter2, &p.axis2) } else { (&p.pattern1, &p.filter1, &p.axis1) };
        Settings {
            pattern: index(pattern),
            axis_deg: f64::from(axis.value()),
            low_cut: index(filter),
            proximity: f64::from(p.proximity.value()),
            output_db: f64::from(p.output.value()),
            phase_invert: p.phase.value(),
            rear_trim_db: f64::from(p.rear_trim.value()),
            swap: p.swap.value(),
        }
    }

    /// Ask for different models if the selection changed; take delivered
    /// ones; push the current controls into whatever is running.
    fn sync(&mut self) {
        let want = self.want();
        if self.asked != Some(want) && self.requests.try_send(want).is_ok() {
            self.asked = Some(want);
        }
        if let Ok((got, loaded)) = self.ready.try_recv() {
            if let Some(old) = self.engine.replace(loaded) {
                let _ = self.trash.try_send(old);
            }
            self.have = Some(got);
        }
        let (s1, s2) = (self.settings(false), self.settings(true));
        let dual = DualSettings {
            mix: f64::from(self.params.mix.value()) / 100.0,
            align_cm: f64::from(self.params.align.value()),
            solo: match self.params.solo.value() {
                1 => Solo::Mic1,
                2 => Solo::Mic2,
                _ => Solo::Off,
            },
        };
        match self.engine.as_mut() {
            Some(Loaded::Single(c)) => {
                if c.settings() != s1 {
                    c.apply(s1, false);
                }
            }
            Some(Loaded::Dual(d)) => {
                if d.mic1.settings() != s1 {
                    d.mic1.apply(s1, false);
                }
                if d.mic2.settings() != s2 {
                    d.mic2.apply(s2, false);
                }
                d.set(dual);
            }
            Some(Loaded::Stereo(e)) => {
                if e.forward.settings() != s1 {
                    e.forward.apply(s1, false);
                }
                if e.backward.settings() != s2 {
                    e.backward.apply(s2, false);
                }
                // after the chains: the backward mic takes the forward one's
                // proximity base
                e.set(StereoSettings {
                    pan: f64::from(self.params.pan.value()) / 100.0,
                    width: f64::from(self.params.width.value()) / 100.0,
                });
            }
            None => {}
        }
    }
}

impl Plugin for FtsMic {
    const NAME: &'static str = PLUGIN_NAME;
    const VENDOR: &'static str = "FastTrackStudio";
    const URL: &'static str = "https://fasttrackstudio.com";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    /// Capsule pair in (front, rear), mic out on both channels.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    type Editor = nice_plug_dioxus::editor::DioxusEditor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        create_dioxus_editor_with_state(self.editor_state.clone(), self.ui_state.clone(), mic_ui::view::App)
    }

    fn activate(&mut self, _layout: &AudioIOLayout, buffer_config: &BufferConfig, context: &mut impl ActivateContext<Self>) -> bool {
        // The sample rate is a whole number of hertz in every host.
        self.sample_rate = u32::try_from(dsp_core::f32_to_index(buffer_config.sample_rate.round())).unwrap_or(48_000);
        context.set_latency_samples(u32::try_from(latency(f64::from(self.sample_rate))).unwrap_or(24));
        self.asked = None;
        true
    }

    fn reset(&mut self) {
        match self.engine.as_mut() {
            Some(Loaded::Single(c)) => c.reset(),
            Some(Loaded::Dual(d)) => {
                d.mic1.reset();
                d.mic2.reset();
            }
            Some(Loaded::Stereo(e)) => {
                e.forward.reset();
                e.backward.reset();
            }
            None => {}
        }
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _context: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.sync();
        let ready = self.have.is_some_and(|h| h.sample_rate == self.sample_rate);
        for mut frame in buffer.iter_samples() {
            let mut it = frame.iter_mut();
            let (Some(l), Some(r)) = (it.next(), it.next()) else { continue };
            let (a, b) = if ready {
                match self.engine.as_mut() {
                    Some(Loaded::Single(c)) => {
                        let y = c.process(f64::from(*l), f64::from(*r));
                        (y, y)
                    }
                    Some(Loaded::Dual(d)) => {
                        let y = d.process(f64::from(*l), f64::from(*r));
                        (y, y)
                    }
                    Some(Loaded::Stereo(e)) => e.process(f64::from(*l), f64::from(*r)),
                    None => (0.0, 0.0),
                }
            } else {
                (0.0, 0.0)
            };
            *l = dsp_core::num::f64_to_f32(a);
            *r = dsp_core::num::f64_to_f32(b);
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsMic {
    const CLAP_ID: &'static str = "com.fasttrackstudio.mic";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Dual-capsule microphone modelling");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Stereo];
}

impl Vst3Plugin for FtsMic {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsMicModeller01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Tools];
}

#[cfg(feature = "export")]
nice_export_clap!(FtsMic);
#[cfg(feature = "export")]
nice_export_vst3!(FtsMic);
