//! Signal domain in-process REAPER extension.
//!
//! Loaded directly by REAPER from `UserPlugins/`. Registers signal-domain
//! actions (action defs come from `signal::actions::signal_actions`) and
//! routes their triggers to the matching handler module in this crate.
//!
//! Migrated off the old SHM-guest model (commit 7a6e470 in daw repo). The
//! `daw_extension_runtime::connect()` / `GuestOptions` API no longer
//! exists; we now use the in-process `ExtensionRuntime::new(context)`
//! pattern shared with session-extension and sync-extension.

pub mod daw_compat;
pub mod demo_profile;
pub mod demo_rig;
pub mod demo_setlist;
pub mod macro_learn;
pub mod place_switch;
pub mod scene_midi;

use std::cell::OnceCell;
use std::error::Error;

use daw::rpc::Daw;
use daw_extension_runtime::ExtensionRuntime;
use fragile::Fragile;
use reaper_low::PluginContext;
use reaper_macros::reaper_extension_plugin;
use signal::actions::signal_actions;
use tracing::{debug, error, info};

thread_local! {
    static APP: OnceCell<Fragile<SignalExtension>> = const { OnceCell::new() };
}

struct SignalExtension {
    runtime: ExtensionRuntime,
}

impl SignalExtension {
    fn new(context: PluginContext) -> eyre::Result<Self> {
        let runtime = ExtensionRuntime::new(context)?;
        let daw = runtime.build_daw()?;

        runtime.spawn(async move {
            // Health beacons for tests.
            let pid = std::process::id();
            let _ = daw
                .ext_state()
                .set("FTS_SIGNAL_EXT", "status", "ready", false)
                .await;
            let _ = daw
                .ext_state()
                .set("FTS_SIGNAL_EXT", "pid", &pid.to_string(), false)
                .await;

            // Register signal-domain actions.
            let registry = daw.action_registry();
            let defs = signal_actions::definitions();
            let total = defs.len();
            let mut registered = 0usize;
            for def in &defs {
                let cmd_name = def.id.to_command_id();
                match registry.register(&cmd_name, &def.display_name()).await {
                    Ok(cmd_id) if cmd_id > 0 => {
                        registered += 1;
                    }
                    Ok(_) => {
                        tracing::warn!("signal action returned cmd_id=0: {cmd_name}");
                    }
                    Err(e) => {
                        error!("signal action register failed {cmd_name}: {e:#}");
                    }
                }
            }
            info!("signal-extension registered {registered}/{total} actions");

            // TODO(daw-track-api): `ActionRegistry::subscribe_actions` was removed
            // from daw main; there is no action-event stream to drive the trigger
            // loop. Actions are still registered above, but their triggers are not
            // handled until daw restores a subscription API. `handle_action` and
            // the `ActionEvent` import are retained for when it returns.
            let _ = &daw;
            error!(
                "signal-extension: action event subscription unavailable on daw main \
                 (subscribe_actions removed) — {registered}/{total} actions registered, \
                 but trigger handling is DISABLED (TODO daw-track-api)"
            );
        });

        Ok(Self { runtime })
    }

    fn timer(&self) {
        self.runtime.process_tasks();
    }
}

extern "C" fn timer_callback() {
    APP.with(|cell| {
        if let Some(app) = cell.get() {
            app.get().timer();
        }
    });
}

#[reaper_extension_plugin]
fn plugin_main(context: PluginContext) -> Result<(), Box<dyn Error>> {
    init_tracing();
    info!("signal-extension starting");

    let app = SignalExtension::new(context)?;
    app.runtime.add_timer(timer_callback)?;

    let stored = APP.with(|cell| cell.set(Fragile::new(app)).is_ok());
    if !stored {
        return Err("signal-extension already initialized".into());
    }

    info!("signal-extension loaded");
    Ok(())
}

fn init_tracing() {
    let Ok(log_file) = std::fs::File::create("/tmp/signal-extension.log") else {
        return;
    };
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(log_file))
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
}

#[allow(dead_code)]
async fn handle_action(daw: &Daw, command_name: &str) {
    info!("signal action triggered: {command_name}");

    match command_name {
        cmd if cmd.ends_with("DEV_LOAD_DEMO_GUITAR_RIG") => {
            if let Err(e) = demo_rig::load_demo_guitar_rig(daw).await {
                error!("load demo guitar rig: {e:#}");
            }
        }
        cmd if cmd.ends_with("DEV_LOAD_DEMO_GUITAR_PROFILE") => {
            if let Err(e) = demo_profile::load_demo_profile(daw).await {
                error!("load demo guitar profile: {e:#}");
            }
        }
        cmd if cmd.ends_with("DEV_GENERATE_SCENE_MIDI_ITEMS") => {
            if let Err(e) = scene_midi::generate_scene_midi_items(daw).await {
                error!("generate scene midi items: {e:#}");
            }
        }
        cmd if cmd.ends_with("DEV_LOAD_DEMO_SETLIST") => {
            if let Err(e) = demo_setlist::load_demo_setlist(daw).await {
                error!("load demo setlist: {e:#}");
            }
        }
        cmd if cmd.ends_with("PLACE_SECTION_SWITCH") => {
            if let Err(e) = place_switch::place_section_switch(daw).await {
                error!("place section switch: {e:#}");
            }
        }
        cmd if cmd.ends_with("PLACE_SONG_SWITCH") => {
            if let Err(e) = place_switch::place_song_switch(daw).await {
                error!("place song switch: {e:#}");
            }
        }
        cmd if cmd.ends_with("PLACE_SCENE_SWITCH") => {
            if let Err(e) = place_switch::place_scene_switch(daw).await {
                error!("place scene switch: {e:#}");
            }
        }
        cmd if cmd.ends_with("MACRO_ARM") => {
            if let Err(e) = macro_learn::handle_macro_arm(daw).await {
                error!("macro arm: {e:#}");
            }
        }
        cmd if cmd.ends_with("MACRO_DISARM") => {
            if let Err(e) = macro_learn::handle_macro_disarm(daw).await {
                error!("macro disarm: {e:#}");
            }
        }
        cmd if cmd.ends_with("MACRO_SET_MIN") => {
            if let Err(e) = macro_learn::handle_macro_set_min(daw).await {
                error!("macro set min: {e:#}");
            }
        }
        cmd if cmd.ends_with("MACRO_SET_MAX") => {
            if let Err(e) = macro_learn::handle_macro_set_max(daw).await {
                error!("macro set max: {e:#}");
            }
        }
        cmd if cmd.ends_with("MACRO_SET_POINT") => {
            if let Err(e) = macro_learn::handle_macro_set_point(daw).await {
                error!("macro set point: {e:#}");
            }
        }
        cmd if cmd.ends_with("MACRO_REMOVE_LAST_POINT") => {
            if let Err(e) = macro_learn::handle_macro_remove_last_point(daw).await {
                error!("macro remove last point: {e:#}");
            }
        }
        cmd if cmd.ends_with("MACRO_CLEAR") => {
            if let Err(e) = macro_learn::handle_macro_clear(daw).await {
                error!("macro clear: {e:#}");
            }
        }
        cmd if cmd.ends_with("MACRO_ADD") => {
            // TODO: Add a new macro knob to the active bank
            debug!("MACRO_ADD not yet implemented");
        }
        _ => {
            // TODO: Dispatch to SignalController (rig/profile/preset operations)
            debug!("unhandled signal action: {command_name}");
        }
    }
}
