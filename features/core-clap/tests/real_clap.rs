//! Signal's blocks against their real CLAP forms (FastTrackStudios/plugins
//! `apps/blocks/*`), loaded through daw's plugin host.
//!
//! Needs the bundles: in a plugins checkout,
//! `cargo run -p fts-plugin-xtask -- bundle block-comp --release --features export`
//! (each block), then
//!
//! ```sh
//! SIGNAL_BLOCK_CLAPS=<…/target/bundled> cargo test -p signal-core-clap --test real_clap -- --ignored --nocapture
//! ```

use std::collections::HashMap;
use std::path::PathBuf;

use signal_core::block::{Backend, BuildContext};
use signal_core::graph::{self, BuildOptions};
use signal_core::host::NoHost;
use signal_core::null_test::{self, NullCase};
use signal_core::param::MACROS;
use signal_core::{BlockFactory, BuiltinFactory, Processor, processor_pair, rigs};
use signal_core_clap::{BlockDev, ClapFactory};

const SR: f32 = 48_000.0;

fn bundles() -> PathBuf {
    PathBuf::from(
        std::env::var_os("SIGNAL_BLOCK_CLAPS")
            .expect("SIGNAL_BLOCK_CLAPS: the plugins' target/bundled"),
    )
}

/// A block's bundle: `FTS <Name> Block.clap`.
fn bundle_of(block: &str) -> PathBuf {
    let name = match block {
        "eq" => "EQ".to_owned(),
        other => {
            let mut c = other.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        }
    };
    bundles().join(format!("FTS {name} Block.clap"))
}

#[test]
#[ignore = "needs SIGNAL_BLOCK_CLAPS, the bundled block CLAPs"]
fn every_block_nulls_compiled_in_against_its_real_clap() {
    let mut failures = Vec::new();
    for name in plugin_processors::NAMES {
        let path = bundle_of(name);
        assert!(path.exists(), "no bundle {}", path.display());
        let clap = ClapFactory {
            overrides: HashMap::from([((*name).to_owned(), path)]),
            ..ClapFactory::default()
        };
        let params = plugin_processors::params_of(name).expect("a schema");
        let mut offset = 0;
        while let Some(case) = NullCase::for_block(name, params, offset, None) {
            let r = null_test::null(
                &case,
                &[&BuiltinFactory],
                &[&clap, &BuiltinFactory],
                &NoHost,
            );
            let backend = r
                .report_b
                .blocks
                .first()
                .map(|b| (b.backend.clone(), b.status.clone()));
            println!(
                "{name}[{offset}..]: peak {:.3}, diff {:.1} dBFS, CLAP side {backend:?}",
                r.peak, r.peak_diff_dbfs
            );
            if backend.as_ref().map(|b| b.0.as_str()) != Some("clap") || !r.nulls() {
                failures.push(format!(
                    "{name}[{offset}..]: {:.1} dBFS ({backend:?})",
                    r.peak_diff_dbfs
                ));
            }
            offset += MACROS;
        }
    }
    assert!(
        failures.is_empty(),
        "blocks whose CLAP form doesn't null:\n{}",
        failures.join("\n")
    );
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
#[ignore = "needs SIGNAL_BLOCK_CLAPS, the bundled block CLAPs"]
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
    let before = play(&mut proc, 4800);
    assert!(before.iter().all(|s| s.is_finite()));

    // Develop the comp: its built CLAP, watched.
    let dir = tempfile::tempdir().expect("temp dir");
    let built = dir.path().join("FTS Comp Block.clap");
    std::fs::copy(bundle_of("comp"), &built).expect("the comp bundle");
    let mut dev = BlockDev::new("comp", &built, dir.path().join("copies"));
    let cx = BuildContext {
        sample_rate: SR,
        max_block: 256,
        host: &NoHost,
    };

    let first = dev.poll().expect("polls").expect("a first copy");
    for cmd in dev
        .swaps(&first, &preset, None, &cx, None)
        .expect("the CLAP stands in")
    {
        assert!(ctl.send(cmd).is_ok());
    }
    let during = play(&mut proc, 4800);
    assert!(
        during.iter().all(|s| s.is_finite()) && during.iter().any(|s| s.abs() > 1e-3),
        "silent after the swap"
    );
    let comp = proc.graph().and_then(|g| g.block(0, 1)).expect("the comp");
    assert!(
        matches!(comp.backend(), Backend::Clap(_)),
        "the comp runs its CLAP"
    );
    assert_eq!(
        comp.params().base(0),
        Some(-20.0),
        "the preset's threshold survives the swap"
    );
    assert!(
        dev.poll().expect("polls").is_none(),
        "nothing rebuilt: no new copy"
    );

    // A rebuild: a newer file. A fresh copy, swapped in again.
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::copy(bundle_of("comp"), &built).expect("rebuilt");
    let second = dev.poll().expect("polls").expect("the rebuild is seen");
    assert_ne!(first, second, "each rebuild loads under a fresh name");
    for cmd in dev
        .swaps(&second, &preset, None, &cx, None)
        .expect("the CLAP stands in")
    {
        assert!(ctl.send(cmd).is_ok());
    }
    let after = play(&mut proc, 4800);
    assert!(after.iter().all(|s| s.is_finite()));
    ctl.collect();

    // Ending the override puts the compiled-in block back.
    for cmd in dev.end(&preset, None, &cx).expect("the linked block") {
        assert!(ctl.send(cmd).is_ok());
    }
    let _ = play(&mut proc, 256);
    let comp = proc.graph().and_then(|g| g.block(0, 1)).expect("the comp");
    assert!(comp.backend().is_linked(), "the compiled-in comp is back");
    assert_eq!(comp.params().base(0), Some(-20.0));
    ctl.collect();
    println!(
        "hot swap: linked -> {} -> {} -> linked, the chain playing throughout",
        first.file_name().unwrap_or_default().to_string_lossy(),
        second.file_name().unwrap_or_default().to_string_lossy()
    );
}
