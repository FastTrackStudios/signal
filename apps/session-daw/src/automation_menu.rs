//! The track panel's automation button: a list of what the track can
//! automate.
//!
//! Its volume, pan, width and mute, and each plugin's parameters a plugin
//! at a time — each ticked where it is shown in a lane, a click showing or
//! hiding it.

use daw::service::{ProjectContext, TrackRef};
use daw_proto::automation::{Automation, EnvelopeLocation, EnvelopeRef};
use daw_proto::primitives::AutomationMode;

use crate::popup::Choice;

/// What a choice in the automation menu does.
#[derive(Clone, Debug)]
pub enum Pick {
    /// Show (or hide) the envelope named `envelope` (as
    /// [`crate::automation_actions::find`] reads names).
    Toggle {
        track: String,
        envelope: String,
        show: bool,
    },
    /// Open plugin `fx` (its place in the chain, from 1)'s parameters.
    Fx { track: String, fx: usize },
    /// Back to the first list.
    Back { track: String },
    /// Expand (or fold) the vocal leveler: its four stages a lane each, or
    /// only their sum on the items.
    Leveler { track: String, show: bool },
    /// Show (or hide) the track's items' volume curves on them.
    TakeLines { track: String, show: bool },
    /// An envelope's automation mode, or (`envelope` `None`) the track's —
    /// what an envelope not made yet writes by.
    Mode {
        track: String,
        envelope: Option<EnvelopeRef>,
        mode: AutomationMode,
    },
}

/// The modes, as a mixer names them.
const MODES: [(&str, AutomationMode); 6] = [
    ("Trim/Read", AutomationMode::TrimRead),
    ("Read", AutomationMode::Read),
    ("Touch", AutomationMode::Touch),
    ("Latch", AutomationMode::Latch),
    ("Write", AutomationMode::Write),
    ("Off", AutomationMode::Off),
];

/// The modes for an envelope (or the track's, `None`), the one it is in
/// ticked.
#[must_use]
pub fn modes(
    track: &str,
    envelope: Option<&EnvelopeRef>,
    now: AutomationMode,
) -> Vec<Choice<Pick>> {
    MODES
        .iter()
        .map(|(label, mode)| {
            Choice::new(
                *label,
                *mode == now,
                Pick::Mode {
                    track: track.to_owned(),
                    envelope: envelope.cloned(),
                    mode: *mode,
                },
            )
        })
        .collect()
}

/// The envelopes of `track` that are shown, by reference.
fn shown(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
) -> Vec<EnvelopeRef> {
    Automation::envelopes(daw, project.clone(), TrackRef::Guid(track.to_owned()))
        .into_iter()
        .filter(|e| e.visible)
        .map(|e| match (e.fx_guid, e.param_index) {
            (Some(fx_guid), Some(param_index)) => EnvelopeRef::FxParam {
                fx_guid,
                param_index,
            },
            _ => EnvelopeRef::Type(e.envelope_type),
        })
        .collect()
}

/// Whether `track` shows its items' volume curves on them.
#[must_use]
pub fn takes_shown(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
) -> bool {
    daw::service::Tracks::ext_state(daw, project.clone(), TrackRef::Guid(track.to_owned()))
        .iter()
        .any(|e| e.key == crate::automation::TAKE_LINES_KEY && e.value == "1")
}

/// Show or hide `track`'s items' volume curves on them.
pub fn show_takes(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
    shown: bool,
) {
    let _ = daw::service::Tracks::set_ext_state(
        daw,
        project.clone(),
        TrackRef::Guid(track.to_owned()),
        crate::automation::TAKE_LINES_KEY,
        if shown { "1" } else { "" },
    );
}

