//! Realtime automation write — touch / latch / write modes.
//!
//! REAPER lets a fader move during playback record into the matching
//! envelope. This module wires that loop:
//!
//! 1. UI calls [`Standalone::touch_param`] when the user grabs a
//!    control.
//! 2. UI streams [`Standalone::write_param`] for every value change.
//!    Each call updates the static value AND, when the envelope's
//!    `automation_mode` allows, records an envelope point at the
//!    current playhead.
//! 3. UI calls [`Standalone::release_param`] when the user lets go.
//! 4. Stopping the transport clears `latched` state (Latch / Write
//!    modes start fresh next playback).
//!
//! Mode-specific record gates:
//!
//! - `Off`, `TrimRead`, `Read` — never record (envelope is read-only)
//! - `Touch` — record only while the param is currently touched
//! - `Latch` — record from first touch until transport stops
//! - `LatchPreview` — same as `Latch` for v1
//! - `Write` — always record

use std::collections::HashSet;

use daw_proto::automation::{
    AddPointParams, EnvelopeRef, EnvelopeShape, SendEnvelopeKind, TakeEnvelopeKind,
};
use daw_proto::primitives::{AutomationMode, PositionInSeconds};
use daw_proto::transport::service::Transport;
use daw_proto::{
    Automation, DawError, DawResult, EnvelopeLocation, EnvelopeType, PlayState, ProjectContext,
    TrackRef,
};

use crate::sync::{EnvelopeKey, Standalone};

/// Identifies an automatable parameter on the project.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum TouchableParam {
    TrackVolume {
        track_guid: String,
    },
    TrackPan {
        track_guid: String,
    },
    TrackMute {
        track_guid: String,
    },
    SendVolume {
        track_guid: String,
        send_index: u32,
    },
    SendPan {
        track_guid: String,
        send_index: u32,
    },
    SendMute {
        track_guid: String,
        send_index: u32,
    },
    TakeVolume {
        item_guid: String,
        take_guid: String,
    },
    TakePan {
        item_guid: String,
        take_guid: String,
    },
    TakeMute {
        item_guid: String,
        take_guid: String,
    },
    TakePitch {
        item_guid: String,
        take_guid: String,
    },
}

impl TouchableParam {
    /// Map to the matching `EnvelopeLocation` so callers can read /
    /// add points using the standard `Automation` trait.
    /// Inverse of [`Self::envelope_location`] — resolve a service-side
    /// `EnvelopeLocation` to the touchable parameter it addresses.
    /// `resolve_track` maps the location's `TrackRef` to a guid.
    pub fn from_location(
        location: &EnvelopeLocation,
        resolve_track: impl Fn(&TrackRef) -> Option<String>,
    ) -> Option<Self> {
        match &location.envelope {
            EnvelopeRef::Type(EnvelopeType::Volume) => Some(Self::TrackVolume {
                track_guid: resolve_track(&location.track)?,
            }),
            EnvelopeRef::Type(EnvelopeType::Pan) => Some(Self::TrackPan {
                track_guid: resolve_track(&location.track)?,
            }),
            EnvelopeRef::Type(EnvelopeType::Mute) => Some(Self::TrackMute {
                track_guid: resolve_track(&location.track)?,
            }),
            EnvelopeRef::Type(_) => None,
            EnvelopeRef::Send { send_index, kind } => {
                let track_guid = resolve_track(&location.track)?;
                Some(match kind {
                    SendEnvelopeKind::Volume => Self::SendVolume {
                        track_guid,
                        send_index: *send_index,
                    },
                    SendEnvelopeKind::Pan => Self::SendPan {
                        track_guid,
                        send_index: *send_index,
                    },
                    SendEnvelopeKind::Mute => Self::SendMute {
                        track_guid,
                        send_index: *send_index,
                    },
                })
            }
            EnvelopeRef::Take {
                item_guid,
                take_guid,
                kind,
            } => Some(match kind {
                TakeEnvelopeKind::Volume => Self::TakeVolume {
                    item_guid: item_guid.clone(),
                    take_guid: take_guid.clone(),
                },
                TakeEnvelopeKind::Pan => Self::TakePan {
                    item_guid: item_guid.clone(),
                    take_guid: take_guid.clone(),
                },
                TakeEnvelopeKind::Mute => Self::TakeMute {
                    item_guid: item_guid.clone(),
                    take_guid: take_guid.clone(),
                },
                TakeEnvelopeKind::Pitch => Self::TakePitch {
                    item_guid: item_guid.clone(),
                    take_guid: take_guid.clone(),
                },
            }),
            EnvelopeRef::FxParam { .. } | EnvelopeRef::ByName(_) => None,
        }
    }

