//! Browser keys rig — the AudioWorklet entry.
//!
//! Wraps daw-standalone's [`WebRenderer`] (the wasm worklet renderer) and
//! signal-sampler's headless [`KeysRig`]: the rig's `Hosting::Lanes`
//! topology (rig folder → engine folders → layer tracks, one
//! [`KeysInstrument`] per lane) is seeded into the SAME `Standalone` the
//! worklet renders, packs arrive as transferred bytes
//! ([`signal_sampler::pack_registry`]), and live MIDI rides the wasm
//! live-MIDI queue (`Standalone::push_live_midi` → per-block drain).
//!
//! This crate exists because the dependency arrow is one-way —
//! signal-sampler depends on the `daw` facade (which contains
//! daw-standalone), so daw-standalone can never depend on signal-sampler.
//! The worklet-with-keys entry lives here, on top of both.
//!
//! [`WebRenderer`]: daw_standalone::audio_engine::web::WebRenderer
//! [`KeysRig`]: signal_sampler::KeysRig
//! [`KeysInstrument`]: signal_sampler::KeysInstrument

use facet::Facet;
use signal_sampler::keys_rig::{LaneEngine, LaneLayer, LaneProgram};
use signal_sampler::rig_node::Container;

// ── The LaneProgram wire shape ──────────────────────────────────────────────
//
// `LaneProgram` itself carries no derives (it holds compiled trees), so the
// worklet takes a Facet mirror parsed from JSON. The `tree` leaves are the
// same `Container` the native styx profiles serialize.

/// One layer: its name + composition subtree (zone included).
#[derive(Debug, Clone, Facet)]
pub struct WireLayer {
    pub name: String,
    pub tree: Container,
}

/// One engine: a folder of layers.
#[derive(Debug, Clone, Facet)]
pub struct WireEngine {
    pub name: String,
    pub layers: Vec<WireLayer>,
}

/// A whole lane program — the JSON the worklet's `open_lanes` message
/// carries.
#[derive(Debug, Clone, Facet)]
pub struct WireProgram {
    pub name: String,
    pub engines: Vec<WireEngine>,
    pub tail: Option<Container>,
}

impl WireProgram {
    /// Parse a program from its JSON wire form.
    pub fn from_json(text: &str) -> eyre::Result<Self> {
        facet_json::from_str(text).map_err(|e| eyre::eyre!("lane program JSON: {e}"))
    }

    /// Convert into the rig's native `LaneProgram`.
    pub fn into_lane_program(self) -> LaneProgram {
        LaneProgram {
            name: self.name,
            engines: self
                .engines
                .into_iter()
                .map(|e| LaneEngine {
                    name: e.name,
                    layers: e
                        .layers
                        .into_iter()
                        .map(|l| LaneLayer {
                            name: l.name,
                            tree: l.tree,
                        })
                        .collect(),
                })
                .collect(),
            tail: self.tail,
        }
    }
}

