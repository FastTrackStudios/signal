//! Signal's block families against their real CLAPs (FTS EQ, FTS Reverb,
//! FTS Delay, FTS Comp, FTS Saturate, FTS Gate: FastTrackStudios/plugins
//! `apps/plugins/*`), loaded through daw's plugin host.
//!
//! Needs the bundles: in a plugins checkout, for each,
//! `cargo run -p fts-plugin-xtask -- bundle eq-plugin --release --features export`; then
//!
//! ```sh
//! SIGNAL_FAMILY_CLAPS=<…/target/bundled> cargo test -p signal-core-clap --test real_clap -- --ignored --nocapture
//! ```

use std::collections::HashMap;
use std::path::PathBuf;

use signal_core::block::{Backend, BuildContext};
use signal_core::graph::{self, BuildOptions};
use signal_core::host::NoHost;
use signal_core::null_test::{self, NullCase};
use signal_core::param::MACROS;
use signal_core::{BlockFactory, BuiltinFactory, Processor, processor_pair, rigs};
use signal_core_clap::{BlockDev, ClapFactory, HostedClap};

const SR: f32 = 48_000.0;

fn bundles() -> PathBuf {
    PathBuf::from(
        std::env::var_os("SIGNAL_FAMILY_CLAPS")
            .expect("SIGNAL_FAMILY_CLAPS: the plugins' target/bundled"),
    )
}

/// A family's CLAP, by its product name.
fn bundle_of(family: &str) -> Option<PathBuf> {
    let name = match family {
        "eq" => "FTS EQ",
        "reverb" => "FTS Reverb",
        "delay" => "FTS Delay",
        "comp" => "FTS Comp",
        "drive" => "FTS Saturate",
        "gate" => "FTS Gate",
        _ => return None,
    };
    Some(bundles().join(format!("{name}.clap")))
}

#[test]
#[ignore = "needs SIGNAL_FAMILY_CLAPS, the bundled family CLAPs"]
fn every_algorithm_nulls_compiled_in_against_its_familys_clap() {
    let mut failures = Vec::new();
    let mut cases = 0;
    for family in plugin_processors::NAMES {
        let Some(path) = bundle_of(family) else {
            println!("{family}: no CLAP form (a Signal-only block)");
            continue;
        };
        assert!(path.exists(), "no bundle {}", path.display());
        let clap = ClapFactory {
            overrides: HashMap::from([((*family).to_owned(), path)]),
            ..ClapFactory::default()
        };
        for algo in plugin_processors::algorithms::family(family) {
            let mut offset = 0;
            while let Some(case) =
                NullCase::for_algorithm(family, Some(algo.id), algo.controls, offset, None)
            {
                let r = null_test::null(
                    &case,
                    &[&BuiltinFactory],
                    &[&clap, &BuiltinFactory],
                    &NoHost,
                );
                let side = r
                    .report_b
                    .blocks
                    .first()
                    .map(|b| (b.backend.clone(), b.status.clone()));
                println!(
                    "{family}/{}[{offset}..]: peak {:.3}, diff {:.1} dBFS",
                    algo.id, r.peak, r.peak_diff_dbfs
                );
                if side.as_ref().map(|s| s.0.as_str()) != Some("clap") || !r.nulls() {
                    failures.push(format!(
                        "{family}/{}[{offset}..]: {:.1} dBFS ({side:?})",
                        algo.id, r.peak_diff_dbfs
                    ));
                }
                cases += 1;
                offset += MACROS;
            }
        }
    }
    println!("{cases} cases");
    assert!(
        failures.is_empty(),
        "algorithms whose CLAP form doesn't null:\n{}",
        failures.join("\n")
    );
}

#[test]
#[ignore = "needs SIGNAL_FAMILY_CLAPS, the bundled family CLAPs"]
fn a_family_clap_renames_its_slots_for_the_algorithm_it_loads() {
    let path = bundle_of("eq").expect("eq");
    let mut clap = HostedClap::load(&path, SR, 256, None).expect("FTS EQ loads");
    let signal = clap.signal().expect("FTS EQ answers signal/1");
    let names = |clap: &mut HostedClap| {
        clap.plugin_mut()
            .params()
            .into_iter()
            .map(|p| p.name)
            .collect::<Vec<_>>()
    };
    let before = names(&mut clap);
    assert_eq!(
        before.len(),
        192,
        "the same host-visible slots whatever the algorithm"
    );
    println!("parametric: {:?}", &before[..4]);
    assert_eq!(before[0], "Band 1 Used");

    let revision = clap.plugin_mut().param_info_revision();
    clap.select_algorithm("ssl-e").expect("ssl-e loads");
    assert!(
        clap.plugin_mut().param_info_revision() > revision,
        "the host was asked to rescan"
    );
    let after = names(&mut clap);
    println!("ssl-e: {:?}", &after[..6]);
    assert_eq!(
        &after[..6],
        ["HPF", "LPF", "LF", "LF Freq", "LF Bell", "LMF"]
    );
    assert_eq!(
        after[14], "(unused 15)",
        "slots past the algorithm's controls say so"
    );
    assert_eq!(
        signal
            .param_stable_id(clap.plugin_mut().params()[2].id)
            .as_deref(),
        Some("lf_gain")
    );

    // A community algorithm, loaded over signal/1, names the slots too.
    let tilt = r#"
id tilt
family eq
name "Tilt (community)"
engine "eq.canonical@1"
controls ({id tilt, name Tilt, unit db, min -6, max 6, default 0})
map {
    band.1.used "1"
    band.1.shape tilt_shelf
    band.1.freq "650"
    band.1.gain tilt
}
"#;
    let args = serde_json::json!({ "styx": tilt }).to_string();
    let reply = signal
        .run_action("SIGNAL_ALGORITHM_LOAD", &args)
        .expect("the file loads");
    assert!(reply.contains("\"algorithm\":\"tilt\""), "{reply}");
    clap.plugin_mut().pump_main_thread();
    assert_eq!(names(&mut clap)[0], "Tilt");
    // A bad file is refused with the reason.
    let bad = serde_json::json!({ "styx": tilt.replace("band.1.gain", "band.1.gane") }).to_string();
    let err = signal
        .run_action("SIGNAL_ALGORITHM_LOAD", &bad)
        .expect_err("refused");
    println!("refused: {err}");
    assert!(err.to_string().contains("band.1.gane"), "{err}");
}

