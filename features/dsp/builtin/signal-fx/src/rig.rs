//! `FTS Bass Rig`: signal's bass on a DI, in one of two modes, by its
//! state.
//!
//! - **Profile** (`{"profile": "Metal", "scene": "Drive", …}`): a signal
//!   bass profile with every scene loaded, switched by the **Scene**
//!   parameter ([`crate::profile`]). What a new rig is.
//! - **Patch** (`{"amp": "…/Amp.nam", "drive": …, "cab": …}`): one fixed
//!   chain of captures ([`crate::bass::BassRig`]) — what a song saved before
//!   profiles holds, played as it was.
//!
//! The parameters are the same in both, so automation and settings saved
//! against them mean the same thing: Input and Output (dB, ±24) and Scene
//! (stepped, 0..1 along the profile's scenes; a patch has none).

use daw_standalone::plugin::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

use crate::bass::BassRig;
use crate::profile::{self, ProfileRig};

/// Parameter ids.
pub const INPUT: u32 = 0;
pub const OUTPUT: u32 = 1;
pub const SCENE: u32 = 2;

/// The trims' range, dB either way.
const TRIM_DB: f64 = 24.0;

/// What the rig plays, as saved: a profile and its scene, or a patch's
/// captures; and its trims.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RigState {
    /// The profile, by name (profile mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// The scene playing, by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    /// A patch's captures (patch mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amp: Option<std::path::PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drive: Option<std::path::PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cab: Option<std::path::PathBuf>,
    #[serde(default)]
    pub input_db: f64,
    #[serde(default)]
    pub output_db: f64,
}

impl RigState {
    /// Whether this is a patch (fixed captures) rather than a profile.
    #[must_use]
    pub const fn is_patch(&self) -> bool {
        self.profile.is_none() && (self.amp.is_some() || self.drive.is_some() || self.cab.is_some())
    }
}

/// The rig, in its mode.
pub struct Rig {
    state: RigState,
    patch: Option<BassRig>,
    profile: Option<ProfileRig>,
    sample_rate: f64,
    block_size: u32,
    prepared: bool,
}

fn gain(db: f64) -> f32 {
    num_traits::ToPrimitive::to_f32(&10_f64.powf(db / 20.0)).unwrap_or(1.0)
}

impl Rig {
    #[must_use]
    pub const fn new(sample_rate: f64) -> Self {
        Self {
            state: RigState {
                profile: None,
                scene: None,
                amp: None,
                drive: None,
                cab: None,
                input_db: 0.0,
                output_db: 0.0,
            },
            patch: None,
            profile: None,
            sample_rate,
            block_size: 0,
            prepared: false,
        }
    }

    /// The profile's scene names, built or not (the library's profile by
    /// the state's name, else its first).
    fn scenes(&self) -> Vec<String> {
        if let Some(rig) = &self.profile {
            return rig.scenes();
        }
        if self.state.is_patch() {
            return Vec::new();
        }
        profile::profile(self.state.profile.as_deref().unwrap_or(""))
            .map(|p| p.patches.iter().map(|s| s.name.clone()).collect())
            .unwrap_or_default()
    }

    /// The scene playing, by index: the built rig's (a switch on the audio
    /// thread names nothing), else the one the state names, else the
    /// profile's default.
    fn scene_index(&self) -> usize {
        if let Some(rig) = &self.profile {
            return rig.scene();
        }
        let scenes = self.scenes();
        let named = self
            .state
            .scene
            .as_deref()
            .and_then(|n| scenes.iter().position(|s| s.eq_ignore_ascii_case(n)));
        named.unwrap_or_else(|| {
            profile::profile(self.state.profile.as_deref().unwrap_or(""))
                .map_or(0, |p| p.default_patch)
        })
    }

    /// Build what the state says, at the size last prepared for — on the
    /// caller's thread, never the audio's.
    fn build(&mut self) -> Result<(), PluginError> {
        if self.state.is_patch() {
            let mut rig = BassRig::new(self.sample_rate);
            let bytes = serde_json::to_vec(&serde_json::json!({
                "amp": self.state.amp,
                "drive": self.state.drive,
                "cab": self.state.cab,
            }))
            .map_err(|e| PluginError::LoadFailed(e.to_string()))?;
            rig.load_state(&bytes)?;
            rig.prepare(self.sample_rate, self.block_size)?;
            self.patch = Some(rig);
            self.profile = None;
        } else {
            let name = self.state.profile.clone().unwrap_or_default();
            // What the state says, not the rig being replaced.
            self.profile = None;
            let scene = self.scene_index();
            self.profile = ProfileRig::build(&name, scene, self.sample_rate, self.block_size);
            if let Some(rig) = &self.profile {
                self.state.profile = Some(rig.profile.clone());
                self.state.scene = rig.scenes().get(rig.scene()).cloned();
            }
            self.patch = None;
        }
        Ok(())
    }
}

