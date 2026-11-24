//! Where a block can run.
//!
//! Signal's own blocks run everywhere the core builds. A hosted plugin
//! block runs only where its format can be loaded (CLAP and VST3 on the
//! desktop; AUv3 on iOS later), and only where the plugin is installed. A
//! preset names what each block needs, and a host that can't meet it
//! builds the block's fallback, or a visible bypass ([`crate::block`]).

use serde::{Deserialize, Serialize};

/// One place the core runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Platform {
    Linux,
    Macos,
    Windows,
    /// iPhone and iPad.
    Ios,
    Android,
    /// The browser (wasm, in an `AudioWorklet`).
    Web,
    /// Anything else: an embedded target, an unknown OS.
    Other,
}

impl Platform {
    /// Every platform, in the order of their bits in [`Platforms`].
    pub const ALL: [Self; 7] = [
        Self::Linux,
        Self::Macos,
        Self::Windows,
        Self::Ios,
        Self::Android,
        Self::Web,
        Self::Other,
    ];

    /// The platform this build is for.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(target_arch = "wasm32") {
            Self::Web
        } else if cfg!(target_os = "ios") {
            Self::Ios
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "android") {
            Self::Android
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Other
        }
    }

    const fn bit(self) -> u8 {
        match self {
            Self::Linux => 1,
            Self::Macos => 1 << 1,
            Self::Windows => 1 << 2,
            Self::Ios => 1 << 3,
            Self::Android => 1 << 4,
            Self::Web => 1 << 5,
            Self::Other => 1 << 6,
        }
    }
}

/// A set of [`Platform`]s: where a block can run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Platforms(u8);

impl Platforms {
    /// Nowhere.
    pub const NONE: Self = Self(0);
    /// Everywhere: Signal's own blocks.
    pub const ALL: Self = Self(0x7f);
    /// The desktop: where CLAP and VST3 plugins load.
    pub const DESKTOP: Self =
        Self(Platform::Linux.bit() | Platform::Macos.bit() | Platform::Windows.bit());

    /// Just `platform`.
    #[must_use]
    pub const fn only(platform: Platform) -> Self {
        Self(platform.bit())
    }

    /// This set and `platform`.
    #[must_use]
    pub const fn with(self, platform: Platform) -> Self {
        Self(self.0 | platform.bit())
    }

    /// Both sets' platforms.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// The platforms in both sets.
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    #[must_use]
    pub const fn contains(self, platform: Platform) -> bool {
        self.0 & platform.bit() != 0
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether this build can run it.
    #[must_use]
    pub const fn here(self) -> bool {
        self.contains(Platform::current())
    }

    /// The platforms, in [`Platform::ALL`] order.
    pub fn iter(self) -> impl Iterator<Item = Platform> {
        Platform::ALL.into_iter().filter(move |p| self.contains(*p))
    }
}

// In a preset, a list of names (`["linux", "macos"]`), or "all"/"desktop".
impl Serialize for Platforms {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if *self == Self::ALL {
            return s.serialize_str("all");
        }
        if *self == Self::DESKTOP {
            return s.serialize_str("desktop");
        }
        s.collect_seq(self.iter())
    }
}

impl<'de> Deserialize<'de> for Platforms {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Form {
            Named(alloc::string::String),
            List(alloc::vec::Vec<Platform>),
        }
        match Form::deserialize(d)? {
            Form::Named(n) => match n.as_str() {
                "all" => Ok(Self::ALL),
                "desktop" => Ok(Self::DESKTOP),
                "none" => Ok(Self::NONE),
                other => Err(serde::de::Error::unknown_variant(
                    other,
                    &["all", "desktop", "none"],
                )),
            },
            Form::List(list) => Ok(list.into_iter().fold(Self::NONE, |set, p| set.with(p))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_is_not_ios_or_web() {
        assert!(Platforms::DESKTOP.contains(Platform::Linux));
        assert!(!Platforms::DESKTOP.contains(Platform::Ios));
        assert!(!Platforms::DESKTOP.contains(Platform::Web));
        assert!(Platforms::ALL.here());
    }

    #[test]
    fn platforms_round_trip_through_json() {
        for set in [
            Platforms::ALL,
            Platforms::DESKTOP,
            Platforms::only(Platform::Ios).with(Platform::Web),
            Platforms::NONE,
        ] {
            let json = serde_json::to_string(&set).unwrap_or_default();
            let back: Platforms = serde_json::from_str(&json).unwrap_or(Platforms::ALL);
            assert_eq!(back, set, "{json}");
        }
    }
}
