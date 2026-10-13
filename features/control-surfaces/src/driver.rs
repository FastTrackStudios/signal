//! The surface driver: event-bus feedback out, surface gestures in.
//!
//! Unlike CSI (which polls REAPER ~50×/sec because REAPER has no push
//! API), this driver is event-driven: the daw event bus pushes every
//! state change, the driver diffs against the [`Shadow`] and sends
//! only real updates to the surface.
//!
//! Dispatch is **zone-driven** (see [`crate::zones`]): a gesture
//! resolves through the active zone's bindings to an [`Action`],
//! which produces [`Intent`]s (pure, unit-testable) that the async
//! edge executes against the `daw-control` services. Feedback walks
//! the same bindings in reverse — a fader bound to `@TrackVolume`
//! gets volume motor feedback, an LCD row bound to `@TrackName`
//! shows the name.
//!
//! Hold gestures: a press whose zone defines a `hold+` binding is
//! deferred; the release decides tap vs hold by duration. Sends
//! zones map strips onto the SELECTED track's send slots (CSI's
//! `SelectedTrackSend` navigator), cached in [`DriverState::sends`]
//! and refetched by the async edge on selection / zone changes.

use std::collections::HashMap;

use daw_control::Daw;
use daw_proto::event_bus::{BusFilter, DawEvent};
use daw_proto::track::TrackEvent;
use daw_proto::transport::TransportEvent;
use daw_proto::{PlayState, Track};

use crate::action::Action;
use crate::mcu::{self, Button, RingMode, StripColor, SurfaceInput};
use crate::midi::SurfacePort;
use crate::navigator::{NavMode, Navigator};
use crate::shadow::Shadow;
use crate::taper;
use crate::zones::{
    ALT, CONTROL, DOUBLE, FLIP, GlobalWidget, HOLD, Modifiers, OPTION, SHIFT, StripWidget, TOGGLE,
    ZoneSet,
};

/// Press-to-hold threshold. CSI defaults to ~1s; half that feels
/// right on transport-sized buttons.
pub const HOLD_MS: u64 = 500;

/// Two taps within this window resolve a `double+` binding.
pub const DOUBLE_MS: u64 = 350;

/// Surface driver configuration.
#[derive(Debug, Clone)]
pub struct CsiConfig {
    /// Case-insensitive substring matched against MIDI port names.
    pub device_match: String,
}

impl Default for CsiConfig {
    fn default() -> Self {
        Self {
            device_match: "x-touch".into(),
        }
    }
}

/// What a decoded gesture wants done. Pure data so gesture handling
/// is testable without services or hardware.
#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    SetVolume {
        guid: String,
        volume: f64,
    },
    SetMasterVolume {
        volume: f64,
    },
    SetPan {
        guid: String,
        pan: f64,
    },
    SetMuted {
        guid: String,
        muted: bool,
    },
    SetSoloed {
        guid: String,
        soloed: bool,
    },
    SetArmed {
        guid: String,
        armed: bool,
    },
    SelectExclusive {
        guid: String,
    },
    SelectAdditive {
        guid: String,
    },
    ClearAllSolo,
    UnmuteAll,
    SetSendVolume {
        guid: String,
        index: u32,
        volume: f64,
    },
    SetSendPan {
        guid: String,
        index: u32,
        pan: f64,
    },
    SetSendMuted {
        guid: String,
        index: u32,
        muted: bool,
    },
    SetPhaseInverted {
        guid: String,
        inverted: bool,
    },
    /// Additively select every guid (range select).
    SelectMany {
        guids: Vec<String>,
    },
    SetReceiveVolume {
        guid: String,
        index: u32,
        volume: f64,
    },
    SetReceivePan {
        guid: String,
        index: u32,
        pan: f64,
    },
    SetReceiveMuted {
        guid: String,
        index: u32,
        muted: bool,
    },
    FxNextPreset {
        guid: String,
        fx_idx: u32,
    },
    FxPrevPreset {
        guid: String,
        fx_idx: u32,
    },
    SetAutomationMode {
        guid: String,
        mode: daw_proto::AutomationMode,
    },
    SetInputMonitor {
        guid: String,
        monitor: daw_proto::track::InputMonitoringMode,
    },
    ToggleMetronome,
    /// Fader touch / release routed into the automation engine
    /// (Touch/Latch gating for envelope recording).
    AutomationTouch {
        guid: String,
        touched: bool,
    },
    SaveProject,
    Undo,
    Redo,
    RunCommand {
        id: String,
    },
    AddMarker,
    PrevMarker,
    NextMarker,
    /// Re-fetch the selected track's send + receive slots (async edge
    /// handles this and calls [`DriverState::set_sends`] /
    /// [`DriverState::set_receives`]).
    RefreshSends,
    SetFxParam {
        guid: String,
        fx_idx: u32,
        param_idx: u32,
        value: f64,
    },
    SetFxEnabled {
        guid: String,
        fx_idx: u32,
        enabled: bool,
    },
    /// Re-fetch the selected track's FX chain + the focused FX's
    /// params (async edge → [`DriverState::set_fx`] /
    /// [`DriverState::set_params`]).
    RefreshFx,
    Play,
    Stop,
    Record,
    StopRecording,
    ToggleLoop,
    NudgePosition {
        seconds: f64,
    },
    /// Navigation already applied to the navigator / zone state —
    /// re-resolve strips and refresh the surface.
    Refresh,
}

/// The analog payload of a gesture, passed to [`Action`] handling.
#[derive(Debug, Clone, Copy)]
enum Gesture {
    /// Button-like: press (releases are filtered before dispatch).
    Press,
    /// Fader move, 14-bit position.
    Fader(u16),
    /// Relative encoder, signed ticks.
    Delta(i8),
}

/// One send slot of the selected track (sends-zone strip context).
#[derive(Debug, Clone, Default)]
pub struct SendSlot {
    pub dest_name: String,
    pub volume: f64,
    pub pan: f64,
    pub muted: bool,
}

/// One FX slot of the selected track (FX-menu zone strip context).
#[derive(Debug, Clone, Default)]
pub struct FxSlot {
    pub guid: String,
    pub name: String,
    pub enabled: bool,
}

/// One parameter of the focused FX (FX param zone strip context).
#[derive(Debug, Clone, Default)]
pub struct ParamSlot {
    pub name: String,
    /// Normalized 0–1.
    pub value: f64,
    /// Plugin-formatted display value ("−12.0 dB").
    pub text: String,
}

/// A press waiting for hold/tap resolution at release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PendingKey {
    Strip(u8, StripWidget),
    Global(GlobalWidget),
}

#[derive(Debug, Clone, Copy)]
struct PendingPress {
    mods: Modifiers,
    at_ms: u64,
}

/// Everything the gesture/feedback logic reads and mutates. No I/O —
/// the async edge owns the port and services.
pub struct DriverState {
    pub nav: Navigator,
    pub shadow: Shadow,
    pub zones: ZoneSet,
    /// Name of the active zone (always resolvable in `zones`).
    pub active_zone: String,
    /// Flat track cache, index == project order.
    pub tracks: Vec<Track>,
    /// Current strip → track-cache index mapping.
    strips: Vec<Option<usize>>,
    /// Send slots of the selected track (sends zones). Refreshed by
    /// the async edge via [`Intent::RefreshSends`].
    pub sends: Vec<SendSlot>,
    /// Window offset into `sends`.
    send_bank: usize,
    /// Receive slots of the selected track (receives zones).
    pub receives: Vec<SendSlot>,
    receive_bank: usize,
    /// FX chain of the selected track (FX-menu zones).
    pub fx_list: Vec<FxSlot>,
    /// Which `fx_list` slot the param zone operates on.
    pub focused_fx: Option<usize>,
    /// Parameters of the focused FX (FX param zones).
    pub params: Vec<ParamSlot>,
    /// Window offset into `params`.
    param_bank: usize,
    pub master_guid: String,
    pub master_volume: f64,
    pub play_state: PlayState,
    pub looping: bool,
    /// Fader touch sensors — while touched, motor feedback for that
    /// strip is suppressed (no fader fights).
    touched: [bool; 9],
    /// Live modifier-key mask (Shift/Option/Control/Alt).
    modifiers: Modifiers,
    /// Latched layer bits (FLIP / TOGGLE) — included in every lookup
    /// until toggled off.
    latched: Modifiers,
    /// Presses deferred until release for hold/tap resolution.
    pending: HashMap<PendingKey, PendingPress>,
    /// Resolved taps waiting for a possible second tap (`double+`).
    pending_taps: HashMap<PendingKey, PendingPress>,
    /// Surface bank follows UI selection (CSI ScrollLink).
    pub scroll_link: bool,
    /// 7-segment readout shows bars/beats instead of time.
    pub beats_display: bool,
    /// Metronome state (click button LED).
    pub metronome: bool,
    /// Latest playhead position from the bus (seconds + musical).
    last_time: Option<f64>,
    last_musical: Option<(i32, i32, i32)>,
    /// Anchor for range select = last exclusive selection.
    sel_anchor: Option<String>,
    /// The last FX param this surface touched (LastTouchedFxParam).
    last_touched: Option<(String, u32, u32)>,
}

impl DriverState {
    pub fn new(
        zones: ZoneSet,
        tracks: Vec<Track>,
        master_guid: String,
        master_volume: f64,
    ) -> Self {
        let active_zone = zones.home.clone();
        let mut s = Self {
            nav: Navigator::default(),
            shadow: Shadow::default(),
            zones,
            active_zone,
            tracks,
            strips: vec![None; mcu::STRIPS],
            sends: Vec::new(),
            send_bank: 0,
            receives: Vec::new(),
            receive_bank: 0,
            fx_list: Vec::new(),
            focused_fx: None,
            params: Vec::new(),
            param_bank: 0,
            master_guid,
            master_volume,
            play_state: PlayState::Stopped,
            looping: false,
            touched: [false; 9],
            modifiers: 0,
            latched: 0,
            pending: HashMap::new(),
            pending_taps: HashMap::new(),
            scroll_link: false,
            beats_display: false,
            metronome: false,
            last_time: None,
            last_musical: None,
            sel_anchor: None,
            last_touched: None,
        };
        s.enter_zone(&s.zones.home.clone());
        s.rebind_strips();
        s
    }

    /// Convenience for tests / embedded default behavior.
    pub fn with_builtin_zones(tracks: Vec<Track>, master_guid: String, master_volume: f64) -> Self {
        Self::new(ZoneSet::builtin(), tracks, master_guid, master_volume)
    }

    /// Re-resolve which track sits on each strip (after banking,
    /// folder navigation, or track list changes).
    pub fn rebind_strips(&mut self) {
        self.strips = self.nav.visible(&self.tracks, mcu::STRIPS);
    }

    /// Project track indices currently on the strips (meter polling).
    pub fn visible_track_indices(&self) -> Vec<Option<u32>> {
        self.strips
            .iter()
            .map(|s| s.and_then(|i| self.tracks.get(i)).map(|t| t.index))
            .collect()
    }

    fn strip_track(&self, strip: u8) -> Option<&Track> {
        self.strips
            .get(strip as usize)
            .copied()
            .flatten()
            .and_then(|i| self.tracks.get(i))
    }

    fn send_slot(&self, strip: u8) -> Option<&SendSlot> {
        self.sends.get(self.send_bank + strip as usize)
    }

    fn send_index(&self, strip: u8) -> u32 {
        (self.send_bank + strip as usize) as u32
    }

    /// First selected track's guid (sends zones bind to it).
    pub fn selected_guid(&self) -> Option<String> {
        self.tracks
            .iter()
            .find(|t| t.selected)
            .map(|t| t.guid.clone())
    }

    /// Replace the send-slot cache (async edge, after RefreshSends).
    pub fn set_sends(&mut self, sends: Vec<SendSlot>) {
        self.sends = sends;
        self.send_bank = self.send_bank.min(self.sends.len().saturating_sub(1));
    }

    /// Replace the receive-slot cache (async edge, after RefreshSends).
    pub fn set_receives(&mut self, receives: Vec<SendSlot>) {
        self.receives = receives;
        self.receive_bank = self.receive_bank.min(self.receives.len().saturating_sub(1));
    }

    fn receive_slot(&self, strip: u8) -> Option<&SendSlot> {
        self.receives.get(self.receive_bank + strip as usize)
    }

    fn receive_index(&self, strip: u8) -> u32 {
        (self.receive_bank + strip as usize) as u32
    }

    /// Latest playhead from the bus → 7-seg text for the current mode.
    pub fn apply_position(&mut self, playhead: &daw_proto::Position) {
        if let Some(t) = &playhead.time {
            self.last_time = Some(t.as_seconds());
        }
        if let Some(m) = &playhead.musical {
            self.last_musical = Some((m.measure, m.beat, m.subdivision));
        }
    }

    fn timecode_text(&self) -> Option<String> {
        if self.beats_display {
            let (measure, beat, sub) = self.last_musical?;
            Some(format!("{measure}.{beat:02}.{sub:03}"))
        } else {
            let t = self.last_time?;
            let h = (t / 3600.0) as u64;
            let m = ((t / 60.0) as u64) % 60;
            let sec = (t as u64) % 60;
            let cents = ((t - t.floor()) * 100.0) as u64;
            Some(format!("{h}.{m:02}.{sec:02}.{cents:02}"))
        }
    }

