//! Offline blocks in the Signal CLAP: analysing them, and where their
//! lanes go.
//!
//! - **Analysing** (`SIGNAL_ANALYZE`): the audio arriving at the block,
//!   handed over by the host (Studio renders it offline and passes a
//!   file: no playback, stopped or playing), or captured while the song
//!   plays through once (any other host). The DSP runs on a worker
//!   thread, so the track's audio never waits for it; the action answers
//!   a job, and the result is applied on the plugin's next main-thread
//!   call (`SIGNAL_ANALYSIS` polls it).
//! - **The lanes**: written through the host's
//!   `com.fasttrackstudio.host-automation/1` as its own automation of
//!   this FX's parameters, every lane in one `begin_edit`/`end_edit` (one
//!   undo step), the user's redrawn points kept. A host without it gets
//!   the lanes played by Signal itself against the transport
//!   ([`Playback::Signal`]).

use std::sync::atomic::Ordering;
use std::sync::{Arc, PoisonError};

use daw_clap_ext::ExtError;
use daw_clap_ext::automation::{
    AutomationPoint, HostAutomationClient, TimeRange, WriteFlags, WriteReport,
};
use serde_json::{Value, json};
use signal_core::block::BlockKind;
use signal_core::offline::capture::{CaptureRing, Recording};
use signal_core::offline::{self, Analysis, Curve, Mode, Playback, Range, State, align, level};
use signal_core::state::{BlockSpec, SignalState};

use crate::plugin::Shared;
use crate::slots::{self, Table};

/// What an analysis made.
enum Made {
    Level(level::Plan),
    Align(align::Found),
}

/// An analysis running on a worker.
struct Job {
    id: u64,
    lane: String,
    block: String,
    range: Range,
    /// Only `range` of the song: the rest of the lanes kept.
    partial: bool,
    source_revision: Option<String>,
    keep_user: bool,
    began: std::time::Instant,
    handle: Option<std::thread::JoinHandle<Result<Made, String>>>,
}

/// A block whose arriving audio is being captured while the song plays.
pub struct Capture {
    pub lane: String,
    pub block: String,
    pub ring: Arc<CaptureRing>,
    pub recording: Recording,
    pub heard_playing: bool,
    pub knobs: level::Knobs,
}

/// The offline side's state, beside the plugin's.
#[derive(Default)]
pub struct Offline {
    next_job: u64,
    jobs: Vec<Job>,
    pub captures: Vec<Capture>,
    /// The host's `host-automation/1`, when it offers it. Main thread.
    pub automation: Option<HostAutomationClient>,
    /// The last finished analysis, said: what `SIGNAL_ANALYSIS` answers.
    last: Vec<(String, String, Value)>,
}

fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, Offline> {
    shared
        .ext
        .offline
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

fn kind_of(spec: &BlockSpec) -> Option<&str> {
    match &spec.kind {
        BlockKind::Signal(n) if offline::mode_of(n) == Mode::Offline => Some(n.as_str()),
        _ => None,
    }
}

/// The block's chain JSON additions: `mode`, and for an offline block its
/// `analysis` (the lanes by count: `SIGNAL_ANALYSIS` gives their points).
pub fn chain_fields(
    shared: &Shared,
    lane: &str,
    spec: &BlockSpec,
    clap_ids: &dyn Fn(&str) -> Option<String>,
) -> (Value, Value) {
    let Some(kind) = kind_of(spec) else {
        return (json!("realtime"), Value::Null);
    };
    let _ = kind;
    let running = lock(shared)
        .jobs
        .iter()
        .any(|j| j.lane == lane && j.block == spec.id)
        || lock(shared)
            .captures
            .iter()
            .any(|c| c.lane == lane && c.block == spec.id);
    let a = spec.analysis.clone().unwrap_or_default();
    (
        json!("offline"),
        analysis_json(&a, running, clap_ids, false),
    )
}

fn analysis_json(
    a: &Analysis,
    running: bool,
    clap_ids: &dyn Fn(&str) -> Option<String>,
    points: bool,
) -> Value {
    let lanes: Vec<Value> = a
        .lanes
        .iter()
        .map(|(param, c)| {
            let mut l = json!({
                "param": param,
                "clap_id": clap_ids(param),
                "points": c.points().len(),
                "edited": a.edited.get(param).map(|r| r.iter().map(|r| json!({"from": r.from, "to": r.to})).collect::<Vec<_>>()),
            });
            if points {
                l["curve"] = json!(c.points().iter().map(|(t, v)| [*t, f64::from(*v)]).collect::<Vec<_>>());
            }
            l
        })
        .collect();
    json!({
        "state": if running { "running" } else { a.state.slug() },
        "range": a.range.map(|r| json!({"from": r.from, "to": r.to})),
        "at": a.at,
        "source_revision": a.source_revision,
        "playback": a.playback.slug(),
        "stale": a.stale.iter().map(|r| json!({"from": r.from, "to": r.to})).collect::<Vec<_>>(),
        "lanes": lanes,
        "values": a.values,
    })
}

/// The CLAP id (decimal) of each of block `(lane, block)`'s parameters,
/// by parameter id, in the running table.
fn clap_ids_of(table: &Table, lane: usize, block: usize) -> impl Fn(&str) -> Option<String> + '_ {
    move |param: &str| {
        table
            .slots
            .iter()
            .position(|s| s.lane == lane && s.block == block && s.spec.id == param)
            .map(|i| nice_plug::wrapper::clap::param_clap_id(&slots::slot_id(i)).to_string())
    }
}

