//! Demo "All-Around" guitar profile builder.
//!
//! Creates the full profile structure in REAPER using stock plugins:
//!
//! ```text
//! Guitar Profile: All-Around/          (profile folder)
//!   Guitar Input                       (profile input — sends to each scene's input)
//!   Scene 1: Clean/                    (scene folder)
//!     Guitar Input: Clean              (scene input — sends to layer)
//!     [L] Clean                        (layer track — FX chain)
//!   Scene 2: Crunch/                   (scene folder)
//!     Guitar Input: Crunch
//!     [L] Crunch
//!   ...8 scenes total
//! ```
//!
//! Each scene uses a different subset of stock REAPER FX to simulate
//! the module types: input, drive, amp, modulation, time, dynamics, master.

use daw::{Daw, TrackHandle};
use eyre::{Result, WrapErr};
use tracing::info;

/// Scene definition: name + which module types are active.
const SCENES: &[(&str, &[&str])] = &[
    ("Clean", &["input", "amp", "master"]),
    ("Crunch", &["input", "drive", "amp", "master"]),
    ("Drive", &["input", "drive", "amp", "dynamics", "master"]),
    (
        "Lead",
        &["input", "drive", "amp", "modulation", "time", "master"],
    ),
    (
        "Funk",
        &["input", "amp", "dynamics", "modulation", "master"],
    ),
    ("Ambient", &["input", "amp", "modulation", "time", "master"]),
    (
        "Q-Tron",
        &["input", "drive", "amp", "modulation", "master"],
    ),
    (
        "Solo",
        &["input", "drive", "amp", "time", "dynamics", "master"],
    ),
];

/// Create the All-Around demo profile in the current REAPER project.
pub async fn load_demo_profile(daw: &Daw) -> Result<()> {
    let project = daw.current_project().await.wrap_err("no current project")?;
    let tracks = project.tracks();

    info!("[demo-profile] Creating All-Around guitar profile");

    // ── Profile folder ───────────────────────────────────────────────
    let profile = tracks.add("Guitar Profile: All-Around", None).await?;
    profile.set_folder_depth(1).await?;
    profile.set_color(0xF97316).await?; // orange

    // FTS Signal Controller on the profile folder (macro host)
    profile
        .fx_chain()
        .add("CLAP: FTS Signal Controller (FastTrack Studio)")
        .await
        .wrap_err("failed to add FTS Signal Controller to profile folder")?;

    // ── Profile-level input track ────────────────────────────────────
    // Receives live guitar. Parent send disabled — routes audio via
    // sends to each scene's input track.
    let profile_input = tracks.add("Guitar Input", None).await?;
    profile_input.set_color(0x6B7280).await?;
    profile_input.set_parent_send(false).await?;

    // ── Build each scene ─────────────────────────────────────────────
    let scene_count = SCENES.len();
    for (i, &(scene_name, module_types)) in SCENES.iter().enumerate() {
        info!("[demo-profile] Building scene {}/{scene_count}: {scene_name}", i + 1);

        let is_last_scene = i == scene_count - 1;

        // Scene folder
        let scene_folder = tracks
            .add(&format!("Scene {}: {scene_name}", i + 1), None)
            .await?;
        scene_folder.set_folder_depth(1).await?;
        scene_folder.set_color(scene_color(scene_name)).await?;

        // Scene input track — receives from profile input, sends to layer
        let scene_input = tracks
            .add(&format!("Guitar Input: {scene_name}"), None)
            .await?;
        scene_input.set_color(0x6B7280).await?;
        scene_input.set_parent_send(false).await?;

        // Layer track
        let layer = tracks
            .add(&format!("[L] {scene_name}"), None)
            .await?;
        layer.set_color(scene_color(scene_name)).await?;

        // Close scene folder (and profile folder on last scene)
        if is_last_scene {
            layer.set_folder_depth(-2).await?; // close scene + profile
        } else {
            layer.set_folder_depth(-1).await?; // close scene only
        }

        // Send: profile input → scene input (mute all except first)
        let send = profile_input.sends().add_to(scene_input.guid()).await?;
        if i > 0 {
            send.mute().await?;
        }

        // Send: scene input → layer
        scene_input.sends().add_to(layer.guid()).await?;

        // Add FX chain to layer based on module types
        add_layer_fx(&layer, module_types).await?;
    }

    // Store profile metadata
    profile
        .set_ext_state("fts_signal", "profile_type", "guitar")
        .await?;
    profile
        .set_ext_state("fts_signal", "profile_name", "All-Around")
        .await?;
    profile
        .set_ext_state(
            "fts_signal",
            "scene_count",
            &scene_count.to_string(),
        )
        .await?;

    info!("[demo-profile] All-Around profile created with {scene_count} scenes");
    Ok(())
}

