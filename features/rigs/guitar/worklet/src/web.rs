//! The wasm entry points: [`GuitarWorklet`] on the AudioWorklet thread and
//! [`NamWorker`] on each Web Worker — one module, two roles.
//!
//! A worker and the worklet share one `SharedArrayBuffer` per remote model
//! (so the page must be cross-origin isolated — COOP/COEP headers). Layout,
//! in bytes:
//!
//! ```text
//!   0  i32[16]  control — SEQ_IN, SEQ_OUT, FRAMES, NPARAMS, CMD, COST_US
//!               (running average while serving), STATE, PARAM_IDS[8],
//!               WORST_US
//!  64  f64[8]   parameter values
//! 128  f32[256] × 4  in L, in R, out L, out R
//! ```
//!
//! The worklet writes the input, then bumps SEQ_IN and notifies. The worker,
//! parked in `Atomics.wait` on SEQ_IN, runs its model and publishes SEQ_OUT.
//! The worklet never blocks (AudioWorklet cannot `Atomics.wait`): it spins,
//! bounded, on SEQ_OUT.

use js_sys::{Atomics, Float32Array, Float64Array, Int32Array, SharedArrayBuffer};
use wasm_bindgen::prelude::*;

use signal_sampler::RigBlock;
use signal_sampler::rig::prepare_chain;

use crate::plan::{Place, Plan, model_budget_us};
use crate::remote::{Lag, Link, MAX_POSTED_PARAMS};
use crate::runner::{BuildOptions, ChainRunner, build, plan_chain};

const SEQ_IN: u32 = 0;
const SEQ_OUT: u32 = 1;
const FRAMES: u32 = 2;
const NPARAMS: u32 = 3;
const CMD: u32 = 4;
const COST_US: u32 = 5;
const STATE: u32 = 6;
const PARAM_IDS: u32 = 7;
/// The worst per-quantum time since the page last reset it, µs.
const WORST_US: u32 = 15;

const MAX_FRAMES: u32 = 256;
const VALUES_AT: u32 = 64;
const AUDIO_AT: u32 = 128;
/// Bytes one remote model's `SharedArrayBuffer` needs.
const SAB_BYTES: u32 = AUDIO_AT + 4 * MAX_FRAMES * 4;

/// A worker's state, in `STATE`.
const STATE_LOADING: i32 = 0;
const STATE_READY: i32 = 1;
const STATE_FAILED: i32 = 2;

/// The browser's render quantum.
const QUANTUM: u32 = 128;

#[wasm_bindgen(start)]
fn start() {
    std::panic::set_hook(Box::new(|info| {
        web_sys_log(&format!("signal-guitar-worklet panicked: {info}"));
    }));
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn web_sys_log(s: &str);
}

/// Bytes to allocate per remote model.
#[wasm_bindgen(js_name = sabBytes)]
#[must_use]
pub fn sab_bytes() -> u32 {
    SAB_BYTES
}

/// Typed views over one model's shared buffer.
struct Views {
    ctl: Int32Array,
    values: Float64Array,
    in_l: Float32Array,
    in_r: Float32Array,
    out_l: Float32Array,
    out_r: Float32Array,
}

impl Views {
    fn new(sab: &SharedArrayBuffer) -> Self {
        let f32s = |i: u32| {
            Float32Array::new_with_byte_offset_and_length(
                sab,
                AUDIO_AT + i * MAX_FRAMES * 4,
                MAX_FRAMES,
            )
        };
        Self {
            ctl: Int32Array::new_with_byte_offset_and_length(sab, 0, 16),
            values: Float64Array::new_with_byte_offset_and_length(
                sab,
                VALUES_AT,
                MAX_POSTED_PARAMS as u32,
            ),
            in_l: f32s(0),
            in_r: f32s(1),
            out_l: f32s(2),
            out_r: f32s(3),
        }
    }

    fn load(&self, i: u32) -> i32 {
        Atomics::load(&self.ctl, i).unwrap_or(0)
    }

    fn store(&self, i: u32, v: i32) {
        let _ = Atomics::store(&self.ctl, i, v);
    }
}

/// Copy `src` into the front of a shared view. `copy_from` wants equal
/// lengths, so the common full quantum goes straight in and anything else
/// through a subarray.
fn put(view: &Float32Array, src: &[f32]) {
    let n = src.len().min(MAX_FRAMES as usize);
    if n == MAX_FRAMES as usize {
        view.copy_from(&src[..n]);
    } else {
        view.subarray(0, n as u32).copy_from(&src[..n]);
    }
}