fn arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

/// `(preset lane index, its name, block index)` of the offline block the
/// action names.
fn find(state: &SignalState, args: &Value) -> Result<(usize, String, usize), ExtError> {
    let block =
        arg(args, "block").ok_or_else(|| ExtError::Failed("block: which offline block".into()))?;
    let (l, b) = crate::ext::find_block(state, block, arg(args, "lane"))?;
    let lane = state
        .preset
        .lanes
        .get(l)
        .ok_or_else(|| ExtError::NotFound(block.into()))?;
    let spec = lane
        .blocks
        .get(b)
        .ok_or_else(|| ExtError::NotFound(block.into()))?;
    if kind_of(spec).is_none() {
        return Err(ExtError::Failed(format!(
            "{block}: a realtime block, nothing to analyse"
        )));
    }
    Ok((l, lane.name.clone(), b))
}

fn knobs(args: &Value) -> level::Knobs {
    let d = level::Knobs::default();
    level::Knobs {
        consistency: args
            .get("consistency")
            .and_then(Value::as_f64)
            .map_or(d.consistency, |c| (c / 100.0).clamp(0.0, 1.0)),
        range_db: args
            .get("range_db")
            .and_then(Value::as_f64)
            .map_or(d.range_db, f64::abs),
        target_db: args.get("target_db").and_then(Value::as_f64),
    }
}

fn amounts(spec: &BlockSpec) -> level::Amounts {
    let v = |i: usize| {
        let p = &level::PARAMS[i];
        f64::from(spec.params.get(p.id).copied().unwrap_or(p.default))
    };
    level::Amounts {
        gate: v(level::GATE_AMOUNT),
        breath: v(level::BREATH_AMOUNT),
        ride: v(level::RIDE_AMOUNT),
        ess: v(level::ESS_AMOUNT),
    }
}

