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
use signal_core::state::{
    BlockSpec, LaneSpec, Preset, SendSpec, SignalState, StateError, TriggerSpec,
};
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

/// A family's algorithms are different DSP or a different face: the
/// block runs the one its spec names. (What keeps the null test from
/// passing on two copies of a default.)
#[test]
fn a_blocks_algorithm_is_the_one_it_runs() {
    let render = |algorithm: &str| {
        let mut preset = Preset::passthrough();
        preset.lanes[0].blocks = vec![
            BlockSpec::signal("verb", "reverb")
                .algorithm(algorithm)
                .param("mix", 1.0),
        ];
        let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
        let (g, report) = graph::build(&preset, &opts(&factories, &NoHost));
        assert!(report.all_active(), "{algorithm}: {report:?}");
        assert!(
            report.warnings.is_empty(),
            "{algorithm}: {:?}",
            report.warnings
        );
        let mut proc = Processor::new();
        let _ = proc.install(g);
        play(&mut proc, &sine(24_000), 2, 2, 256)
    };
    let (plate, hall, spring) = (render("plate"), render("hall"), render("spring"));
    let differ =
        |a: &[Vec<f32>], b: &[Vec<f32>]| a[0].iter().zip(&b[0]).any(|(x, y)| (x - y).abs() > 1e-3);
    assert!(differ(&plate, &hall), "plate and hall sound the same");
    assert!(differ(&hall, &spring), "hall and spring sound the same");

    // An algorithm this machine doesn't have is reported, not guessed at.
    let mut preset = Preset::passthrough();
    preset.lanes[0].blocks = vec![BlockSpec::signal("verb", "reverb").algorithm("not-downloaded")];
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let (_, report) = graph::build(&preset, &opts(&factories, &NoHost));
    assert!(
        matches!(&report.blocks[0].status, BlockStatus::Bypassed { reason: Unavailable::Missing { what } } if what.contains("not-downloaded")),
        "{:?}",
        report.blocks[0].status
    );
}

/// A mono 16-bit WAV of `samples`.
fn wav16(samples: &[f32]) -> Vec<u8> {
    let data: Vec<u8> = samples
        .iter()
        .flat_map(|s| ((s.clamp(-1.0, 1.0) * 32_767.0) as i16).to_le_bytes())
        .collect();
    let mut w = Vec::new();
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes()); // PCM
    w.extend_from_slice(&1u16.to_le_bytes()); // mono
    w.extend_from_slice(&48_000u32.to_le_bytes());
    w.extend_from_slice(&96_000u32.to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(data.len() as u32).to_le_bytes());
    w.extend_from_slice(&data);
    w
}

fn lane(name: &str) -> LaneSpec {
    LaneSpec {
        name: name.into(),
        ..LaneSpec::default()
    }
}

