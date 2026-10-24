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
