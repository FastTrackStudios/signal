//! Macro arm/learn action handlers.
//!
//! Manages the arm/learn workflow in the signal extension process:
//! - Maintains `LearnState` for the active learn session
//! - Polls `GetLastTouchedFX` to track which parameter the user is adjusting
//! - Handles arm, disarm, set-point, remove-last-point, and clear actions

use daw::Daw;
use eyre::{Result, WrapErr};
use macromod::learn::LearnState;
use macromod::DawParamTarget;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use tracing::{info, warn};

/// Global learn state — shared between action handlers.
///
/// Using std::Mutex since we never hold it across await points.
static LEARN_STATE: Mutex<LearnState> = Mutex::new(LearnState {
    armed_knob_id: None,
    pending_bindings: Vec::new(),
    last_touched: None,
});

/// Arm a macro for learning.
///
/// For now, auto-generates a knob ID. In the future this will be driven
/// by the UI (selecting which knob to arm).
pub async fn handle_macro_arm(_daw: &Daw) -> Result<()> {
    let mut state = LEARN_STATE.lock().unwrap();

    if state.is_armed() {
        info!(
            "[macro-learn] Already armed: {:?} — disarming first",
            state.armed_knob_id
        );
        let bindings = state.disarm();
        info!(
            "[macro-learn] Auto-disarmed with {} bindings (discarded)",
            bindings.len()
        );
    }

    // Generate a unique knob ID
    static COUNTER: AtomicU32 = AtomicU32::new(1);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let knob_id = format!("macro-{n}");
    state.arm(&knob_id);
    info!("[macro-learn] Armed macro '{knob_id}' — touch FX parameters to bind them");
    Ok(())
}

/// Disarm the current macro and finalize bindings.
pub async fn handle_macro_disarm(_daw: &Daw) -> Result<()> {
    let mut state = LEARN_STATE.lock().unwrap();

    if !state.is_armed() {
        warn!("[macro-learn] No macro is armed");
        return Ok(());
    }

    let knob_id = state.armed_knob_id.clone().unwrap();
    let bindings = state.disarm();

    info!(
        "[macro-learn] Disarmed macro '{knob_id}' with {} binding(s):",
        bindings.len()
    );
    for b in &bindings {
        info!(
            "  - {} / {} → {} point(s)",
            b.fx_name,
            b.param_name,
            b.curve.len()
        );
    }

    // TODO: Persist bindings to the macro knob in the signal model.
    // For now we just log them — the next step is wiring this into
    // the SignalController to actually apply the curves during playback.

    Ok(())
}

/// Set a curve point for the last touched parameter.
///
/// Reads the last touched FX parameter from the DAW, then captures
/// the current macro knob position and parameter value as a curve point.
pub async fn handle_macro_set_point(daw: &Daw) -> Result<()> {
    // First, poll the last touched FX from the DAW
    let last_touched = daw
        .last_touched_fx()
        .await?
        .ok_or_else(|| eyre::eyre!("No FX parameter has been touched"))?;

    let target = DawParamTarget {
        track_guid: last_touched.track_guid.clone(),
        fx_index: last_touched.fx_index,
        param_index: last_touched.param_index,
        is_input_fx: last_touched.is_input_fx,
    };

    // Get the current parameter value and name from the DAW
    let project = daw.current_project().await.wrap_err("no current project")?;
    let tracks = project.tracks();
    let track = tracks
        .by_guid(&last_touched.track_guid)
        .await?
        .ok_or_else(|| eyre::eyre!("Track not found: {}", last_touched.track_guid))?;

    let chain = if last_touched.is_input_fx {
        track.input_fx_chain()
    } else {
        track.fx_chain()
    };

    let fx = chain
        .by_index(last_touched.fx_index)
        .await?
        .ok_or_else(|| eyre::eyre!("FX not found at index {}", last_touched.fx_index))?;

    let fx_info = fx.info().await?;
    let param_info = fx.param(last_touched.param_index).info().await?;

    // Update the learn state
    let mut state = LEARN_STATE.lock().unwrap();
    if !state.is_armed() {
        return Err(eyre::eyre!("No macro is armed — arm a macro first"));
    }

    state.set_last_touched(target);

    // TODO: Read the actual macro knob position from the Signal Controller plugin.
    // For now, use the parameter value as a placeholder for the macro position.
    // The real implementation will read the knob's current value from the plugin UI.
    let macro_value = param_info.value;
    let param_value = param_info.value;

    state
        .set_point(macro_value, param_value, &param_info.name, &fx_info.name)
        .map_err(|e| eyre::eyre!(e))?;

    let binding_count = state.pending_bindings.len();
    let point_count = state
        .pending_bindings
        .last()
        .map(|b| b.curve.len())
        .unwrap_or(0);

    info!(
        "[macro-learn] Set point: {} / {} = {:.3} at macro={:.3} ({} binding(s), {} point(s) for this param)",
        fx_info.name,
        param_info.name,
        param_value,
        macro_value,
        binding_count,
        point_count,
    );

    Ok(())
}

/// Remove the last curve point for the last touched parameter.
pub async fn handle_macro_remove_last_point(daw: &Daw) -> Result<()> {
    // Poll last touched to update state
    if let Ok(Some(last_touched)) = daw.last_touched_fx().await {
        let mut state = LEARN_STATE.lock().unwrap();
        state.set_last_touched(DawParamTarget {
            track_guid: last_touched.track_guid,
            fx_index: last_touched.fx_index,
            param_index: last_touched.param_index,
            is_input_fx: last_touched.is_input_fx,
        });
    }

    let mut state = LEARN_STATE.lock().unwrap();
    state
        .remove_last_point()
        .map_err(|e| eyre::eyre!(e))?;

    info!("[macro-learn] Removed last point");
    Ok(())
}

/// Clear all pending bindings for the armed macro.
pub async fn handle_macro_clear(_daw: &Daw) -> Result<()> {
    let mut state = LEARN_STATE.lock().unwrap();
    if !state.is_armed() {
        warn!("[macro-learn] No macro is armed");
        return Ok(());
    }
    state.clear();
    info!("[macro-learn] Cleared all pending bindings");
    Ok(())
}
