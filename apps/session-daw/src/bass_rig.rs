//! signal's bass rig on the session's bass tracks.
//!
//! By default each gets a **profile** from Session's own copy of signal's
//! bass library ([`signal_fx::profile`]) — every scene of it loaded, the
//! Scene parameter switching between them (and automatable, section by
//! section) — starting on the scene its name holds (a "Crunch" track on
//! Crunch), else the profile's own default. The copy is Session's, not the
//! player's live library: that one's scenes and order are a gig's.
//!
//! The older fixed **patches** (drive capture, amp capture, cab, trims; a
//! patch to a track by the name it holds) remain, asked for by name.

use daw::service::{FxChainContext, ProjectContext, Tracks};

/// One patch: what the rig plays.
#[derive(Clone, Debug, PartialEq)]
pub struct Patch {
    /// Its name, and what a track's name holds to take it.
    pub name: &'static str,
    pub drive: Option<&'static str>,
    pub amp: Option<&'static str>,
    pub cab: Option<&'static str>,
    pub input_db: f64,
    pub output_db: f64,
}

/// Where the captures are.
const NAM: &str = "/run/media/AudioHaven/Signal/NAM_models";

/// The DP3X bass preamp, dimed: grit that keeps the low end.
const DP3X: &str = "Jason Z Tech21 dUg DP3X bass preamp pedal all dimed no shift.nam";
/// An HM2 voiced bass-heavy: a drive in front of it.
const HM2: &str = "Jason Z Boss HM2 v1 kinda bass heavy with medium distortion pure everything turned all the way up tone.nam";

/// The patches, by what a track is called.
pub const PATCHES: [Patch; 3] = [
    // Barely breaking up: the preamp fed low, its level made up after.
    Patch {
        name: "Clean",
        drive: None,
        amp: Some(DP3X),
        cab: None,
        input_db: -15.0,
        output_db: 9.0,
    },
    // The preamp as captured: its harmonics are the point.
    Patch {
        name: "Harmonics",
        drive: None,
        amp: Some(DP3X),
        cab: None,
        input_db: 0.0,
        output_db: 0.0,
    },
    // A drive in front of it.
    Patch {
        name: "Crunch",
        drive: Some(HM2),
        amp: Some(DP3X),
        cab: None,
        input_db: -6.0,
        output_db: -3.0,
    },
];

/// The patch a track called `name` takes, if one fits.
#[must_use]
pub fn patch_for(name: &str) -> Option<&'static Patch> {
    let name = name.to_ascii_lowercase();
    PATCHES
        .iter()
        .find(|p| name.contains(&p.name.to_ascii_lowercase()))
}

impl Patch {
    /// The rig's state for this patch.
    #[must_use]
    pub fn state(&self) -> serde_json::Value {
        let file = |f: Option<&str>| f.map(|f| format!("{NAM}/{f}"));
        serde_json::json!({
            "amp": file(self.amp),
            "drive": file(self.drive),
            "cab": file(self.cab),
            "input_db": self.input_db,
            "output_db": self.output_db,
        })
    }

    /// Whether every capture it names is on disk.
    #[must_use]
    pub fn present(&self) -> bool {
        [self.drive, self.amp, self.cab]
            .into_iter()
            .flatten()
            .all(|f| std::path::Path::new(NAM).join(f).exists())
    }
}

/// How the rig is set up: a profile (its name; the library's first when
/// empty), or the fixed patches.
#[derive(Clone, Debug)]
pub enum Setup {
    Profile(String),
    Patches,
}

/// The rig's state for a track called `name` on `profile`: its scene the one
/// the name holds, else the profile's default. `None` when the library has
/// no such profile.
fn profile_state(profile: &str, name: &str) -> Option<(String, String, serde_json::Value)> {
    let found = signal_fx::profile::profile(profile)?;
    let lower = name.to_ascii_lowercase();
    let scene = found
        .patches
        .iter()
        .find(|p| lower.contains(&p.name.to_ascii_lowercase()))
        .or_else(|| found.patches.get(found.default_patch))
        .map(|p| p.name.clone())?;
    let state = serde_json::json!({ "profile": found.name, "scene": scene });
    Some((found.name.clone(), scene, state))
}

/// Put the rig on each bass track — first in its chain, before its tone —
/// as `setup` says, or, `dry`, say what each would get. One line a track.
///
/// # Errors
///
/// The engine refusing a write.
pub fn set_up(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    setup: &Setup,
    dry: bool,
) -> Result<Vec<String>, String> {
    let tracks = Tracks::all(daw, project.clone());
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let mut said = Vec::new();
    for track in tracks.iter().filter(|t| !t.is_folder) {
        let folders = crate::gain_stage::path_of(track, &by_guid);
        if !folders
            .first()
            .is_some_and(|p| p.trim().eq_ignore_ascii_case("bass"))
        {
            continue;
        }
        let state = match setup {
            Setup::Profile(profile) => {
                let Some((profile, scene, state)) = profile_state(profile, &track.name) else {
                    said.push(format!(
                        "{}: the bass library has no profile {profile:?}",
                        folders.join("/")
                    ));
                    continue;
                };
                said.push(format!(
                    "{}: the {profile} profile, on {scene}",
                    folders.join("/")
                ));
                state
            }
            Setup::Patches => {
                let Some(patch) = patch_for(&track.name) else {
                    said.push(format!("{}: no patch fits it", folders.join("/")));
                    continue;
                };
                if !patch.present() {
                    said.push(format!(
                        "{}: the {} patch's captures are not on disk",
                        folders.join("/"),
                        patch.name
                    ));
                    continue;
                }
                said.push(format!("{}: the {} patch", folders.join("/"), patch.name));
                patch.state()
            }
        };
        if dry {
            continue;
        }
        let chain = FxChainContext::Track(track.guid.clone());
        let fx = match daw::service::Effects::list(daw, project.clone(), chain.clone())
            .into_iter()
            .find(|f| f.name.eq_ignore_ascii_case(signal_fx::BASS_RIG))
        {
            Some(f) => f.guid,
            None => daw::service::Effects::add_at(
                daw,
                project.clone(),
                daw_proto::fx::AddFxAtRequest {
                    context: chain,
                    name: signal_fx::BASS_RIG.to_owned(),
                    index: 0,
                },
            )
            .ok_or("the bass rig would not add")?,
        };
        daw.apply_plugin_state(&fx, state.to_string().as_bytes())
            .map_err(|e| format!("the bass rig would not load: {e}"))?;
    }
    if !dry {
        crate::studio::request_resync();
        crate::actions::list().changed("bass.rig", "the bass rig was set up");
    }
    Ok(said)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_track_takes_the_patch_its_name_holds() {
        assert_eq!(patch_for("Clean").map(|p| p.name), Some("Clean"));
        assert_eq!(
            patch_for("Bass Harmonics").map(|p| p.name),
            Some("Harmonics")
        );
        assert_eq!(patch_for("crunch 2").map(|p| p.name), Some("Crunch"));
        assert!(patch_for("Sub").is_none());
    }

    #[test]
    fn a_patch_is_the_rigs_state() {
        let crunch = patch_for("Crunch").expect("crunch");
        let state = crunch.state();
        assert!(state["drive"].as_str().is_some_and(|d| d.ends_with(HM2)));
        assert!(state["amp"].as_str().is_some_and(|a| a.ends_with(DP3X)));
        assert!(state["cab"].is_null());
    }
}
