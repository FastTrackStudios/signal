//! signal-core end to end: presets built and played, macros on both
//! backends, blocks that can't run here, the null test, the hot swap.

use signal_core::block::{
    Backend, BlockKind, BlockStatus, ParamChange, PluginFormat, PluginRef, Unavailable,
};
use signal_core::engine::{Command, HostParam, HostParamEvent};
use signal_core::graph::{self, BuildOptions};
use signal_core::host::{AssetRef, MemoryHost, NoHost};
use signal_core::null_test::{self, LoopbackClapFactory, NullCase};
use signal_core::platform::{Platform, Platforms};
use signal_core::state::{BlockSpec, Preset, SignalState, StateError};
use signal_core::{BlockFactory, BuiltinFactory, Processor, processor_pair, rigs};

const SR: f32 = 48_000.0;

fn opts<'a>(
    factories: &'a [&'a dyn BlockFactory],
    host: &'a dyn signal_core::SignalHost,
) -> BuildOptions<'a> {
    BuildOptions {
        sample_rate: SR,
        max_block: 256,
        port: None,
        factories,
        host,
    }
}

fn sine(frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|i| 0.5 * (i as f32 * 440.0 * std::f32::consts::TAU / SR).sin())
        .collect()
}

/// Run `proc` over `input` on every channel, `block` frames at a time.
fn play(
    proc: &mut Processor,
    input: &[f32],
    channels: usize,
    in_channels: usize,
    block: usize,
) -> Vec<Vec<f32>> {
    let mut out = vec![input.to_vec(); channels];
    let mut start = 0;
    while start < input.len() {
        let end = (start + block).min(input.len());
        let mut io: Vec<&mut [f32]> = out.iter_mut().map(|c| &mut c[start..end]).collect();
        proc.process(&mut io, in_channels, end - start, &[], &[]);
        start = end;
    }
    out
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0_f32, |m, s| m.max(s.abs()))
}

#[test]
fn the_dsp_chain_builds_and_plays() {
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let (g, report) = graph::build(&rigs::dsp_chain(), &opts(&factories, &NoHost));
    assert!(report.all_active(), "{report:?}");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert!(report.blocks.iter().all(|b| b.backend == "linked"));
    let mut proc = Processor::new();
    let _ = proc.install(g);
    let input = sine(48_000);
    let out = play(&mut proc, &input, 2, 2, 512);
    for ch in &out {
        assert!(ch.iter().all(|s| s.is_finite()));
        assert!(peak(ch) > 0.05, "silent");
    }
    let diff = out[0]
        .iter()
        .zip(&input)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f32::max);
    assert!(diff > 0.01, "the chain did nothing");
}

#[test]
fn a_set_moves_the_base_and_a_modulation_keeps_it() {
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let (g, _) = graph::build(&rigs::dsp_chain(), &opts(&factories, &NoHost));
    let mut proc = Processor::new();
    let _ = proc.install(g);
    // Macro 1 sets the EQ's high gain; macro 2 modulates the delay's mix.
    let events = [
        HostParamEvent {
            offset: 10,
            param: HostParam::Macro(0),
            value: 1.0,
        },
        HostParamEvent {
            offset: 20,
            param: HostParam::Macro(1),
            value: 1.0,
        },
    ];
    let mut a = vec![0.0_f32; 64];
    let mut b = vec![0.0_f32; 64];
    let mut io: Vec<&mut [f32]> = vec![&mut a, &mut b];
    proc.process(&mut io, 2, 64, &[], &events);
    let g = proc.graph().expect("installed");
    let eq = g.block(0, 0).expect("eq");
    let hg = eq.params().index("high_gain").expect("param");
    assert_eq!(eq.params().base(hg), Some(6.0));
    let delay = g.block(0, 2).expect("delay");
    let mix = delay.params().index("mix").expect("param");
    assert_eq!(
        delay.params().base(mix),
        Some(0.1),
        "modulation must keep the base"
    );
    assert!((delay.params().modulation(mix).unwrap_or(0.0) - 0.3).abs() < 1e-6);
    let eff = delay.params().effective(mix).unwrap_or(0.0);
    // 0.1 on its 0–1 range, +0.3 normalized.
    assert!((eff - 0.4).abs() < 1e-6, "{eff}");
}

#[test]
fn a_block_that_cant_run_here_falls_back_or_is_bypassed_visibly() {
    let plugin = BlockSpec {
        kind: BlockKind::Plugin(PluginRef {
            format: PluginFormat::Clap,
            id: "com.valhalladsp.supermassive".into(),
            name: "Valhalla Supermassive".into(),
            version: Some("2.x".into()),
        }),
        requires: Some(Platforms::DESKTOP),
        ..BlockSpec::signal("verb", "plugin")
    }
    .fallback(BlockSpec::signal("verb", "reverb").param("mix", 0.3));
    let ipad_only = BlockSpec {
        requires: Some(Platforms::only(Platform::Ios)),
        ..BlockSpec::signal("eq", "eq")
    };
    let mut preset = Preset::passthrough();
    preset.lanes[0].blocks = vec![plugin, ipad_only];
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let (_, report) = graph::build(&preset, &opts(&factories, &NoHost));
    // No CLAP host in the core: the plugin's fallback runs, and says why.
    match &report.blocks[0].status {
        BlockStatus::Fallback { fallback, .. } => {
            assert_eq!(fallback, &BlockKind::Signal("reverb".into()))
        }
        other => panic!("{other:?}"),
    }
    // Here isn't an iPad: bypassed, and the reason names where it runs.
    assert_eq!(
        report.blocks[1].status,
        BlockStatus::Bypassed {
            reason: Unavailable::Platform {
                needs: Platforms::only(Platform::Ios)
            }
        }
    );
}