fn now_unix() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// `SIGNAL_ANALYZE`.
///
/// - `audio: {path, sample_rate, start_s}`: a file of the audio arriving
///   at the block (its first channel read), from `start_s` on the song;
/// - `values`: an alignment found elsewhere (Studio's `LISTEN_ALIGN`), set
///   as it is;
/// - `reference: {path, sample_rate}` with `audio`: an `align` block
///   searched against it;
/// - neither: captured as the song plays, analysed when it stops (or on
///   `finish: true`).
///
/// `from`/`to` narrow it to part of the song (the rest kept), `knobs`
/// (`consistency`, `range_db`, `target_db`) for `level`,
/// `source_revision` what the host calls the audio, and
/// `replace_user_edits` to write over the user's redrawn points.
pub fn analyze(shared: &Shared, args: &Value, on_main: bool) -> Result<String, ExtError> {
    let state = shared.state();
    let (l, lane, b) = find(&state, args)?;
    let spec = state.preset.lanes[l].blocks[b].clone();
    let kind = kind_of(&spec).unwrap_or_default().to_owned();
    if let Some(values) = args.get("values").and_then(Value::as_object) {
        return set_values(shared, (l, b), values, args, on_main);
    }
    if args.get("finish").and_then(Value::as_bool) == Some(true) {
        return finish_capture(shared, &lane, &spec.id);
    }
    let Some(audio) = args.get("audio") else {
        return start_capture(shared, &lane, &spec.id, knobs(args), on_main);
    };
    let load = |v: &Value| -> Result<(Vec<f64>, f64, f64), ExtError> {
        let path = arg(v, "path").ok_or_else(|| ExtError::Failed("audio.path".into()))?;
        let bytes = std::fs::read(path).map_err(|e| ExtError::Failed(format!("{path}: {e}")))?;
        let samples = signal_core::wav::first_channel(&bytes)
            .map_err(|e| ExtError::Failed(format!("{path}: {e}")))?;
        let rate = v
            .get("sample_rate")
            .and_then(Value::as_f64)
            .unwrap_or(48_000.0);
        let start = v.get("start_s").and_then(Value::as_f64).unwrap_or(0.0);
        Ok((samples.into_iter().map(f64::from).collect(), rate, start))
    };
    let (mut samples, rate, start) = load(audio)?;
    // The audio into the FX: through this lane's blocks before the block,
    // so it is what arrives at it.
    if arg(audio, "at") == Some("fx_input") && b > 0 {
        samples = upstream(&state, l, b, &samples, rate);
    }
    // Part of the song (the rest kept) when `from`/`to` say so; the
    // range is always finite, so the state it is kept in reads back.
    let ranged = args.get("from").is_some() || args.get("to").is_some();
    #[allow(clippy::cast_precision_loss)]
    let end = start + samples.len() as f64 / rate;
    let range = Range::new(
        args.get("from").and_then(Value::as_f64).unwrap_or(start),
        args.get("to").and_then(Value::as_f64).unwrap_or(end),
    );
    let reference = args.get("reference").map(load).transpose()?;
    let previous = spec.analysis.clone();
    let k = knobs(args);
    let am = amounts(&spec);
    let work = move || -> Result<Made, String> {
        match kind.as_str() {
            level::NAME => {
                let keep = ranged
                    .then(|| previous.as_ref().and_then(levels_kept))
                    .flatten();
                level::plan(
                    &[level::Segment {
                        start_s: start,
                        samples: &samples,
                        sample_rate: rate,
                    }],
                    &k,
                    &am,
                    keep,
                )
                .map(Made::Level)
                .map_err(|e| format!("{e:?}"))
            }
            align::NAME => {
                let (r, _, _) = reference.ok_or("align: a reference to align to")?;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let max = (rate * 0.02) as usize;
                Ok(Made::Align(align::find(&r, &samples, max)))
            }
            other => Err(format!("{other}: no analysis")),
        }
    };
    let id = start_job(
        shared,
        (lane.clone(), spec.id.clone()),
        (range, ranged),
        args,
        work,
    )?;
    Ok(json!({"job": id, "block": arg(args, "block"), "state": "running"}).to_string())
}

/// `input` (mono, the FX's input) through lane `l`'s blocks before block
/// `b`, offline: the audio arriving at block `b`. Built from the state as
/// it is, compiled in; a block that cannot be built here passes audio as
/// it would in the chain (its fallback, or through).
fn upstream(state: &SignalState, l: usize, b: usize, input: &[f64], rate: f64) -> Vec<f64> {
    let Some(lane) = state.preset.lanes.get(l) else {
        return input.to_vec();
    };
    let mut part = signal_core::state::Preset::passthrough();
    part.lanes[0].blocks = lane
        .blocks
        .get(..b)
        .map(<[BlockSpec]>::to_vec)
        .unwrap_or_default();
    part.lanes[0].trims = lane.trims.clone();
    part.lanes[0].bypass_phases = lane.bypass_phases.clone();
    // Keys and hits come from elsewhere in a kit: unkeyed here.
    for blk in &mut part.lanes[0].blocks {
        blk.key = None;
    }
    let factories: [&dyn signal_core::BlockFactory; 1] = [&signal_core::BuiltinFactory];
    #[allow(clippy::cast_possible_truncation)]
    let (graph, _) = signal_core::graph::build(
        &part,
        &signal_core::graph::BuildOptions {
            sample_rate: rate as f32,
            max_block: 1024,
            port: None,
            factories: &factories,
            host: &crate::plugin::DiskHost,
        },
    );
    let mut p = signal_core::Processor::new();
    p.install(graph);
    p.set_position(0);
    #[allow(clippy::cast_possible_truncation)]
    let mut l: Vec<f32> = input.iter().map(|x| *x as f32).collect();
    let mut r = l.clone();
    for (cl, cr) in l.chunks_mut(1024).zip(r.chunks_mut(1024)) {
        let n = cl.len();
        let mut io: [&mut [f32]; 2] = [cl, cr];
        p.process(&mut io, 2, n, &[], &[]);
    }
    l.into_iter().map(f64::from).collect()
}

fn levels_kept(a: &Analysis) -> Option<level::Levels> {
    Some(level::Levels {
        target_db: *a.values.get("target_db")?,
        level_db: *a.values.get("level_db")?,
        makeup_db: *a.values.get("makeup_db")?,
    })
}