    pub fn envelope_location(&self) -> EnvelopeLocation {
        match self {
            Self::TrackVolume { track_guid } => EnvelopeLocation::new(
                TrackRef::Guid(track_guid.clone()),
                EnvelopeRef::Type(EnvelopeType::Volume),
            ),
            Self::TrackPan { track_guid } => EnvelopeLocation::new(
                TrackRef::Guid(track_guid.clone()),
                EnvelopeRef::Type(EnvelopeType::Pan),
            ),
            Self::TrackMute { track_guid } => EnvelopeLocation::new(
                TrackRef::Guid(track_guid.clone()),
                EnvelopeRef::Type(EnvelopeType::Mute),
            ),
            Self::SendVolume {
                track_guid,
                send_index,
            } => EnvelopeLocation::new(
                TrackRef::Guid(track_guid.clone()),
                EnvelopeRef::Send {
                    send_index: *send_index,
                    kind: SendEnvelopeKind::Volume,
                },
            ),
            Self::SendPan {
                track_guid,
                send_index,
            } => EnvelopeLocation::new(
                TrackRef::Guid(track_guid.clone()),
                EnvelopeRef::Send {
                    send_index: *send_index,
                    kind: SendEnvelopeKind::Pan,
                },
            ),
            Self::SendMute {
                track_guid,
                send_index,
            } => EnvelopeLocation::new(
                TrackRef::Guid(track_guid.clone()),
                EnvelopeRef::Send {
                    send_index: *send_index,
                    kind: SendEnvelopeKind::Mute,
                },
            ),
            Self::TakeVolume {
                item_guid,
                take_guid,
            } => EnvelopeLocation::new(
                TrackRef::Master, // ignored for Take envelopes
                EnvelopeRef::Take {
                    item_guid: item_guid.clone(),
                    take_guid: take_guid.clone(),
                    kind: TakeEnvelopeKind::Volume,
                },
            ),
            Self::TakePan {
                item_guid,
                take_guid,
            } => EnvelopeLocation::new(
                TrackRef::Master,
                EnvelopeRef::Take {
                    item_guid: item_guid.clone(),
                    take_guid: take_guid.clone(),
                    kind: TakeEnvelopeKind::Pan,
                },
            ),
            Self::TakeMute {
                item_guid,
                take_guid,
            } => EnvelopeLocation::new(
                TrackRef::Master,
                EnvelopeRef::Take {
                    item_guid: item_guid.clone(),
                    take_guid: take_guid.clone(),
                    kind: TakeEnvelopeKind::Mute,
                },
            ),
            Self::TakePitch {
                item_guid,
                take_guid,
            } => EnvelopeLocation::new(
                TrackRef::Master,
                EnvelopeRef::Take {
                    item_guid: item_guid.clone(),
                    take_guid: take_guid.clone(),
                    kind: TakeEnvelopeKind::Pitch,
                },
            ),
        }
    }
}

/// The mode an envelope plays and writes by: its own when it has been
/// given one (anything but Trim/Read, every new envelope's), else its
/// track's — REAPER's track envelopes follow the track's mode, so a first
/// point written in Latch does not leave the envelope it made in
/// Trim/Read for the rest of the pass.
#[must_use]
pub fn effective_mode(
    envelope: Option<AutomationMode>,
    track: Option<AutomationMode>,
) -> AutomationMode {
    match envelope {
        Some(own) if own != AutomationMode::TrimRead => own,
        _ => track.unwrap_or(AutomationMode::TrimRead),
    }
}

/// Touch + latch state shared across all bay/automation operations
/// on a `Standalone`.
#[derive(Default)]
pub(crate) struct AutomationTouchState {
    /// Params currently held by the user.
    pub touched: HashSet<TouchableParam>,
    /// Params that have been touched at least once since transport
    /// started playing (cleared on Stop).
    pub latched: HashSet<TouchableParam>,
    /// Where each param last laid a point in this pass: the next one
    /// replaces what the envelope had between them, as a write pass
    /// over old automation does.
    pub last_written: std::collections::HashMap<TouchableParam, f64>,
}

impl Standalone {
    /// Mark `param` as currently being touched. Latches into the
    /// `latched` set as well so Latch-mode envelopes start recording.
    pub fn touch_param(&self, param: TouchableParam) {
        let mut s = self
            .automation_touch
            .lock()
            .expect("automation_touch poisoned");
        s.touched.insert(param.clone());
        s.latched.insert(param);
    }

    /// Release a previously-touched param. Stays in `latched` until
    /// transport stops (Latch mode keeps recording).
    pub fn release_param(&self, param: TouchableParam) {
        let mut s = self
            .automation_touch
            .lock()
            .expect("automation_touch poisoned");
        s.touched.remove(&param);
        s.last_written.remove(&param);
    }

