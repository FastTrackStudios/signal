//! FX zones against a REAL CLAP plugin: surface gestures focus an FX,
//! turn its parameters, and the value lands in the engine + echoes on
//! the event bus with the plugin's real parameter names.
//!
//! Needs a CLAP bundle. Probes `DAW_TEST_CLAP_BUNDLE`, then the
//! local FTS plugins, then LSP's system locations; skips when none
//! exist (CI without plugins).

use daw_csi::driver::{DriverState, Intent, execute_intent, refresh_fx_caches};
use daw_csi::mcu;
use daw_proto::ProjectInfo;
use daw_proto::event_bus::{BusFilter, DawEvent};
use daw_proto::fx::FxEvent;
use daw_standalone::bootstrap::build_in_process_daw;
use daw_standalone::sync::Standalone;

/// Candidate CLAP bundles, most-preferred first. Entries may be
/// stale symlinks — the test tries each until one actually loads.
fn clap_candidates() -> Vec<String> {
    let mut v = Vec::new();
    if let Some(p) = std::env::var_os("DAW_TEST_CLAP_BUNDLE") {
        v.push(p.to_string_lossy().into_owned());
    }
    let home = std::env::var("HOME").unwrap_or_default();
    for candidate in [
        format!("{home}/.clap/delay-plugin.clap"),
        format!("{home}/.clap/chorus-plugin.clap"),
        format!("{home}/.clap/gate-plugin.clap"),
        format!("{home}/.clap/eq-plugin.clap"),
        "/usr/lib/clap/lsp-plugins-clap.clap".to_string(),
        "/usr/local/lib/clap/lsp-plugins-clap.clap".to_string(),
    ] {
        if std::path::Path::new(&candidate).exists() {
            v.push(candidate);
        }
    }
    v
}

fn seeded() -> Standalone {
    let s = Standalone::new();
    s.seed_project(ProjectInfo {
        guid: "fx-test".into(),
        name: "fx".into(),
        path: String::new(),
    });
    s
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn surface_drives_real_clap_params() -> eyre::Result<()> {
    let standalone = seeded();
    let daw = build_in_process_daw(standalone).await?;
    let project = daw.daw.current_project().await?;
    let transport = project.transport();

    // One selected track; try CLAP candidates until one really
    // loads (real plugin = real parameter names, not "Param N").
    let track = project.add_track("Synth", None).await?;
    track.select_exclusive().await?;
    let mut loaded = None;
    for bundle in clap_candidates() {
        let fx = track.fx_chain().add(&bundle).await?;
        let params = fx.parameters().await?;
        let is_real = params
            .first()
            .is_some_and(|p| !p.name.starts_with("Param "));
        if is_real {
            loaded = Some((bundle, fx, params));
            break;
        }
        fx.remove().await?;
    }
    let Some((bundle, fx, params)) = loaded else {
        eprintln!("(skip) no loadable CLAP bundle found — set DAW_TEST_CLAP_BUNDLE");
        return Ok(());
    };
    eprintln!("testing against {bundle} ({} params)", params.len());

    // Bus subscription with FX events enabled.
    let mut bus = daw
        .daw
        .events()
        .subscribe(BusFilter {
            fx: true,
            ..Default::default()
        })
        .await?;

    // Drive the surface: enter the FX menu, focus FX 0, turn param 0.
    let tracks = project.tracks().all().await?;
    let mut state = DriverState::with_builtin_zones(tracks, "master".into(), 1.0);
    assert!(!state.handle_midi(&[0x90, 0x2B, 0x7F], 0).is_empty()); // assign_plugin
    state.handle_midi(&[0x90, 0x2B, 0x00], 50);
    refresh_fx_caches(&project, &mut state).await;
    assert_eq!(state.fx_list.len(), 1, "FX chain should show the plugin");

    // Focus the FX (select strip 0) → param zone.
    state.handle_midi(&[0x90, 0x18, 0x7F], 100);
    state.handle_midi(&[0x90, 0x18, 0x00], 150);
    assert_eq!(state.active_zone, "fx");
    refresh_fx_caches(&project, &mut state).await;
    assert!(
        !state.params.is_empty(),
        "param cache should hold the plugin's parameters"
    );
    assert_eq!(state.params[0].name, params[0].name);

    // Fader strip 0 → param 0 to 75%.
    let intents = state.handle_midi(&mcu::encode_fader(0, 12287), 200);
    assert_eq!(intents.len(), 1);
    let expected = match &intents[0] {
        Intent::SetFxParam {
            param_idx, value, ..
        } => {
            assert_eq!(*param_idx, 0);
            *value
        }
        other => panic!("expected SetFxParam, got {other:?}"),
    };
    for intent in intents {
        execute_intent(&project, &transport, intent).await?;
    }

    // The engine's normalized view reflects the gesture…
    let now = fx.param(0).get().await?;
    assert!(
        (now - expected).abs() < 1e-6,
        "param should be {expected}, engine says {now}"
    );

    // …and the echo arrives on the bus as a ParameterChanged.
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), bus.recv())
        .await
        .expect("fx event timed out")?
        .expect("bus closed");
    let mut got = None;
    let _ = event.map(|e| got = Some(e));
    let Some(DawEvent::Fx(fe)) = got else {
        panic!("expected an FX event");
    };
    assert!(matches!(
        fe.event,
        FxEvent::ParameterChanged { param_index: 0, .. }
    ));
    Ok(())
}