fn start_job(
    shared: &Shared,
    (lane, block): (String, String),
    (range, partial): (Range, bool),
    args: &Value,
    work: impl FnOnce() -> Result<Made, String> + Send + 'static,
) -> Result<u64, ExtError> {
    let handle = std::thread::Builder::new()
        .name("signal-analysis".into())
        .spawn(work)
        .map_err(|e| ExtError::Failed(format!("no thread to analyse on: {e}")))?;
    let mut o = lock(shared);
    o.next_job += 1;
    let id = o.next_job;
    o.jobs.retain(|j| !(j.lane == lane && j.block == block));
    o.jobs.push(Job {
        id,
        lane,
        block,
        range,
        partial,
        source_revision: arg(args, "source_revision").map(str::to_owned),
        keep_user: args.get("replace_user_edits").and_then(Value::as_bool) != Some(true),
        began: std::time::Instant::now(),
        handle: Some(handle),
    });
    drop(o);
    shared.ext.revision.fetch_add(1, Ordering::AcqRel);
    Ok(id)
}

/// Apply every finished analysis: kept in the block's spec, its lanes
/// written to the host (or played by Signal), the chain rebuilt. Main
/// thread (`on_main`: the host may be called), the only place a result
/// lands. Answers what each finished job did.
pub fn poll(shared: &Shared, on_main: bool) -> Vec<Value> {
    let done: Vec<Job> = {
        let mut o = lock(shared);
        let (done, running): (Vec<Job>, Vec<Job>) = o.jobs.drain(..).partition(|j| {
            j.handle
                .as_ref()
                .is_none_or(std::thread::JoinHandle::is_finished)
        });
        o.jobs = running;
        done
    };
    if done.is_empty() {
        return Vec::new();
    }
    let mut state = shared.state();
    let mut said = Vec::new();
    let mut writes: Vec<(usize, usize, Vec<(String, Curve)>, Range, bool)> = Vec::new();
    for mut job in done {
        let result = job.handle.take().map_or(Err("no worker".into()), |h| {
            h.join().unwrap_or(Err("the analysis panicked".into()))
        });
        let Some((l, b)) = state.preset.lanes.iter().enumerate().find_map(|(l, lane)| {
            (lane.name == job.lane)
                .then(|| {
                    lane.blocks
                        .iter()
                        .position(|s| s.id == job.block)
                        .map(|b| (l, b))
                })
                .flatten()
        }) else {
            continue;
        };
        let spec = &mut state.preset.lanes[l].blocks[b];
        let ms = job.began.elapsed().as_secs_f64() * 1e3;
        match result {
            Err(why) => {
                tracing::warn!(signal.block = %job.block, signal.analysis = "failed", %why, "offline analysis failed");
                said.push(json!({"job": job.id, "block": job.block, "ok": false, "error": why}));
            }
            Ok(made) => {
                let a = spec.analysis.get_or_insert_with(Analysis::default);
                let analysed = match made {
                    Made::Level(plan) => {
                        let part = job.partial;
                        let mut lanes = Vec::new();
                        for (id, c) in plan.lanes.by_param() {
                            let new = if part {
                                a.lanes.get(id).map_or_else(
                                    || c.clone(),
                                    |old| old.spliced(job.range, &c.within(job.range)),
                                )
                            } else {
                                c.clone()
                            };
                            a.lanes.insert(id.to_owned(), new.clone());
                            lanes.push((
                                id.to_owned(),
                                if part { c.within(job.range) } else { new },
                            ));
                        }
                        a.values.insert("target_db".into(), plan.levels.target_db);
                        a.values.insert("level_db".into(), plan.levels.level_db);
                        a.values.insert("makeup_db".into(), plan.levels.makeup_db);
                        writes.push((
                            l,
                            b,
                            lanes,
                            if part {
                                job.range
                            } else {
                                Range::new(f64::NEG_INFINITY, f64::INFINITY)
                            },
                            job.keep_user,
                        ));
                        json!({"target_db": plan.levels.target_db, "level_db": plan.levels.level_db, "makeup_db": plan.levels.makeup_db})
                    }
                    Made::Align(found) => {
                        a.values
                            .insert("delay_samples".into(), f64::from(found.delay_samples));
                        a.values
                            .insert("invert".into(), if found.invert { 1.0 } else { 0.0 });
                        a.values.insert("correlation".into(), found.correlation);
                        #[allow(clippy::cast_precision_loss)]
                        spec.params
                            .insert("delay_samples".into(), found.delay_samples as f32);
                        spec.params
                            .insert("invert".into(), if found.invert { 1.0 } else { 0.0 });
                        json!({"delay_samples": found.delay_samples, "invert": found.invert, "correlation": found.correlation})
                    }
                };
                let a = spec.analysis.get_or_insert_with(Analysis::default);
                a.state = State::Fresh;
                a.at = Some(now_unix());
                a.range = Some(match (job.partial, a.range) {
                    (true, Some(r)) => r.union(&job.range),
                    _ => job.range,
                });
                a.source_revision = job.source_revision.clone().or(a.source_revision.take());
                a.stale.retain(|r| !r.overlaps(&job.range));
                if a.stale.is_empty() && a.state == State::Stale {
                    a.state = State::Fresh;
                }
                tracing::info!(signal.block = %job.block, signal.analysis = "fresh", signal.ms = ms, "offline analysis");
                said.push(json!({"job": job.id, "block": job.block, "ok": true, "made": analysed, "analysis_ms": ms}));
            }
        }
    }
    // Where the lanes go: the host's automation when it has the extension
    // (and the main thread is ours to call it from), else Signal plays them.
    let client = if on_main {
        lock(shared).automation
    } else {
        None
    };
    let table = shared.params.shown.get();
    let mut written: Vec<Value> = Vec::new();
    if let Some(client) = client.filter(|_| !writes.is_empty()) {
        let began = std::time::Instant::now();
        let _ = client.begin_edit("Analyse with Signal");
        for (l, b, lanes, range, keep_user) in &writes {
            let spec = &state.preset.lanes[*l].blocks[*b];
            let ids = clap_ids_of(&table, local_lane(&state, *l), *b);
            let mut all_ok = true;
            for (param, curve) in lanes {
                match write_lane(&client, spec, param, curve, *range, *keep_user, &ids) {
                    Ok(report) => written.push(json!({"block": spec.id, "param": param, "written": report.written, "kept": report.kept.len(), "created": report.created})),
                    Err(e) => {
                        all_ok = false;
                        written.push(json!({"block": spec.id, "param": param, "error": e.to_string()}));
                    }
                }
            }
            if let Some(a) = state.preset.lanes[*l].blocks[*b].analysis.as_mut() {
                a.playback = if all_ok {
                    Playback::Host
                } else {
                    Playback::Signal
                };
            }
        }
        let _ = client.end_edit();
        tracing::info!(
            signal.lanes = written.len(),
            signal.write_ms = began.elapsed().as_secs_f64() * 1e3,
            "offline lanes written to the host"
        );
    } else {
        for (l, b, ..) in &writes {
            if let Some(a) = state.preset.lanes[*l].blocks[*b].analysis.as_mut() {
                a.playback = Playback::Signal;
            }
        }
    }
    if let Ok(text) = String::from_utf8(state.to_blob()) {
        shared.load_state_text(text, on_main);
    }
    let mut o = lock(shared);
    for s in &said {
        let block = s["block"].as_str().unwrap_or_default().to_owned();
        o.last.retain(|(_, b, _)| *b != block);
        let mut s = s.clone();
        s["lanes_written"] = json!(
            written
                .iter()
                .filter(|w| w["block"] == block)
                .collect::<Vec<_>>()
        );
        o.last.push((String::new(), block, s));
    }
    o.last.iter().map(|(_, _, v)| v.clone()).collect()
}

