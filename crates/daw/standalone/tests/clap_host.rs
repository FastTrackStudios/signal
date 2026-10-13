//! CLAP host smoke tests. Verifies the public API compiles + handles
//! error paths cleanly. Real-plugin tests are gated on a local CLAP
//! bundle path being set via `DAW_TEST_CLAP_BUNDLE` env var (skipped
//! otherwise so CI without a plugin installed stays green).

#![cfg(feature = "clap-host")]

use std::path::PathBuf;

use daw_standalone::audio_engine::plugin_host::{ClapHost, ClapHostError, ClapPluginSelector};

#[test]
fn load_from_bogus_path_errors_cleanly() {
    let host = ClapHost::default();
    let err = host
        .list_in_bundle(&PathBuf::from("/definitely/not/a/clap/bundle.clap"))
        .unwrap_err();
    assert!(matches!(err, ClapHostError::BundleLoad));
}

#[test]
fn host_constructs_with_custom_identity() {
    let host = ClapHost::new("MyDaw", "com.example.mydaw");
    let _ = host; // construction must not panic; visible via dropping the value
}

fn lsp_parametric_eq_bundle_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("DAW_TEST_LSP_CLAP_BUNDLE") {
        return Some(PathBuf::from(path));
    }

    [
        "/usr/lib/clap/lsp-plugins-clap.clap",
        "/usr/local/lib/clap/lsp-plugins-clap.clap",
        "/usr/lib/clap/LSP Parametric Equalizer.clap",
        "/usr/local/lib/clap/LSP Parametric Equalizer.clap",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|p| p.exists())
}

fn fts_eq_bundle_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("DAW_TEST_FTS_EQ_CLAP_BUNDLE") {
        return Some(PathBuf::from(path));
    }

    [
        "/home/cody/.clap/eq-plugin.clap",
        "/home/cody/fasttrackstudio/UserPlugins/FX/eq-plugin.clap",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|p| p.exists())
}