// ── The wasm worklet entry ──────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;

    use wasm_bindgen::prelude::*;

    use daw_standalone::audio_engine::web::WebRenderer;
    use signal_sampler::KeysRig;
    use signal_sampler::keys_rig::LaneProgram;

    use crate::WireProgram;

    fn js_err(e: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&e.to_string())
    }

    /// The keys-rig worklet: a [`WebRenderer`] plus (once `openLanes` ran) a
    /// headless [`KeysRig`] seeded into the same `Standalone`. Construct one
    /// per `AudioWorkletProcessor` on the worklet thread; `RefCell` is fine —
    /// the AudioWorkletGlobalScope is single-threaded.
    #[wasm_bindgen]
    pub struct KeysWorklet {
        renderer: WebRenderer,
        rig: RefCell<Option<KeysRig>>,
        /// The last opened program — kept so `reloadLanes` (after a late
        /// `attachPack`) can re-install instruments as a same-shape,
        /// glitch-free per-track swap.
        program: RefCell<Option<LaneProgram>>,
    }

    #[wasm_bindgen]
    impl KeysWorklet {
        /// `sample_rate` is the worklet's actual `sampleRate`.
        #[wasm_bindgen(constructor)]
        pub fn new(sample_rate: u32) -> KeysWorklet {
            KeysWorklet {
                renderer: WebRenderer::new(sample_rate),
                rig: RefCell::new(None),
                program: RefCell::new(None),
            }
        }

        /// Install a `.signalpack`'s bytes under the spec-path key the lane
        /// trees reference (`sample_block(_, key)`). Call before `openLanes`
        /// for lanes to sound immediately; a pack attached later needs one
        /// `reloadLanes` to reach the running instruments.
        #[wasm_bindgen(js_name = attachPack)]
        pub fn attach_pack(&self, key: &str, bytes: &[u8]) -> Result<(), JsValue> {
            signal_sampler::pack_registry::install(key, bytes.to_vec()).map_err(js_err)
        }

        /// Build the headless keys rig from a lane-program JSON (see
        /// `WireProgram`), select its project, and roll the transport so
        /// `render` produces the lanes. Returns the number of layer lanes.
        #[wasm_bindgen(js_name = openLanes)]
        pub fn open_lanes(&self, program_json: &str) -> Result<u32, JsValue> {
            let program = WireProgram::from_json(program_json)
                .map_err(js_err)?
                .into_lane_program();
            let lanes: u32 = program
                .engines
                .iter()
                .map(|e| e.layers.len() as u32)
                .sum();
            let rig = KeysRig::open_headless_on(
                self.renderer.standalone(),
                self.renderer.output_sample_rate(),
                &program,
            )
            .map_err(js_err)?;
            self.renderer.select_project(rig.project_guid());
            self.renderer.play();
            *self.rig.borrow_mut() = Some(rig);
            *self.program.borrow_mut() = Some(program);
            Ok(lanes)
        }

        /// Re-install the current program's lane instruments — same shape ⇒
        /// glitch-free per-track swap. Call after `attachPack` landed a pack
        /// the running lanes were still missing.
        #[wasm_bindgen(js_name = reloadLanes)]
        pub fn reload_lanes(&self) -> Result<(), JsValue> {
            let program = self.program.borrow();
            let Some(program) = program.as_ref() else {
                return Err(js_err("no lane program open"));
            };
            let mut rig = self.rig.borrow_mut();
            let Some(rig) = rig.as_mut() else {
                return Err(js_err("no lane program open"));
            };
            rig.load_lanes(program).map_err(js_err)
        }

        /// Render one block into the worklet's output channels.
        pub fn render(&self, out_left: &mut [f32], out_right: &mut [f32]) {
            self.renderer.render(out_left, out_right);
        }

        // ── Live MIDI (dispatched to every layer track — each lane's zone
        // filters its own notes, exactly like the native rig) ────────────

        #[wasm_bindgen(js_name = noteOn)]
        pub fn note_on(&self, key: u8, velocity: u8) {
            if let Some(rig) = self.rig.borrow().as_ref() {
                rig.note_on(key, velocity);
            }
        }

        #[wasm_bindgen(js_name = noteOff)]
        pub fn note_off(&self, key: u8) {
            if let Some(rig) = self.rig.borrow().as_ref() {
                rig.note_off(key);
            }
        }

        pub fn cc(&self, controller: u8, value: u8) {
            if let Some(rig) = self.rig.borrow().as_ref() {
                rig.cc(controller, value);
            }
        }

        /// Pitch wheel (14-bit raw, 8192 = center).
        #[wasm_bindgen(js_name = pitchBend)]
        pub fn pitch_bend(&self, raw: u16) {
            if let Some(rig) = self.rig.borrow().as_ref() {
                rig.pitch_bend(raw);
            }
        }

        /// Raw 3-byte MIDI (e.g. straight from WebMIDI) to every lane.
        pub fn midi(&self, status: u8, data1: u8, data2: u8) {
            self.renderer.midi_to_all_tracks(status, data1, data2);
        }

        /// All Notes Off (CC 123).
        #[wasm_bindgen(js_name = allNotesOff)]
        pub fn all_notes_off(&self) {
            if let Some(rig) = self.rig.borrow().as_ref() {
                rig.all_notes_off();
            }
        }

        /// Panic — All Sound Off (CC 120).
        pub fn panic(&self) {
            if let Some(rig) = self.rig.borrow().as_ref() {
                rig.panic();
            }
        }

        // ── Transport (the lane project rolls after `openLanes`; these
        // mirror the plain worklet's messages) ───────────────────────────

        pub fn play(&self) {
            self.renderer.play();
        }
        pub fn pause(&self) {
            self.renderer.pause();
        }
        pub fn stop(&self) {
            self.renderer.stop();
        }

        // ── Read-side ────────────────────────────────────────────────────

        /// Post-fader peaks per project track (rig folder first, then
        /// engines/layers in project order) — read off the live meter bank.
        #[wasm_bindgen(js_name = trackPeaks)]
        pub fn track_peaks(&self) -> Vec<f32> {
            self.renderer.track_peaks()
        }

        /// The keys of every installed in-memory pack.
        #[wasm_bindgen(js_name = installedPacks)]
        pub fn installed_packs(&self) -> Vec<JsValue> {
            signal_sampler::pack_registry::keys()
                .into_iter()
                .map(|k| JsValue::from_str(&k))
                .collect()
        }

        /// Whether a lane program is open.
        #[wasm_bindgen(js_name = isOpen)]
        pub fn is_open(&self) -> bool {
            self.rig.borrow().is_some()
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use web::KeysWorklet;