/// The lane's index among the lanes this instance runs (the table's).
fn local_lane(state: &SignalState, preset_lane: usize) -> usize {
    let port = match &state.instance {
        signal_core::state::Instance::Port(link) => Some(link.port.0.as_str()),
        signal_core::state::Instance::MultiChannel => None,
    };
    let name = state.preset.lanes.get(preset_lane).map(|l| l.name.as_str());
    signal_core::graph::lanes_for(&state.preset, port)
        .iter()
        .position(|l| Some(l.name.as_str()) == name)
        .unwrap_or(0)
}

/// One lane to the host: its points normalized on the parameter's own
/// range, linear, replacing `range` (the user's points kept).
fn write_lane(
    client: &HostAutomationClient,
    spec: &BlockSpec,
    param: &str,
    curve: &Curve,
    range: Range,
    keep_user: bool,
    ids: &dyn Fn(&str) -> Option<String>,
) -> Result<WriteReport, ExtError> {
    let kind = kind_of(spec).unwrap_or_default();
    let specs: &[signal_core::param::ParamSpec] = match kind {
        level::NAME => &level::PARAMS,
        align::NAME => &align::PARAMS,
        _ => &[],
    };
    let ps = specs
        .iter()
        .find(|p| p.id == param)
        .ok_or_else(|| ExtError::NotFound(param.into()))?;
    let id: u32 = ids(param)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| ExtError::Failed(format!("{param}: no host parameter shows it")))?;
    let points: Vec<AutomationPoint> = curve
        .points()
        .iter()
        .map(|(t, v)| AutomationPoint::new(t.max(0.0), f64::from(ps.normalize(*v))))
        .collect();
    let mut flags = WriteFlags::CREATE_LANE.with(WriteFlags::SHOW_LANE);
    if keep_user {
        flags = flags.with(WriteFlags::KEEP_USER_EDITS);
    }
    client.write(id, TimeRange::new(range.from, range.to), &points, flags)
}