/// A kick in two mics and a trigger, as the Crescendum session has it:
/// the trigger mic plays the sample, the mics and the sample sum on the
/// `Kick` bus, and the raw mics send pre-chain to a parallel `Sub`.
#[test]
fn a_kit_piece_triggers_its_sample_and_sums_on_its_bus() {
    // The sample: a 0.5 step for 100 frames.
    let host = MemoryHost::default().with("kick.wav", wav16(&[0.5; 100]));
    let preset = Preset {
        name: "Kick".into(),
        inputs: 3,
        outputs: 2,
        lanes: vec![
            LaneSpec {
                input: vec![0],
                to: Some("Kick".into()),
                sends: vec![SendSpec::to("Sub", 0.0).pre()],
                ..lane("In")
            },
            LaneSpec {
                input: vec![1],
                to: Some("Kick".into()),
                ..lane("Out")
            },
            // The trigger mic: heard nowhere, it only fires the sample.
            LaneSpec {
                input: vec![2],
                trigger: Some(TriggerSpec {
                    threshold_db: -30.0,
                    note: None,
                }),
                mute: true,
                ..lane("Trig")
            },
            LaneSpec {
                to: Some("Kick".into()),
                blocks: vec![
                    BlockSpec::signal("sample", "sample")
                        .asset(AssetRef::path("kick.wav"))
                        .param("dynamics", 0.0)
                        .key("Trig"),
                ],
                ..lane("Sample")
            },
            // Listed before what feeds it: the graph orders the lanes.
            lane("Sub"),
            lane("Kick"),
        ],
        macros: vec![],
    };
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let (g, report) = graph::build(&preset, &opts(&factories, &host));
    assert!(report.all_active(), "{report:?}");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let mut proc = Processor::new();
    let _ = proc.install(g);

    // In 0.1, Out 0.2 throughout; the trigger hits at frame 1000.
    let frames = 2048;
    let mut chans = vec![
        vec![0.1_f32; frames],
        vec![0.2_f32; frames],
        vec![0.0_f32; frames],
    ];
    for (i, s) in chans[2].iter_mut().enumerate().skip(1000).take(300) {
        *s = 0.9 * (-((i - 1000) as f32) / 60.0).exp() * if i % 2 == 0 { 1.0 } else { -1.0 };
    }
    let mut start = 0;
    while start < frames {
        let end = (start + 256).min(frames);
        let mut io: Vec<&mut [f32]> = chans.iter_mut().map(|c| &mut c[start..end]).collect();
        proc.process(&mut io, 3, end - start, &[], &[]);
        start = end;
    }
    // The trigger lane looks ahead 2 ms (96 frames, for a hit's onset
    // before its detection): the graph says so, and everything is heard
    // that late.
    let la = 96;
    assert_eq!(proc.latency(), la as u32);
    // The Kick bus (In + Out + Sample) and the Sub (In, pre) on 0/1.
    let base = 0.1 + 0.2 + 0.1;
    let out = &chans[0];
    assert!(
        (out[500] - base).abs() < 1e-4,
        "before the hit: {}",
        out[500]
    );
    let onset = 500
        + out[500..]
            .iter()
            .position(|s| (s - base).abs() > 0.1)
            .expect("the sample plays");
    // On the hit's onset (1000, heard at 1096), though the detector only
    // confirmed it in the next piece.
    assert_eq!(onset, 1000 + la, "the sample lands on the hit");
    assert!(
        (out[onset + 10] - (base + 0.5)).abs() < 1e-3,
        "{}",
        out[onset + 10]
    );
    assert!(
        (out[onset + 150] - base).abs() < 1e-4,
        "and ends: {}",
        out[onset + 150]
    );
}

#[test]
fn a_routing_loop_is_refused_not_run() {
    let preset = Preset {
        name: "Loop".into(),
        inputs: 2,
        outputs: 2,
        lanes: vec![
            LaneSpec {
                input: vec![0, 1],
                to: Some("B".into()),
                ..lane("A")
            },
            LaneSpec {
                to: Some("A".into()),
                ..lane("B")
            },
            LaneSpec {
                input: vec![0, 1],
                to: Some("Nowhere".into()),
                ..lane("C")
            },
        ],
        macros: vec![],
    };
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let (g, report) = graph::build(&preset, &opts(&factories, &NoHost));
    let w = report.warnings.join("\n");
    assert!(w.contains("A: in a routing loop"), "{w}");
    assert!(w.contains("B: in a routing loop"), "{w}");
    assert!(w.contains("C: no lane Nowhere"), "{w}");
    // Every lane still plays, to the outputs.
    let mut proc = Processor::new();
    let _ = proc.install(g);
    let out = play(&mut proc, &[0.25; 512], 2, 2, 256);
    assert!((out[0][100] - 0.5).abs() < 1e-5, "A and C: {}", out[0][100]);
}