    /// Resolve pending taps whose double-press window expired. Call
    /// from the periodic tick; returns intents from the resolved
    /// single-tap bindings.
    pub fn flush_pending(&mut self, now_ms: u64) -> Vec<Intent> {
        let expired: Vec<PendingKey> = self
            .pending_taps
            .iter()
            .filter(|(_, p)| now_ms.saturating_sub(p.at_ms) >= DOUBLE_MS)
            .map(|(k, _)| *k)
            .collect();
        let mut out = Vec::new();
        for key in expired {
            if let Some(p) = self.pending_taps.remove(&key) {
                out.extend(self.dispatch_key(key, p.mods));
            }
        }
        out
    }

    /// Replace the FX-chain cache (async edge, after RefreshFx).
    pub fn set_fx(&mut self, fx: Vec<FxSlot>) {
        self.fx_list = fx;
        if self.focused_fx.is_some_and(|i| i >= self.fx_list.len()) {
            self.focused_fx = None;
            self.params.clear();
        }
    }

    /// Replace the focused FX's parameter cache.
    pub fn set_params(&mut self, params: Vec<ParamSlot>) {
        self.params = params;
        self.param_bank = self.param_bank.min(self.params.len().saturating_sub(1));
    }

    fn param_slot(&self, strip: u8) -> Option<&ParamSlot> {
        self.params.get(self.param_bank + strip as usize)
    }

    fn param_index(&self, strip: u8) -> u32 {
        (self.param_bank + strip as usize) as u32
    }

    fn fx_slot(&self, strip: u8) -> Option<&FxSlot> {
        self.fx_list.get(strip as usize)
    }

    /// Whether the active zone binds any send action — the async edge
    /// uses this to refetch sends on selection changes.
    pub fn uses_sends(&self) -> bool {
        self.zones
            .zone(&self.active_zone)
            .map(|z| {
                z.strip.values().any(|a| {
                    matches!(
                        a,
                        Action::SendVolume
                            | Action::SendPan
                            | Action::SendMute
                            | Action::SendNameDisplay
                            | Action::SendVolumeDisplay
                            | Action::SendPanDisplay
                    )
                })
            })
            .unwrap_or(false)
    }

    /// Whether the active zone binds any FX action — refetch trigger
    /// for selection changes and FX add/remove events.
    pub fn uses_fx(&self) -> bool {
        self.zones
            .zone(&self.active_zone)
            .map(|z| {
                z.strip.values().any(|a| {
                    matches!(
                        a,
                        Action::FxMenuSelect { .. }
                            | Action::FxMenuBypass
                            | Action::FxParam
                            | Action::FxMenuNameDisplay
                            | Action::FxParamNameDisplay
                            | Action::FxParamValueDisplay
                    )
                })
            })
            .unwrap_or(false)
    }

    /// Apply an FX event from the bus to the caches. Returns `true`
    /// when the chain SHAPE changed (added/removed) — the caller
    /// should refetch via RefreshFx semantics.
    pub fn apply_fx_event(&mut self, event: &daw_proto::FxEvent) -> bool {
        use daw_proto::FxEvent as E;
        match event {
            E::Added { .. } | E::Removed { .. } | E::Moved { .. } => return true,
            E::EnabledChanged {
                fx_guid, enabled, ..
            } => {
                if let Some(slot) = self.fx_list.iter_mut().find(|f| &f.guid == fx_guid) {
                    slot.enabled = *enabled;
                }
            }
            E::ParameterChanged {
                fx_guid,
                param_index,
                value,
                ..
            } => {
                // Only the focused FX's params are cached.
                let focused = self
                    .focused_fx
                    .and_then(|i| self.fx_list.get(i))
                    .is_some_and(|f| &f.guid == fx_guid);
                if focused && let Some(p) = self.params.get_mut(*param_index as usize) {
                    p.value = *value;
                }
            }
            _ => {}
        }
        false
    }

    /// Activate a zone: pin its navigator mode and rebind.
    fn enter_zone(&mut self, name: &str) {
        let Some(zone) = self.zones.zone(name) else {
            tracing::warn!("csi: @GoZone to unknown zone '{name}'");
            return;
        };
        let nav_mode = zone.navigator;
        self.active_zone = name.to_string();
        if let Some(mode) = nav_mode {
            self.nav.set_mode(mode);
        }
        self.send_bank = 0;
        self.rebind_strips();
    }

    // ── Gestures: surface → intents ─────────────────────────────────

    /// Decode raw MIDI from the surface, resolve it through the
    /// active zone, and return the intents to execute. `now_ms` is a
    /// monotonic clock for hold/tap resolution. Navigation actions
    /// mutate local state here and yield `Refresh`.
    pub fn handle_midi(&mut self, raw: &[u8], now_ms: u64) -> Vec<Intent> {
        let Some(input) = mcu::decode(raw) else {
            tracing::trace!(?raw, "csi: undecoded MIDI from surface");
            return Vec::new();
        };
        tracing::debug!(zone = %self.active_zone, ?input, "csi: surface input");
        let intents = self.handle_input(input, now_ms);
        if !intents.is_empty() {
            tracing::debug!(?intents, "csi: → intents");
        }
        intents
    }

    fn handle_input(&mut self, input: SurfaceInput, now_ms: u64) -> Vec<Intent> {
        match input {
            SurfaceInput::Fader { strip, pos } => {
                if strip == mcu::MASTER {
                    self.dispatch_global(GlobalWidget::MasterFader, Gesture::Fader(pos))
                } else {
                    self.dispatch_strip(strip, StripWidget::Fader, Gesture::Fader(pos))
                }
            }
            SurfaceInput::FaderTouch { strip, touched } => {
                // Touch isn't bindable — it's motor-feedback
                // suppression infrastructure, and it gates Touch/Latch
                // automation recording on the strip's volume.
                if let Some(cell) = self.touched.get_mut(strip as usize) {
                    *cell = touched;
                }
                if strip < mcu::MASTER
                    && matches!(
                        self.zones
                            .zone(&self.active_zone)
                            .and_then(|z| z.strip_action(self.latched, StripWidget::Fader)),
                        Some(Action::TrackVolume)
                    )
                    && let Some(t) = self.strip_track(strip)
                {
                    return vec![Intent::AutomationTouch {
                        guid: t.guid.clone(),
                        touched,
                    }];
                }
                Vec::new()
            }
            SurfaceInput::VPot { strip, delta } => {
                self.dispatch_strip(strip, StripWidget::VPot, Gesture::Delta(delta))
            }
            SurfaceInput::VPotPress { strip, pressed } => {
                if !pressed {
                    return Vec::new();
                }
                self.dispatch_strip(strip, StripWidget::VPotPress, Gesture::Press)
            }
            SurfaceInput::Jog { delta } => {
                self.dispatch_global(GlobalWidget::Jog, Gesture::Delta(delta))
            }
            SurfaceInput::Button { button, pressed } => {
                // Modifier keys update the mask and are never bindable.
                let modifier = match button {
                    Button::Shift => Some(SHIFT),
                    Button::Option => Some(OPTION),
                    Button::Control => Some(CONTROL),
                    Button::Alt => Some(ALT),
                    _ => None,
                };
                if let Some(bit) = modifier {
                    if pressed {
                        self.modifiers |= bit;
                    } else {
                        self.modifiers &= !bit;
                    }
                    return Vec::new();
                }
                let key = match button {
                    Button::Rec(s) => PendingKey::Strip(s, StripWidget::Rec),
                    Button::Solo(s) => PendingKey::Strip(s, StripWidget::Solo),
                    Button::Mute(s) => PendingKey::Strip(s, StripWidget::Mute),
                    Button::Select(s) => PendingKey::Strip(s, StripWidget::Select),
                    other => PendingKey::Global(GlobalWidget::Button(other)),
                };
                if pressed {
                    self.button_press(key, now_ms)
                } else {
                    self.button_release(key, now_ms)
                }
            }
        }
    }

    /// Press: defer when a `hold+` binding exists for this widget,
    /// otherwise dispatch immediately (zero added latency for the
    /// common case).
    fn button_press(&mut self, key: PendingKey, now_ms: u64) -> Vec<Intent> {
        let mods = self.modifiers | self.latched;
        let (has_hold, has_double) = self
            .zones
            .zone(&self.active_zone)
            .map(|z| match key {
                PendingKey::Strip(_, w) => (
                    z.strip_has_exact(mods | HOLD, w),
                    z.strip_has_exact(mods | DOUBLE, w),
                ),
                PendingKey::Global(w) => (
                    z.global_has_exact(mods | HOLD, w),
                    z.global_has_exact(mods | DOUBLE, w),
                ),
            })
            .unwrap_or((false, false));
        // Second tap within the window → the double binding fires now.
        if has_double
            && let Some(prev) = self.pending_taps.remove(&key)
            && now_ms.saturating_sub(prev.at_ms) < DOUBLE_MS
        {
            return self.dispatch_key(key, mods | DOUBLE);
        }
        if has_hold || has_double {
            self.pending.insert(
                key,
                PendingPress {
                    mods,
                    at_ms: now_ms,
                },
            );
            return Vec::new();
        }
        self.dispatch_key(key, mods)
    }

    /// Release: resolve a deferred press as tap or hold by duration.
    /// Taps with a `double+` binding wait out the double window (the
    /// periodic flush fires them as singles).
    fn button_release(&mut self, key: PendingKey, now_ms: u64) -> Vec<Intent> {
        let Some(p) = self.pending.remove(&key) else {
            return Vec::new();
        };
        let exact = |z: &crate::zones::CompiledZone, mods: Modifiers| match key {
            PendingKey::Strip(_, w) => z.strip_has_exact(mods, w),
            PendingKey::Global(w) => z.global_has_exact(mods, w),
        };
        if now_ms.saturating_sub(p.at_ms) >= HOLD_MS
            && self
                .zones
                .zone(&self.active_zone)
                .map(|z| exact(z, p.mods | HOLD))
                .unwrap_or(false)
        {
            return self.dispatch_key(key, p.mods | HOLD);
        }
        let has_double = self
            .zones
            .zone(&self.active_zone)
            .map(|z| exact(z, p.mods | DOUBLE))
            .unwrap_or(false);
        if has_double {
            self.pending_taps.insert(
                key,
                PendingPress {
                    mods: p.mods,
                    at_ms: now_ms,
                },
            );
            return Vec::new();
        }
        self.dispatch_key(key, p.mods)
    }

    fn dispatch_key(&mut self, key: PendingKey, mods: Modifiers) -> Vec<Intent> {
        match key {
            PendingKey::Strip(s, w) => self.dispatch_strip_with(s, w, Gesture::Press, mods),
            PendingKey::Global(w) => self.dispatch_global_with(w, Gesture::Press, mods),
        }
    }

    fn dispatch_strip(&mut self, strip: u8, widget: StripWidget, gesture: Gesture) -> Vec<Intent> {
        self.dispatch_strip_with(strip, widget, gesture, self.modifiers | self.latched)
    }

    fn dispatch_strip_with(
        &mut self,
        strip: u8,
        widget: StripWidget,
        gesture: Gesture,
        mods: Modifiers,
    ) -> Vec<Intent> {
        let Some(zone) = self.zones.zone(&self.active_zone) else {
            return Vec::new();
        };
        let Some(action) = zone.strip_action(mods, widget).cloned() else {
            return Vec::new();
        };
        self.apply_action(&action, Some(strip), gesture)
    }

    fn dispatch_global(&mut self, widget: GlobalWidget, gesture: Gesture) -> Vec<Intent> {
        self.dispatch_global_with(widget, gesture, self.modifiers | self.latched)
    }

    fn dispatch_global_with(
        &mut self,
        widget: GlobalWidget,
        gesture: Gesture,
        mods: Modifiers,
    ) -> Vec<Intent> {
        let Some(zone) = self.zones.zone(&self.active_zone) else {
            return Vec::new();
        };
        let Some(action) = zone.global_action(mods, widget).cloned() else {
            return Vec::new();
        };
        self.apply_action(&action, None, gesture)
    }