/// `values` with an all-pass correction as LISTEN_ALIGN says it —
/// `allpass: [{hz, q}]` (up to two second-order sections) and
/// `first_order: {hz}` — as the block's parameters, a section not named
/// switched off.
fn flat_values(values: &serde_json::Map<String, Value>) -> Vec<(String, Value)> {
    let mut flat: Vec<(String, Value)> = Vec::new();
    for (k, v) in values {
        match (k.as_str(), v) {
            ("allpass", Value::Array(sections)) => {
                for (i, (hz, q)) in [
                    ("allpass.0.hz", "allpass.0.q"),
                    ("allpass.1.hz", "allpass.1.q"),
                ]
                .iter()
                .enumerate()
                {
                    let sec = sections.get(i);
                    flat.push((
                        (*hz).into(),
                        sec.and_then(|s| s.get("hz")).cloned().unwrap_or(json!(0.0)),
                    ));
                    if let Some(qv) = sec.and_then(|s| s.get("q")) {
                        flat.push(((*q).into(), qv.clone()));
                    }
                }
            }
            ("first_order", v) => flat.push((
                "allpass.first.hz".into(),
                v.get("hz").cloned().unwrap_or(json!(0.0)),
            )),
            _ => flat.push((k.clone(), v.clone())),
        }
    }
    flat
}

/// Values found elsewhere set as an analysis (`align` from Studio's
/// `LISTEN_ALIGN`): the block's parameters, kept as fresh.
fn set_values(
    shared: &Shared,
    (l, b): (usize, usize),
    values: &serde_json::Map<String, Value>,
    args: &Value,
    on_main: bool,
) -> Result<String, ExtError> {
    let mut state = shared.state();
    let spec = &mut state.preset.lanes[l].blocks[b];
    let a = spec.analysis.get_or_insert_with(Analysis::default);
    for (k, v) in &flat_values(values) {
        let Some(x) = v
            .as_f64()
            .or_else(|| v.as_bool().map(|b| if b { 1.0 } else { 0.0 }))
        else {
            continue;
        };
        a.values.insert(k.clone(), x);
        #[allow(clippy::cast_possible_truncation)]
        spec.params.insert(k.clone(), x as f32);
    }
    let a = spec.analysis.get_or_insert_with(Analysis::default);
    a.state = State::Fresh;
    a.at = Some(now_unix());
    a.source_revision = arg(args, "source_revision")
        .map(str::to_owned)
        .or(a.source_revision.take());
    a.stale.clear();
    let text = String::from_utf8(state.to_blob()).map_err(|e| ExtError::Failed(e.to_string()))?;
    shared.load_state_text(text, on_main);
    Ok(crate::ext::chain(shared).to_string())
}

// ── Capturing while the song plays (a host that cannot render) ─────────

fn start_capture(
    shared: &Shared,
    lane: &str,
    block: &str,
    knobs: level::Knobs,
    on_main: bool,
) -> Result<String, ExtError> {
    {
        let mut o = lock(shared);
        o.captures.retain(|c| !(c.lane == lane && c.block == block));
        o.captures.push(Capture {
            lane: lane.into(),
            block: block.into(),
            ring: Arc::new(CaptureRing::new(shared.sample_rate(), 4.0)),
            recording: Recording::default(),
            heard_playing: false,
            knobs,
        });
    }
    // Rebuilt with the block tapped.
    let state = shared.state();
    if let Ok(text) = String::from_utf8(state.to_blob()) {
        shared.load_state_text(text, on_main);
    }
    Ok(json!({
        "block": block,
        "state": "running",
        "capturing": true,
        "note": "play the song (or the part to analyse) through once: the analysis runs when it stops, or on SIGNAL_ANALYZE {finish: true}",
    })
    .to_string())
}

/// Drain every capture's ring into its recording: the main thread, often
/// (each host-events delivery, each action). A capture whose transport
/// stopped after playing is analysed.
pub fn drain(shared: &Shared) {
    let playing = shared.ext.transport_playing.load(Ordering::Acquire);
    let stopped: Vec<(String, String)> = {
        let mut o = lock(shared);
        for c in &mut o.captures {
            c.ring.drain(&mut c.recording);
            c.heard_playing |= playing;
        }
        o.captures
            .iter()
            .filter(|c| c.heard_playing && !playing && !c.recording.left.is_empty())
            .map(|c| (c.lane.clone(), c.block.clone()))
            .collect()
    };
    for (lane, block) in stopped {
        let _ = finish_capture(shared, &lane, &block);
    }
}