/// The per-track mode: Signal on the kick's trigger track and on the kick
/// track, one session. The trigger instance publishes its hits; the kick
/// instance plays its sample from them, on the hit, though the host runs
/// it first in every cycle (the worst order).
#[test]
fn a_hit_on_one_track_plays_the_sample_on_another() {
    use signal_core::session::{HitBus, Session, SessionId};
    let host = MemoryHost::default().with("kick.wav", wav16(&[0.5; 100]));
    let preset = Preset {
        name: "Kick".into(),
        inputs: 2,
        outputs: 2,
        lanes: vec![
            LaneSpec {
                port: Some("Kick/Trig".into()),
                input: vec![0],
                mute: true,
                trigger: Some(TriggerSpec {
                    threshold_db: -30.0,
                    note: None,
                }),
                ..lane("Trig")
            },
            LaneSpec {
                port: Some("Kick".into()),
                input: vec![0],
                ..lane("In")
            },
            LaneSpec {
                port: Some("Kick".into()),
                blocks: vec![
                    BlockSpec::signal("sample", "sample")
                        .asset(AssetRef::path("kick.wav"))
                        .param("dynamics", 0.0)
                        .key("Trig"),
                ],
                ..lane("Sample")
            },
        ],
        macros: vec![],
    };
    let session = std::sync::Arc::new(Session {
        id: SessionId("kit".into()),
        hits: HitBus::new(),
    });
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let block = 256;
    let instance = |port: &'static str| {
        let o = BuildOptions {
            port: Some(port),
            ..opts(&factories, &host)
        };
        let (mut g, report) = graph::build(&preset, &o);
        assert!(report.all_active(), "{report:?}");
        assert!(report.warnings.is_empty(), "{port}: {:?}", report.warnings);
        g.join(session.clone());
        let mut p = Processor::new();
        let _ = p.install(g);
        p
    };
    let mut trig = instance("Kick/Trig");
    let mut kick = instance("Kick");
    assert_eq!(trig.latency(), 96, "the trigger track looks ahead 2 ms");
    assert_eq!(
        kick.latency(),
        (block + 96) as u32,
        "the kick hears a host block and the far detector's lookback late"
    );

    let frames = 4096;
    let mut trig_in = vec![0.0_f32; frames];
    for (i, s) in trig_in.iter_mut().enumerate().skip(1000).take(300) {
        *s = 0.9 * (-((i - 1000) as f32) / 60.0).exp() * if i % 2 == 0 { 1.0 } else { -1.0 };
    }
    let mut kick_out = vec![0.1_f32; frames];
    let mut start = 0;
    while start < frames {
        let end = (start + block).min(frames);
        let at = start as u64;
        // The kick first: the hits it needs were published cycles ago.
        kick.set_position(at);
        let mut io: Vec<&mut [f32]> = vec![&mut kick_out[start..end]];
        kick.process(&mut io, 1, end - start, &[], &[]);
        trig.set_position(at);
        let mut io: Vec<&mut [f32]> = vec![&mut trig_in[start..end]];
        trig.process(&mut io, 1, end - start, &[], &[]);
        start = end;
    }
    let la = block + 96;
    let base = 0.1;
    assert!(
        (kick_out[la + 10] - base).abs() < 1e-4,
        "{}",
        kick_out[la + 10]
    );
    let onset = la
        + kick_out[la..]
            .iter()
            .position(|s| (s - base).abs() > 0.1)
            .expect("the sample plays");
    assert_eq!(onset, 1000 + la, "on the hit, {la} late");
    assert!((kick_out[onset + 10] - (base + 0.5)).abs() < 1e-3);
}

/// The Crescendum kit: every lane builds and is routed, in the
/// multi-channel mode and on every track of the per-track mode, and the
/// tracks that follow a trigger find it over the session.
#[test]
fn the_drum_kit_builds_in_both_modes() {
    kit_builds_in_both_modes(&rigs::drum_kit(&rigs::KitPieces::default()));
}