fn get(view: &Float32Array, dst: &mut [f32]) {
    let n = dst.len().min(MAX_FRAMES as usize);
    if n == MAX_FRAMES as usize {
        view.copy_to(&mut dst[..n]);
    } else {
        view.subarray(0, n as u32).copy_to(&mut dst[..n]);
    }
}

// ─── The worklet end ────────────────────────────────────────────────────

/// A [`Link`] over a shared buffer, from the worklet.
struct SabLink {
    v: Views,
    seq: i32,
}

// SAFETY: the worklet is single-threaded and this wasm module is built
// without the atomics target feature, so there is no other thread in this
// instance that could observe the JS handles. `Link: Send` only exists for
// the native hosts.
unsafe impl Send for SabLink {}

impl Link for SabLink {
    fn post(&mut self, in_l: &[f32], in_r: &[f32], params: &[(u32, f64)]) {
        put(&self.v.in_l, in_l);
        put(&self.v.in_r, in_r);
        let np = params.len().min(MAX_POSTED_PARAMS);
        for (i, &(id, value)) in params[..np].iter().enumerate() {
            self.v.store(PARAM_IDS + i as u32, id as i32);
            self.v.values.set_index(i as u32, value);
        }
        self.v.store(NPARAMS, np as i32);
        self.v.store(FRAMES, in_l.len() as i32);
        self.seq = self.seq.wrapping_add(1);
        self.v.store(SEQ_IN, self.seq);
        let _ = Atomics::notify(&self.v.ctl, SEQ_IN);
    }

    fn ready(&self) -> bool {
        self.v.load(SEQ_OUT) == self.seq
    }

    fn take(&mut self, out_l: &mut [f32], out_r: &mut [f32]) {
        get(&self.v.out_l, out_l);
        get(&self.v.out_r, out_r);
    }

    fn wait(&self, budget_us: u32) -> bool {
        // The worklet has no fine clock (performance is Date-backed there),
        // so the deadline is in whole milliseconds, rounded up; the spin
        // reads the clock only every so often.
        let deadline = js_sys::Date::now() + f64::from(budget_us.div_ceil(1000));
        let mut spins = 0u32;
        while !self.ready() {
            spins += 1;
            if spins % 64 == 0 && js_sys::Date::now() > deadline {
                return false;
            }
            std::hint::spin_loop();
        }
        true
    }
}

fn parse_blocks(json: &str) -> Result<Vec<RigBlock>, JsValue> {
    facet_json::from_str(json).map_err(|e| JsValue::from_str(&format!("chain JSON: {e}")))
}

fn cost_fn(costs: &[u32]) -> impl Fn(usize, &RigBlock) -> u32 + '_ {
    // A model nobody has measured yet: a full-size A2 model's cost in a
    // browser worker, measured (1.45–1.6 ms), rounded up.
    move |slot, _| costs.get(slot).copied().filter(|&c| c > 0).unwrap_or(1600)
}

fn place_name(p: Place) -> &'static str {
    match p {
        Place::Inline => "inline",
        Place::Worker(Lag::Zero) => "parallel",
        Place::Worker(Lag::One) => "lag",
    }
}

/// Where a chain's models run, as JSON the page acts on:
/// `{"lagQuanta":n,"models":[{"slot":i,"place":"inline"|"parallel"|"lag"}]}`.
/// `costs[slot]` is a model's measured µs per quantum (0 = not measured).
/// The worklet plans the same chain from the same inputs identically.
#[wasm_bindgen(js_name = planChain)]
pub fn plan_chain_json(
    blocks_json: &str,
    costs: &[u32],
    budget_us: u32,
) -> Result<String, JsValue> {
    let blocks = parse_blocks(blocks_json)?;
    let plan = plan_chain(&blocks, &cost_fn(costs), budget_us);
    let models: Vec<String> = plan
        .slots
        .iter()
        .zip(&plan.places)
        .map(|(s, p)| format!("{{\"slot\":{s},\"place\":\"{}\"}}", place_name(*p)))
        .collect();
    Ok(format!(
        "{{\"lagQuanta\":{},\"models\":[{}]}}",
        plan.lag_quanta,
        models.join(",")
    ))
}