    /// Clear all latched state. Called automatically when transport
    /// stops; callable manually too.
    pub fn clear_automation_latch(&self) {
        let mut s = self
            .automation_touch
            .lock()
            .expect("automation_touch poisoned");
        s.latched.clear();
        s.touched.clear();
        s.last_written.clear();
    }

    /// Write a value to a parameter. Always updates the static
    /// (so the UI reflects), and records an envelope point at the
    /// current playhead if the envelope's `automation_mode` is in a
    /// recording state and the touch gate is satisfied.
    pub fn write_param(
        &self,
        project: ProjectContext,
        param: TouchableParam,
        value: f64,
    ) -> DawResult<()> {
        // 1) Update the static value so the fader / pan knob etc.
        // reflects immediately.
        update_static(self, project.clone(), &param, value)?;

        // 2) Decide whether to record a point based on mode + touch.
        let location = param.envelope_location();
        // Envelope-level mode wins when the envelope exists; otherwise
        // the TRACK's automation mode is the authority (REAPER's
        // I_AUTOMODE) — first write in Write mode must record even
        // though no envelope exists yet.
        let mode = effective_mode(
            self.read_envelope_mode(project.clone(), &location),
            self.read_track_mode(project.clone(), &param),
        );
        let should_record = match mode {
            AutomationMode::Off | AutomationMode::TrimRead | AutomationMode::Read => false,
            AutomationMode::Touch => self
                .automation_touch
                .lock()
                .expect("automation_touch poisoned")
                .touched
                .contains(&param),
            AutomationMode::Latch | AutomationMode::LatchPreview => self
                .automation_touch
                .lock()
                .expect("automation_touch poisoned")
                .latched
                .contains(&param),
            AutomationMode::Write => true,
        };
        if !should_record {
            return Ok(());
        }
        // 3) Only record while transport is playing — REAPER's
        // semantics. Stopped → no points get laid down.
        if !Transport::is_playing(self, project.clone()) {
            return Ok(());
        }
        let time_seconds = Transport::get_position(self, project.clone());
        // What the pass rode over since its last point goes: the new
        // automation replaces the old, never interleaves with it.
        let previous = self
            .automation_touch
            .lock()
            .expect("automation_touch poisoned")
            .last_written
            .insert(param.clone(), time_seconds);
        if let Some(from) = previous.filter(|from| *from < time_seconds) {
            Automation::delete_points_in_range(
                self,
                project.clone(),
                location.clone(),
                daw_proto::automation::TimeRangeParams::new(
                    PositionInSeconds::from_seconds(from + 1e-6),
                    PositionInSeconds::from_seconds(time_seconds),
                ),
            );
        }
        Automation::add_point(
            self,
            project,
            location,
            AddPointParams {
                time: PositionInSeconds::from_seconds(time_seconds),
                value,
                shape: EnvelopeShape::Linear,
            },
        );
        Ok(())
    }

    /// The owning track's automation mode (for track / send scoped
    /// params). `None` for take params or unknown tracks.
    fn read_track_mode(
        &self,
        project: ProjectContext,
        param: &TouchableParam,
    ) -> Option<AutomationMode> {
        let track_guid = match param {
            TouchableParam::TrackVolume { track_guid }
            | TouchableParam::TrackPan { track_guid }
            | TouchableParam::TrackMute { track_guid }
            | TouchableParam::SendVolume { track_guid, .. }
            | TouchableParam::SendPan { track_guid, .. }
            | TouchableParam::SendMute { track_guid, .. } => track_guid.clone(),
            _ => return None,
        };
        let guid = match project {
            ProjectContext::Project(g) => g,
            ProjectContext::Current => self.state.lock().ok()?.current_project_guid.clone()?,
        };
        self.with_project(&guid, |p| {
            p.tracks
                .iter()
                .find(|t| t.guid == track_guid)
                .map(|t| t.automation_mode)
        })
        .ok()
        .flatten()
    }

    /// Read the `automation_mode` for the envelope identified by
    /// `location`. `None` if the envelope doesn't exist yet.
    fn read_envelope_mode(
        &self,
        project: ProjectContext,
        location: &EnvelopeLocation,
    ) -> Option<AutomationMode> {
        let guid = match project {
            ProjectContext::Project(g) => g,
            ProjectContext::Current => self.state.lock().ok()?.current_project_guid.clone()?,
        };
        let key = EnvelopeKey::from_ref(&location.envelope);
        // Take envelopes use "" as the owner field — others use the
        // resolved track guid.
        let owner = match &key {
            EnvelopeKey::Take { .. } => String::new(),
            _ => match &location.track {
                TrackRef::Guid(g) => g.clone(),
                TrackRef::Index(i) => self
                    .read_project(&guid, |p| p.tracks.get(*i as usize).map(|t| t.guid.clone()))
                    .flatten()?,
                TrackRef::Master => "master".to_string(),
            },
        };
        self.read_project(&guid, |p| {
            p.envelopes.get(&(owner, key)).map(|d| d.automation_mode)
        })
        .flatten()
    }
}