fn kit_builds_in_both_modes(preset: &Preset) {
    let preset = preset.clone();
    // A kit's macros at rest leave the preset as it is: they work up from
    // it (a `Set` macro at 0 would pin its targets to their minimum).
    for m in &preset.macros {
        for t in &m.targets {
            assert!(
                t.mode == signal_core::state::MacroMode::Modulate && t.min == 0.0,
                "{}: macro {} moves {}/{} at rest",
                preset.name,
                m.name,
                t.lane,
                t.block
            );
        }
    }
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let (g, report) = graph::build(&preset, &opts(&factories, &NoHost));
    assert!(report.all_active(), "{report:?}");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(g.remote_keys(), 0, "one instance: every key is local");
    // The triggers look ahead 2 ms; the kick's Sub, gated (the drum
    // gate's 5 ms lookahead), is the latest path, and the kit waits for it.
    assert!(g.latency() >= 96 + 240, "{}: {}", preset.name, g.latency());

    // It plays: 20 mics of noise bursts in, two channels out.
    let mut proc = Processor::new();
    let _ = proc.install(g);
    let frames = 4096;
    let mut chans: Vec<Vec<f32>> = (0..20)
        .map(|c| {
            (0..frames)
                .map(|i| if (i + c * 37) % 997 < 40 { 0.5 } else { 0.0 })
                .collect()
        })
        .collect();
    let mut io: Vec<&mut [f32]> = chans.iter_mut().map(|c| &mut c[..]).collect();
    proc.process(&mut io, 20, 256, &[], &[]);
    assert!(
        chans[0][..256]
            .iter()
            .chain(&chans[1][..256])
            .all(|s| s.is_finite())
    );

    for lane in &preset.lanes {
        let port = lane.port.as_deref().expect("every kit lane is a track");
        let o = BuildOptions {
            port: Some(port),
            ..opts(&factories, &NoHost)
        };
        let (g, report) = graph::build(&preset, &o);
        assert!(report.all_active(), "{port}: {report:?}");
        assert!(report.warnings.is_empty(), "{port}: {:?}", report.warnings);
        let keyed = lane.blocks.iter().filter(|b| b.key.is_some()).count();
        assert_eq!(g.remote_keys(), keyed, "{port}: its keys are other tracks'");
    }
}

/// A block that is only latency: `frames` late, and says so.
struct Late {
    l: std::collections::VecDeque<f32>,
    r: std::collections::VecDeque<f32>,
}

const LATE: &[signal_core::param::ParamSpec] = &[signal_core::param::ParamSpec::new(
    "frames",
    "Frames",
    signal_core::param::Unit::Plain(0),
    0.0,
    4096.0,
    0.0,
)];

impl signal_core::block::BlockProcessor for Late {
    fn params(&self) -> &'static [signal_core::param::ParamSpec] {
        LATE
    }
    fn set_param(&mut self, _: usize, _: f32) {}
    fn reset(&mut self) {}
    fn latency(&self) -> u32 {
        self.l.len() as u32
    }
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for s in left.iter_mut() {
            self.l.push_back(*s);
            *s = self.l.pop_front().unwrap_or(0.0);
        }
        for s in right.iter_mut() {
            self.r.push_back(*s);
            *s = self.r.pop_front().unwrap_or(0.0);
        }
    }
}

struct LateFactory;

impl BlockFactory for LateFactory {
    fn runs_on(&self, kind: &BlockKind) -> Option<Platforms> {
        matches!(kind, BlockKind::Signal(n) if n == "late").then_some(Platforms::ALL)
    }
    fn build(
        &self,
        spec: &BlockSpec,
        _: &signal_core::block::BuildContext<'_>,
    ) -> Result<signal_core::block::Built, Unavailable> {
        let n = spec.params.get("frames").copied().unwrap_or(0.0) as usize;
        Ok(signal_core::block::Built {
            backend: Backend::Linked(Box::new(Late {
                l: std::iter::repeat_n(0.0, n).collect(),
                r: std::iter::repeat_n(0.0, n).collect(),
            })),
            params: LATE,
            sources: &[],
        })
    }
}