fn gui_hold_duration(default_secs: u64) -> std::time::Duration {
    let hold_secs = std::env::var("DAW_TEST_CLAP_GUI_HOLD_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(default_secs);
    std::time::Duration::from_secs(hold_secs)
}

/// Manual GUI smoke test for LSP Parametric Equalizer x32 Stereo CLAP.
///
/// This is ignored because it opens a native window and requires a
/// graphical session plus the LSP CLAP bundle. Run it manually with:
///
/// ```bash
/// DAW_TEST_LSP_CLAP_BUNDLE=/path/to/lsp-plugins-clap.clap \
/// DAW_TEST_CLAP_GUI_HOLD_SECS=20 \
/// cargo test -p daw-standalone --features clap-host \
///   clap_lsp_parametric_equalizer_x32_stereo_gui_opens -- --ignored --nocapture
/// ```
#[test]
#[ignore = "manual GUI test: opens a native CLAP plugin window"]
fn clap_lsp_parametric_equalizer_x32_stereo_gui_opens() {
    let Some(path) = lsp_parametric_eq_bundle_path() else {
        eprintln!(
            "(skip) set DAW_TEST_LSP_CLAP_BUNDLE to the LSP CLAP bundle path, e.g. lsp-plugins-clap.clap"
        );
        return;
    };

    let host = ClapHost::default();
    let selector = ClapPluginSelector::terms(["parametric", "equalizer", "x32", "stereo"]);
    let (plugin_index, descriptor) = host
        .find_in_bundle(&path, &selector)
        .expect("LSP Parametric Equalizer x32 Stereo descriptor should exist");
    eprintln!(
        "opening LSP Parametric Equalizer x32 Stereo CLAP GUI for descriptor #{plugin_index}: '{}' id='{}' from {}",
        descriptor.name,
        descriptor.id,
        path.display()
    );

    let result = host
        .smoke_test_gui(&path, &selector, gui_hold_duration(10))
        .expect("CLAP GUI should open");
    eprintln!(
        "CLAP GUI smoke test complete: descriptor #{} '{}' held for {:?}",
        result.plugin_index, result.descriptor.name, result.held_for
    );
}

/// Generic native-GUI smoke test for CI. This is intentionally
/// selector-driven so plugin projects such as FTS-EQ can reuse the
/// DAW host without adding a bespoke test per plugin.
///
/// ```bash
/// DAW_TEST_CLAP_GUI_BUNDLE=/path/to/plugin.clap \
/// DAW_TEST_CLAP_GUI_TERMS=fts,eq \
/// DAW_TEST_CLAP_GUI_HOLD_SECS=2 \
/// cargo test -p daw-standalone --features clap-host \
///   clap_selected_plugin_gui_opens -- --ignored --nocapture
/// ```
#[test]
#[ignore = "manual/CI GUI test: opens a native CLAP plugin window"]
fn clap_selected_plugin_gui_opens() {
    let Some(path) = std::env::var_os("DAW_TEST_CLAP_GUI_BUNDLE") else {
        eprintln!("(skip) set DAW_TEST_CLAP_GUI_BUNDLE to a .clap bundle path");
        return;
    };

    let host = ClapHost::default();
    let mut selector = ClapPluginSelector::from_env("DAW_TEST_CLAP_GUI");
    if selector.id.is_none() && selector.required_terms.is_empty() {
        selector = ClapPluginSelector::any();
    }
    let result = host
        .smoke_test_gui(&PathBuf::from(path), &selector, gui_hold_duration(2))
        .expect("selected CLAP GUI should open");
    eprintln!(
        "CLAP GUI smoke test complete: descriptor #{} '{}' id='{}' held for {:?}",
        result.plugin_index, result.descriptor.name, result.descriptor.id, result.held_for
    );
}

/// FTS-EQ native-GUI smoke test. CI can point this at the plugin
/// build artifact with `DAW_TEST_FTS_EQ_CLAP_BUNDLE`; local developer
/// machines can use the default FastTrackStudio install locations.
#[test]
#[ignore = "manual/CI GUI test: opens the FTS-EQ CLAP plugin window"]
fn clap_fts_eq_gui_opens() {
    let Some(path) = fts_eq_bundle_path() else {
        eprintln!("(skip) set DAW_TEST_FTS_EQ_CLAP_BUNDLE to the FTS-EQ .clap bundle path");
        return;
    };

    let host = ClapHost::default();
    let mut selector = ClapPluginSelector::from_env("DAW_TEST_FTS_EQ_CLAP");
    if selector.id.is_none() && selector.required_terms.is_empty() {
        selector = ClapPluginSelector::any();
    }
    let result = host
        .smoke_test_gui(&path, &selector, gui_hold_duration(2))
        .expect("FTS-EQ CLAP GUI should open");
    eprintln!(
        "FTS-EQ GUI smoke test complete: descriptor #{} '{}' id='{}' held for {:?}",
        result.plugin_index, result.descriptor.name, result.descriptor.id, result.held_for
    );
}

/// Real-plugin smoke test: list descriptors. Set
/// `DAW_TEST_CLAP_BUNDLE=/path/to/something.clap` before running.
#[test]
fn lists_descriptors_in_a_real_bundle() {
    let Some(path) = std::env::var_os("DAW_TEST_CLAP_BUNDLE") else {
        eprintln!("(skip) set DAW_TEST_CLAP_BUNDLE to a .clap bundle path to exercise this test");
        return;
    };
    let descriptors = ClapHost::default()
        .list_in_bundle(&PathBuf::from(path))
        .expect("bundle should load");
    assert!(!descriptors.is_empty(), "bundle should expose ≥1 plugin");
    let first = &descriptors[0];
    assert!(!first.id.is_empty(), "plugin should have an id");
    eprintln!(
        "loaded {} descriptor(s); first = id={:?} name={:?} vendor={:?} version={:?}",
        descriptors.len(),
        first.id,
        first.name,
        first.vendor,
        first.version,
    );
}

/// Real-plugin processing smoke test. Activates the plugin, pushes
/// 4 blocks of audio through it, asserts every output sample is
/// finite (no NaN/Inf), then deactivates.
#[test]
fn processes_audio_through_a_real_plugin() {
    let Some(path) = std::env::var_os("DAW_TEST_CLAP_BUNDLE") else {
        eprintln!("(skip) set DAW_TEST_CLAP_BUNDLE to a .clap bundle path to exercise this test");
        return;
    };
    let host = ClapHost::default();
    let mut plugin = host
        .load(&PathBuf::from(path), 0)
        .expect("plugin should instantiate");

    let sample_rate = 48_000.0;
    let block = 128usize;
    plugin
        .prepare(sample_rate, block as u32)
        .expect("plugin should activate");
    assert!(plugin.is_prepared());
    assert_eq!(plugin.sample_rate(), Some(sample_rate));
    assert_eq!(plugin.block_size(), Some(block as u32));

    // 4 blocks of 1 kHz sine input, push through, capture output.
    let total = block * 4;
    let mut input_l = Vec::with_capacity(total);
    let mut input_r = Vec::with_capacity(total);
    for i in 0..total {
        let s = (i as f64 / sample_rate * 1000.0 * std::f64::consts::TAU).sin() as f32 * 0.5;
        input_l.push(s);
        input_r.push(s);
    }
    let mut out_l = vec![0.0f32; total];
    let mut out_r = vec![0.0f32; total];
    for chunk in 0..4 {
        let start = chunk * block;
        let end = start + block;
        plugin
            .process_block(
                &input_l[start..end],
                &input_r[start..end],
                &mut out_l[start..end],
                &mut out_r[start..end],
                &daw_standalone::plugin::PluginEvents::EMPTY,
            )
            .expect("process_block should succeed");
    }
    plugin.deactivate();
    assert!(!plugin.is_prepared());

    for (i, s) in out_l.iter().chain(out_r.iter()).enumerate() {
        assert!(s.is_finite(), "non-finite sample at {i}: {s}");
    }
}

/// End-to-end render-pipeline test: add a real CLAP plugin to a
/// track's FX chain via the Effects service, render a block via
/// ProjectRenderer, assert the plugin saw audio (output is finite
/// and the chain's `plugin_instances` map carries the plugin).
#[cfg(feature = "decode")]
#[test]
fn clap_plugin_processes_in_render_pipeline() {
    let Some(path) = std::env::var_os("DAW_TEST_CLAP_BUNDLE") else {
        eprintln!("(skip) set DAW_TEST_CLAP_BUNDLE to a .clap bundle path to exercise this test");
        return;
    };
    use daw_proto::fx::{Effects, FxChainContext};
    use daw_proto::midi::Midi;
    use daw_proto::project::ProjectContext;
    use daw_proto::{ItemRef, ProjectInfo, TrackRef, Tracks};
    use daw_standalone::audio_engine::DecodedAudio;
    use daw_standalone::audio_engine::materialize::attach_audio_source;
    use daw_standalone::audio_engine::render::ProjectRenderer;
    use daw_standalone::sync::Standalone;

    let path_str = path.to_string_lossy().into_owned();
    let daw = Standalone::new();
    daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Project("p".into());
    let track_guid = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();

    // Add the CLAP plugin to this track's FX chain. The Effects::add
    // path sniffs the format from the path string (ends in `.clap`).
    let fx_guid = Effects::add(
        &daw,
        ctx.clone(),
        FxChainContext::Track(track_guid.clone()),
        &path_str,
    )
    .expect("FX add should succeed");

    assert!(
        daw.has_plugin_instance(&fx_guid),
        "plugin instance should be stored under fx_guid"
    );

    // Mint a 1s mono const audio item on the track so there's signal
    // flowing into the FX chain.
    let loc =
        Midi::create_midi_item(&daw, ctx.clone(), TrackRef::Guid(track_guid), 0.0, 1.0).unwrap();
    let item_guid = match loc.item {
        ItemRef::Guid(g) => g,
        _ => panic!(),
    };
    let active =
        daw_proto::Takes::get_active_take(&daw, ctx.clone(), ItemRef::Guid(item_guid)).unwrap();
    daw.write_project("p", |p| {
        for tl in p.takes.values_mut() {
            for t in tl.takes.iter_mut() {
                if t.guid == active.guid {
                    t.is_midi = false;
                    t.source_type = daw_proto::item::SourceType::Audio;
                }
            }
        }
    });
    let sample_rate = 48_000;
    let frames = (sample_rate as usize) / 10;
    attach_audio_source(
        &daw,
        "p",
        &active.guid,
        DecodedAudio::new(vec![0.3f32; sample_rate as usize], 1, sample_rate),
    );

    let block = ProjectRenderer::new(&daw, "p", sample_rate).render_block(0, frames);
    for (i, s) in block.samples.iter().enumerate() {
        assert!(s.is_finite(), "non-finite sample at {i}: {s}");
    }
    assert!(
        daw.plugin_is_prepared(&fx_guid),
        "plugin should be prepared after first render block"
    );
}

/// Verify the new audio_ports / note_ports / GUI introspection
/// surface. The plugin doesn't need to implement these extensions
/// for the test to pass — we just check the host returns sensible
/// defaults rather than panicking.
#[test]
fn introspects_ports_and_gui_without_panic() {
    let Some(path) = std::env::var_os("DAW_TEST_CLAP_BUNDLE") else {
        eprintln!("(skip) set DAW_TEST_CLAP_BUNDLE to a .clap bundle path to exercise this test");
        return;
    };
    let host = ClapHost::default();
    let mut plugin = host
        .load(&PathBuf::from(path), 0)
        .expect("plugin should instantiate");
    let (a_in, a_out) = plugin.audio_port_count();
    let (n_in, n_out) = plugin.note_port_count();
    let has_gui = plugin.has_gui();
    eprintln!(
        "audio ports: in={a_in} out={a_out}; note ports: in={n_in} out={n_out}; has_gui={has_gui}"
    );
    // Both inputs/outputs should report non-negative counts (impl
    // returns u32 so this is trivially true — the assertion guards
    // against a future signed type slip).
    let _ = (a_in, a_out, n_in, n_out);
    // For plugins that have ≥1 audio input, the first port's info
    // should be retrievable.
    if a_in > 0 {
        let info = plugin.audio_port_info(0, true);
        eprintln!("first audio input port: {info:?}");
    }
}

/// Real-plugin smoke test: load + list params + report latency.
/// Skipped without `DAW_TEST_CLAP_BUNDLE`.
#[test]
fn loads_real_plugin_and_inspects_params() {
    let Some(path) = std::env::var_os("DAW_TEST_CLAP_BUNDLE") else {
        eprintln!("(skip) set DAW_TEST_CLAP_BUNDLE to a .clap bundle path to exercise this test");
        return;
    };
    let host = ClapHost::default();
    let mut plugin = host
        .load(&PathBuf::from(path), 0)
        .expect("plugin should instantiate");
    let descriptor = plugin.descriptor().clone();
    let params = plugin.params();
    let latency = plugin.latency();
    eprintln!(
        "plugin '{}' v{} by '{}' — {} param(s), reported latency = {} samples",
        descriptor.name,
        descriptor.version,
        descriptor.vendor,
        params.len(),
        latency,
    );
    for p in params.iter().take(5) {
        eprintln!(
            "  param #{} '{}' min={} max={} default={}",
            p.id, p.name, p.min, p.max, p.default
        );
    }
}

/// A plugin that declares a side chain must be given a buffer for it.
///
/// This is the regression test for the fault that made FabFilter's Pro-C
/// unusable through this host. `process_block` built a single-port input list
/// while Pro-C declares two input ports, so the plugin indexed past the end of
/// the array it was handed and dereferenced whatever followed — the observed
/// crash address decoded to the ASCII of its own `"Side Chain"` port name.
///
/// It is undefined behaviour, so the symptom was platform-dependent and
/// intermittent: a `SIGSEGV` roughly two runs in three on native macOS, and
/// silence, reliably, through yabridge on Linux. That is why the test renders
/// repeatedly rather than once — a single clean block proved nothing, and this
/// failed about as often as it passed before the fix.
///
/// Set `DAW_TEST_CLAP_MULTIPORT_BUNDLE` to a plugin declaring more than one
/// input port (any Pro-C, Pro-MB, Pro-DS, or any compressor with an external
/// side chain). Skipped otherwise, since it needs a plugin CI does not have.
#[test]
fn a_plugin_declaring_a_side_chain_renders_without_reading_past_its_ports() {
    let Some(path) = std::env::var_os("DAW_TEST_CLAP_MULTIPORT_BUNDLE") else {
        eprintln!(
            "(skip) set DAW_TEST_CLAP_MULTIPORT_BUNDLE to a .clap bundle \
             declaring >1 input port to exercise this test"
        );
        return;
    };
    let host = ClapHost::default();
    let mut plugin = host
        .load(&PathBuf::from(path), 0)
        .expect("plugin should load");

    let (inputs, outputs) = plugin.audio_port_count();
    assert!(
        inputs > 1,
        "this test needs a plugin with more than one input port; got {inputs} in / {outputs} out"
    );

    const BLOCK: usize = 512;
    plugin.prepare(48_000.0, BLOCK as u32).expect("prepare");

    // The invariant, asserted directly. A render-and-check test is not enough
    // on its own: the old single-port list produced *silence* on Linux rather
    // than a fault, so 64 clean blocks prove nothing there. These two do,
    // on every platform.
    assert_eq!(
        plugin.prepared_port_counts(),
        Some((inputs as usize, outputs as usize)),
        "the host must prepare a buffer list matching what the plugin declares"
    );
    assert_eq!(
        plugin.aux_buffer_counts(),
        Some(((inputs as usize - 1) * 2, (outputs as usize - 1) * 2)),
        "two channels must be held for every port beyond the main bus"
    );

    // A quarter-scale tone, loud enough to drive a compressor's detector so
    // the side-chain path is actually walked rather than skipped on silence.
    let in_l: Vec<f32> = (0..BLOCK).map(|i| 0.25 * (i as f32 * 0.05).sin()).collect();
    let in_r = in_l.clone();
    let mut out_l = vec![0.0f32; BLOCK];
    let mut out_r = vec![0.0f32; BLOCK];

    let events = daw_standalone::plugin::PluginEvents {
        params: &[],
        midi: &[],
        note_expressions: &[],
    };

    for block in 0..64 {
        plugin
            .process_block(&in_l, &in_r, &mut out_l, &mut out_r, &events)
            .unwrap_or_else(|e| panic!("block {block} failed: {e:?}"));
        assert!(
            out_l.iter().chain(out_r.iter()).all(|v| v.is_finite()),
            "block {block} produced a non-finite sample"
        );
    }
    plugin.deactivate();
    eprintln!("rendered 64 blocks through a plugin with {inputs} input / {outputs} output ports");
}