impl PluginInstance for Rig {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "fts.signal.bass_rig".into(),
            name: crate::BASS_RIG.into(),
            vendor: "FastTrackStudio".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }

    fn params(&mut self) -> Vec<PluginParamInfo> {
        let trim = |id, name: &str| PluginParamInfo {
            id,
            name: name.into(),
            min: -TRIM_DB,
            max: TRIM_DB,
            default: 0.0,
        };
        vec![
            trim(INPUT, "Input"),
            trim(OUTPUT, "Output"),
            PluginParamInfo {
                id: SCENE,
                name: "Scene".into(),
                min: 0.0,
                max: 1.0,
                default: 0.0,
            },
        ]
    }

    fn param_value(&mut self, id: u32) -> Option<f64> {
        match id {
            INPUT => Some(self.state.input_db),
            OUTPUT => Some(self.state.output_db),
            SCENE => Some(profile::normalized(self.scene_index(), self.scenes().len())),
            _ => None,
        }
    }

    fn value_to_text(&mut self, id: u32, value: f64) -> Option<String> {
        match id {
            INPUT | OUTPUT => Some(format!("{value:.1} dB")),
            SCENE => {
                let scenes = self.scenes();
                scenes.get(profile::scene_at(value, scenes.len())).cloned()
            }
            _ => None,
        }
    }

    fn text_to_value(&mut self, id: u32, text: &str) -> Option<f64> {
        match id {
            INPUT | OUTPUT => text
                .trim()
                .trim_end_matches("dB")
                .trim()
                .parse::<f64>()
                .ok()
                .map(|v| v.clamp(-TRIM_DB, TRIM_DB)),
            SCENE => {
                let scenes = self.scenes();
                scenes
                    .iter()
                    .position(|s| s.eq_ignore_ascii_case(text.trim()))
                    .map(|i| profile::normalized(i, scenes.len()))
            }
            _ => None,
        }
    }

    fn param_choices(&mut self, id: u32) -> Vec<(f64, String)> {
        if id != SCENE {
            return Vec::new();
        }
        let scenes = self.scenes();
        let count = scenes.len();
        scenes
            .into_iter()
            .enumerate()
            .map(|(i, name)| (profile::normalized(i, count), name))
            .collect()
    }

    fn latency(&mut self) -> u32 {
        match (&mut self.patch, &mut self.profile) {
            (Some(rig), _) => rig.latency(),
            (_, Some(rig)) => rig.latency(),
            _ => 0,
        }
    }

    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.sample_rate = sample_rate;
        self.block_size = block_size;
        self.build()?;
        self.prepared = true;
        Ok(())
    }

    fn is_prepared(&self) -> bool {
        self.prepared
    }

    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        for &(id, value) in events.params {
            match id {
                INPUT => self.state.input_db = value.clamp(-TRIM_DB, TRIM_DB),
                OUTPUT => self.state.output_db = value.clamp(-TRIM_DB, TRIM_DB),
                SCENE => {
                    if let Some(rig) = &mut self.profile {
                        let count = rig.chains.len();
                        rig.switch(profile::scene_at(value, count));
                    }
                }
                _ => {}
            }
        }
        if let Some(rig) = &mut self.profile {
            return rig.process(
                (in_l, in_r),
                (out_l, out_r),
                (gain(self.state.input_db), gain(self.state.output_db)),
            );
        }
        if let Some(rig) = &mut self.patch {
            let trims = [(INPUT, self.state.input_db), (OUTPUT, self.state.output_db)];
            return rig.process_block(
                in_l,
                in_r,
                out_l,
                out_r,
                &PluginEvents {
                    params: &trims,
                    ..PluginEvents::EMPTY
                },
            );
        }
        out_l.copy_from_slice(in_l);
        out_r.copy_from_slice(in_r);
        Ok(())
    }

    fn load_state(&mut self, state: &[u8]) -> Result<(), PluginError> {
        self.state = serde_json::from_slice(state)
            .map_err(|e| PluginError::LoadFailed(format!("bass rig state: {e}")))?;
        if self.prepared {
            self.build()?;
        }
        Ok(())
    }

    fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        if let Some(rig) = &self.profile {
            self.state.scene = rig.scenes().get(rig.scene()).cloned();
        }
        serde_json::to_vec(&self.state).map_err(|e| PluginError::LoadFailed(e.to_string()))
    }

    fn deactivate(&mut self) {
        self.prepared = false;
    }
}