/// Parallel lanes of different latencies sum in line: a click through a
/// 100-frame lane and a dry one into a bus is one click, 100 late, and so
/// is a lane straight to the outputs beside it, and a pre send.
#[test]
fn parallel_lanes_sum_in_line() {
    let late = |frames: f32| BlockSpec::signal("late", "late").param("frames", frames);
    let preset = Preset {
        name: "Parallel".into(),
        inputs: 1,
        outputs: 2,
        lanes: vec![
            LaneSpec {
                input: vec![0],
                to: Some("Bus".into()),
                sends: vec![SendSpec::to("Pre", 0.0).pre()],
                ..lane("Dry")
            },
            LaneSpec {
                input: vec![0],
                to: Some("Bus".into()),
                blocks: vec![late(100.0)],
                ..lane("Wet")
            },
            lane("Bus"),
            LaneSpec {
                blocks: vec![late(30.0)],
                ..lane("Pre")
            },
            LaneSpec {
                input: vec![0],
                ..lane("Beside")
            },
        ],
        macros: vec![],
    };
    let factories: [&dyn BlockFactory; 2] = [&LateFactory, &BuiltinFactory];
    let (g, report) = graph::build(&preset, &opts(&factories, &NoHost));
    assert!(report.all_active(), "{report:?}");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(g.latency(), 100);
    let mut proc = Processor::new();
    let _ = proc.install(g);
    let mut input = vec![0.0_f32; 1024];
    input[10] = 1.0;
    let out = play(&mut proc, &input, 2, 1, 64);
    let clicks: Vec<(usize, f32)> = out[0]
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, s)| s.abs() > 1e-6)
        .collect();
    // Dry + Wet + Beside + Pre, all at 110.
    assert_eq!(clicks, vec![(110, 4.0)], "one click, in line");
}

/// Each mix phase of a lane is gain neutral once its make-up is measured:
/// the lane as loud with its phases on as with them bypassed, and a phase
/// out of the method's order reported.
#[test]
fn phases_are_made_gain_neutral() {
    use signal_core::state::Phase;
    let preset = |blocks: Vec<BlockSpec>| Preset {
        name: "Phases".into(),
        inputs: 1,
        outputs: 2,
        lanes: vec![LaneSpec {
            input: vec![0],
            blocks,
            ..lane("Snare")
        }],
        macros: vec![],
    };
    let chain = vec![
        BlockSpec::signal("rescue", "eq")
            .algorithm("channel")
            .param("low_cut", 80.0)
            .phase(Phase::Rescue),
        BlockSpec::signal("comp", "comp")
            .param("threshold", -30.0)
            .param("ratio", 4.0)
            .phase(Phase::Tone),
        BlockSpec::signal("sat", "drive")
            .param("drive", 12.0)
            .phase(Phase::Tone),
        BlockSpec::signal("up", "gain")
            .param("gain", 6.0)
            .phase(Phase::Polish),
    ];
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let input = sine(48_000);
    let render = |p: &Preset| {
        let (g, report) = graph::build(p, &opts(&factories, &NoHost));
        let mut proc = Processor::new();
        let _ = proc.install(g);
        let out = play(&mut proc, &input, 2, 1, 256);
        (
            out,
            proc.graph()
                .map(graph::Graph::phase_levels)
                .unwrap_or_default(),
            report,
        )
    };
    let rms = |x: &[f32]| (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt();
    let mut p = preset(chain.clone());
    let (_, levels, report) = render(&p);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(
        levels.iter().map(|l| l.phase).collect::<Vec<_>>(),
        vec![Phase::Rescue, Phase::Tone, Phase::Polish]
    );
    assert!(
        (levels[2].neutral_trim_db() + 6.0).abs() < 0.01,
        "the +6 dB gain: {:?}",
        levels[2]
    );
    for _ in 0..3 {
        let (_, levels, _) = render(&p);
        graph::apply_neutral_trims(&mut p, &levels);
    }
    let (on, ..) = render(&p);
    // Over what the trims were measured on: the whole song.
    let bypassed = rms(&input);
    let active = rms(&on[0]);
    assert!(
        (20.0 * (active / bypassed).log10()).abs() < 0.5,
        "on {active}, bypassed {bypassed}"
    );

    // Tone after Polish: out of order, said so.
    let mut back = chain;
    back.swap(1, 3);
    let (_, _, report) = render(&preset(back));
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("out of the mix's phase order")),
        "{:?}",
        report.warnings
    );
}

