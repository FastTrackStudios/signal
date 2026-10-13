//! Verifies that `project_loader::load_rpp_text` walks the
//! `<FXCHAIN>` blocks of each track, instantiates the listed
//! plugins through `Effects::add`, and applies any state data they
//! carry via `Standalone::apply_plugin_state`.
//!
//! Real-plugin tests are gated on `DAW_TEST_VST3_BUNDLE` pointing
//! at a `.vst3` bundle present on the host (resolver walks the
//! standard `$HOME/.vst3` + `/usr/(local/)?lib/vst3` paths).

#![cfg(all(feature = "rpp-project", feature = "vst3-host"))]

use std::path::PathBuf;

use daw_standalone::project_loader::load_rpp_text;
use daw_standalone::sync::Standalone;
use dawfile_reaper::RppSerialize;
use dawfile_reaper::builder::ReaperProjectBuilder;

#[test]
fn loads_vst3_fx_chain_from_rpp_and_restores_state() {
    let Some(env_path) = std::env::var_os("DAW_TEST_VST3_BUNDLE") else {
        eprintln!("(skip) set DAW_TEST_VST3_BUNDLE to a .vst3 bundle to exercise this test");
        return;
    };
    let bundle_path = PathBuf::from(&env_path);
    // dawfile-reaper writes the filename only, no path. The project
    // loader's resolver walks ~/.vst3, /usr/lib/vst3, etc. — so the
    // bundle has to actually live there for the test to find it.
    let filename = bundle_path
        .file_name()
        .and_then(|s| s.to_str())
        .expect("bundle path has a filename");
    if filename.contains(char::is_whitespace) {
        // REAPER's `.rpp` format tokenizes the VST filename on
        // whitespace, so a bundle named "Foo Bar.vst3" round-trips
        // as the partial "Foo". This is a REAPER-format constraint,
        // not a host bug — skip with a note so the suite stays green
        // when the user only has spacey-named bundles installed.
        eprintln!(
            "(skip) bundle filename {filename:?} contains whitespace — RPP tokenizer can't represent it. Try a bundle without spaces in the .vst3 filename."
        );
        return;
    }
    let display_name = bundle_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("PluginUnderTest")
        .to_string();

    // Round 1: build a fresh plugin instance, save its baseline state
    // bytes. We'll round-trip these through an RPP chunk in round 2.
    let baseline_state = {
        use daw_standalone::audio_engine::vst3_host::Vst3Host;
        let mut plugin = Vst3Host::new().load(&bundle_path, 0).expect("load plugin");
        plugin.prepare(48_000.0, 256).expect("prepare");
        let state = plugin.save_state().expect("save_state");
        plugin.deactivate();
        state
    };
    assert!(baseline_state.len() >= 12);
    assert_eq!(&baseline_state[..4], b"DAW3");

    // Re-pack the daw-internal blob as a REAPER-style base64 chunk
    // so the loader's parser can decode it. The parser sees an
    // unknown header → falls through to "everything is component
    // state", which `LoadedVst3Plugin::load_state` accepts because
    // it understands DAW3 framing.
    let rpp_state_lines = encode_to_rpp_lines(&baseline_state);

    // Build a minimal RPP project with one track + one VST3 FX
    // carrying the state we just saved.
    let mut project = ReaperProjectBuilder::new()
        .track("FX Test", |t| t.vst3(&display_name, filename))
        .build();
    // The builder's `.vst3()` helper doesn't expose a way to chain
    // `.state()` onto the same FX, so reach into the built struct
    // and inject the state lines on the single plugin we added.
    if let Some(fxc) = project.tracks[0].fx_chain.as_mut() {
        if let Some(dawfile_reaper::types::fx_chain::FxChainNode::Plugin(p)) = fxc.nodes.first_mut()
        {
            p.state_data = rpp_state_lines.clone();
        } else {
            panic!("expected a single Plugin node on the test track");
        }
    } else {
        panic!("track's fx_chain should be Some after .vst3()");
    }
    let rpp_text = project.to_rpp_string();

    // Load via the public project_loader API.
    let daw = Standalone::new();
    let summary =
        load_rpp_text(&daw, "fx-state-test", "/tmp/test.rpp", &rpp_text).expect("load_rpp_text");
    eprintln!(
        "loaded {} track(s); FX warnings: {:?}",
        summary.track_count, summary.warnings
    );
    assert_eq!(summary.track_count, 1);

    // Look up the loaded FX and check the plugin instance is live.
    use daw_proto::fx::{Effects, FxChainContext};
    use daw_proto::project::ProjectContext;
    let ctx = ProjectContext::Project(summary.project_guid.clone());
    let tracks: Vec<String> = daw
        .read_project(&summary.project_guid, |p| {
            p.tracks.iter().map(|t| t.guid.clone()).collect()
        })
        .unwrap_or_default();
    assert_eq!(tracks.len(), 1, "expected one track");
    let fx_list = Effects::list(&daw, ctx.clone(), FxChainContext::Track(tracks[0].clone()));
    assert_eq!(
        fx_list.len(),
        1,
        "expected one FX on the track, got {:?}",
        fx_list
    );
    let fx_guid = fx_list[0].guid.clone();
    assert!(
        daw.has_plugin_instance(&fx_guid),
        "FX guid should be backed by a real plugin instance, not just synthetic"
    );

    // Compare the post-load state against the baseline — the loader
    // applied our chunk, so save_plugin_state should reproduce it
    // bit-for-bit.
    let restored = daw.save_plugin_state(&fx_guid).expect("save_plugin_state");
    eprintln!(
        "baseline={} bytes, restored={} bytes, byte-equal? {}",
        baseline_state.len(),
        restored.len(),
        baseline_state == restored
    );
    assert_eq!(
        baseline_state, restored,
        "round-tripped state should match baseline"
    );
}

/// Take a raw byte blob and turn it into the base64 lines REAPER
/// writes inside a `<VST …>` block. Lines wrap at 128 chars to mirror
/// REAPER's emission style.
fn encode_to_rpp_lines(bytes: &[u8]) -> Vec<String> {
    use base64::Engine;
    let single = base64::engine::general_purpose::STANDARD.encode(bytes);
    let chunk = 128;
    single
        .as_bytes()
        .chunks(chunk)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect()
}
