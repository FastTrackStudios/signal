//! The library's default profile ("All-Around") and inheritance by reference.
//!
//! One profile in the library is the **default**. A song scene whose source is
//! [`SceneSource::DefaultProfile`] does not name a patch: it resolves, at load
//! time, to whatever the default profile's default patch is *then*. Editing
//! that patch therefore changes every inheriting song at once.
//!
//! A song's differences are the scene's own `overrides`, layered on top of the
//! inherited patch's overrides, nearest scope wins per path. They are never a
//! copy of the profile: clearing a song override makes that parameter follow
//! All-Around again.
//!
//! Everything here is plain data plus pure functions over ids, so it runs off
//! the audio thread (resolution happens before `LoadPatch`) and a sync codec
//! can carry [`LibrarySettings`] and the scene source as they are.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::overrides::{NodeOverrideOp, NodePath, Override};
use crate::profile::{Patch, PatchId, PatchTarget, Profile, ProfileId};
use crate::song::{Scene, SceneSource};

/// Name of the default profile seeded into a library that has none.
pub const ALL_AROUND_NAME: &str = "All-Around";

/// Name of the patch the seeded All-Around profile starts with.
pub const ALL_AROUND_DEFAULT_PATCH_NAME: &str = "Main";

/// Storage key for [`LibrarySettings::default_profile_id`].
pub const DEFAULT_PROFILE_SETTING_KEY: &str = "default_profile_id";

// ─── Library settings ───────────────────────────────────────────

/// Library-wide settings: what applies to the whole library rather than to
/// one rig, profile or song.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct LibrarySettings {
    /// The profile every scene without an explicit source inherits from.
    /// `None` only in a library that has not been seeded yet.
    pub default_profile_id: Option<ProfileId>,
}

impl LibrarySettings {
    #[must_use]
    pub const fn with_default_profile(id: ProfileId) -> Self {
        Self {
            default_profile_id: Some(id),
        }
    }

    #[must_use]
    pub fn is_default(&self, id: &ProfileId) -> bool {
        self.default_profile_id.as_ref() == Some(id)
    }

    /// Refuse to delete the default profile: every inheriting song would lose
    /// its sound. Pick another default first.
    ///
    /// # Errors
    ///
    /// [`DefaultProfileError::CannotDeleteDefault`] when `id` is the default.
    pub fn check_delete(&self, id: &ProfileId) -> Result<(), DefaultProfileError> {
        if self.is_default(id) {
            return Err(DefaultProfileError::CannotDeleteDefault(id.clone()));
        }
        Ok(())
    }

    /// The default profile among `profiles`.
    ///
    /// # Errors
    ///
    /// [`DefaultProfileError::NoDefault`] when no default is set,
    /// [`DefaultProfileError::DefaultMissing`] when it names an absent profile.
    pub fn default_profile<'a>(
        &self,
        profiles: &'a [Profile],
    ) -> Result<&'a Profile, DefaultProfileError> {
        let id = self
            .default_profile_id
            .as_ref()
            .ok_or(DefaultProfileError::NoDefault)?;
        profiles
            .iter()
            .find(|p| &p.id == id)
            .ok_or_else(|| DefaultProfileError::DefaultMissing(id.clone()))
    }
}

/// Why the default profile could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefaultProfileError {
    /// The library has no default profile set.
    NoDefault,
    /// The default profile id names a profile the library does not hold.
    DefaultMissing(ProfileId),
    /// The default profile's `default_patch_id` is not one of its patches.
    NoDefaultPatch(ProfileId),
    /// A scene names a patch no profile holds.
    PatchNotFound(PatchId),
    /// Deleting the default profile is refused.
    CannotDeleteDefault(ProfileId),
}

impl std::fmt::Display for DefaultProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoDefault => write!(f, "the library has no default profile"),
            Self::DefaultMissing(id) => write!(f, "default profile {id} not found"),
            Self::NoDefaultPatch(id) => write!(f, "default profile {id} has no default patch"),
            Self::PatchNotFound(id) => write!(f, "patch {id} not found in any profile"),
            Self::CannotDeleteDefault(id) => write!(
                f,
                "profile {id} is the library default; choose another default before deleting it"
            ),
        }
    }
}

