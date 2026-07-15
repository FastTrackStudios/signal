//! Built-in FX through the standalone daw's `Effects` service — the
//! session-mixer path: install [`NativeFxFactory`], `Effects::add` a
//! built-in by display name / ident onto a track's FX chain, and
//! verify a LIVE `PluginInstance` backs the entry (real param names,
//! instance present for the renderer) rather than the synthetic
//! 8-param placeholder.

use std::sync::Arc;

use daw::service::fx::{Effects, FxChainContext, FxRef, FxTarget};
use daw::service::track::Tracks;
use daw::service::{ProjectContext, ProjectInfo};
use daw::standalone::sync::Standalone;
use signal_fx::NativeFxFactory;

fn seeded() -> (Standalone, ProjectContext, String) {
    let daw = Standalone::new();
    let guid = daw.seed_project(ProjectInfo {
        guid: "fx-test".into(),
        name: "FX Test".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Project(guid);
    let track_guid = Tracks::add(&daw, ctx.clone(), "Vocals", None).expect("add track");
    (daw, ctx, track_guid)
}

#[test]
fn add_builtin_creates_live_instance_with_real_params() {
    let (daw, ctx, track) = seeded();
    daw.set_fx_factory(Arc::new(NativeFxFactory));

    // Catalog is advertised.
    let installed = Effects::list_installed(&daw);
    assert!(
        installed.iter().any(|f| f.ident == "signal.fx.reverb"),
        "factory catalog reaches list_installed: {installed:?}"
    );

    // Add by display name — the session mixer's "add Reverb" gesture.
    let chain = FxChainContext::Track(track.clone());
    let fx_guid = Effects::add(&daw, ctx.clone(), chain.clone(), "Reverb").expect("add Reverb");

    // A live instance backs the entry (this is what the render loop
    // looks up per block).
    let named = daw
        .with_plugin_instance(&fx_guid, |p| p.descriptor().name)
        .expect("plugin instance stored under the fx guid");
    assert_eq!(named, "Reverb");

    // Parameters resolve against the DSP (real names, not "Param N").
    let target = FxTarget {
        context: chain.clone(),
        fx: FxRef::Guid(fx_guid.clone()),
    };
    let params = Effects::parameters(&daw, ctx.clone(), target.clone());
    assert!(
        params.iter().any(|p| p.name == "mix") && params.iter().any(|p| p.name == "decay"),
        "live reverb params expected, got {:?}",
        params.iter().map(|p| &p.name).collect::<Vec<_>>()
    );

    // Removing the FX also drops the live instance.
    Effects::remove(&daw, ctx.clone(), target).expect("remove");
    assert!(daw.with_plugin_instance(&fx_guid, |_| ()).is_none());
}

#[test]
fn add_by_ident_and_unknown_falls_back_to_synthetic() {
    let (daw, ctx, track) = seeded();
    daw.set_fx_factory(Arc::new(NativeFxFactory));
    let chain = FxChainContext::Track(track);

    let eq = Effects::add(&daw, ctx.clone(), chain.clone(), "signal.fx.eq").expect("add by ident");
    assert!(daw.with_plugin_instance(&eq, |_| ()).is_some());

    // Unknown names still create the synthetic placeholder (no DSP).
    let synth = Effects::add(&daw, ctx.clone(), chain, "TotallyUnknownFx").expect("synthetic add");
    assert!(daw.with_plugin_instance(&synth, |_| ()).is_none());
}

#[test]
fn without_factory_builtins_stay_synthetic() {
    let (daw, ctx, track) = seeded();
    let chain = FxChainContext::Track(track);
    let fx = Effects::add(&daw, ctx, chain, "Reverb").expect("add");
    assert!(
        daw.with_plugin_instance(&fx, |_| ()).is_none(),
        "no factory installed ⇒ no live instance (pre-existing behavior)"
    );
}