    /// Turn a bound action + gesture into intents. `strip` is `Some`
    /// for strip-context widgets.
    fn apply_action(
        &mut self,
        action: &Action,
        strip: Option<u8>,
        gesture: Gesture,
    ) -> Vec<Intent> {
        let strip_guid = |s: &Self| strip.and_then(|i| s.strip_track(i)).map(|t| t.guid.clone());
        match action {
            Action::TrackVolume => {
                let Gesture::Fader(pos) = gesture else {
                    return Vec::new();
                };
                let Some(guid) = strip_guid(self) else {
                    return Vec::new();
                };
                let volume = taper::fader_to_volume(pos);
                // Update the cache now so the echo event diffs clean.
                if let Some(t) = self.tracks.iter_mut().find(|t| t.guid == guid) {
                    t.volume = volume;
                }
                vec![Intent::SetVolume { guid, volume }]
            }
            Action::TrackPan => {
                let Gesture::Delta(delta) = gesture else {
                    return Vec::new();
                };
                let Some(guid) = strip_guid(self) else {
                    return Vec::new();
                };
                // Shift = fine adjustment (CSI convention).
                let step = if self.modifiers & SHIFT != 0 {
                    0.005
                } else {
                    0.02
                };
                let current = self
                    .tracks
                    .iter()
                    .find(|t| t.guid == guid)
                    .map(|t| t.pan)
                    .unwrap_or(0.0);
                let pan = (current + delta as f64 * step).clamp(-1.0, 1.0);
                if let Some(t) = self.tracks.iter_mut().find(|t| t.guid == guid) {
                    t.pan = pan;
                }
                vec![Intent::SetPan { guid, pan }]
            }
            Action::TrackPanReset => {
                let Some(guid) = strip_guid(self) else {
                    return Vec::new();
                };
                if let Some(t) = self.tracks.iter_mut().find(|t| t.guid == guid) {
                    t.pan = 0.0;
                }
                vec![Intent::SetPan { guid, pan: 0.0 }]
            }
            Action::TrackMute => self.toggle_flag(
                strip,
                |t| t.muted,
                |guid, v| Intent::SetMuted { guid, muted: v },
            ),
            Action::TrackSolo => self.toggle_flag(
                strip,
                |t| t.soloed,
                |guid, v| Intent::SetSoloed { guid, soloed: v },
            ),
            Action::TrackRecordArm => self.toggle_flag(
                strip,
                |t| t.armed,
                |guid, v| Intent::SetArmed { guid, armed: v },
            ),
            Action::TrackSelect => {
                let Some(guid) = strip_guid(self) else {
                    return Vec::new();
                };
                self.sel_anchor = Some(guid.clone());
                vec![Intent::SelectExclusive { guid }]
            }
            Action::TrackSelectAdditive => strip_guid(self)
                .map(|guid| Intent::SelectAdditive { guid })
                .into_iter()
                .collect(),
            Action::FolderSpill => {
                let Some(t) = strip.and_then(|s| self.strip_track(s)).cloned() else {
                    return Vec::new();
                };
                // Folders drill/pop; plain tracks fall through to
                // selection (CSI's ToggleFolderSpill).
                if self.nav.folder_select(&t) {
                    self.rebind_strips();
                    return vec![Intent::Refresh];
                }
                vec![Intent::SelectExclusive { guid: t.guid }]
            }
            Action::VcaSpill => {
                let Some(t) = strip.and_then(|s| self.strip_track(s)).cloned() else {
                    return Vec::new();
                };
                if self.nav.vca_select(&t) {
                    self.rebind_strips();
                    return vec![Intent::Refresh];
                }
                vec![Intent::SelectExclusive { guid: t.guid }]
            }
            Action::SendVolume => {
                let Some(strip) = strip else {
                    return Vec::new();
                };
                let Some(guid) = self.selected_guid() else {
                    return Vec::new();
                };
                let index = self.send_index(strip);
                let volume = match gesture {
                    Gesture::Fader(pos) => taper::fader_to_volume(pos),
                    Gesture::Delta(d) => {
                        let cur = self.send_slot(strip).map(|s| s.volume).unwrap_or(0.0);
                        (cur + d as f64 * 0.02).max(0.0)
                    }
                    Gesture::Press => return Vec::new(),
                };
                if self.send_slot(strip).is_none() {
                    return Vec::new();
                }
                if let Some(s) = self.sends.get_mut(self.send_bank + strip as usize) {
                    s.volume = volume;
                }
                vec![Intent::SetSendVolume {
                    guid,
                    index,
                    volume,
                }]
            }
            Action::SendPan => {
                let Some(strip) = strip else {
                    return Vec::new();
                };
                let Gesture::Delta(delta) = gesture else {
                    return Vec::new();
                };
                let Some(guid) = self.selected_guid() else {
                    return Vec::new();
                };
                if self.send_slot(strip).is_none() {
                    return Vec::new();
                }
                let index = self.send_index(strip);
                let step = if self.modifiers & SHIFT != 0 {
                    0.005
                } else {
                    0.02
                };
                let cur = self.send_slot(strip).map(|s| s.pan).unwrap_or(0.0);
                let pan = (cur + delta as f64 * step).clamp(-1.0, 1.0);
                if let Some(s) = self.sends.get_mut(self.send_bank + strip as usize) {
                    s.pan = pan;
                }
                vec![Intent::SetSendPan { guid, index, pan }]
            }
            Action::SendMute => {
                let Some(strip) = strip else {
                    return Vec::new();
                };
                let Some(guid) = self.selected_guid() else {
                    return Vec::new();
                };
                let Some(slot) = self.send_slot(strip) else {
                    return Vec::new();
                };
                let muted = !slot.muted;
                let index = self.send_index(strip);
                if let Some(s) = self.sends.get_mut(self.send_bank + strip as usize) {
                    s.muted = muted;
                }
                vec![Intent::SetSendMuted { guid, index, muted }]
            }
            Action::FxMenuSelect { zone } => {
                let Some(strip) = strip else {
                    return Vec::new();
                };
                if self.fx_slot(strip).is_none() {
                    return Vec::new();
                }
                self.focused_fx = Some(strip as usize);
                self.param_bank = 0;
                self.params.clear();
                let zone = zone.clone();
                self.enter_zone(&zone);
                vec![Intent::Refresh, Intent::RefreshFx]
            }
            Action::FxMenuBypass => {
                let Some(strip) = strip else {
                    return Vec::new();
                };
                let Some(guid) = self.selected_guid() else {
                    return Vec::new();
                };
                let Some(slot) = self.fx_slot(strip) else {
                    return Vec::new();
                };
                let enabled = !slot.enabled;
                let fx_idx = strip as u32;
                if let Some(s) = self.fx_list.get_mut(strip as usize) {
                    s.enabled = enabled;
                }
                vec![Intent::SetFxEnabled {
                    guid,
                    fx_idx,
                    enabled,
                }]
            }
            Action::FxParam => {
                let Some(strip) = strip else {
                    return Vec::new();
                };
                let Some(guid) = self.selected_guid() else {
                    return Vec::new();
                };
                let Some(fx_idx) = self.focused_fx else {
                    return Vec::new();
                };
                let value = match gesture {
                    Gesture::Fader(pos) => pos as f64 / 16383.0,
                    Gesture::Delta(d) => {
                        let step = if self.modifiers & SHIFT != 0 {
                            0.002
                        } else {
                            0.01
                        };
                        let cur = self.param_slot(strip).map(|p| p.value).unwrap_or(0.0);
                        (cur + d as f64 * step).clamp(0.0, 1.0)
                    }
                    Gesture::Press => return Vec::new(),
                };
                if self.param_slot(strip).is_none() {
                    return Vec::new();
                }
                let param_idx = self.param_index(strip);
                if let Some(p) = self.params.get_mut(self.param_bank + strip as usize) {
                    p.value = value;
                }
                self.last_touched = Some((guid.clone(), fx_idx as u32, param_idx));
                vec![Intent::SetFxParam {
                    guid,
                    fx_idx: fx_idx as u32,
                    param_idx,
                    value,
                }]
            }
            Action::BankFxParams { amount } => {
                let max = self.params.len().saturating_sub(1);
                self.param_bank =
                    (self.param_bank as isize + *amount as isize).clamp(0, max as isize) as usize;
                vec![Intent::Refresh]
            }
            Action::TrackTogglePolarity => {
                let Some(t) = strip.and_then(|s| self.strip_track(s)) else {
                    return Vec::new();
                };
                let guid = t.guid.clone();
                let inverted = !t.phase_inverted;
                if let Some(t) = self.tracks.iter_mut().find(|t| t.guid == guid) {
                    t.phase_inverted = inverted;
                }
                vec![Intent::SetPhaseInverted { guid, inverted }]
            }
            Action::TrackRangeSelect => {
                let Some(t) = strip.and_then(|s| self.strip_track(s)) else {
                    return Vec::new();
                };
                let to = t.guid.clone();
                let from = self.sel_anchor.clone().unwrap_or_else(|| to.clone());
                let a = self.tracks.iter().position(|t| t.guid == from);
                let b = self.tracks.iter().position(|t| t.guid == to);
                let (Some(a), Some(b)) = (a, b) else {
                    return Vec::new();
                };
                let (lo, hi) = (a.min(b), a.max(b));
                let guids = self.tracks[lo..=hi]
                    .iter()
                    .map(|t| t.guid.clone())
                    .collect();
                vec![Intent::SelectMany { guids }]
            }
            Action::ReceiveVolume => {
                let Some(strip) = strip else {
                    return Vec::new();
                };
                let Some(guid) = self.selected_guid() else {
                    return Vec::new();
                };
                let volume = match gesture {
                    Gesture::Fader(pos) => taper::fader_to_volume(pos),
                    Gesture::Delta(d) => {
                        let cur = self.receive_slot(strip).map(|s| s.volume).unwrap_or(0.0);
                        (cur + d as f64 * 0.02).max(0.0)
                    }
                    Gesture::Press => return Vec::new(),
                };
                if self.receive_slot(strip).is_none() {
                    return Vec::new();
                }
                let index = self.receive_index(strip);
                if let Some(r) = self.receives.get_mut(self.receive_bank + strip as usize) {
                    r.volume = volume;
                }
                vec![Intent::SetReceiveVolume {
                    guid,
                    index,
                    volume,
                }]
            }
            Action::ReceivePan => {
                let Some(strip) = strip else {
                    return Vec::new();
                };
                let Gesture::Delta(delta) = gesture else {
                    return Vec::new();
                };
                let Some(guid) = self.selected_guid() else {
                    return Vec::new();
                };
                if self.receive_slot(strip).is_none() {
                    return Vec::new();
                }
                let index = self.receive_index(strip);
                let step = if self.modifiers & SHIFT != 0 {
                    0.005
                } else {
                    0.02
                };
                let cur = self.receive_slot(strip).map(|s| s.pan).unwrap_or(0.0);
                let pan = (cur + delta as f64 * step).clamp(-1.0, 1.0);
                if let Some(r) = self.receives.get_mut(self.receive_bank + strip as usize) {
                    r.pan = pan;
                }
                vec![Intent::SetReceivePan { guid, index, pan }]
            }
            Action::ReceiveMute => {
                let Some(strip) = strip else {
                    return Vec::new();
                };
                let Some(guid) = self.selected_guid() else {
                    return Vec::new();
                };
                let Some(slot) = self.receive_slot(strip) else {
                    return Vec::new();
                };
                let muted = !slot.muted;
                let index = self.receive_index(strip);
                if let Some(r) = self.receives.get_mut(self.receive_bank + strip as usize) {
                    r.muted = muted;
                }
                vec![Intent::SetReceiveMuted { guid, index, muted }]
            }
            Action::BankReceives { amount } => {
                let max = self.receives.len().saturating_sub(1);
                self.receive_bank =
                    (self.receive_bank as isize + *amount as isize).clamp(0, max as isize) as usize;
                vec![Intent::Refresh]
            }
            Action::FxNextPreset | Action::FxPrevPreset => {
                let Some(guid) = self.selected_guid() else {
                    return Vec::new();
                };
                let Some(fx_idx) = self.focused_fx else {
                    return Vec::new();
                };
                let fx_idx = fx_idx as u32;
                if matches!(action, Action::FxNextPreset) {
                    vec![Intent::FxNextPreset { guid, fx_idx }, Intent::RefreshFx]
                } else {
                    vec![Intent::FxPrevPreset { guid, fx_idx }, Intent::RefreshFx]
                }
            }
            Action::LastTouchedFxParam => {
                let Gesture::Delta(delta) = gesture else {
                    return Vec::new();
                };
                let Some((guid, fx_idx, param_idx)) = self.last_touched.clone() else {
                    return Vec::new();
                };
                let step = if self.modifiers & SHIFT != 0 {
                    0.002
                } else {
                    0.01
                };
                // Nudge the cached value when it's the focused FX,
                // else fire a relative-ish set from 0.5 (best effort).
                let cur = self
                    .params
                    .get(param_idx as usize)
                    .map(|p| p.value)
                    .unwrap_or(0.5);
                let value = (cur + delta as f64 * step).clamp(0.0, 1.0);
                if let Some(p) = self.params.get_mut(param_idx as usize) {
                    p.value = value;
                }
                vec![Intent::SetFxParam {
                    guid,
                    fx_idx,
                    param_idx,
                    value,
                }]
            }
            Action::SaveProject => vec![Intent::SaveProject],
            Action::Undo => vec![Intent::Undo],
            Action::Redo => vec![Intent::Redo],
            Action::Command { id } => vec![Intent::RunCommand { id: id.clone() }],
            Action::AddMarker => vec![Intent::AddMarker],
            Action::PrevMarker => vec![Intent::PrevMarker],
            Action::NextMarker => vec![Intent::NextMarker],
            Action::Flip => {
                self.latched ^= FLIP;
                vec![Intent::Refresh]
            }
            Action::Toggle => {
                self.latched ^= TOGGLE;
                vec![Intent::Refresh]
            }
            Action::ToggleScrollLink => {
                self.scroll_link = !self.scroll_link;
                vec![Intent::Refresh]
            }
            Action::TrackAutoMode { mode } => {
                let Some(guid) = self.selected_guid() else {
                    return Vec::new();
                };
                use crate::action::AutoMode as A;
                use daw_proto::AutomationMode as P;
                let mode = match mode {
                    A::Trim => P::TrimRead,
                    A::Read => P::Read,
                    A::Touch => P::Touch,
                    A::Write => P::Write,
                    A::Latch => P::Latch,
                };
                if let Some(t) = self.tracks.iter_mut().find(|t| t.guid == guid) {
                    t.automation_mode = mode;
                }
                vec![Intent::SetAutomationMode { guid, mode }]
            }
            Action::CycleInputMonitor => {
                let Some(t) = strip.and_then(|s| self.strip_track(s)) else {
                    return Vec::new();
                };
                use daw_proto::track::InputMonitoringMode as M;
                let next = match t.input_monitor {
                    M::Off => M::Normal,
                    M::Normal => M::NotWhenPlaying,
                    M::NotWhenPlaying => M::Off,
                };
                let guid = t.guid.clone();
                if let Some(t) = self.tracks.iter_mut().find(|t| t.guid == guid) {
                    t.input_monitor = next;
                }
                vec![Intent::SetInputMonitor {
                    guid,
                    monitor: next,
                }]
            }
            Action::ToggleMetronome => vec![Intent::ToggleMetronome],
            Action::CycleTimeDisplay => {
                self.beats_display = !self.beats_display;
                vec![Intent::Refresh]
            }
            Action::MasterVolume => {
                let Gesture::Fader(pos) = gesture else {
                    return Vec::new();
                };
                let volume = taper::fader_to_volume(pos);
                self.master_volume = volume;
                vec![Intent::SetMasterVolume { volume }]
            }
            Action::Play => vec![Intent::Play],
            Action::Stop => vec![Intent::Stop],
            Action::Record => {
                if matches!(self.play_state, PlayState::Recording) {
                    vec![Intent::StopRecording]
                } else {
                    vec![Intent::Record]
                }
            }
            Action::ToggleLoop => vec![Intent::ToggleLoop],
            Action::ClearAllSolo => vec![Intent::ClearAllSolo],
            Action::UnmuteAll => vec![Intent::UnmuteAll],
            Action::NudgePosition { seconds } => vec![Intent::NudgePosition { seconds: *seconds }],
            Action::JogPosition { seconds_per_tick } => {
                let Gesture::Delta(delta) = gesture else {
                    return Vec::new();
                };
                let fine = if self.modifiers & SHIFT != 0 {
                    0.1
                } else {
                    1.0
                };
                vec![Intent::NudgePosition {
                    seconds: delta as f64 * seconds_per_tick * fine,
                }]
            }
            Action::Bank { amount } => {
                self.nav.bank(*amount as isize, &self.tracks, mcu::STRIPS);
                self.rebind_strips();
                vec![Intent::Refresh]
            }
            Action::BankSends { amount } => {
                let max = self.sends.len().saturating_sub(1);
                self.send_bank =
                    (self.send_bank as isize + *amount as isize).clamp(0, max as isize) as usize;
                vec![Intent::Refresh]
            }
            Action::GoZone { zone } => {
                self.enter_zone(&zone.clone());
                // Sends zones need fresh slot data for the new context.
                vec![Intent::Refresh, Intent::RefreshSends]
            }
            // Display actions are feedback-only; binding one to an
            // input widget is inert.
            Action::TrackName
            | Action::PanDisplay
            | Action::VolumeDisplay
            | Action::FolderIndicator
            | Action::SendNameDisplay
            | Action::SendVolumeDisplay
            | Action::SendPanDisplay
            | Action::ReceiveNameDisplay
            | Action::ReceiveVolumeDisplay
            | Action::ReceivePanDisplay
            | Action::FxMenuNameDisplay
            | Action::FxParamNameDisplay
            | Action::FxParamValueDisplay
            | Action::Fixed { .. }
            | Action::Blank
            | Action::NoAction => Vec::new(),
        }
    }