/// Add stock REAPER FX to a layer track based on which module types are active.
///
/// Module types and their stock plugin equivalents:
///   input      → ReaGate + ReaComp (gate + input comp)
///   drive      → ReaEQ (mid-boost as overdrive character)
///   amp        → ReaEQ (4-band tone stack)
///   dynamics   → ReaComp (dynamics processor)
///   modulation → ReaDelay (short delay as chorus)
///   time       → ReaDelay (delay) + ReaDelay (reverb)
///   master     → ReaComp (limiter) + ReaEQ (final shape)
pub async fn add_layer_fx(layer: &TrackHandle, module_types: &[&str]) -> Result<()> {
    let fx = layer.fx_chain();

    for &module_type in module_types {
        match module_type {
            "input" => {
                let gate = fx.add("ReaGate").await?;
                gate.param_by_name("Threshold").set(0.35).await?;

                let comp = fx.add("ReaComp").await?;
                comp.param_by_name("Thresh").set(0.55).await?;
                comp.param_by_name("Ratio").set(0.3).await?;
                comp.param_by_name("Attack").set(0.15).await?;
                comp.param_by_name("Release").set(0.3).await?;
            }
            "drive" => {
                let eq = fx.add("ReaEQ").await?;
                eq.param(0).set(1.0).await?; // band enabled
                eq.param(2).set(0.35).await?; // freq
                eq.param(3).set(0.55).await?; // gain (drive amount)
                eq.param(4).set(0.4).await?; // bandwidth
            }
            "amp" => {
                let eq = fx.add("ReaEQ").await?;
                // Bass
                eq.param(0).set(1.0).await?;
                eq.param(2).set(0.15).await?;
                eq.param(3).set(0.55).await?;
                eq.param(4).set(0.3).await?;
                // Mid
                eq.param(5).set(1.0).await?;
                eq.param(7).set(0.35).await?;
                eq.param(8).set(0.5).await?;
                eq.param(9).set(0.4).await?;
                // Treble
                eq.param(10).set(1.0).await?;
                eq.param(12).set(0.6).await?;
                eq.param(13).set(0.55).await?;
                eq.param(14).set(0.3).await?;
                // Presence
                eq.param(15).set(1.0).await?;
                eq.param(17).set(0.75).await?;
                eq.param(18).set(0.52).await?;
                eq.param(19).set(0.5).await?;
            }
            "dynamics" => {
                let comp = fx.add("ReaComp").await?;
                comp.param_by_name("Thresh").set(0.5).await?;
                comp.param_by_name("Ratio").set(0.4).await?;
                comp.param_by_name("Attack").set(0.2).await?;
                comp.param_by_name("Release").set(0.35).await?;
            }
            "modulation" => {
                let dly = fx.add("ReaDelay").await?;
                dly.param_by_name("Length").set(0.02).await?;
                dly.param_by_name("Feedback").set(0.0).await?;
                dly.param_by_name("Wet").set(0.3).await?;
            }
            "time" => {
                // Delay
                let dly = fx.add("ReaDelay").await?;
                dly.param_by_name("Length").set(0.35).await?;
                dly.param_by_name("Feedback").set(0.35).await?;
                dly.param_by_name("Wet").set(0.25).await?;

                // Reverb
                let rev = fx.add("ReaDelay").await?;
                rev.param_by_name("Length").set(0.08).await?;
                rev.param_by_name("Feedback").set(0.6).await?;
                rev.param_by_name("Wet").set(0.2).await?;
            }
            "master" => {
                let comp = fx.add("ReaComp").await?;
                comp.param_by_name("Thresh").set(0.7).await?;
                comp.param_by_name("Ratio").set(0.8).await?;
                comp.param_by_name("Attack").set(0.05).await?;
                comp.param_by_name("Release").set(0.2).await?;

                let _eq = fx.add("ReaEQ").await?;
            }
            _ => {}
        }
    }

    Ok(())
}

/// Pick a color for each scene/section to visually distinguish them in REAPER.
pub fn scene_color(name: &str) -> u32 {
    match name {
        // Profile scenes
        "Clean" => 0x22C55E,         // green
        "Crunch" => 0xEAB308,        // yellow
        "Drive" => 0xEF4444,         // red
        "Lead" => 0xF97316,          // orange
        "Funk" => 0x8B5CF6,          // violet
        "Ambient" => 0x06B6D4,       // cyan
        "Q-Tron" => 0xEC4899,        // pink
        "Solo" => 0x3B82F6,          // blue
        // Setlist sections
        "Rhythm" => 0x84CC16,        // lime
        "Edge" => 0xF43F5E,          // rose
        "Djent" => 0xDC2626,         // red-dark
        "Harmony Lead" => 0xFBBF24,  // amber
        "Chug" => 0xB45309,          // amber-dark
        "Filtered" => 0xA855F7,      // purple
        "Dry Lead" => 0xFB923C,      // orange-light
        "Dry Drive" => 0xF87171,     // red-light
        "Default" => 0x9CA3AF,       // gray-light
        _ => 0x6B7280,               // gray
    }
}