impl std::error::Error for DefaultProfileError {}

// ─── Seeding ────────────────────────────────────────────────────

/// A fresh All-Around profile with one patch (its default) on `target`.
#[must_use]
pub fn all_around_profile(
    id: impl Into<ProfileId>,
    patch_id: impl Into<PatchId>,
    target: PatchTarget,
) -> Profile {
    Profile::new(
        id,
        ALL_AROUND_NAME,
        Patch::new(patch_id, ALL_AROUND_DEFAULT_PATCH_NAME, target),
    )
}

/// Deterministic ids for the seeded All-Around, so two devices seeding an
/// empty library independently agree on what they made.
#[must_use]
pub fn all_around_seed_ids() -> (ProfileId, PatchId) {
    (
        ProfileId::from_uuid(crate::seed_id("signal-default-profile-all-around")),
        PatchId::from_uuid(crate::seed_id("signal-default-profile-all-around-main")),
    )
}

/// Pick the library's default profile, or describe the one to create.
///
/// Keeps a valid existing pointer; otherwise adopts a profile already named
/// "All-Around"; otherwise returns `None` and the caller seeds one with
/// [`all_around_profile`].
#[must_use]
pub fn choose_default_profile<'a>(
    settings: &LibrarySettings,
    profiles: &'a [Profile],
) -> Option<&'a Profile> {
    settings
        .default_profile(profiles)
        .ok()
        .or_else(|| profiles.iter().find(|p| p.name == ALL_AROUND_NAME))
}

// ─── Resolution ─────────────────────────────────────────────────

/// What a scene plays once inheritance is resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneResolution<'a> {
    /// The profile the patch came from.
    pub profile: &'a Profile,
    /// The patch the scene plays.
    pub patch: &'a Patch,
    /// True when the scene inherits the library default (no explicit source).
    pub inherits_default: bool,
    /// The patch's overrides with the scene's on top, one entry per path,
    /// nearest scope winning.
    pub overrides: Vec<Override>,
}

impl SceneResolution<'_> {
    /// "All-Around" or "All-Around + overrides": what the song shows as its
    /// active profile.
    #[must_use]
    pub fn label(&self, scene: &Scene) -> String {
        if scene.overrides.is_empty() {
            self.profile.name.clone()
        } else {
            format!("{} + overrides", self.profile.name)
        }
    }

    /// The resolved value of the parameter at `path`, if anything sets it.
    #[must_use]
    pub fn param_value(&self, path: &NodePath) -> Option<f32> {
        set_value(&self.overrides, path)
    }
}

/// Resolve a scene to the patch it plays, following inheritance.
///
/// Returns `Ok(None)` for sources that do not go through a profile
/// ([`SceneSource::Node`], [`SceneSource::RigScene`]); those resolve as before.
///
/// # Errors
///
/// A [`DefaultProfileError`] when the default (or the named patch) is missing.
pub fn resolve_scene<'a>(
    scene: &Scene,
    profiles: &'a [Profile],
    settings: &LibrarySettings,
) -> Result<Option<SceneResolution<'a>>, DefaultProfileError> {
    let (profile, patch, inherits_default) = match &scene.source {
        SceneSource::DefaultProfile => {
            let profile = settings.default_profile(profiles)?;
            let patch = profile
                .default_patch()
                .ok_or_else(|| DefaultProfileError::NoDefaultPatch(profile.id.clone()))?;
            (profile, patch, true)
        }
        SceneSource::Patch { patch_id } => {
            let (profile, patch) = profiles
                .iter()
                .find_map(|p| p.patch(patch_id).map(|patch| (p, patch)))
                .ok_or_else(|| DefaultProfileError::PatchNotFound(patch_id.clone()))?;
            (profile, patch, false)
        }
        SceneSource::Node { .. } | SceneSource::RigScene { .. } => return Ok(None),
    };
    Ok(Some(SceneResolution {
        profile,
        patch,
        inherits_default,
        overrides: layer_overrides(&patch.overrides, &scene.overrides),
    }))
}