#[test]
#[ignore = "needs SIGNAL_FAMILY_CLAPS, the bundled family CLAPs"]
fn a_family_claps_state_keeps_its_algorithm_and_values() {
    let path = bundle_of("reverb").expect("reverb");
    let mut a = HostedClap::load(&path, SR, 256, None).expect("FTS Reverb loads");
    a.select_algorithm("hall").expect("hall");
    let state = a.plugin_mut().save_state().expect("saves");
    let mut b = HostedClap::load(&path, SR, 256, None).expect("a second instance");
    assert!(b.plugin_mut().load_state(&state).is_ok(), "restores");
    b.plugin_mut().pump_main_thread();
    let chain = b.signal().expect("signal/1").chain().expect("the chain");
    println!("restored: {chain}");
    assert!(chain.contains("\"algorithm\":\"hall\""), "{chain}");
    assert_eq!(b.plugin_mut().params()[0].name, "Decay");
}

fn play(proc: &mut Processor, frames: usize) -> Vec<f32> {
    let input = null_test::test_signal(frames, SR);
    let (mut l, mut r) = (input.clone(), input);
    for start in (0..frames).step_by(256) {
        let end = (start + 256).min(frames);
        let mut io: Vec<&mut [f32]> = vec![&mut l[start..end], &mut r[start..end]];
        proc.process(&mut io, 2, end - start, &[], &[]);
    }
    l
}

#[test]
#[ignore = "needs SIGNAL_FAMILY_CLAPS, the bundled family CLAPs"]
fn a_block_under_development_hot_swaps_while_the_chain_plays() {
    let preset = rigs::dsp_chain();
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let opts = BuildOptions {
        sample_rate: SR,
        max_block: 256,
        port: None,
        factories: &factories,
        host: &NoHost,
    };
    let (g, _) = graph::build(&preset, &opts);
    let (mut ctl, mut proc) = processor_pair();
    assert!(ctl.install(g).is_ok());
    assert!(play(&mut proc, 4800).iter().all(|s| s.is_finite()));

    // Develop the EQ (the chain's runs the Channel Strip): FTS EQ, watched.
    let dir = tempfile::tempdir().expect("temp dir");
    let built = dir.path().join("FTS EQ.clap");
    std::fs::copy(bundle_of("eq").expect("eq"), &built).expect("the bundle");
    let mut dev = BlockDev::new("eq", &built, dir.path().join("copies"));
    let cx = BuildContext {
        sample_rate: SR,
        max_block: 256,
        host: &NoHost,
    };

    let first = dev.poll().expect("polls").expect("a first copy");
    for cmd in dev
        .swaps(&first, &preset, None, &cx, None)
        .expect("FTS EQ stands in")
    {
        assert!(ctl.send(cmd).is_ok());
    }
    let during = play(&mut proc, 4800);
    assert!(during.iter().all(|s| s.is_finite()) && during.iter().any(|s| s.abs() > 1e-3));
    let eq = proc.graph().and_then(|g| g.block(0, 0)).expect("the eq");
    assert!(
        matches!(eq.backend(), Backend::Clap(_)),
        "the EQ runs FTS EQ"
    );
    let low_cut = eq
        .params()
        .index("low_cut")
        .expect("the channel strip's low cut");
    assert_eq!(
        eq.params().base(low_cut),
        Some(60.0),
        "the preset survives the swap"
    );

    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::copy(bundle_of("eq").expect("eq"), &built).expect("rebuilt");
    let second = dev.poll().expect("polls").expect("the rebuild is seen");
    assert_ne!(first, second);
    for cmd in dev.swaps(&second, &preset, None, &cx, None).expect("again") {
        assert!(ctl.send(cmd).is_ok());
    }
    assert!(play(&mut proc, 4800).iter().all(|s| s.is_finite()));
    ctl.collect();

    for cmd in dev.end(&preset, None, &cx).expect("the linked block") {
        assert!(ctl.send(cmd).is_ok());
    }
    let _ = play(&mut proc, 256);
    let eq = proc.graph().and_then(|g| g.block(0, 0)).expect("the eq");
    assert!(eq.backend().is_linked());
    assert_eq!(eq.params().base(low_cut), Some(60.0));
    ctl.collect();
    println!(
        "hot swap: linked -> {} -> {} -> linked",
        first.display(),
        second.display()
    );
}