    fn toggle_flag(
        &self,
        strip: Option<u8>,
        read: impl Fn(&Track) -> bool,
        make: impl Fn(String, bool) -> Intent,
    ) -> Vec<Intent> {
        strip
            .and_then(|s| self.strip_track(s))
            .map(|t| make(t.guid.clone(), !read(t)))
            .into_iter()
            .collect()
    }

    // ── Feedback: events → surface messages ─────────────────────────

    /// Apply one track event to the cache. Returns `true` when the
    /// track LIST changed shape (added/removed/moved) — the caller
    /// must refetch the list and do a full refresh.
    pub fn apply_track_event(&mut self, event: &TrackEvent) -> bool {
        let by_guid = |tracks: &mut Vec<Track>, guid: &str| -> Option<usize> {
            tracks.iter().position(|t| t.guid == guid)
        };
        match event {
            TrackEvent::Added(_) | TrackEvent::Removed(_) | TrackEvent::Moved { .. } => {
                return true;
            }
            TrackEvent::VolumeChanged { guid, volume } => {
                if guid == &self.master_guid {
                    self.master_volume = *volume;
                } else if let Some(i) = by_guid(&mut self.tracks, guid) {
                    self.tracks[i].volume = *volume;
                }
            }
            TrackEvent::PanChanged { guid, pan } => {
                if let Some(i) = by_guid(&mut self.tracks, guid) {
                    self.tracks[i].pan = *pan;
                }
            }
            TrackEvent::MuteChanged { guid, muted } => {
                if let Some(i) = by_guid(&mut self.tracks, guid) {
                    self.tracks[i].muted = *muted;
                }
            }
            TrackEvent::SoloChanged { guid, soloed } => {
                if let Some(i) = by_guid(&mut self.tracks, guid) {
                    self.tracks[i].soloed = *soloed;
                }
            }
            TrackEvent::ArmChanged { guid, armed } => {
                if let Some(i) = by_guid(&mut self.tracks, guid) {
                    self.tracks[i].armed = *armed;
                }
            }
            TrackEvent::SelectionChanged { guid, selected } => {
                if let Some(i) = by_guid(&mut self.tracks, guid) {
                    self.tracks[i].selected = *selected;
                    // ScrollLink: bank so the newly-selected track is
                    // on a strip.
                    if *selected && self.scroll_link {
                        self.nav.scroll_to(&self.tracks, i, mcu::STRIPS);
                        self.rebind_strips();
                    }
                }
            }
            TrackEvent::AutomationModeChanged { guid, mode } => {
                if let Some(i) = by_guid(&mut self.tracks, guid) {
                    self.tracks[i].automation_mode = *mode;
                }
            }
            TrackEvent::InputMonitorChanged { guid, monitor } => {
                if let Some(i) = by_guid(&mut self.tracks, guid) {
                    self.tracks[i].input_monitor = *monitor;
                }
            }
            TrackEvent::PhaseInvertedChanged { guid, inverted } => {
                if let Some(i) = by_guid(&mut self.tracks, guid) {
                    self.tracks[i].phase_inverted = *inverted;
                }
            }
            TrackEvent::Renamed { guid, name } => {
                if let Some(i) = by_guid(&mut self.tracks, guid) {
                    self.tracks[i].name = name.clone();
                }
            }
            TrackEvent::ColorChanged { guid, color } => {
                if let Some(i) = by_guid(&mut self.tracks, guid) {
                    self.tracks[i].color = *color;
                }
            }
            _ => {}
        }
        false
    }

    pub fn apply_transport_event(&mut self, event: &TransportEvent) {
        match event {
            TransportEvent::PlayStateChanged { play_state, .. } => {
                self.play_state = *play_state;
            }
            TransportEvent::LoopingChanged { looping, .. } => {
                self.looping = *looping;
            }
            TransportEvent::MetronomeChanged { enabled, .. } => {
                self.metronome = *enabled;
            }
            _ => {}
        }
    }

    /// The feedback text for a display action on a strip.
    fn display_text(&self, action: &Action, strip: u8, track: Option<&Track>) -> String {
        let send = self.send_slot(strip);
        match action {
            Action::TrackName => track.map(|t| t.name.clone()).unwrap_or_default(),
            Action::PanDisplay => track.map(|t| pan_label(t.pan)).unwrap_or_default(),
            Action::VolumeDisplay => track.map(|t| volume_label(t.volume)).unwrap_or_default(),
            // Legacy full-cell indicator (older configs) — the
            // glyph-in-name style above replaced it in the builtin.
            Action::FolderIndicator => track
                .map(|t| {
                    if t.is_folder && self.nav.mode == NavMode::Folder {
                        if self.nav.depth() > 0 {
                            "FOLDER>".to_string()
                        } else {
                            "FOLDER".to_string()
                        }
                    } else {
                        pan_label(t.pan)
                    }
                })
                .unwrap_or_default(),
            Action::SendNameDisplay => send.map(|s| s.dest_name.clone()).unwrap_or_default(),
            Action::SendVolumeDisplay => send.map(|s| volume_label(s.volume)).unwrap_or_default(),
            Action::SendPanDisplay => send.map(|s| pan_label(s.pan)).unwrap_or_default(),
            Action::ReceiveNameDisplay => self
                .receive_slot(strip)
                .map(|s| s.dest_name.clone())
                .unwrap_or_default(),
            Action::ReceiveVolumeDisplay => self
                .receive_slot(strip)
                .map(|s| volume_label(s.volume))
                .unwrap_or_default(),
            Action::ReceivePanDisplay => self
                .receive_slot(strip)
                .map(|s| pan_label(s.pan))
                .unwrap_or_default(),
            Action::FxMenuNameDisplay => self
                .fx_slot(strip)
                .map(|f| f.name.clone())
                .unwrap_or_default(),
            Action::FxParamNameDisplay => self
                .param_slot(strip)
                .map(|p| p.name.clone())
                .unwrap_or_default(),
            Action::FxParamValueDisplay => self
                .param_slot(strip)
                .map(|p| {
                    if p.text.is_empty() {
                        format!("{:>4.0}%", p.value * 100.0)
                    } else {
                        p.text.clone()
                    }
                })
                .unwrap_or_default(),
            Action::Fixed { text } => text.clone(),
            _ => String::new(),
        }
    }

    /// The LED state for an action bound to a lit button.
    fn led_state(&self, action: &Action, strip: Option<u8>, track: Option<&Track>) -> bool {
        let send = strip.and_then(|s| self.send_slot(s));
        match action {
            Action::TrackMute => track.map(|t| t.muted).unwrap_or(false),
            Action::TrackSolo => track.map(|t| t.soloed).unwrap_or(false),
            Action::TrackRecordArm => track.map(|t| t.armed).unwrap_or(false),
            Action::TrackSelect | Action::TrackSelectAdditive => {
                track.map(|t| t.selected).unwrap_or(false)
            }
            // Spill affordance: SELECT lights on strips you can drill
            // into (folders / VCA leads). CSI's official zones show
            // selection here instead (their spill is Feedback=No);
            // the lit-when-drillable cue is strictly more useful.
            Action::FolderSpill => track.map(|t| t.is_folder).unwrap_or(false),
            Action::VcaSpill => track.map(|t| t.grouping.vca_lead != 0).unwrap_or(false),
            Action::SendMute => send.map(|s| s.muted).unwrap_or(false),
            Action::ReceiveMute => strip
                .and_then(|s| self.receive_slot(s))
                .map(|s| s.muted)
                .unwrap_or(false),
            Action::TrackTogglePolarity => track.map(|t| t.phase_inverted).unwrap_or(false),
            Action::Flip => self.latched & FLIP != 0,
            Action::Toggle => self.latched & TOGGLE != 0,
            Action::ToggleScrollLink => self.scroll_link,
            Action::TrackAutoMode { mode } => {
                use crate::action::AutoMode as A;
                use daw_proto::AutomationMode as P;
                let want = match mode {
                    A::Trim => P::TrimRead,
                    A::Read => P::Read,
                    A::Touch => P::Touch,
                    A::Write => P::Write,
                    A::Latch => P::Latch,
                };
                self.tracks
                    .iter()
                    .find(|t| t.selected)
                    .map(|t| t.automation_mode == want)
                    .unwrap_or(false)
            }
            Action::ToggleMetronome => self.metronome,
            Action::CycleTimeDisplay => self.beats_display,
            // Bypass LED lit = FX ACTIVE (CSI's FXBypassDisplay).
            Action::FxMenuBypass => strip
                .and_then(|s| self.fx_slot(s))
                .map(|f| f.enabled)
                .unwrap_or(false),
            // Select LED marks the focused FX in the menu zone.
            Action::FxMenuSelect { .. } => {
                strip.is_some_and(|s| self.focused_fx == Some(s as usize))
            }
            Action::Play => matches!(self.play_state, PlayState::Playing | PlayState::Recording),
            Action::Record => matches!(self.play_state, PlayState::Recording),
            Action::Stop => matches!(self.play_state, PlayState::Stopped | PlayState::Paused),
            Action::ToggleLoop => self.looping,
            Action::GoZone { zone } => &self.active_zone == zone,
            _ => false,
        }
    }