fn finish_capture(shared: &Shared, lane: &str, block: &str) -> Result<String, ExtError> {
    let capture = {
        let mut o = lock(shared);
        let at = o
            .captures
            .iter()
            .position(|c| c.lane == lane && c.block == block)
            .ok_or_else(|| ExtError::Failed(format!("{block}: not capturing")))?;
        let mut c = o.captures.remove(at);
        c.ring.drain(&mut c.recording);
        c
    };
    let rate = f64::from(shared.sample_rate());
    let state = shared.state();
    let spec = state
        .preset
        .lanes
        .iter()
        .find(|l| l.name == lane)
        .and_then(|l| l.blocks.iter().find(|b| b.id == block))
        .cloned()
        .ok_or_else(|| ExtError::NotFound(block.into()))?;
    let am = amounts(&spec);
    #[allow(clippy::cast_precision_loss)]
    let start_s = capture.recording.start as f64 / rate;
    let samples = capture.recording.front();
    #[allow(clippy::cast_precision_loss)]
    let range = Range::new(start_s, start_s + samples.len() as f64 / rate);
    let knobs = capture.knobs;
    let kind = kind_of(&spec).unwrap_or_default().to_owned();
    let work = move || -> Result<Made, String> {
        if kind != level::NAME {
            return Err(format!(
                "{kind}: a capture analyses level blocks; align takes values"
            ));
        }
        level::plan(
            &[level::Segment {
                start_s,
                samples: &samples,
                sample_rate: rate,
            }],
            &knobs,
            &am,
            None,
        )
        .map(Made::Level)
        .map_err(|e| format!("{e:?}"))
    };
    let id = start_job(
        shared,
        (lane.into(), block.into()),
        (range, false),
        &json!({}),
        work,
    )?;
    Ok(
        json!({"job": id, "block": block, "state": "running", "captured_s": range.to - range.from})
            .to_string(),
    )
}

// ── The rest of the actions ─────────────────────────────────────────────

/// `SIGNAL_ANALYSIS {block}`: the block's analysis with its lanes' points,
/// after applying whatever finished; `{block: null}` every offline block's.
pub fn analysis(shared: &Shared, args: &Value, on_main: bool) -> Result<String, ExtError> {
    drain(shared);
    let finished = poll(shared, on_main);
    let state = shared.state();
    let table = shared.params.shown.get();
    let jobs: Vec<(String, String, u64)> = lock(shared)
        .jobs
        .iter()
        .map(|j| (j.lane.clone(), j.block.clone(), j.id))
        .collect();
    let wanted = arg(args, "block")
        .map(|b| crate::ext::find_block(&state, b, arg(args, "lane")))
        .transpose()?;
    let mut blocks = Vec::new();
    for (l, lane) in state.preset.lanes.iter().enumerate() {
        for (b, spec) in lane.blocks.iter().enumerate() {
            if kind_of(spec).is_none() || wanted.is_some_and(|w| w != (l, b)) {
                continue;
            }
            let job = jobs
                .iter()
                .find(|(jl, jb, _)| *jl == lane.name && *jb == spec.id)
                .map(|j| j.2);
            let ids = clap_ids_of(&table, local_lane(&state, l), b);
            blocks.push(json!({
                "block": slots::chain_id(spec.phase, &spec.id),
                "lane": lane.name,
                "kind": kind_of(spec),
                "job": job,
                "analysis": analysis_json(&spec.analysis.clone().unwrap_or_default(), job.is_some(), &ids, true),
            }));
        }
    }
    Ok(json!({"blocks": blocks, "finished": finished}).to_string())
}

/// `SIGNAL_ANALYSIS_CLEAR {block}`: its result gone — its lanes cleared in
/// the host, its parameters back to theirs.
pub fn clear(shared: &Shared, args: &Value, on_main: bool) -> Result<String, ExtError> {
    let mut state = shared.state();
    let (l, _, b) = find(&state, args)?;
    let table = shared.params.shown.get();
    let ids = clap_ids_of(&table, local_lane(&state, l), b);
    let spec = &mut state.preset.lanes[l].blocks[b];
    let client = if on_main {
        lock(shared).automation
    } else {
        None
    };
    if let (Some(client), Some(a)) = (client, spec.analysis.as_ref()) {
        if a.playback == Playback::Host {
            let _ = client.begin_edit("Clear Signal's analysis");
            for param in a.lanes.keys() {
                if let Some(id) = ids(param).and_then(|s| s.parse::<u32>().ok()) {
                    let _ = client.clear(id, TimeRange::ALL);
                }
            }
            let _ = client.end_edit();
        }
    }
    spec.analysis = None;
    let text = String::from_utf8(state.to_blob()).map_err(|e| ExtError::Failed(e.to_string()))?;
    shared.load_state_text(text, on_main);
    Ok(crate::ext::chain(shared).to_string())
}

