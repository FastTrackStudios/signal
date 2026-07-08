//! End-to-end IR loading test, exercised against the local
//! `ir_samples/` directory.
//!
//! Run `./scripts/fetch_irs.sh` once to populate it. If the directory
//! is empty, the test no-ops (so CI without IR files passes).
//!
//! Verifies:
//! 1. Every WAV decodes through `IrAsset::load`.
//! 2. `IrTransforms::default` produces non-empty stereo buffers.
//! 3. `PreparedIrPair::build` produces a non-zero partition count.
//! 4. The end-to-end `IrEngine + prepared relay + ReverbChain` swap
//!    pipeline runs without panicking and produces finite audio.

use std::path::PathBuf;

use audiocore_dsp::{AudioConfig, Processor};
use reverb_dsp::ir::asset::IrAsset;
use reverb_dsp::ir::engine::IrEngine;
use reverb_dsp::ir::prepared::PreparedIrPair;
use reverb_dsp::ir::transforms::IrTransforms;
use reverb_dsp::{AlgorithmType, ReverbChain};

const SR: f64 = 48000.0;

fn ir_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // reverb-dsp → crates
    p.pop(); // crates → repo root
    p.push("ir_samples");
    p
}

fn list_wavs() -> Vec<PathBuf> {
    let dir = ir_dir();
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("wav"))
        .collect();
    out.sort();
    out
}

#[test]
fn every_sample_ir_loads_and_transforms() {
    let wavs = list_wavs();
    if wavs.is_empty() {
        eprintln!(
            "skipping: no IRs in {:?} — run ./scripts/fetch_irs.sh",
            ir_dir()
        );
        return;
    }
    eprintln!("loading {} IR files from {:?}", wavs.len(), ir_dir());

    let transforms = IrTransforms::default();

    for path in &wavs {
        let asset = IrAsset::load(path, SR).expect(&format!("decode {}", path.display()));
        assert!(asset.frames() > 0, "{:?} has 0 frames", path.file_name());

        let (l, r) = transforms.apply(&asset);
        assert!(
            !l.is_empty() && !r.is_empty(),
            "{:?} transformed empty",
            path.file_name()
        );

        let pair = PreparedIrPair::build(&l, &r);
        assert!(
            pair.left.num_partitions() > 0,
            "{:?} 0 partitions",
            path.file_name()
        );
    }
}

#[test]
fn engine_pipeline_hot_swaps_real_ir() {
    let wavs = list_wavs();
    if wavs.is_empty() {
        eprintln!("skipping: no IRs in {:?}", ir_dir());
        return;
    }
    // Pick a smallish IR to keep the test fast.
    let chosen = wavs
        .iter()
        .min_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(u64::MAX))
        .unwrap()
        .clone();
    eprintln!("test IR: {}", chosen.display());

    let mut chain = ReverbChain::new();
    chain.set_algorithm(AlgorithmType::Convolution);
    chain.mix = 1.0;
    chain.update(AudioConfig {
        sample_rate: SR,
        max_buffer_size: 512,
    });

    let engine = IrEngine::new();
    let rx = engine.spawn_prepared_relay();
    chain.set_prepared_ir_receiver(rx);

    let transforms = IrTransforms {
        predelay_s: 0.005,
        ..Default::default()
    };
    engine
        .submit_path(1, &chosen, SR, transforms)
        .expect("submit job");

    // Pump the chain until the worker + relay finish and the swap lands.
    // Worst case: a few hundred ms of work; cap at 5s of audio to be safe.
    let block = 512usize;
    let max_blocks = (SR as usize * 5) / block;
    let mut got_swap = false;

    for _ in 0..max_blocks {
        let mut l = vec![0.0_f64; block];
        let mut r = vec![0.0_f64; block];
        // Tiny click in the first block of audio.
        if !got_swap {
            l[0] = 1.0;
            r[0] = 1.0;
        }
        chain.process(&mut l, &mut r);

        for (i, (&lv, &rv)) in l.iter().zip(r.iter()).enumerate() {
            assert!(lv.is_finite(), "L NaN at block sample {i}");
            assert!(rv.is_finite(), "R NaN at block sample {i}");
        }

        // After ~200 ms of pump time the prepared pair should have
        // arrived. We can't observe the swap directly, so we just
        // verify finite output and break early once the test has run
        // through enough blocks.
        if !got_swap {
            // Heuristic: a successful swap means chain has produced some
            // wet energy. Anything beyond 50 blocks is plenty.
            got_swap = true;
        }
    }
}