    /// Render the full surface state through the shadow, walking the
    /// active zone's bindings. Returns the MIDI messages that
    /// actually need sending.
    pub fn render(&mut self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let Some(zone) = self.zones.zone(&self.active_zone).cloned() else {
            return out;
        };
        let mut colors = [StripColor::Off; mcu::STRIPS];

        for strip in 0..mcu::STRIPS as u8 {
            let track = self.strip_track(strip).cloned();
            let send = self.send_slot(strip).cloned();

            // Motor fader follows the fader binding's value.
            if !self.touched[strip as usize] {
                let pos = match zone.strip_action(self.latched, StripWidget::Fader) {
                    Some(Action::TrackVolume) => track
                        .as_ref()
                        .map(|t| taper::volume_to_fader(t.volume))
                        .unwrap_or(0),
                    Some(Action::SendVolume) => send
                        .as_ref()
                        .map(|s| taper::volume_to_fader(s.volume))
                        .unwrap_or(0),
                    Some(Action::ReceiveVolume) => self
                        .receive_slot(strip)
                        .map(|s| taper::volume_to_fader(s.volume))
                        .unwrap_or(0),
                    Some(Action::FxParam) => self
                        .param_slot(strip)
                        .map(|p| (p.value * 16383.0).round() as u16)
                        .unwrap_or(0),
                    _ => 0,
                };
                self.shadow.fader(&mut out, strip, pos);
            }

            // V-pot ring follows the vpot binding.
            match zone.strip_action(self.latched, StripWidget::VPot) {
                Some(Action::TrackPan) if track.is_some() => self.shadow.ring(
                    &mut out,
                    strip,
                    RingMode::BoostCut,
                    mcu::pan_to_ring(track.as_ref().unwrap().pan),
                    false,
                ),
                Some(Action::ReceivePan) if self.receive_slot(strip).is_some() => {
                    let pan = self.receive_slot(strip).unwrap().pan;
                    self.shadow.ring(
                        &mut out,
                        strip,
                        RingMode::BoostCut,
                        mcu::pan_to_ring(pan),
                        false,
                    )
                }
                Some(Action::SendPan) if send.is_some() => self.shadow.ring(
                    &mut out,
                    strip,
                    RingMode::BoostCut,
                    mcu::pan_to_ring(send.as_ref().unwrap().pan),
                    false,
                ),
                Some(Action::SendVolume) if send.is_some() => self.shadow.ring(
                    &mut out,
                    strip,
                    RingMode::Wrap,
                    ((send.as_ref().unwrap().volume.min(1.0)) * 11.0).round() as u8,
                    false,
                ),
                Some(Action::FxParam) if self.param_slot(strip).is_some() => {
                    let value = self.param_slot(strip).unwrap().value;
                    self.shadow.ring(
                        &mut out,
                        strip,
                        RingMode::Wrap,
                        (value * 11.0).round().max(1.0) as u8,
                        false,
                    )
                }
                _ => self
                    .shadow
                    .ring(&mut out, strip, RingMode::SingleDot, 0, false),
            }

            // Strip button LEDs follow their bound action's state.
            for (widget, button) in [
                (StripWidget::Rec, Button::Rec(strip)),
                (StripWidget::Solo, Button::Solo(strip)),
                (StripWidget::Mute, Button::Mute(strip)),
                (StripWidget::Select, Button::Select(strip)),
            ] {
                let lit = zone
                    .strip_action(self.latched, widget)
                    .map(|a| self.led_state(a, Some(strip), track.as_ref()))
                    .unwrap_or(false);
                self.shadow.led(&mut out, button, lit);
            }

            // LCD rows. While a strip's fader is TOUCHED, the bottom
            // row shows the live volume instead of its bound display
            // — CSI's `Touch+DisplayLower TrackVolumeDisplay` from
            // the official X-Touch zones. In folder mode, folder
            // strips carry a glyph in the bottom row's last cell:
            // `>` = drillable, `<` = spilled parent (SELECT pops) —
            // hidden while the volume readout needs the full row.
            for (widget, row) in [(StripWidget::LcdTop, 0u8), (StripWidget::LcdBottom, 1u8)] {
                let touched_volume = row == 1
                    && self.touched[strip as usize]
                    && matches!(
                        zone.strip_action(self.latched, StripWidget::Fader),
                        Some(Action::TrackVolume)
                    );
                let mut text = if touched_volume {
                    track
                        .as_ref()
                        .map(|t| volume_label(t.volume))
                        .unwrap_or_default()
                } else {
                    zone.strip_action(self.latched, widget)
                        .map(|a| self.display_text(a, strip, track.as_ref()))
                        .unwrap_or_default()
                };
                if row == 1
                    && !touched_volume
                    && let Some(t) = track.as_ref()
                    && self.nav.mode == NavMode::Folder
                    && t.is_folder
                {
                    let icon = if self.nav.current_parent() == Some(t.guid.as_str()) {
                        '<'
                    } else {
                        '>'
                    };
                    // Glyph FIRST — info may be any length, the
                    // drill cue must always be visible.
                    let info: String = text.chars().take(6).collect();
                    text = format!("{icon}{info}");
                }
                self.shadow.lcd(&mut out, strip, row, &text);
            }

            // Zone color override beats per-track colors (CSI's
            // SetXTouchDisplayColors on zone activation).
            colors[strip as usize] = zone.display_color.unwrap_or_else(|| {
                track
                    .as_ref()
                    .map(|t| {
                        t.color
                            .map(mcu::rgb_to_strip_color)
                            .unwrap_or(StripColor::White)
                    })
                    .unwrap_or(StripColor::Off)
            });
        }
        self.shadow.colors(&mut out, colors);

        // Master fader.
        if !self.touched[mcu::MASTER as usize] {
            let pos = match zone.global_action(self.latched, GlobalWidget::MasterFader) {
                Some(Action::MasterVolume) => taper::volume_to_fader(self.master_volume),
                _ => 0,
            };
            self.shadow.fader(&mut out, mcu::MASTER, pos);
        }

        // Master-section button LEDs follow their bound actions —
        // only the bindings on the ACTIVE layer (latched mask);
        // modifier-qualified bindings don't fight the base layer.
        for ((mods, widget), action) in zone.global.iter().map(|(k, v)| (*k, v)) {
            if mods != self.latched {
                continue;
            }
            if let GlobalWidget::Button(button) = widget {
                let lit = self.led_state(action, None, None);
                self.shadow.led(&mut out, button, lit);
            }
        }

        // 7-segment timecode (only when we have a position yet).
        if let Some(text) = self.timecode_text() {
            self.shadow.timecode(&mut out, &text);
        }
        out
    }
}

fn pan_label(pan: f64) -> String {
    if pan.abs() < 0.01 {
        "  C".into()
    } else if pan < 0.0 {
        format!("{:.0}L", pan.abs() * 100.0)
    } else {
        format!("{:.0}R", pan * 100.0)
    }
}

fn volume_label(volume: f64) -> String {
    if volume <= 0.0 {
        "-inf".into()
    } else {
        format!("{:+.1}dB", 20.0 * volume.log10())
    }
}

// ── Async edge: services + hardware ─────────────────────────────────

/// Move the next event out of the event bus's client-filtered
/// `EventStream` (the architect `#[subscribe]` port of the old
/// `Rx`-based `next_owned`).
async fn next_owned_bus(
    rx: &mut daw_control::EventStream<DawEvent>,
) -> eyre::Result<Option<DawEvent>> {
    match rx.recv().await {
        Ok(Some(selfref)) => {
            let mut taken: Option<DawEvent> = None;
            let _ = selfref.map(|value| {
                taken = Some(value);
            });
            Ok(taken)
        }
        Ok(None) => Ok(None),
        Err(e) => Err(eyre::eyre!("event stream error: {e:?}")),
    }
}