/// `SIGNAL_ANALYSIS_STALE {block?, from?, to?, source_revision?}`: the
/// audio arriving changed — every offline block (or the one named) stale
/// over the range (the whole analysed range when none), unless
/// `source_revision` is what it was made from.
pub fn stale(shared: &Shared, args: &Value, on_main: bool) -> Result<String, ExtError> {
    let mut state = shared.state();
    let only = arg(args, "block")
        .map(|b| crate::ext::find_block(&state, b, arg(args, "lane")))
        .transpose()?;
    let range = match (
        args.get("from").and_then(Value::as_f64),
        args.get("to").and_then(Value::as_f64),
    ) {
        (Some(f), Some(t)) => Some(Range::new(f, t)),
        _ => None,
    };
    let revision = arg(args, "source_revision");
    let mut changed = 0;
    for (l, lane) in state.preset.lanes.iter_mut().enumerate() {
        for (b, spec) in lane.blocks.iter_mut().enumerate() {
            if kind_of(spec).is_none() || only.is_some_and(|o| o != (l, b)) {
                continue;
            }
            let Some(a) = spec.analysis.as_mut() else {
                continue;
            };
            let was = a.clone();
            match revision {
                Some(r) => a.check_source(r, range),
                None => {
                    if let Some(r) = range.or(a.range) {
                        a.mark_stale(r);
                    }
                }
            }
            changed += usize::from(*a != was);
        }
    }
    if changed > 0 {
        let text =
            String::from_utf8(state.to_blob()).map_err(|e| ExtError::Failed(e.to_string()))?;
        shared.load_state_text(text, on_main);
    }
    Ok(json!({"stale": changed}).to_string())
}

/// The user edited one of this FX's lanes in the host (`automation`
/// topic): the offline block it belongs to marked edited there, so a new
/// analysis keeps the redraw.
pub fn user_edited(shared: &Shared, event: &Value) {
    let (Some(id), Some(from), Some(to)) = (
        event.get("param_id").and_then(Value::as_u64),
        event.get("from").and_then(Value::as_f64),
        event.get("to").and_then(Value::as_f64),
    ) else {
        return;
    };
    let table = shared.params.shown.get();
    let Some(slot) = table
        .slots
        .iter()
        .enumerate()
        .find(|(i, _)| {
            u64::from(nice_plug::wrapper::clap::param_clap_id(&slots::slot_id(*i))) == id
        })
        .map(|(_, s)| s.clone())
    else {
        return;
    };
    let mut state = shared.state();
    let Some(spec) = state
        .preset
        .lanes
        .iter_mut()
        .find(|l| l.name == slot.lane_name)
        .and_then(|l| l.blocks.iter_mut().find(|b| b.id == slot.block_id))
    else {
        return;
    };
    if kind_of(spec).is_none() {
        return;
    }
    let Some(a) = spec.analysis.as_mut() else {
        return;
    };
    a.mark_edited(slot.spec.id, Range::new(from, to));
    tracing::info!(signal.block = %slot.block_id, signal.param = slot.spec.id, signal.edited_from = from, signal.edited_to = to, "a user redrew an offline block's lane");
    if let Ok(text) = String::from_utf8(state.to_blob()) {
        // The state only: the graph plays the host's automation either way.
        shared.params.state.store_running(text);
        shared.ext.revision.fetch_add(1, Ordering::AcqRel);
    }
}

/// Arm the captures' taps on a graph just built (before it is installed).
pub fn arm(shared: &Shared, state: &SignalState, graph: &mut signal_core::graph::Graph) {
    let o = lock(shared);
    if o.captures.is_empty() {
        return;
    }
    let port = match &state.instance {
        signal_core::state::Instance::Port(link) => Some(link.port.0.as_str()),
        signal_core::state::Instance::MultiChannel => None,
    };
    for (l, lane) in signal_core::graph::lanes_for(&state.preset, port)
        .iter()
        .enumerate()
    {
        for c in o.captures.iter().filter(|c| c.lane == lane.name) {
            if let Some(b) = lane.blocks.iter().position(|s| s.id == c.block) {
                graph.tap(l, b, c.ring.clone());
            }
        }
    }
}