/// What each block of a chain costs here, µs per quantum, run `quanta`
/// times on a test signal — JSON `[{"name":…,"us":…}, …]` in chain order.
/// Every block runs inline (this is a measurement, not a plan); bypassed
/// blocks are measured too, as if switched on. The assets must be installed.
#[wasm_bindgen(js_name = profileChain)]
pub fn profile_chain(blocks_json: &str, sample_rate: u32, quanta: u32) -> Result<String, JsValue> {
    let blocks = parse_blocks(blocks_json)?;
    let ids: Vec<String> = (0..blocks.len()).map(|i| format!("slot{i}")).collect();
    let built = prepare_chain(&blocks, &ids, sample_rate).map_err(|e| JsValue::from_str(&e))?;
    let q = QUANTUM as usize;
    let events = signal_plugin_host::PluginEvents::default();
    let mut signal: Vec<f32> = (0..q).map(|i| ((i as f32) * 0.05).sin() * 0.3).collect();
    let (mut l, mut r) = (vec![0.0; q], vec![0.0; q]);
    let mut rows = Vec::new();
    for ((_, mut b), def) in built.into_blocks().into_iter().zip(&blocks) {
        for _ in 0..8 {
            let _ = b.process_block(&signal, &signal, &mut l, &mut r, &events);
        }
        let t0 = now_ms();
        for _ in 0..quanta {
            let _ = b.process_block(&signal, &signal, &mut l, &mut r, &events);
        }
        let us = (now_ms() - t0) * 1000.0 / f64::from(quanta.max(1));
        signal.copy_from_slice(&l);
        rows.push(format!(
            "{{\"name\":\"{}\",\"us\":{us:.1},\"bypassed\":{}}}",
            def.name.replace('"', "'"),
            def.bypassed
        ));
    }
    Ok(format!("[{}]", rows.join(",")))
}

/// The render-thread model budget for a quantum at `sample_rate`, taking
/// `share` of it (see [`model_budget_us`]).
#[wasm_bindgen(js_name = modelBudgetUs)]
#[must_use]
pub fn model_budget(sample_rate: f64, share: f64) -> u32 {
    model_budget_us(QUANTUM, sample_rate, share)
}

/// Make these bytes the content of `key` (a model or IR path) in this
/// instance — the worklet's and each worker's registries are their own.
#[wasm_bindgen(js_name = installAsset)]
pub fn install_asset(key: String, bytes: Vec<u8>) {
    signal_sampler::assets::install(key, bytes);
}

/// NAM level calibration: the interface's full-scale input in dBu, or NaN
/// for off. Applies to chains built afterwards.
#[wasm_bindgen(js_name = setCalibration)]
pub fn set_calibration(dbu: f32) {
    signal_sampler::nam::set_interface_calibration_dbu(dbu.is_finite().then_some(dbu));
}

/// Size A2 models are built at from now on (1 = full).
#[wasm_bindgen(js_name = setModelSize)]
pub fn set_model_size(size: f64) {
    signal_sampler::nam::set_model_size(size);
}

/// The guitar rig's render loop, in the AudioWorklet.
#[wasm_bindgen]
pub struct GuitarWorklet {
    sample_rate: u32,
    runner: Option<ChainRunner>,
}