/// Push the value into the matching proto setter. Doesn't touch
/// envelope points — that's `write_param`'s job.
fn update_static(
    daw: &Standalone,
    project: ProjectContext,
    param: &TouchableParam,
    value: f64,
) -> DawResult<()> {
    use daw_proto::{RouteLocation, RouteRef, RouteType, Routing, Tracks};
    match param {
        TouchableParam::TrackVolume { track_guid } => {
            Tracks::set_volume(daw, project, TrackRef::Guid(track_guid.clone()), value)
        }
        TouchableParam::TrackPan { track_guid } => Tracks::set_pan(
            daw,
            project,
            TrackRef::Guid(track_guid.clone()),
            (value - 0.5) * 2.0,
        ),
        TouchableParam::TrackMute { track_guid } => Tracks::set_muted(
            daw,
            project,
            TrackRef::Guid(track_guid.clone()),
            value > 0.5,
        ),
        TouchableParam::SendVolume {
            track_guid,
            send_index,
        } => Routing::set_volume(
            daw,
            project,
            RouteLocation {
                track: TrackRef::Guid(track_guid.clone()),
                route_type: RouteType::Send,
                route: RouteRef::Index(*send_index),
            },
            value,
        ),
        TouchableParam::SendPan {
            track_guid,
            send_index,
        } => Routing::set_pan(
            daw,
            project,
            RouteLocation {
                track: TrackRef::Guid(track_guid.clone()),
                route_type: RouteType::Send,
                route: RouteRef::Index(*send_index),
            },
            (value - 0.5) * 2.0,
        ),
        TouchableParam::SendMute {
            track_guid,
            send_index,
        } => Routing::set_muted(
            daw,
            project,
            RouteLocation {
                track: TrackRef::Guid(track_guid.clone()),
                route_type: RouteType::Send,
                route: RouteRef::Index(*send_index),
            },
            value > 0.5,
        ),
        // Take params don't have proto setters that operate on
        // active-take fields directly — update via project state.
        TouchableParam::TakeVolume {
            item_guid,
            take_guid,
        } => take_mutate(daw, project, item_guid, take_guid, |t| t.volume = value),
        TouchableParam::TakePan { .. } => {
            // proto `Take` has no pan field today; the envelope is
            // still recorded by write_param via the standard
            // automation path. No-op for the static side.
            let _ = value;
            Ok(())
        }
        TouchableParam::TakeMute {
            item_guid,
            take_guid,
        } => {
            // Mute is on the item, not the take.
            mute_item_via_take(daw, project, item_guid, take_guid, value > 0.5)
        }
        TouchableParam::TakePitch {
            item_guid,
            take_guid,
        } => take_mutate(daw, project, item_guid, take_guid, |t| t.pitch = value),
    }
}

fn take_mutate(
    daw: &Standalone,
    project: ProjectContext,
    item_guid: &str,
    take_guid: &str,
    f: impl FnOnce(&mut daw_proto::Take),
) -> DawResult<()> {
    let guid = match project {
        ProjectContext::Project(g) => g,
        ProjectContext::Current => daw
            .state
            .lock()
            .map_err(|_| DawError::operation_failed("state poisoned"))?
            .current_project_guid
            .clone()
            .ok_or_else(|| DawError::not_found("Project", "current"))?,
    };
    let did = daw
        .write_project(&guid, |p| {
            let Some(tl) = p.takes.get_mut(item_guid) else {
                return false;
            };
            let Some(t) = tl.takes.iter_mut().find(|t| t.guid == take_guid) else {
                return false;
            };
            f(t);
            true
        })
        .unwrap_or(false);
    if did {
        Ok(())
    } else {
        Err(DawError::not_found("Take", take_guid))
    }
}

fn mute_item_via_take(
    daw: &Standalone,
    project: ProjectContext,
    item_guid: &str,
    _take_guid: &str,
    muted: bool,
) -> DawResult<()> {
    use daw_proto::{ItemRef, Items};
    Items::set_muted(daw, project, ItemRef::Guid(item_guid.to_string()), muted)
}

/// Bridge: cleared automatically on Stop. Hook into the existing
/// transport setter so we don't need engine plumbing.
impl Standalone {
    pub(crate) fn on_transport_state(&self, state: PlayState) {
        if matches!(state, PlayState::Stopped) {
            self.clear_automation_latch();
        }
    }
}