/// A reset clears every tail: a reverb that has rung, reset, is silent.
#[test]
fn a_reset_clears_the_reverbs() {
    use signal_core::state::Phase;
    for algorithm in ["nonlinear", "plate", "room", "hall"] {
        let preset = Preset {
            name: "Verb".into(),
            inputs: 1,
            outputs: 2,
            lanes: vec![LaneSpec {
                input: vec![0],
                blocks: vec![
                    BlockSpec::signal("verb", "reverb")
                        .algorithm(algorithm)
                        .param("mix", 1.0)
                        .phase(Phase::Depth),
                ],
                ..lane("Verb")
            }],
            macros: vec![],
        };
        let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
        let (g, _) = graph::build(&preset, &opts(&factories, &NoHost));
        let mut proc = Processor::new();
        let _ = proc.install(g);
        let _ = play(&mut proc, &sine(24_000), 2, 1, 256);
        proc.reset();
        let after = play(&mut proc, &vec![0.0; 4096], 2, 1, 256);
        assert!(
            peak(&after[0]) < 1e-6 && peak(&after[1]) < 1e-6,
            "{algorithm} rings on after a reset: {}",
            peak(&after[0]).max(peak(&after[1]))
        );
    }
}

/// The host's placed hits (Studio's `hits:<piece>`): a piece's whole list,
/// handed to the processor, plays its sample on each hit's frame, with no
/// trigger and nothing detected.
#[test]
fn placed_hits_play_on_their_frames() {
    use signal_core::session::{HitBus, Session, SessionId};
    let host = MemoryHost::default().with("kick.wav", wav16(&[0.5; 100]));
    let preset = Preset {
        name: "Kick".into(),
        inputs: 1,
        outputs: 2,
        lanes: vec![
            LaneSpec {
                port: Some("Kick/Trig".into()),
                input: vec![0],
                mute: true,
                trigger: Some(TriggerSpec {
                    threshold_db: -30.0,
                    note: None,
                }),
                ..lane("Trig")
            },
            LaneSpec {
                port: Some("Kick".into()),
                blocks: vec![
                    BlockSpec::signal("sample", "sample")
                        .asset(AssetRef::path("kick.wav"))
                        .param("dynamics", 0.0)
                        .key("Trig"),
                ],
                ..lane("Sample")
            },
        ],
        macros: vec![],
    };
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let (mut g, _) = graph::build(
        &preset,
        &BuildOptions {
            port: Some("Kick"),
            ..opts(&factories, &host)
        },
    );
    g.join(std::sync::Arc::new(Session {
        id: SessionId("placed".into()),
        hits: HitBus::new(),
    }));
    let (mut controller, mut proc) = processor_pair();
    let _ = proc.install(g);
    // The Trig lane is the preset's lane 0: its hits at 0.5 s and 1 s.
    let _ = controller.send(Command::PlannedHits {
        piece: 0,
        hits: vec![(24_000, 1.0), (48_000, 1.0)].into_boxed_slice(),
    });
    let latency = proc.latency() as usize;
    let out = play(&mut proc, &vec![0.0; 72_000], 2, 0, 256);
    let onsets: Vec<usize> = out[0]
        .windows(2)
        .enumerate()
        .filter(|(_, w)| w[0] < 0.25 && w[1] >= 0.25)
        .map(|(i, _)| i + 1)
        .collect();
    assert_eq!(
        onsets,
        vec![24_000 + latency, 48_000 + latency],
        "on the placed frames, {latency} late"
    );
    controller.collect();
}