#[wasm_bindgen]
impl GuitarWorklet {
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate as u32,
            runner: None,
        }
    }

    /// Build and switch to a chain. `remotes` pairs each worker-placed slot
    /// with its model's shared buffer: `[[slot, sab], …]`, as `planChain`
    /// placed them for the same `costs` and `budget_us`. Returns the added
    /// latency in frames.
    #[wasm_bindgen(js_name = loadChain)]
    pub fn load_chain(
        &mut self,
        blocks_json: &str,
        costs: &[u32],
        budget_us: u32,
        remotes: js_sys::Array,
    ) -> Result<u32, JsValue> {
        let blocks = parse_blocks(blocks_json)?;
        let ids: Vec<String> = blocks
            .iter()
            .enumerate()
            .map(|(i, b)| {
                if b.id.is_empty() {
                    format!("slot{i}")
                } else {
                    b.id.clone()
                }
            })
            .collect();
        let plan: Plan = plan_chain(&blocks, &cost_fn(costs), budget_us);
        let sab_for = |slot: usize| {
            remotes.iter().find_map(|pair| {
                let pair = js_sys::Array::from(&pair);
                (pair.get(0).as_f64() == Some(slot as f64))
                    .then(|| pair.get(1).unchecked_into::<SharedArrayBuffer>())
            })
        };
        let mut link = |slot: usize, _: &RigBlock| -> Result<Box<dyn Link>, String> {
            let sab = sab_for(slot).ok_or_else(|| format!("no worker for slot {slot}"))?;
            let v = Views::new(&sab);
            if v.load(STATE) != STATE_READY {
                return Err(format!("the worker for slot {slot} is not ready"));
            }
            let seq = v.load(SEQ_OUT);
            Ok(Box::new(SabLink { v, seq }))
        };
        // Drop the old chain first: its links must not share a buffer with
        // the new one's.
        self.runner = None;
        let runner = build(
            &blocks,
            &ids,
            &plan,
            BuildOptions {
                sample_rate: self.sample_rate,
                quantum: QUANTUM,
                budget_us,
                link: &mut link,
            },
        )
        .map_err(|e| JsValue::from_str(&e))?;
        let latency = runner.added_latency();
        self.runner = Some(runner);
        Ok(latency)
    }

    /// Stop playing any chain (silence out).
    /// The current patch's input and output trims, dB.
    #[wasm_bindgen(js_name = setTrims)]
    pub fn set_trims(&mut self, input_db: f32, output_db: f32) {
        if let Some(r) = &mut self.runner {
            r.set_trims(input_db, output_db);
        }
    }

    #[wasm_bindgen(js_name = clearChain)]
    pub fn clear_chain(&mut self) {
        self.runner = None;
    }

    /// One render quantum: mono guitar in, stereo out.
    pub fn process(&mut self, input: &[f32], out_l: &mut [f32], out_r: &mut [f32]) {
        match &mut self.runner {
            Some(r) => r.process(input, out_l, out_r),
            None => {
                out_l.fill(0.0);
                out_r.fill(0.0);
            }
        }
    }

    #[wasm_bindgen(js_name = setParam)]
    pub fn set_param(&mut self, block_id: &str, param: u32, value: f64) -> bool {
        self.runner
            .as_mut()
            .is_some_and(|r| r.set_param(block_id, param, value))
    }

    #[wasm_bindgen(js_name = setBypass)]
    pub fn set_bypass(&mut self, block_id: &str, bypass: bool) -> bool {
        self.runner
            .as_mut()
            .is_some_and(|r| r.set_bypass(block_id, bypass))
    }

    /// `[input peak, output peak]` since the last call, linear.
    #[wasm_bindgen(js_name = takePeaks)]
    pub fn take_peaks(&mut self) -> Vec<f32> {
        let (i, o) = self
            .runner
            .as_mut()
            .map_or((0.0, 0.0), ChainRunner::take_peaks);
        vec![i, o]
    }

    /// Latency the chain adds right now, in frames (lagged models that are
    /// switched on).
    #[wasm_bindgen(js_name = addedLatency)]
    #[must_use]
    pub fn added_latency(&self) -> u32 {
        self.runner.as_ref().map_or(0, ChainRunner::added_latency)
    }

    /// Quanta a worker was late for, since the chain was loaded — the page
    /// re-plans when this climbs.
    #[must_use]
    pub fn misses(&self) -> u32 {
        self.runner.as_ref().map_or(0, ChainRunner::misses)
    }
}

// ─── The worker end ─────────────────────────────────────────────────────

/// `performance.now()` where the scope has it (workers do), else the
/// millisecond clock.
fn now_ms() -> f64 {
    let perf = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("performance")).ok();
    perf.and_then(|p| {
        let now = js_sys::Reflect::get(&p, &JsValue::from_str("now")).ok()?;
        now.dyn_ref::<js_sys::Function>()?.call0(&p).ok()?.as_f64()
    })
    .unwrap_or_else(js_sys::Date::now)
}

/// One NAM model on a Web Worker. See the module docs for the protocol.
#[wasm_bindgen]
pub struct NamWorker {
    v: Views,
    block: Option<Box<dyn signal_plugin_host::PluginInstance>>,
    in_l: Vec<f32>,
    in_r: Vec<f32>,
    out_l: Vec<f32>,
    out_r: Vec<f32>,
    params: Vec<(u32, f64)>,
    done: i32,
    cost_us: f64,
}

#[wasm_bindgen]
impl NamWorker {
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new(sab: SharedArrayBuffer) -> Self {
        let n = MAX_FRAMES as usize;
        let v = Views::new(&sab);
        v.store(STATE, STATE_LOADING);
        Self {
            v,
            block: None,
            in_l: vec![0.0; n],
            in_r: vec![0.0; n],
            out_l: vec![0.0; n],
            out_r: vec![0.0; n],
            params: Vec::with_capacity(MAX_POSTED_PARAMS),
            done: 0,
            cost_us: 0.0,
        }
    }