/// `base` with `top` layered over it: one entry per path, `top` winning, in
/// first-seen order.
#[must_use]
pub fn layer_overrides(base: &[Override], top: &[Override]) -> Vec<Override> {
    let mut out: Vec<Override> = Vec::with_capacity(base.len() + top.len());
    for ov in base.iter().chain(top) {
        if let Some(existing) = out.iter_mut().find(|o| o.path == ov.path) {
            *existing = ov.clone();
        } else {
            out.push(ov.clone());
        }
    }
    out
}

/// The value the last `Set` at `path` in `overrides` sets.
#[must_use]
pub fn set_value(overrides: &[Override], path: &NodePath) -> Option<f32> {
    overrides.iter().rev().find_map(|o| match &o.op {
        NodeOverrideOp::Set { value } if &o.path == path => Some(value.get()),
        _ => None,
    })
}

/// Set (or replace) the override at `ov.path` in `overrides`.
pub fn upsert_override(overrides: &mut Vec<Override>, ov: Override) {
    if let Some(existing) = overrides.iter_mut().find(|o| o.path == ov.path) {
        *existing = ov;
    } else {
        overrides.push(ov);
    }
}

/// Remove every override at `path`; true if one was there.
pub fn clear_override(overrides: &mut Vec<Override>, path: &NodePath) -> bool {
    let before = overrides.len();
    overrides.retain(|o| &o.path != path);
    overrides.len() != before
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rig::{RigId, RigSceneId};
    use crate::song::{SceneId, Song, SongId};

    fn gain() -> NodePath {
        NodePath::module("drive")
            .with_block("od")
            .with_parameter("gain")
    }

    fn tone() -> NodePath {
        NodePath::module("drive")
            .with_block("od")
            .with_parameter("tone")
    }

    fn rig_target() -> PatchTarget {
        PatchTarget::RigScene {
            rig_id: RigId::new(),
            scene_id: RigSceneId::new(),
        }
    }

    /// A library with All-Around (gain 0.5, tone 0.3) as its default and a
    /// second, unrelated profile.
    fn library() -> (Vec<Profile>, LibrarySettings) {
        let (pid, patch_id) = all_around_seed_ids();
        let mut all_around = all_around_profile(pid.clone(), patch_id.clone(), rig_target());
        let main = all_around.patch_mut(&patch_id).expect("main patch");
        main.overrides.push(Override::set(gain(), 0.5));
        main.overrides.push(Override::set(tone(), 0.3));

        let other = Profile::new(
            ProfileId::new(),
            "Blues",
            Patch::new(PatchId::new(), "Crunch", rig_target()).with_override(Override::set(gain(), 0.9)),
        );
        (
            vec![other, all_around],
            LibrarySettings::with_default_profile(pid),
        )
    }

    fn all_around_main(profiles: &mut [Profile]) -> &mut Patch {
        let (pid, patch_id) = all_around_seed_ids();
        profiles
            .iter_mut()
            .find(|p| p.id == pid)
            .and_then(|p| p.patch_mut(&patch_id))
            .expect("All-Around main")
    }

    // (a)
    #[test]
    fn a_song_with_no_source_resolves_to_all_around_default_patch() {
        let (profiles, settings) = library();
        let song = Song::inheriting_default(SongId::new(), "Sweet Caroline");
        let scene = song.default_scene().expect("scene");
        assert_eq!(scene.source, SceneSource::DefaultProfile);

        let r = resolve_scene(scene, &profiles, &settings)
            .expect("resolves")
            .expect("goes through a profile");
        assert!(r.inherits_default);
        assert_eq!(r.profile.name, ALL_AROUND_NAME);
        assert_eq!(r.patch.id, all_around_seed_ids().1);
        assert_eq!(r.label(scene), "All-Around");
        assert_eq!(r.param_value(&gain()), Some(0.5));
    }

    // (b)
    #[test]
    fn editing_all_around_changes_every_inheriting_song() {
        let (mut profiles, settings) = library();
        let songs: Vec<Song> = (0..32)
            .map(|i| Song::inheriting_default(SongId::new(), format!("Song {i}")))
            .collect();

        upsert_override(&mut all_around_main(&mut profiles).overrides, Override::set(gain(), 0.7));

        for song in &songs {
            let scene = song.default_scene().expect("scene");
            let r = resolve_scene(scene, &profiles, &settings).unwrap().unwrap();
            assert_eq!(r.param_value(&gain()), Some(0.7), "{}", song.name);
        }
    }

    // (c)
    #[test]
    fn a_song_override_wins_and_clearing_it_falls_back_to_all_around() {
        let (mut profiles, settings) = library();
        let mut scene = Scene::inheriting_default(SceneId::new(), "Main");
        scene.set_override(Override::set(gain(), 0.9));

        {
            let r = resolve_scene(&scene, &profiles, &settings).unwrap().unwrap();
            assert_eq!(r.param_value(&gain()), Some(0.9), "song override wins");
            assert_eq!(r.param_value(&tone()), Some(0.3), "untouched param inherits");
            assert_eq!(r.label(&scene), "All-Around + overrides");
            assert_eq!(r.overrides.len(), 2, "one entry per path");
        }

        // All-Around moving gain does not move an overridden song…
        upsert_override(&mut all_around_main(&mut profiles).overrides, Override::set(gain(), 0.2));
        // …but does move its other params.
        upsert_override(&mut all_around_main(&mut profiles).overrides, Override::set(tone(), 0.6));
        {
            let r = resolve_scene(&scene, &profiles, &settings).unwrap().unwrap();
            assert_eq!(r.param_value(&gain()), Some(0.9));
            assert_eq!(r.param_value(&tone()), Some(0.6));
        }

        assert!(scene.clear_override(&gain()));
        assert!(!scene.clear_override(&gain()), "already cleared");
        let r = resolve_scene(&scene, &profiles, &settings).unwrap().unwrap();
        assert_eq!(r.param_value(&gain()), Some(0.2), "falls back to All-Around");
        assert_eq!(r.label(&scene), "All-Around");
    }

    // (d)
    #[test]
    fn explicit_sources_resolve_unchanged() {
        let (profiles, settings) = library();
        let blues_patch = profiles[0].default_patch_id.clone();

        let explicit = Scene::from_patch(SceneId::new(), "Verse", blues_patch.clone());
        let r = resolve_scene(&explicit, &profiles, &settings).unwrap().unwrap();
        assert!(!r.inherits_default);
        assert_eq!(r.patch.id, blues_patch);
        assert_eq!(r.profile.name, "Blues");
        assert_eq!(r.param_value(&gain()), Some(0.9));

        let rig = Scene::from_rig_scene(SceneId::new(), "Intro", RigId::new(), RigSceneId::new());
        assert!(resolve_scene(&rig, &profiles, &settings).unwrap().is_none());

        // Old serialized scenes keep deserializing to the same source.
        let old = serde_json::to_string(&explicit.source).unwrap();
        let back: SceneSource = serde_json::from_str(&old).unwrap();
        assert_eq!(back, explicit.source);
    }

    // (e)
    #[test]
    fn the_default_profile_cannot_be_deleted() {
        let (profiles, settings) = library();
        let default_id = all_around_seed_ids().0;
        assert_eq!(
            settings.check_delete(&default_id),
            Err(DefaultProfileError::CannotDeleteDefault(default_id))
        );
        assert!(settings.check_delete(&profiles[0].id).is_ok());
    }

    #[test]
    fn missing_default_is_an_error_not_a_panic() {
        let (profiles, _) = library();
        let scene = Scene::inheriting_default(SceneId::new(), "Main");
        assert_eq!(
            resolve_scene(&scene, &profiles, &LibrarySettings::default()),
            Err(DefaultProfileError::NoDefault)
        );
    }

    #[test]
    fn choose_default_adopts_an_existing_all_around() {
        let (profiles, settings) = library();
        let chosen = choose_default_profile(&LibrarySettings::default(), &profiles).unwrap();
        assert_eq!(chosen.name, ALL_AROUND_NAME);
        assert_eq!(
            choose_default_profile(&settings, &profiles).unwrap().id,
            all_around_seed_ids().0
        );
        assert!(choose_default_profile(&LibrarySettings::default(), &profiles[..1]).is_none());
    }
}