/// The first list: the track's own envelopes, then its plugins.
#[must_use]
pub fn top(track: &str) -> Vec<Choice<Pick>> {
    use daw_proto::automation::EnvelopeType as T;
    let Ok((daw, project)) = crate::actions::song() else {
        return Vec::new();
    };
    let on = shown(daw, &project, track);
    let mut out = vec![Choice::heading("Track")];
    for (label, ty) in [
        ("Volume", T::Volume),
        ("Pan", T::Pan),
        ("Width", T::Width),
        ("Mute", T::Mute),
        ("Volume pre-FX", T::VolumePrefx),
    ] {
        let is_on = on.contains(&EnvelopeRef::Type(ty));
        out.push(Choice::new(
            label,
            is_on,
            Pick::Toggle {
                track: track.to_owned(),
                envelope: label.to_owned(),
                show: !is_on,
            },
        ));
    }
    let takes_on = takes_shown(daw, &project, track);
    out.push(Choice::new(
        "Item volume curves",
        takes_on,
        Pick::TakeLines {
            track: track.to_owned(),
            show: !takes_on,
        },
    ));
    if crate::vocal_level::has_lanes(daw, &project, track) {
        let is_on = crate::vocal_level::lane_shown(daw, &project, track);
        out.push(Choice::new(
            crate::automation::LEVELER,
            is_on,
            Pick::Leveler {
                track: track.to_owned(),
                show: !is_on,
            },
        ));
    }
    // The track's mode: how a fader move writes where no envelope is yet.
    let now = daw::service::Tracks::get(daw, project.clone(), TrackRef::Guid(track.to_owned()))
        .map_or(AutomationMode::TrimRead, |t| t.automation_mode);
    out.push(Choice::heading("Track mode"));
    out.extend(modes(track, None, now));
    let chain = daw_proto::fx::FxChainContext::Track(track.to_owned());
    let fxs = daw_proto::fx::Effects::list(daw, project, chain);
    if !fxs.is_empty() {
        out.push(Choice::heading("Effects"));
    }
    for (i, fx) in fxs.iter().enumerate() {
        let automated = on
            .iter()
            .any(|t| matches!(t, EnvelopeRef::FxParam { fx_guid, .. } if *fx_guid == fx.guid));
        let mut choice = Choice::more(
            fx.name.clone(),
            Pick::Fx {
                track: track.to_owned(),
                fx: i.saturating_add(1),
            },
        );
        choice.ticked = automated;
        out.push(choice);
    }
    out
}

/// One plugin's parameters.
#[must_use]
pub fn params(track: &str, fx: usize) -> Vec<Choice<Pick>> {
    let Ok((daw, project)) = crate::actions::song() else {
        return Vec::new();
    };
    let on = shown(daw, &project, track);
    let chain = daw_proto::fx::FxChainContext::Track(track.to_owned());
    let Some(plugin) = daw_proto::fx::Effects::list(daw, project.clone(), chain.clone())
        .into_iter()
        .nth(fx.saturating_sub(1))
    else {
        return Vec::new();
    };
    let mut out = vec![
        Choice::new(
            "Back",
            false,
            Pick::Back {
                track: track.to_owned(),
            },
        ),
        Choice::heading(plugin.name.clone()),
    ];
    let target =
        daw_proto::fx::FxTarget::new(chain, daw_proto::fx::FxRef::Guid(plugin.guid.clone()));
    for p in daw_proto::fx::Effects::parameters(daw, project, target) {
        let is_on = on.contains(&EnvelopeRef::FxParam {
            fx_guid: plugin.guid.clone(),
            param_index: p.index,
        });
        out.push(Choice::new(
            p.name.clone(),
            is_on,
            Pick::Toggle {
                track: track.to_owned(),
                envelope: format!("#{fx}/{}", p.name),
                show: !is_on,
            },
        ));
    }
    out
}

/// Do what `pick` says: the next list to show, or `None` when the menu is
/// done.
#[must_use]
pub fn act(pick: &Pick) -> Option<Vec<Choice<Pick>>> {
    match pick {
        Pick::Fx { track, fx } => Some(params(track, *fx)),
        Pick::Back { track } => Some(top(track)),
        Pick::Mode {
            track,
            envelope,
            mode,
        } => {
            if let Ok((daw, project)) = crate::actions::song() {
                match envelope {
                    Some(target) => Automation::set_automation_mode(
                        daw,
                        project,
                        EnvelopeLocation::new(TrackRef::Guid(track.clone()), target.clone()),
                        *mode,
                    ),
                    None => {
                        let _ = daw::service::Tracks::set_automation_mode(
                            daw,
                            project,
                            TrackRef::Guid(track.clone()),
                            *mode,
                        );
                    }
                }
                crate::studio::request_resync();
            }
            None
        }
        Pick::TakeLines { track, show } => {
            if let Ok((daw, project)) = crate::actions::song() {
                show_takes(daw, &project, track, *show);
                crate::studio::request_resync();
            }
            None
        }
        Pick::Leveler { track, show } => {
            if let Ok((daw, project)) = crate::actions::song() {
                crate::vocal_level::show_lane(daw, &project, track, *show);
                crate::studio::request_resync();
            }
            None
        }
        Pick::Toggle {
            track,
            envelope,
            show,
        } => {
            let Ok((daw, project)) = crate::actions::song() else {
                return None;
            };
            match crate::automation_actions::find(daw, &project, track, envelope) {
                Ok(found) => {
                    if *show {
                        crate::automation_actions::show(daw, &project, track, &found, 64);
                    } else {
                        Automation::set_visible(
                            daw,
                            project,
                            EnvelopeLocation::new(TrackRef::Guid(track.clone()), found.target),
                            false,
                        );
                    }
                    crate::studio::request_resync();
                }
                Err(error) => tracing::warn!(error = %error, "automation menu: no such envelope"),
            }
            None
        }
    }
}
