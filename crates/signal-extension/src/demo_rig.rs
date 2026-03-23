//! Demo guitar rig builder — creates a single-scene rig in REAPER.
//!
//! Follows the same folder-based layout that `ReaperPatchApplier` uses:
//!
//! ```text
//! Guitar Rig/                  (folder — FTS Signal Controller w/ rig macros)
//!   Guitar Input               (input track — parent send disabled, sends to layer)
//!   [L] Clean                  (layer track — full FX chain for the "Clean" variation)
//! ```
//!
//! The layer track holds the entire signal chain as a flat FX list:
//!   ReaGate → ReaComp → ReaEQ(drive) → ReaEQ(amp) → ReaDelay(mod)
//!   → ReaDelay(delay) → ReaDelay(reverb) → ReaComp(output) → ReaEQ(output)

use daw::Daw;
use eyre::{Result, WrapErr};
use tracing::info;

/// Create a demo guitar rig in the current REAPER project.
///
/// Macros (on the rig folder's FTS Signal Controller):
///   0: Drive    (drive EQ gain)
///   1: Tone     (amp EQ high-mid)
///   2: Bass     (amp EQ low shelf)
///   3: Presence (amp EQ high shelf)
///   4: Delay    (delay wet/dry)
///   5: Reverb   (reverb wet/dry)
///   6: Gate     (input gate threshold)
///   7: Volume   (output comp makeup / volume)
pub async fn load_demo_guitar_rig(daw: &Daw) -> Result<()> {
    let project = daw.current_project().await.wrap_err("no current project")?;
    let tracks = project.tracks();

    info!("[demo-rig] Creating guitar rig folder structure");

    // ── Rig folder ───────────────────────────────────────────────────
    let rig = tracks.add("Guitar Rig", None).await?;
    rig.set_folder_depth(1).await?;
    rig.set_color(0xF97316).await?; // orange

    // FTS Signal Controller on rig folder (macro host)
    rig.fx_chain()
        .add("CLAP: FTS Signal Controller (FastTrack Studio)")
        .await
        .wrap_err("failed to add FTS Signal Controller to rig folder")?;

    // ── Input track ──────────────────────────────────────────────────
    let input = tracks.add("Guitar Input", None).await?;
    input.set_color(0x6B7280).await?; // gray
    input.set_parent_send(false).await?;

    // ── Layer track: [L] Clean ───────────────────────────────────────
    let layer = tracks.add("[L] Clean", None).await?;
    layer.set_color(0x22C55E).await?; // green
    layer.set_folder_depth(-1).await?; // close rig folder

    // Send: input → layer
    input.sends().add_to(layer.guid()).await?;

    info!("[demo-rig] Tracks created, adding FX to layer");

    // ── Layer FX chain ───────────────────────────────────────────────
    let fx = layer.fx_chain();

    // [input] Gate
    let gate = fx.add("ReaGate").await?;
    gate.param_by_name("Threshold").set(0.35).await?;

    // [input] Compressor
    let comp = fx.add("ReaComp").await?;
    comp.param_by_name("Thresh").set(0.55).await?;
    comp.param_by_name("Ratio").set(0.3).await?;
    comp.param_by_name("Attack").set(0.15).await?;
    comp.param_by_name("Release").set(0.3).await?;

    // [drive] ReaEQ as mid-boost tone shaper
    let drive = fx.add("ReaEQ").await?;
    drive.param(0).set(1.0).await?; // band 0 enabled
    drive.param(2).set(0.3).await?; // frequency
    drive.param(3).set(0.4).await?; // gain (moderate)
    drive.param(4).set(0.5).await?; // bandwidth

    // [amp] 4-band tone stack
    let amp_eq = fx.add("ReaEQ").await?;
    // Bass shelf
    amp_eq.param(0).set(1.0).await?;
    amp_eq.param(2).set(0.15).await?;
    amp_eq.param(3).set(0.55).await?;
    amp_eq.param(4).set(0.3).await?;
    // Mid bell
    amp_eq.param(5).set(1.0).await?;
    amp_eq.param(7).set(0.35).await?;
    amp_eq.param(8).set(0.5).await?;
    amp_eq.param(9).set(0.4).await?;
    // Treble
    amp_eq.param(10).set(1.0).await?;
    amp_eq.param(12).set(0.6).await?;
    amp_eq.param(13).set(0.55).await?;
    amp_eq.param(14).set(0.3).await?;
    // Presence shelf
    amp_eq.param(15).set(1.0).await?;
    amp_eq.param(17).set(0.75).await?;
    amp_eq.param(18).set(0.52).await?;
    amp_eq.param(19).set(0.5).await?;

    // [modulation] Short ReaDelay for chorus-like effect
    let mod_dly = fx.add("ReaDelay").await?;
    mod_dly.param_by_name("Length").set(0.02).await?;
    mod_dly.param_by_name("Feedback").set(0.0).await?;
    mod_dly.param_by_name("Wet").set(0.3).await?;

    // [time] Delay
    let dly = fx.add("ReaDelay").await?;
    dly.param_by_name("Length").set(0.35).await?;
    dly.param_by_name("Feedback").set(0.35).await?;
    dly.param_by_name("Wet").set(0.25).await?;

    // [time] Reverb
    let rev = fx.add("ReaDelay").await?;
    rev.param_by_name("Length").set(0.08).await?;
    rev.param_by_name("Feedback").set(0.6).await?;
    rev.param_by_name("Wet").set(0.2).await?;

    // [master] Output compressor (limiter)
    let out_comp = fx.add("ReaComp").await?;
    out_comp.param_by_name("Thresh").set(0.7).await?;
    out_comp.param_by_name("Ratio").set(0.8).await?;
    out_comp.param_by_name("Attack").set(0.05).await?;
    out_comp.param_by_name("Release").set(0.2).await?;

    // [master] Output EQ (final shape)
    let _out_eq = fx.add("ReaEQ").await?;

    info!("[demo-rig] FX chain added, storing macro bindings");

    // ── Macro binding config ─────────────────────────────────────────
    // FX order on layer: 0=Gate, 1=Comp, 2=Drive EQ, 3=Amp EQ,
    //   4=Mod, 5=Delay, 6=Reverb, 7=OutComp, 8=OutEQ
    let macro_config = serde_json::json!({
        "macros": [
            {
                "id": "drive",
                "label": "Drive",
                "color": "#EF4444",
                "value": 0.4,
                "bindings": [{ "layer": "[L] Clean", "fx_index": 2, "param_index": 3, "min": 0.0, "max": 1.0 }]
            },
            {
                "id": "tone",
                "label": "Tone",
                "color": "#EAB308",
                "value": 0.5,
                "bindings": [{ "layer": "[L] Clean", "fx_index": 3, "param_index": 8, "min": 0.3, "max": 0.7 }]
            },
            {
                "id": "bass",
                "label": "Bass",
                "color": "#F97316",
                "value": 0.55,
                "bindings": [{ "layer": "[L] Clean", "fx_index": 3, "param_index": 3, "min": 0.35, "max": 0.7 }]
            },
            {
                "id": "presence",
                "label": "Presence",
                "color": "#EC4899",
                "value": 0.5,
                "bindings": [{ "layer": "[L] Clean", "fx_index": 3, "param_index": 18, "min": 0.4, "max": 0.65 }]
            },
            {
                "id": "delay",
                "label": "Delay",
                "color": "#3B82F6",
                "value": 0.25,
                "bindings": [{ "layer": "[L] Clean", "fx_index": 5, "param": "Wet", "min": 0.0, "max": 0.6 }]
            },
            {
                "id": "reverb",
                "label": "Reverb",
                "color": "#06B6D4",
                "value": 0.2,
                "bindings": [{ "layer": "[L] Clean", "fx_index": 6, "param": "Wet", "min": 0.0, "max": 0.5 }]
            },
            {
                "id": "gate",
                "label": "Gate",
                "color": "#6B7280",
                "value": 0.35,
                "bindings": [{ "layer": "[L] Clean", "fx_index": 0, "param": "Threshold", "min": 0.1, "max": 0.6 }]
            },
            {
                "id": "volume",
                "label": "Volume",
                "color": "#22C55E",
                "value": 0.7,
                "bindings": [{ "layer": "[L] Clean", "fx_index": 7, "param": "Thresh", "min": 0.5, "max": 1.0 }]
            }
        ]
    });

    rig.set_ext_state("fts_signal", "macro_config", &macro_config.to_string())
        .await?;
    rig.set_ext_state("fts_signal", "rig_type", "guitar")
        .await?;
    rig.set_ext_state("fts_signal", "rig_name", "Guitar Rig")
        .await?;

    info!("[demo-rig] Demo guitar rig created successfully");
    Ok(())
}