    /// Build the model (the block's `nam` key must be installed in this
    /// worker) and measure what it costs per quantum. Returns that cost in
    /// µs, which is also published in the shared buffer.
    pub fn load(&mut self, block_json: &str, sample_rate: f32) -> Result<u32, JsValue> {
        self.v.store(STATE, STATE_LOADING);
        let block: RigBlock = facet_json::from_str(block_json).map_err(|e| {
            self.v.store(STATE, STATE_FAILED);
            JsValue::from_str(&format!("block JSON: {e}"))
        })?;
        let built = prepare_chain(
            std::slice::from_ref(&block),
            &[block.id.clone()],
            sample_rate as u32,
        )
        .map_err(|e| {
            self.v.store(STATE, STATE_FAILED);
            JsValue::from_str(&e)
        })?;
        let mut boxed = built
            .into_blocks()
            .pop()
            .map(|(_, b)| b)
            .ok_or_else(|| JsValue::from_str("the model built nothing"))?;

        // Measure: a warm-up, then enough quanta that a ms clock resolves.
        let q = QUANTUM as usize;
        let noise: Vec<f32> = (0..q)
            .map(|i| ((i * 7919 % 97) as f32 / 97.0 - 0.5) * 0.2)
            .collect();
        let events = signal_plugin_host::PluginEvents::default();
        let (mut l, mut r) = (vec![0.0; q], vec![0.0; q]);
        for _ in 0..20 {
            let _ = boxed.process_block(&noise, &noise, &mut l, &mut r, &events);
        }
        let runs = 200;
        let t0 = now_ms();
        for _ in 0..runs {
            let _ = boxed.process_block(&noise, &noise, &mut l, &mut r, &events);
        }
        let us = (now_ms() - t0) * 1000.0 / f64::from(runs);
        // Flush the noise out of the model's receptive field (a few
        // thousand samples) so it does not ring into the first real quantum.
        let silence = vec![0.0; q];
        for _ in 0..64 {
            let _ = boxed.process_block(&silence, &silence, &mut l, &mut r, &events);
        }

        self.block = Some(boxed);
        self.cost_us = us;
        self.done = self.v.load(SEQ_IN);
        self.v.store(SEQ_OUT, self.done);
        self.v.store(COST_US, us as i32);
        self.v.store(STATE, STATE_READY);
        Ok(us as u32)
    }

    /// Serve the worklet until the page raises CMD (to deliver a message).
    /// Blocks the worker's thread in `Atomics.wait` between quanta.
    pub fn serve(&mut self) {
        loop {
            if self.v.load(CMD) != 0 {
                return;
            }
            let _ = Atomics::wait_with_timeout(&self.v.ctl, SEQ_IN, self.done, 250.0);
            let seq = self.v.load(SEQ_IN);
            if seq == self.done {
                continue;
            }
            self.run_one(seq);
        }
    }

    fn run_one(&mut self, seq: i32) {
        let t0 = now_ms();
        let n = (self.v.load(FRAMES).max(0) as usize).min(MAX_FRAMES as usize);
        get(&self.v.in_l, &mut self.in_l[..n]);
        get(&self.v.in_r, &mut self.in_r[..n]);
        self.params.clear();
        let np = (self.v.load(NPARAMS).max(0) as usize).min(MAX_POSTED_PARAMS);
        for i in 0..np {
            let id = self.v.load(PARAM_IDS + i as u32) as u32;
            self.params.push((id, self.v.values.get_index(i as u32)));
        }
        match &mut self.block {
            Some(b) => {
                let events = signal_plugin_host::PluginEvents {
                    params: &self.params,
                    ..Default::default()
                };
                if b.process_block(
                    &self.in_l[..n],
                    &self.in_r[..n],
                    &mut self.out_l[..n],
                    &mut self.out_r[..n],
                    &events,
                )
                .is_err()
                {
                    self.out_l[..n].fill(0.0);
                    self.out_r[..n].fill(0.0);
                }
            }
            None => {
                self.out_l[..n].fill(0.0);
                self.out_r[..n].fill(0.0);
            }
        }
        put(&self.v.out_l, &self.out_l[..n]);
        put(&self.v.out_r, &self.out_r[..n]);
        self.done = seq;
        self.v.store(SEQ_OUT, seq);
        // What serving really costs — on whatever core the browser put this
        // worker — as a running average and the worst since the last read.
        let us = (now_ms() - t0) * 1000.0;
        self.cost_us = 0.98 * self.cost_us + 0.02 * us;
        self.v.store(COST_US, self.cost_us as i32);
        if us as i32 > self.v.load(WORST_US) {
            self.v.store(WORST_US, us as i32);
        }
    }

    /// What the model measured at load, µs per quantum.
    #[wasm_bindgen(js_name = costUs)]
    #[must_use]
    pub fn cost_us(&self) -> f64 {
        self.cost_us
    }
}
