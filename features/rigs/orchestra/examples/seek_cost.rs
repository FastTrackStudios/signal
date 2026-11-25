//! What a seek costs the audio thread in document mode, on a dense part
//! played by a real library (Cinematic Studio Strings 1st Violins by
//! default).
//!
//! 1. **Inline** (phase 2): the relocate replays the activity span
//!    containing the playhead on the audio thread, at the seek block —
//!    `reconstruct_at` timed at seek positions every `--every` seconds.
//! 2. **Worker** (phase 3, `SeekPolicy::live`): the same seeks through a
//!    `DocumentPlayer` paced in real time, block by block — the longest
//!    `process()` call around each seek (the audio thread's cost), and the
//!    silent gap before the reconstructed bank comes in.
//!
//! ```text
//! cargo run --release -p signal-orchestra --example seek_cost -- \
//!     <score.mxl> [part filter, default "violin"] [--every 5] [--block 256] \
//!     [--library strings] [--section "1st Violins"]
//! ```

use std::time::{Duration, Instant};

use keyflow_lang::orchestra::{Config, ProfileKind, process_part};
use sampler::{
    BlockTransport, DocumentPlayer, Input, Played, SamplerBank, SeekPolicy, annotate,
    reconstruct_at,
};
use signal_orchestra::part_to_document;

const ID: &str = "seek";
const SR: u32 = 48_000;
const SEED: u64 = 0x5EE4_C057;

fn flag(all: &[String], name: &str) -> Option<String> {
    all.iter()
        .position(|a| a == name)
        .and_then(|i| all.get(i + 1).cloned())
}

/// `values` in ms: (median, p95, max).
fn stats(values: &mut [f64]) -> (f64, f64, f64) {
    values.sort_by(f64::total_cmp);
    let at = |q: f64| {
        let i = ((values.len() as f64 - 1.0) * q).round() as usize;
        values.get(i).copied().unwrap_or(0.0)
    };
    (at(0.5), at(0.95), values.last().copied().unwrap_or(0.0))
}