/// Fetch the selected track's send slots for the driver cache.
async fn fetch_sends(
    project: &daw_control::Project,
    selected_guid: Option<String>,
) -> Vec<SendSlot> {
    let Some(guid) = selected_guid else {
        return Vec::new();
    };
    let Ok(Some(handle)) = project.tracks().by_guid(&guid).await else {
        return Vec::new();
    };
    match handle.sends().all().await {
        Ok(routes) => routes
            .into_iter()
            .map(|r| SendSlot {
                dest_name: r.dest_track_name.unwrap_or_default(),
                volume: r.volume,
                pan: r.pan,
                muted: r.muted,
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Fetch the selected track's receive slots for the driver cache.
async fn fetch_receives(
    project: &daw_control::Project,
    selected_guid: Option<String>,
) -> Vec<SendSlot> {
    let Some(guid) = selected_guid else {
        return Vec::new();
    };
    let Ok(Some(handle)) = project.tracks().by_guid(&guid).await else {
        return Vec::new();
    };
    match handle.receives().all().await {
        Ok(routes) => routes
            .into_iter()
            .map(|r| SendSlot {
                dest_name: r.dest_track_name.unwrap_or_default(),
                volume: r.volume,
                pan: r.pan,
                muted: r.muted,
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Fetch the selected track's FX chain for the driver cache.
async fn fetch_fx(project: &daw_control::Project, selected_guid: Option<String>) -> Vec<FxSlot> {
    let Some(guid) = selected_guid else {
        return Vec::new();
    };
    let Ok(Some(handle)) = project.tracks().by_guid(&guid).await else {
        return Vec::new();
    };
    match handle.fx_chain().all().await {
        Ok(list) => list
            .into_iter()
            .map(|fx| FxSlot {
                guid: fx.guid,
                name: fx.name,
                enabled: fx.enabled,
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Fetch the focused FX's parameters for the driver cache.
async fn fetch_params(
    project: &daw_control::Project,
    selected_guid: Option<String>,
    fx_idx: Option<usize>,
) -> Vec<ParamSlot> {
    let (Some(guid), Some(fx_idx)) = (selected_guid, fx_idx) else {
        return Vec::new();
    };
    let Ok(Some(handle)) = project.tracks().by_guid(&guid).await else {
        return Vec::new();
    };
    let Ok(Some(fx)) = handle.fx_chain().by_index(fx_idx as u32).await else {
        return Vec::new();
    };
    match fx.parameters().await {
        Ok(params) => params
            .into_iter()
            .map(|p| ParamSlot {
                name: p.name,
                value: p.value,
                text: p.formatted,
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Refresh both FX caches (chain + focused params). Public so
/// headless surfaces / tests can drive the same path as the run loop.
pub async fn refresh_fx_caches(project: &daw_control::Project, state: &mut DriverState) {
    let fx = fetch_fx(project, state.selected_guid()).await;
    state.set_fx(fx);
    let params = fetch_params(project, state.selected_guid(), state.focused_fx).await;
    state.set_params(params);
}

fn now_ms(epoch: std::time::Instant) -> u64 {
    epoch.elapsed().as_millis() as u64
}

/// Run the surface driver until the event stream closes or the
/// surface disconnects. Spawn with `architect::platform::spawn`.
pub async fn run(daw: Daw, config: CsiConfig) -> eyre::Result<()> {
    let mut port = SurfacePort::open(&config.device_match)?;
    let zones = ZoneSet::load()?;
    let epoch = std::time::Instant::now();

    let project = daw.current_project().await?;
    let tracks_api = project.tracks();
    let transport = project.transport();

    let master = tracks_api.master().await?;
    let master_guid = master.guid().to_string();
    let master_volume = master.volume().await.unwrap_or(1.0);
    let tracks = tracks_api.all().await?;

    let mut state = DriverState::new(zones, tracks, master_guid, master_volume);
    state.play_state = transport.get_play_state().await.unwrap_or_default();
    state.looping = transport.is_looping().await.unwrap_or(false);
    state.shadow.invalidate();
    for msg in state.render() {
        port.send(&msg);
    }

    let mut bus = daw
        .events()
        .subscribe(BusFilter {
            tracks: true,
            fx: true,
            transport_state: true,
            transport_position: true,
            projects: true,
            ..Default::default()
        })
        .await?;

    // Periodic tick: double-press flush + meter polling (the bridge
    // decays on its own; ~13Hz keeps it lively without flooding).
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(75));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = tick.tick() => {
                for intent in state.flush_pending(now_ms(epoch)) {
                    if let Err(e) = execute_intent(&project, &transport, intent).await {
                        tracing::warn!("daw-csi: intent failed: {e}");
                    }
                }
                // Meter bridge: post-fader peaks for the visible strips.
                let mut msgs = Vec::new();
                for (strip, idx) in state.visible_track_indices().into_iter().enumerate() {
                    let Some(idx) = idx else { continue };
                    let l = project
                        .track_peak(daw_proto::TrackRef::Index(idx), 0)
                        .await
                        .map(|p| p.peak_db)
                        .unwrap_or(-150.0);
                    let r = project
                        .track_peak(daw_proto::TrackRef::Index(idx), 1)
                        .await
                        .map(|p| p.peak_db)
                        .unwrap_or(-150.0);
                    let db = l.max(r);
                    // −60 dB…0 dB → 0…12 segments.
                    let level = (((db + 60.0) / 5.0).clamp(0.0, 12.0)) as u8;
                    state.shadow.meter(&mut msgs, strip as u8, level);
                }
                for msg in msgs {
                    port.send(&msg);
                }
                for msg in state.render() {
                    port.send(&msg);
                }
            }
            ev = next_owned_bus(&mut bus) => {
                match ev? {
                    None => return Ok(()), // bus closed — host shutting down
                    Some(DawEvent::Track(te)) => {
                        let selection_changed =
                            matches!(te.event, TrackEvent::SelectionChanged { .. });
                        if state.apply_track_event(&te.event) {
                            // List shape changed — refetch + revalidate.
                            state.tracks = tracks_api.all().await?;
                            state.nav.revalidate(&state.tracks);
                            state.rebind_strips();
                        }
                        // Sends / FX zones track the selected track.
                        if selection_changed && state.uses_sends() {
                            let sends = fetch_sends(&project, state.selected_guid()).await;
                            state.set_sends(sends);
                            let receives = fetch_receives(&project, state.selected_guid()).await;
                            state.set_receives(receives);
                        }
                        if selection_changed && state.uses_fx() {
                            state.focused_fx = None;
                            refresh_fx_caches(&project, &mut state).await;
                        }
                    }
                    Some(DawEvent::Fx(fe)) => {
                        if state.apply_fx_event(&fe.event) && state.uses_fx() {
                            // Chain shape changed — refetch.
                            refresh_fx_caches(&project, &mut state).await;
                        }
                    }
                    Some(DawEvent::TransportState(te)) => state.apply_transport_event(&te),
                    Some(DawEvent::TransportPosition(tick)) => {
                        state.apply_position(&tick.playhead);
                    }
                    Some(_) => {}
                }
                for msg in state.render() {
                    port.send(&msg);
                }
            }
            raw = port.rx.recv() => {
                let Some(raw) = raw else {
                    return Err(eyre::eyre!("surface MIDI input closed"));
                };
                let intents = state.handle_midi(&raw, now_ms(epoch));
                let mut refresh_param_text = false;
                for intent in intents {
                    match &intent {
                        Intent::RefreshSends => {
                            let sends = fetch_sends(&project, state.selected_guid()).await;
                            state.set_sends(sends);
                            let receives = fetch_receives(&project, state.selected_guid()).await;
                            state.set_receives(receives);
                            continue;
                        }
                        Intent::RefreshFx => {
                            refresh_fx_caches(&project, &mut state).await;
                            continue;
                        }
                        // The plugin formats the display text — refetch
                        // it after the value lands.
                        Intent::SetFxParam { .. } => refresh_param_text = true,
                        _ => {}
                    }
                    if let Err(e) = execute_intent(&project, &transport, intent).await {
                        tracing::warn!("daw-csi: intent failed: {e}");
                    }
                }
                if refresh_param_text {
                    let params =
                        fetch_params(&project, state.selected_guid(), state.focused_fx).await;
                    state.set_params(params);
                }
                for msg in state.render() {
                    port.send(&msg);
                }
            }
        }
    }
}

#[derive(PartialEq)]
enum MarkerJump {
    None,
    Prev,
    Next,
}

/// Execute one [`Intent`] against the daw-control services. Public so
/// headless surfaces / tests can drive the same path as the hardware
/// loop. (`RefreshSends` is a no-op here — it needs `DriverState`
/// access, so the run loop handles it inline.)
pub async fn execute_intent(
    project: &daw_control::Project,
    transport: &daw_control::Transport,
    intent: Intent,
) -> eyre::Result<()> {
    let intent_kind = match &intent {
        Intent::NextMarker => MarkerJump::Next,
        Intent::PrevMarker => MarkerJump::Prev,
        _ => MarkerJump::None,
    };
    match intent {
        Intent::SetVolume { guid, volume } => {
            // Automation-aware: in touch/latch/write modes while
            // playing this records envelope points (the engine also
            // updates the static fader value).
            let location = daw_proto::automation::EnvelopeLocation::new(
                daw_proto::TrackRef::Guid(guid.clone()),
                daw_proto::automation::EnvelopeRef::Type(daw_proto::EnvelopeType::Volume),
            );
            if project.automation_write(location, volume).await.is_err()
                && let Some(h) = project.tracks().by_guid(&guid).await?
            {
                h.set_volume(volume).await?;
            }
        }
        Intent::SetMasterVolume { volume } => {
            project.tracks().master().await?.set_volume(volume).await?;
        }
        Intent::SetPan { guid, pan } => {
            if let Some(h) = project.tracks().by_guid(&guid).await? {
                h.set_pan(pan).await?;
            }
        }
        Intent::SetMuted { guid, muted } => {
            if let Some(h) = project.tracks().by_guid(&guid).await? {
                if muted {
                    h.mute().await?
                } else {
                    h.unmute().await?
                }
            }
        }
        Intent::SetSoloed { guid, soloed } => {
            if let Some(h) = project.tracks().by_guid(&guid).await? {
                if soloed {
                    h.solo().await?
                } else {
                    h.unsolo().await?
                }
            }
        }
        Intent::SetArmed { guid, armed } => {
            if let Some(h) = project.tracks().by_guid(&guid).await? {
                if armed {
                    h.arm().await?
                } else {
                    h.disarm().await?
                }
            }
        }
        Intent::SelectExclusive { guid } => {
            if let Some(h) = project.tracks().by_guid(&guid).await? {
                h.select_exclusive().await?;
            }
        }
        Intent::SelectAdditive { guid } => {
            if let Some(h) = project.tracks().by_guid(&guid).await? {
                h.select().await?;
            }
        }
        Intent::ClearAllSolo => project.tracks().clear_solo().await?,
        Intent::UnmuteAll => project.tracks().unmute_all().await?,
        Intent::SetSendVolume {
            guid,
            index,
            volume,
        } => {
            if let Some(h) = project.tracks().by_guid(&guid).await?
                && let Some(send) = h.sends().by_index(index).await?
            {
                send.set_volume(volume).await?;
            }
        }
        Intent::SetSendPan { guid, index, pan } => {
            if let Some(h) = project.tracks().by_guid(&guid).await?
                && let Some(send) = h.sends().by_index(index).await?
            {
                send.set_pan(pan).await?;
            }
        }
        Intent::SetSendMuted { guid, index, muted } => {
            if let Some(h) = project.tracks().by_guid(&guid).await?
                && let Some(send) = h.sends().by_index(index).await?
            {
                if muted {
                    send.mute().await?;
                } else {
                    send.unmute().await?;
                }
            }
        }
        Intent::SetFxParam {
            guid,
            fx_idx,
            param_idx,
            value,
        } => {
            if let Some(h) = project.tracks().by_guid(&guid).await?
                && let Some(fx) = h.fx_chain().by_index(fx_idx).await?
            {
                fx.param(param_idx).set(value).await?;
            }
        }
        Intent::SetFxEnabled {
            guid,
            fx_idx,
            enabled,
        } => {
            if let Some(h) = project.tracks().by_guid(&guid).await?
                && let Some(fx) = h.fx_chain().by_index(fx_idx).await?
            {
                if enabled {
                    fx.enable().await?;
                } else {
                    fx.disable().await?;
                }
            }
        }
        Intent::SetPhaseInverted { guid, inverted } => {
            if let Some(h) = project.tracks().by_guid(&guid).await? {
                h.set_phase_inverted(inverted).await?;
            }
        }
        Intent::SelectMany { guids } => {
            for guid in guids {
                if let Some(h) = project.tracks().by_guid(&guid).await? {
                    h.select().await?;
                }
            }
        }
        Intent::SetReceiveVolume {
            guid,
            index,
            volume,
        } => {
            if let Some(h) = project.tracks().by_guid(&guid).await?
                && let Some(r) = h.receives().by_index(index).await?
            {
                r.set_volume(volume).await?;
            }
        }
        Intent::SetReceivePan { guid, index, pan } => {
            if let Some(h) = project.tracks().by_guid(&guid).await?
                && let Some(r) = h.receives().by_index(index).await?
            {
                r.set_pan(pan).await?;
            }
        }
        Intent::SetReceiveMuted { guid, index, muted } => {
            if let Some(h) = project.tracks().by_guid(&guid).await?
                && let Some(r) = h.receives().by_index(index).await?
            {
                if muted {
                    r.mute().await?;
                } else {
                    r.unmute().await?;
                }
            }
        }
        Intent::FxNextPreset { guid, fx_idx } => {
            if let Some(h) = project.tracks().by_guid(&guid).await?
                && let Some(fx) = h.fx_chain().by_index(fx_idx).await?
            {
                fx.next_preset().await?;
            }
        }
        Intent::FxPrevPreset { guid, fx_idx } => {
            if let Some(h) = project.tracks().by_guid(&guid).await?
                && let Some(fx) = h.fx_chain().by_index(fx_idx).await?
            {
                fx.prev_preset().await?;
            }
        }
        Intent::SetAutomationMode { guid, mode } => {
            if let Some(h) = project.tracks().by_guid(&guid).await? {
                h.set_automation_mode(mode).await?;
            }
        }
        Intent::SetInputMonitor { guid, monitor } => {
            if let Some(h) = project.tracks().by_guid(&guid).await? {
                h.set_input_monitor(monitor).await?;
            }
        }
        Intent::ToggleMetronome => {
            let on = transport.metronome_enabled().await?;
            transport.set_metronome(!on).await?;
        }
        Intent::AutomationTouch { guid, touched } => {
            let location = daw_proto::automation::EnvelopeLocation::new(
                daw_proto::TrackRef::Guid(guid),
                daw_proto::automation::EnvelopeRef::Type(daw_proto::EnvelopeType::Volume),
            );
            if touched {
                project.automation_touch(location).await?;
            } else {
                project.automation_release(location).await?;
            }
        }
        Intent::SaveProject => project.save().await?,
        Intent::Undo => {
            project.undo().await?;
        }
        Intent::Redo => {
            project.redo().await?;
        }
        Intent::RunCommand { id } => {
            project.run_command(&id).await?;
        }
        Intent::AddMarker => {
            let pos = transport.get_position().await?;
            project.markers().add(pos, "").await?;
        }
        Intent::PrevMarker | Intent::NextMarker => {
            let forward = matches!(intent_kind, MarkerJump::Next);
            let pos = transport.get_position().await?;
            let markers = project.markers().all().await?;
            let seconds = |m: &daw_proto::Marker| m.position.time.map(|t| t.as_seconds());
            let target = if forward {
                markers
                    .iter()
                    .filter_map(&seconds)
                    .filter(|&t| t > pos + 1e-6)
                    .min_by(f64::total_cmp)
            } else {
                markers
                    .iter()
                    .filter_map(seconds)
                    .filter(|&t| t < pos - 1e-6)
                    .max_by(f64::total_cmp)
            };
            if let Some(t) = target {
                transport.set_position(t).await?;
            }
        }
        Intent::Play => transport.play().await?,
        Intent::Stop => transport.stop().await?,
        Intent::Record => transport.record().await?,
        Intent::StopRecording => transport.stop_recording().await?,
        Intent::ToggleLoop => transport.toggle_loop().await?,
        Intent::NudgePosition { seconds } => {
            let pos = transport.get_position().await?;
            transport.set_position((pos + seconds).max(0.0)).await?;
        }
        // Navigation / cache refreshes are applied by the run loop;
        // nothing to do service-side.
        Intent::Refresh | Intent::RefreshSends | Intent::RefreshFx => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(guid: &str, name: &str, parent: Option<&str>, is_folder: bool) -> Track {
        Track {
            guid: guid.into(),
            name: name.into(),
            volume: 1.0,
            parent_guid: parent.map(Into::into),
            is_folder,
            ..Default::default()
        }
    }

    fn state() -> DriverState {
        DriverState::with_builtin_zones(
            vec![
                track("drums", "DRUMS", None, true),
                track("kick", "Kick", Some("drums"), false),
                track("snare", "Snare", Some("drums"), false),
                track("bass", "Bass", None, false),
            ],
            "master".into(),
            1.0,
        )
    }

    /// Tap = press+release inside the hold window.
    fn tap(s: &mut DriverState, note: u8, t: u64) -> Vec<Intent> {
        let mut out = s.handle_midi(&[0x90, note, 0x7F], t);
        out.extend(s.handle_midi(&[0x90, note, 0x00], t + 50));
        out
    }

    #[test]
    fn fader_gesture_sets_volume_and_caches() {
        let mut s = state();
        let raw = mcu::encode_fader(0, taper::volume_to_fader(1.0));
        let intents = s.handle_midi(&raw, 0);
        assert_eq!(intents.len(), 1);
        let Intent::SetVolume { guid, volume } = &intents[0] else {
            panic!("expected SetVolume, got {intents:?}");
        };
        assert_eq!(guid, "drums");
        assert!((volume - 1.0).abs() < 1e-3);
        // Cache updated immediately → echo event diffs clean.
        assert!((s.tracks[0].volume - 1.0).abs() < 1e-3);
    }

    #[test]
    fn master_fader_routes_to_master() {
        let mut s = state();
        let raw = mcu::encode_fader(mcu::MASTER, 0);
        assert_eq!(
            s.handle_midi(&raw, 0),
            vec![Intent::SetMasterVolume { volume: 0.0 }]
        );
    }

    #[test]
    fn mute_button_toggles_from_cache() {
        let mut s = state();
        assert_eq!(
            tap(&mut s, 0x10, 0),
            vec![Intent::SetMuted {
                guid: "drums".into(),
                muted: true
            }]
        );
        // After the echo event lands, the next press unmutes.
        s.apply_track_event(&TrackEvent::MuteChanged {
            guid: "drums".into(),
            muted: true,
        });
        assert_eq!(
            tap(&mut s, 0x10, 1000),
            vec![Intent::SetMuted {
                guid: "drums".into(),
                muted: false
            }]
        );
    }

    #[test]
    fn zone_hop_changes_select_semantics() {
        let mut s = state();
        // Folder mode is the default home base now.
        assert_eq!(s.active_zone, "folder");
        assert_eq!(s.nav.mode, NavMode::Folder);
        // Root: [DRUMS, Bass]. Select strip 0 (DRUMS folder) → drill.
        assert_eq!(tap(&mut s, 0x18, 100), vec![Intent::Refresh]);
        // Spill: [DRUMS, Kick, Snare] — strip 1 is Kick now; selecting
        // it is a plain selection, not navigation.
        assert_eq!(
            tap(&mut s, 0x19, 200),
            vec![Intent::SelectExclusive {
                guid: "kick".into()
            }]
        );
        // Select DRUMS (strip 0, current spill parent) again → pop.
        assert_eq!(tap(&mut s, 0x18, 300), vec![Intent::Refresh]);
        assert_eq!(s.nav.depth(), 0);
        // GlobalView hops to the flat track list and back.
        assert_eq!(
            tap(&mut s, 0x33, 400),
            vec![Intent::Refresh, Intent::RefreshSends]
        );
        assert_eq!(s.active_zone, "home");
        assert_eq!(s.nav.mode, NavMode::Track);
        assert_eq!(
            tap(&mut s, 0x33, 500),
            vec![Intent::Refresh, Intent::RefreshSends]
        );
        assert_eq!(s.active_zone, "folder");
    }

    #[test]
    fn banking_rebinds_strips() {
        // More tracks than strips, otherwise banking correctly clamps.
        let tracks = (0..12)
            .map(|i| track(&format!("t{i}"), &format!("Track {i}"), None, false))
            .collect();
        let mut s = DriverState::with_builtin_zones(tracks, "master".into(), 1.0);
        // Plain tracks live in the flat list — hop out of folder mode.
        tap(&mut s, 0x33, 0);
        assert_eq!(tap(&mut s, 0x31, 0), vec![Intent::Refresh]); // chan right
        // Strip 0 now shows track 1.
        let intents = s.handle_midi(&mcu::encode_fader(0, 16383), 100);
        let Intent::SetVolume { guid, .. } = &intents[0] else {
            panic!();
        };
        assert_eq!(guid, "t1");
        // Bank right (+8) → strip 0 shows t4 (offset clamped to 12−8).
        assert_eq!(tap(&mut s, 0x2F, 200), vec![Intent::Refresh]);
        let intents = s.handle_midi(&mcu::encode_fader(0, 16383), 300);
        let Intent::SetVolume { guid, .. } = &intents[0] else {
            panic!();
        };
        assert_eq!(guid, "t4");
    }

    #[test]
    fn touch_suppresses_motor_feedback() {
        let mut s = state();
        let _ = s.render(); // settle shadow
        // Touch strip 0, then a volume event for its track arrives.
        s.handle_midi(&[0x90, 0x68, 0x7F], 0);
        s.apply_track_event(&TrackEvent::VolumeChanged {
            guid: "drums".into(),
            volume: 0.5,
        });
        let msgs = s.render();
        assert!(
            !msgs.iter().any(|m| m[0] == 0xE0),
            "motor fader moved while touched: {msgs:?}"
        );
        // Release → next render moves the motor.
        s.handle_midi(&[0x90, 0x68, 0x00], 100);
        let msgs = s.render();
        assert!(msgs.iter().any(|m| m[0] == 0xE0));
    }

    #[test]
    fn render_full_surface_then_diffs_empty() {
        let mut s = state();
        // Flat track list so every demo track sits on a strip.
        tap(&mut s, 0x33, 0);
        let first = s.render();
        assert!(!first.is_empty());
        // LCD for strip 0 contains the track name.
        assert!(
            first
                .iter()
                .any(|m| m.len() == 15 && m[5] == 0x12 && &m[7..12] == b"DRUMS")
        );
        // Steady state: nothing changed → nothing sent.
        assert!(s.render().is_empty());
        // One event → only that strip's messages.
        s.apply_track_event(&TrackEvent::PanChanged {
            guid: "bass".into(),
            pan: -1.0,
        });
        let diff = s.render();
        assert!(
            diff.iter().all(|m| m[0] != 0xE0),
            "pan change moved a fader"
        );
        assert!(!diff.is_empty());
    }

    #[test]
    fn transport_buttons() {
        let mut s = state();
        assert_eq!(tap(&mut s, 0x5E, 0), vec![Intent::Play]);
        assert_eq!(tap(&mut s, 0x5D, 100), vec![Intent::Stop]);
        assert_eq!(tap(&mut s, 0x56, 200), vec![Intent::ToggleLoop]);
        assert_eq!(tap(&mut s, 0x5F, 300), vec![Intent::Record]);
        s.play_state = PlayState::Recording;
        assert_eq!(tap(&mut s, 0x5F, 400), vec![Intent::StopRecording]);
    }

    #[test]
    fn modifier_bindings_dispatch() {
        // Custom zone set: shift+mute = record-arm.
        let zones = ZoneSet::parse(
            r#"
zones {
    home {
        strip {
            mute @TrackMute
            shift+mute @TrackRecordArm
        }
    }
}
"#,
        )
        .unwrap();
        let mut s = DriverState::new(
            zones,
            vec![track("a", "A", None, false)],
            "master".into(),
            1.0,
        );
        // Plain mute.
        assert_eq!(
            tap(&mut s, 0x10, 0),
            vec![Intent::SetMuted {
                guid: "a".into(),
                muted: true
            }]
        );
        // Hold shift → same button arms instead.
        s.handle_midi(&[0x90, 0x46, 0x7F], 100); // shift down
        assert_eq!(
            tap(&mut s, 0x10, 150),
            vec![Intent::SetArmed {
                guid: "a".into(),
                armed: true
            }]
        );
        s.handle_midi(&[0x90, 0x46, 0x00], 300); // shift up
        assert_eq!(
            tap(&mut s, 0x10, 400),
            vec![Intent::SetMuted {
                guid: "a".into(),
                muted: true
            }]
        );
    }

    #[test]
    fn hold_binding_defers_and_resolves() {
        // CSI's official Folder.zon shape: tap = select, hold = spill.
        let zones = ZoneSet::parse(
            r#"
zones {
    home {
        navigator @Folder
        strip {
            select @TrackSelect
            hold+select @FolderSpill
        }
    }
}
"#,
        )
        .unwrap();
        let mut s = DriverState::new(
            zones,
            vec![
                track("drums", "DRUMS", None, true),
                track("kick", "Kick", Some("drums"), false),
                track("bass", "Bass", None, false),
            ],
            "master".into(),
            1.0,
        );
        // Press alone yields nothing (deferred for hold resolution).
        assert!(s.handle_midi(&[0x90, 0x18, 0x7F], 0).is_empty());
        // Quick release → tap action (select).
        assert_eq!(
            s.handle_midi(&[0x90, 0x18, 0x00], 100),
            vec![Intent::SelectExclusive {
                guid: "drums".into()
            }]
        );
        // Press + long release → hold action (folder spill).
        assert!(s.handle_midi(&[0x90, 0x18, 0x7F], 1000).is_empty());
        assert_eq!(
            s.handle_midi(&[0x90, 0x18, 0x00], 1000 + HOLD_MS),
            vec![Intent::Refresh]
        );
        assert_eq!(s.nav.depth(), 1);
    }

    #[test]
    fn sends_zone_maps_strips_to_send_slots() {
        let zones = ZoneSet::parse(
            r#"
zones {
    home {
        strip {select @TrackSelect}
        buttons {global_view @GoZone{zone sends}}
    }
    sends {
        display_color @Cyan
        strip {
            fader @SendVolume
            vpot @SendPan
            mute @SendMute
            lcd_top @SendNameDisplay
            lcd_bottom @SendVolumeDisplay
        }
        buttons {global_view @GoZone{zone home}}
    }
}
"#,
        )
        .unwrap();
        let mut tracks = vec![
            track("a", "A", None, false),
            track("verb", "Verb", None, false),
        ];
        tracks[0].selected = true;
        let mut s = DriverState::new(zones, tracks, "master".into(), 1.0);

        // Enter the sends zone; async edge would now refetch sends.
        assert_eq!(
            tap(&mut s, 0x33, 0),
            vec![Intent::Refresh, Intent::RefreshSends]
        );
        s.set_sends(vec![SendSlot {
            dest_name: "Verb".into(),
            volume: 0.5,
            pan: 0.0,
            muted: false,
        }]);
        assert!(s.uses_sends());

        // Fader on strip 0 = send 0 volume of the selected track.
        let intents = s.handle_midi(&mcu::encode_fader(0, taper::volume_to_fader(1.0)), 100);
        assert_eq!(
            intents,
            vec![Intent::SetSendVolume {
                guid: "a".into(),
                index: 0,
                volume: taper::fader_to_volume(taper::volume_to_fader(1.0)),
            }]
        );
        // Strip 1 has no send slot → inert.
        assert!(s.handle_midi(&mcu::encode_fader(1, 8000), 200).is_empty());

        // Mute toggles the slot.
        assert_eq!(
            tap(&mut s, 0x10, 300),
            vec![Intent::SetSendMuted {
                guid: "a".into(),
                index: 0,
                muted: true
            }]
        );

        // Render: zone color override + send name on the LCD.
        s.shadow.invalidate();
        let msgs = s.render();
        assert!(
            msgs.iter()
                .any(|m| m.len() == 15 && m[5] == 0x12 && &m[7..11] == b"Verb"),
            "send name missing from LCD: {msgs:?}"
        );
        let color_msg = msgs.iter().find(|m| m.len() == 15 && m[5] == 0x72).unwrap();
        assert!(
            color_msg[6..14]
                .iter()
                .all(|&c| c == StripColor::Cyan as u8),
            "zone color override not applied: {color_msg:?}"
        );
    }

    #[test]
    fn vca_zone_spills_followers() {
        use daw_proto::track::TrackGrouping;
        let zones = ZoneSet::parse(
            r#"
zones {
    home {
        navigator @Vca
        strip {
            select @TrackSelect
            hold+select @VcaSpill
        }
    }
}
"#,
        )
        .unwrap();
        let mut lead = track("vca", "BAND VCA", None, false);
        lead.grouping = TrackGrouping {
            vca_lead: 0b10,
            ..Default::default()
        };
        let mut f1 = track("gtr", "Gtr", None, false);
        f1.grouping = TrackGrouping {
            vca_follow: 0b10,
            ..Default::default()
        };
        let plain = track("talk", "Talk", None, false);
        let mut s = DriverState::new(zones, vec![lead, f1, plain], "master".into(), 1.0);

        // VCA root: only the lead is visible.
        assert_eq!(s.strip_track(0).map(|t| t.guid.as_str()), Some("vca"));
        assert!(s.strip_track(1).is_none());

        // Hold select on the lead → spill [lead, follower].
        s.handle_midi(&[0x90, 0x18, 0x7F], 0);
        assert_eq!(
            s.handle_midi(&[0x90, 0x18, 0x00], HOLD_MS),
            vec![Intent::Refresh]
        );
        assert_eq!(s.strip_track(0).map(|t| t.guid.as_str()), Some("vca"));
        assert_eq!(s.strip_track(1).map(|t| t.guid.as_str()), Some("gtr"));

        // Hold select on the lead again → pop back to the lead list.
        s.handle_midi(&[0x90, 0x18, 0x7F], 2000);
        assert_eq!(
            s.handle_midi(&[0x90, 0x18, 0x00], 2000 + HOLD_MS),
            vec![Intent::Refresh]
        );
        assert!(s.strip_track(1).is_none());
    }

    #[test]
    fn folder_mode_select_leds_mark_drillable_strips() {
        let mut s = state(); // DRUMS(folder){Kick,Snare}, Bass
        // Folder mode is the boot default.
        assert_eq!(s.active_zone, "folder");
        s.shadow.invalidate();
        let msgs = s.render();
        // Root shows only DRUMS — strip 0's SELECT LED lit (drillable),
        // strip 1 dark (empty).
        let led = |msgs: &[Vec<u8>], note: u8| {
            msgs.iter()
                .find(|m| m[0] == 0x90 && m[1] == note)
                .map(|m| m[2] > 0)
        };
        assert_eq!(
            led(&msgs, 0x18),
            Some(true),
            "folder strip should light SELECT"
        );
        assert_eq!(led(&msgs, 0x19), Some(false), "empty strip dark");

        // Drill in: spill = [DRUMS, Kick, Snare] — parent still lit
        // (toggling exits), children dark (plain tracks).
        tap(&mut s, 0x18, 100);
        s.shadow.invalidate();
        let msgs = s.render();
        assert_eq!(led(&msgs, 0x18), Some(true), "spilled parent stays lit");
        assert_eq!(led(&msgs, 0x19), Some(false), "child (Kick) not drillable");
    }

    #[test]
    fn folder_glyphs_and_touch_volume_display() {
        let mut s = state(); // boots in folder mode
        s.shadow.invalidate();
        let msgs = s.render();
        // Cell content of strip 0's bottom row (LCD offset 56).
        let strip0_bottom = |msgs: &[Vec<u8>]| -> Option<Vec<u8>> {
            msgs.iter()
                .find(|m| m.len() == 15 && m[5] == 0x12 && m[6] == 56)
                .map(|m| m[7..14].to_vec())
        };
        let lcd_with = |msgs: &[Vec<u8>], text: &[u8]| {
            msgs.iter()
                .any(|m| m.len() == 15 && m[5] == 0x12 && m[7..7 + text.len()] == *text)
        };
        // Root: name row is the plain name; the bottom row carries
        // pan info + the drillable glyph in the last cell.
        assert!(lcd_with(&msgs, b"DRUMS"), "plain name missing");
        assert_eq!(
            strip0_bottom(&msgs).as_deref(),
            Some(b">  C   ".as_ref()),
            "bottom row should be pan + drill glyph"
        );

        // Drill in → parent's glyph flips to `<` (press to go back).
        tap(&mut s, 0x18, 0);
        s.shadow.invalidate();
        let msgs = s.render();
        assert_eq!(
            strip0_bottom(&msgs).as_deref(),
            Some(b"<  C   ".as_ref()),
            "spilled parent should show the back glyph"
        );
        // Children show plain names + glyph-less pan.
        assert!(lcd_with(&msgs, b"Kick"), "child name missing");

        // Touch strip 0's fader → bottom row flips to live volume
        // (glyph yields the cell to the readout).
        s.handle_midi(&[0x90, 0x68, 0x7F], 100);
        let msgs = s.render();
        assert_eq!(
            strip0_bottom(&msgs).as_deref(),
            Some(b"+0.0dB ".as_ref()),
            "touched strip should show volume"
        );
        // Release → back to pan + glyph.
        s.handle_midi(&[0x90, 0x68, 0x00], 200);
        let msgs = s.render();
        assert_eq!(
            strip0_bottom(&msgs).as_deref(),
            Some(b"<  C   ".as_ref()),
            "release should restore pan + glyph"
        );
    }

    #[test]
    fn fx_menu_focus_and_param_flow() {
        // Builtin set: assign_plugin (0x2B) hops to the FX menu;
        // select focuses an FX into the param zone.
        let mut tracks = vec![track("a", "A", None, false)];
        tracks[0].selected = true;
        let mut s = DriverState::with_builtin_zones(tracks, "master".into(), 1.0);

        assert_eq!(
            tap(&mut s, 0x2B, 0),
            vec![Intent::Refresh, Intent::RefreshSends]
        );
        assert_eq!(s.active_zone, "fxmenu");
        assert!(s.uses_fx());
        // Async edge would now run RefreshFx; simulate the fetch.
        s.set_fx(vec![
            FxSlot {
                guid: "fx0".into(),
                name: "FTS EQ".into(),
                enabled: true,
            },
            FxSlot {
                guid: "fx1".into(),
                name: "FTS Comp".into(),
                enabled: false,
            },
        ]);

        // LCDs show FX names; mute LED = enabled state.
        s.shadow.invalidate();
        let msgs = s.render();
        assert!(
            msgs.iter()
                .any(|m| m.len() == 15 && m[5] == 0x12 && &m[7..13] == b"FTS EQ"),
            "fx name missing: {msgs:?}"
        );

        // Bypass strip 1 (currently disabled → enable).
        assert_eq!(
            tap(&mut s, 0x11, 100),
            vec![Intent::SetFxEnabled {
                guid: "a".into(),
                fx_idx: 1,
                enabled: true
            }]
        );

        // Select strip 0 → focus + hop to the param zone.
        assert_eq!(
            tap(&mut s, 0x18, 200),
            vec![Intent::Refresh, Intent::RefreshFx]
        );
        assert_eq!(s.active_zone, "fx");
        assert_eq!(s.focused_fx, Some(0));
        s.set_params(vec![
            ParamSlot {
                name: "Freq".into(),
                value: 0.5,
                text: "1.0 kHz".into(),
            },
            ParamSlot {
                name: "Gain".into(),
                value: 0.25,
                text: "-6.0 dB".into(),
            },
        ]);

        // V-pot strip 1 nudges Gain.
        let intents = s.handle_midi(&[0xB0, 0x11, 0x01], 300);
        assert_eq!(
            intents,
            vec![Intent::SetFxParam {
                guid: "a".into(),
                fx_idx: 0,
                param_idx: 1,
                value: 0.26
            }]
        );
        // Cache updated for instant ring feedback.
        assert!((s.params[1].value - 0.26).abs() < 1e-9);

        // Fader strip 0 sets Freq absolutely.
        let intents = s.handle_midi(&mcu::encode_fader(0, 16383), 400);
        assert_eq!(
            intents,
            vec![Intent::SetFxParam {
                guid: "a".into(),
                fx_idx: 0,
                param_idx: 0,
                value: 1.0
            }]
        );

        // Param-zone render shows names + plugin-formatted values.
        s.shadow.invalidate();
        let msgs = s.render();
        assert!(
            msgs.iter()
                .any(|m| m.len() == 15 && m[5] == 0x12 && &m[7..11] == b"Gain"),
            "param name missing: {msgs:?}"
        );
        assert!(
            msgs.iter()
                .any(|m| m.len() == 15 && m[5] == 0x12 && &m[7..14] == b"-6.0 dB"),
            "formatted value missing: {msgs:?}"
        );

        // Echo from the bus updates the cached value.
        s.apply_fx_event(&daw_proto::FxEvent::ParameterChanged {
            context: daw_proto::FxChainContext::Track("a".into()),
            fx_guid: "fx0".into(),
            param_index: 1,
            value: 0.9,
        });
        assert!((s.params[1].value - 0.9).abs() < 1e-9);

        // assign_plugin pops back to the menu.
        assert_eq!(
            tap(&mut s, 0x2B, 500),
            vec![Intent::Refresh, Intent::RefreshSends]
        );
        assert_eq!(s.active_zone, "fxmenu");
    }

    #[test]
    fn double_press_resolves_via_flush() {
        let zones = ZoneSet::parse(
            r#"
zones {
    home {
        strip {
            select @TrackSelect
            double+select @TrackSelectAdditive
        }
    }
}
"#,
        )
        .unwrap();
        let mut s = DriverState::new(
            zones,
            vec![track("a", "A", None, false)],
            "master".into(),
            1.0,
        );
        // Single tap: defers through the double window…
        assert!(s.handle_midi(&[0x90, 0x18, 0x7F], 0).is_empty());
        assert!(s.handle_midi(&[0x90, 0x18, 0x00], 50).is_empty());
        // …nothing until the window expires via the periodic flush.
        assert!(s.flush_pending(100).is_empty());
        assert_eq!(
            s.flush_pending(50 + DOUBLE_MS),
            vec![Intent::SelectExclusive { guid: "a".into() }]
        );
        // Double tap: second press inside the window fires the
        // double binding immediately.
        assert!(s.handle_midi(&[0x90, 0x18, 0x7F], 1000).is_empty());
        assert!(s.handle_midi(&[0x90, 0x18, 0x00], 1050).is_empty());
        assert_eq!(
            s.handle_midi(&[0x90, 0x18, 0x7F], 1100),
            vec![Intent::SelectAdditive { guid: "a".into() }]
        );
        // The pending single must not also fire later.
        assert!(s.flush_pending(5000).is_empty());
    }

    #[test]
    fn flip_layer_swaps_fader_binding() {
        let zones = ZoneSet::parse(
            r#"
zones {
    home {
        strip {
            fader @TrackVolume
            flip+fader @TrackPan
        }
        buttons {flip @Flip}
    }
}
"#,
        )
        .unwrap();
        let mut s = DriverState::new(
            zones,
            vec![track("a", "A", None, false)],
            "master".into(),
            1.0,
        );
        // Base layer: fader writes volume.
        let intents = s.handle_midi(&mcu::encode_fader(0, 16383), 0);
        assert!(matches!(intents[0], Intent::SetVolume { .. }));
        // Latch FLIP → the fader binding swaps to pan… but TrackPan
        // ignores Fader gestures (it's a Delta action), so the move
        // is inert — exactly the binding swap, no crash.
        assert_eq!(
            s.handle_midi(&[0x90, 0x32, 0x7F], 100),
            vec![Intent::Refresh]
        );
        assert!(s.handle_midi(&mcu::encode_fader(0, 0), 200).is_empty());
        // Unlatch → volume again.
        assert_eq!(
            s.handle_midi(&[0x90, 0x32, 0x7F], 300),
            vec![Intent::Refresh]
        );
        let intents = s.handle_midi(&mcu::encode_fader(0, 0), 400);
        assert!(matches!(intents[0], Intent::SetVolume { .. }));
    }

    #[test]
    fn polarity_toggle_and_led() {
        let mut s = state();
        tap(&mut s, 0x33, 0); // flat list
        // control+rec on strip 0 toggles polarity.
        s.handle_midi(&[0x90, 0x48, 0x7F], 100); // control down
        assert_eq!(
            tap(&mut s, 0x00, 150),
            vec![Intent::SetPhaseInverted {
                guid: "drums".into(),
                inverted: true
            }]
        );
        s.handle_midi(&[0x90, 0x48, 0x00], 300); // control up
        // Echo applies to the cache.
        s.apply_track_event(&TrackEvent::PhaseInvertedChanged {
            guid: "drums".into(),
            inverted: true,
        });
        assert!(s.tracks[0].phase_inverted);
    }

    #[test]
    fn timecode_modes_render_digits() {
        let mut s = state();
        s.apply_position(&daw_proto::Position {
            time: Some(daw_proto::primitives::PositionInSeconds::from_seconds(
                75.25,
            )),
            musical: Some(daw_proto::MusicalPosition {
                measure: 3,
                beat: 2,
                subdivision: 480,
            }),
            midi: None,
        });
        s.shadow.invalidate();
        let msgs = s.render();
        // Time mode: "0.01.15.25" → digits land on CC 0x40-0x49.
        let digits: Vec<&Vec<u8>> = msgs
            .iter()
            .filter(|m| m.len() == 3 && m[0] == 0xB0 && (0x40..0x4A).contains(&m[1]))
            .collect();
        assert!(!digits.is_empty(), "no timecode digits: {msgs:?}");
        // Cycle to beats and confirm the readout changes.
        let before: Vec<Vec<u8>> = digits.into_iter().cloned().collect();
        tap(&mut s, 0x35, 100); // smpte/beats button
        let msgs = s.render();
        let after: Vec<Vec<u8>> = msgs
            .iter()
            .filter(|m| m.len() == 3 && m[0] == 0xB0 && (0x40..0x4A).contains(&m[1]))
            .cloned()
            .collect();
        assert!(
            !after.is_empty() && before != after,
            "mode cycle changed nothing"
        );
    }

    #[test]
    fn unbound_widget_is_inert() {
        // Zone with ONLY a fader binding — buttons do nothing.
        let zones = ZoneSet::parse(
            r#"
zones {
    home {
        strip {fader @TrackVolume}
    }
}
"#,
        )
        .unwrap();
        let mut s = DriverState::new(
            zones,
            vec![track("a", "A", None, false)],
            "master".into(),
            1.0,
        );
        assert!(tap(&mut s, 0x10, 0).is_empty()); // mute
        assert!(tap(&mut s, 0x5E, 100).is_empty()); // play
        assert!(!s.handle_midi(&mcu::encode_fader(0, 100), 200).is_empty());
    }

    #[test]
    fn unmodified_buttons_have_no_press_latency() {
        // Widgets without hold+/double+ bindings dispatch on PRESS.
        let mut s = state();
        let press_only = s.handle_midi(&[0x90, 0x5E, 0x7F], 0);
        assert_eq!(press_only, vec![Intent::Play]);
        // Folder select carries a hold layer → defers to release.
        assert!(s.handle_midi(&[0x90, 0x18, 0x7F], 100).is_empty());
        assert_eq!(
            s.handle_midi(&[0x90, 0x18, 0x00], 150),
            vec![Intent::Refresh]
        );
    }
}