#[test]
fn every_block_nulls_compiled_in_against_its_clap_form() {
    let results = null_test::null_every_block(&LoopbackClapFactory);
    assert!(!results.is_empty());
    for (case, r) in &results {
        assert!(
            r.report_b.blocks.iter().all(|b| b.backend == "clap"),
            "{case}: {:?}",
            r.report_b
        );
        println!(
            "{case}: peak {:.3}, diff {:.1} dBFS",
            r.peak, r.peak_diff_dbfs
        );
        assert!(
            r.nulls(),
            "{case} doesn't null: peak {} diff {} dBFS",
            r.peak,
            r.peak_diff_dbfs
        );
    }
}

#[test]
fn a_block_swaps_to_its_clap_at_a_block_boundary_with_its_params() {
    let (mut ctl, mut proc) = processor_pair();
    let builtin: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let (g, _) = graph::build(&rigs::dsp_chain(), &opts(&builtin, &NoHost));
    assert!(ctl.install(g).is_ok());
    let input = sine(4096);
    let _ = play(&mut proc, &input[..1024], 2, 2, 256);
    // Move a parameter, then override the EQ with its CLAP form.
    assert!(
        ctl.send(Command::Param {
            lane: 0,
            block: 0,
            param: 2,
            change: ParamChange::Set(4.0)
        })
        .is_ok()
    );
    let cx = signal_core::block::BuildContext {
        sample_rate: SR,
        max_block: 256,
        host: &NoHost,
    };
    let built = LoopbackClapFactory
        .build(&rigs::dsp_chain().lanes[0].blocks[0], &cx)
        .expect("builds");
    assert!(
        ctl.send(Command::SwapBackend {
            lane: 0,
            block: 0,
            backend: Box::new(built.backend)
        })
        .is_ok()
    );
    let out = play(&mut proc, &input[1024..], 2, 2, 256);
    assert!(out.iter().flatten().all(|s| s.is_finite()));
    let eq = proc.graph().and_then(|g| g.block(0, 0)).expect("eq");
    assert!(matches!(eq.backend(), Backend::Clap(_)));
    assert_eq!(
        eq.params().base(2),
        Some(4.0),
        "the swap must keep the block's parameters"
    );
    ctl.collect();
}

#[test]
fn state_round_trips_and_a_newer_one_is_refused() {
    let state = SignalState::new(rigs::dsp_chain());
    let back = SignalState::from_blob(&state.to_blob()).expect("reads its own");
    assert_eq!(back, state);
    let mut newer = state;
    newer.version += 1;
    assert!(matches!(
        SignalState::from_blob(&newer.to_blob()),
        Err(StateError::TooNew { .. })
    ));
    assert!(SignalState::from_blob(b"{}").is_err());
}

fn guitar_host() -> Option<MemoryHost> {
    let nam = std::env::var_os("SIGNAL_TEST_NAM")?;
    let model = std::fs::read(&nam).ok()?;
    // A synthetic cab: a short decaying, low-passed impulse.
    let ir: Vec<f32> = (0..512)
        .map(|i| (-(i as f32) / 40.0).exp() * if i % 2 == 0 { 1.0 } else { 0.6 })
        .collect();
    Some(
        MemoryHost::default()
            .with("amp.nam", model)
            .with("cab.wav", signal_core::wav::write_mono_f32(&ir, 48_000)),
    )
}

/// The guitar rig with a real capture: `SIGNAL_TEST_NAM=<a .nam>` (the
/// NAM core's MIT example models do; captures are never committed).
#[test]
#[ignore = "needs SIGNAL_TEST_NAM, a .nam model"]
fn the_guitar_rig_plays_a_capture() {
    let host = guitar_host().expect("SIGNAL_TEST_NAM names a readable .nam");
    let preset = rigs::guitar_rig(AssetRef::path("amp.nam"), AssetRef::path("cab.wav"));
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let (g, report) = graph::build(&preset, &opts(&factories, &host));
    assert!(report.all_active(), "{report:?}");
    let mut proc = Processor::new();
    let _ = proc.install(g);
    // Mono guitar in on channel 1, stereo out.
    let out = play(&mut proc, &sine(48_000), 2, 1, 128);
    assert!(out.iter().flatten().all(|s| s.is_finite()));
    assert!(peak(&out[0]) > 1e-3 && peak(&out[1]) > 1e-3, "silent rig");
}

/// The NAM block compiled in against its CLAP form, macros on its params.
#[test]
#[ignore = "needs SIGNAL_TEST_NAM, a .nam model"]
fn the_nam_block_nulls_against_its_clap_form() {
    let host = guitar_host().expect("SIGNAL_TEST_NAM names a readable .nam");
    let params = {
        let cx = signal_core::block::BuildContext {
            sample_rate: SR,
            max_block: 256,
            host: &host,
        };
        let spec = BlockSpec::signal("b", "nam").asset(AssetRef::path("amp.nam"));
        BuiltinFactory.build(&spec, &cx).expect("builds").params
    };
    let case =
        NullCase::for_block("nam", params, 0, Some(AssetRef::path("amp.nam"))).expect("a case");
    let r = null_test::null(&case, &[&BuiltinFactory], &[&LoopbackClapFactory], &host);
    println!("nam: peak {:.3}, diff {:.1} dBFS", r.peak, r.peak_diff_dbfs);
    assert!(r.nulls(), "{r:?}");
}