fn main() -> eyre::Result<()> {
    let all: Vec<String> = std::env::args().collect();
    let mxl = all
        .get(1)
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .ok_or_else(|| eyre::eyre!("usage: seek_cost <score.mxl> [part]"))?;
    let filter = all
        .get(2)
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| "violin".into());
    let every: f64 = flag(&all, "--every").and_then(|v| v.parse().ok()).unwrap_or(5.0);
    let block: usize = flag(&all, "--block").and_then(|v| v.parse().ok()).unwrap_or(256);
    let lib = flag(&all, "--library").unwrap_or_else(|| "strings".into());
    let section = flag(&all, "--section").unwrap_or_else(|| "1st Violins".into());

    let sc = keyflow_lang::orchestra::score::load(std::path::Path::new(&mxl))
        .map_err(|e| eyre::eyre!("score: {e}"))?;
    let part = sc
        .parts
        .iter()
        .filter(|p| p.name.to_lowercase().contains(&filter.to_lowercase()))
        .max_by_key(|p| p.notes.len())
        .ok_or_else(|| eyre::eyre!("no part matching {filter:?}"))?;
    let cfg = Config {
        profile: ProfileKind::Strings,
        timing_comp: false,
        tempo_map: Some(sc.meta.tempos.clone()),
        ..Config::default()
    };
    let doc = part_to_document(&process_part(part, &cfg), &sc.meta.tempos, SEED);

    let library =
        signal_orchestra::library(&lib).ok_or_else(|| eyre::eyre!("no library {lib:?}"))?;
    let patch = signal_orchestra::load_patch(library, &section).map_err(|e| eyre::eyre!(e))?;
    let bank = |preload: bool| {
        let mut b = SamplerBank::with_cache_budget(SR, Some(16 << 30));
        b.load_patch(ID, patch.clone(), "library", section.clone(), "Mix");
        b.set_solo_mic(ID, Some("Mix".into()));
        if preload {
            let _ = b.preload_instrument(ID);
        }
        b
    };
    let loading = Instant::now();
    let mut inline_bank = bank(true);
    println!(
        "{section} loaded and preloaded in {:.1} s",
        loading.elapsed().as_secs_f64()
    );
    let spec = inline_bank
        .instrument_spec(ID)
        .ok_or_else(|| eyre::eyre!("no spec"))?;
    let sched = std::sync::Arc::new(annotate(&doc, &spec, SR));
    let end = sched.end_frame;
    let longest_span = sched.busy.iter().map(|(a, b)| b - a).max().unwrap_or(0);
    println!(
        "part {:?}: {} notes ({} legato, {} shorts) over {:.1} s; {} activity spans, the longest {:.1} s",
        part.name,
        sched.note_count,
        sched.legato_count,
        sched.short_count,
        end as f64 / f64::from(SR),
        sched.busy.len(),
        longest_span as f64 / f64::from(SR),
    );
    let step = (every * f64::from(SR)) as i64;
    let seeks: Vec<i64> = (1..)
        .map(|i| i * step)
        .take_while(|p| *p < end as i64)
        .collect();
    let budget_ms = block as f64 * 1000.0 / f64::from(SR);

    // 1. Inline: the replay on the audio thread.
    let mut scratch = vec![0.0f32; sampler::document_rt::WARM_CHUNK * 2];
    let mut inline = Vec::new();
    let mut worst = (0i64, 0.0f64, 0u64);
    for &p in &seeks {
        let t0 = Instant::now();
        let done = reconstruct_at(&mut inline_bank, ID, &sched, p, &mut scratch);
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        let replayed = p.max(0) as u64 - sched.reconstruction_start(p.max(0) as u64);
        if ms > worst.1 {
            worst = (p, ms, replayed);
        }
        inline.push(ms);
        let _ = done;
    }
    let (med, p95, max) = stats(&mut inline);
    println!(
        "inline: {} seeks, median {med:.2} ms, p95 {p95:.2} ms, max {max:.2} ms \
         (at {:.1} s, {:.1} s replayed) — the block budget at {block} frames is {budget_ms:.2} ms",
        seeks.len(),
        worst.0 as f64 / f64::from(SR),
        worst.2 as f64 / f64::from(SR),
    );

    // 2. Worker: the same seeks, paced in real time.
    let policy = SeekPolicy::live(SR);
    let mut player =
        DocumentPlayer::new(ID, bank(false), None, block).with_worker(bank(false), policy)?;
    let mut out = vec![0.0f32; block * 2];
    let mut seek_costs = Vec::new();
    let mut gaps = Vec::new();
    let period = Duration::from_secs_f64(block as f64 / f64::from(SR));
    for &p in &seeks {
        let mut pos = p;
        let mut longest = 0.0f64;
        let mut silent = 0usize;
        for _ in 0..(SR as usize / block) {
            let started = Instant::now();
            let t = BlockTransport {
                playing: true,
                pos_frame: pos,
                tempo_bpm: None,
            };
            let played = player.process(Some(&sched), &t, Input::Track, &[], &mut out);
            let took = started.elapsed();
            longest = longest.max(took.as_secs_f64() * 1e3);
            if played == Played::Reconstructing {
                silent += block;
            }
            pos += block as i64;
            if let Some(rest) = period.checked_sub(took) {
                std::thread::sleep(rest);
            }
        }
        seek_costs.push(longest);
        gaps.push(silent as f64 * 1000.0 / f64::from(SR));
    }
    let (med, p95, max) = stats(&mut seek_costs);
    let (gmed, gp95, gmax) = stats(&mut gaps);
    println!(
        "worker: longest process() around a seek: median {med:.2} ms, p95 {p95:.2} ms, max {max:.2} ms; \
         silent before it comes in: median {gmed:.0} ms, p95 {gp95:.0} ms, max {gmax:.0} ms"
    );
    Ok(())
}
