use facet::Facet;
use serde::{Deserialize, Serialize};

// ─── Module category ────────────────────────────────────────────

/// High-level grouping of module types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(C)]
pub enum ModuleCategory {
    Vocal,
    Instrument,
    Other,
}

impl ModuleCategory {
    pub const fn all() -> &'static [ModuleCategory] {
        &[Self::Vocal, Self::Instrument, Self::Other]
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vocal => "vocal",
            Self::Instrument => "instrument",
            Self::Other => "other",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Vocal => "Vocal",
            Self::Instrument => "Instrument",
            Self::Other => "Other",
        }
    }
}

// ─── Module color ───────────────────────────────────────────────

/// Color configuration for a module type (Tailwind-compatible hex values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleColor {
    pub bg: &'static str,
    pub fg: &'static str,
    pub border: &'static str,
}

// ─── Module type (table-driven macro) ───────────────────────────

/// Declares the `ModuleType` enum and generates all accessor methods from a
/// single table of variant metadata.
///
/// Mirrors the `block_types!` macro pattern from `block.rs`.
macro_rules! module_types {
    // ── Internal: extract display name (override present) ───────
    (@display $variant:ident, as $display:literal) => { $display };
    // ── Internal: extract display name (no override → stringify) ─
    (@display $variant:ident,) => { stringify!($variant) };

    (
        default: $default_variant:ident,
        $default_storage:literal, $default_category:ident,
        ($default_bg:literal, $default_fg:literal, $default_border:literal)
        $(, as $default_display:literal)?;

        $(
            $variant:ident,
            $storage:literal, $category:ident,
            ($bg:literal, $fg:literal, $border:literal)
            $(, as $display:literal)?
        );+ $(;)?
    ) => {
        /// Functional category of a processing module in the signal chain.
        ///
        /// Determines where the module fits in the signal chain and how the UI
        /// groups and labels it. Covers both instrument and vocal chains.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Facet)]
        #[repr(C)]
        pub enum ModuleType {
            $default_variant,
            $( $variant, )+
        }

        impl Default for ModuleType {
            fn default() -> Self { Self::$default_variant }
        }

        /// All module type variants for iteration.
        pub const ALL_MODULE_TYPES: &[ModuleType] = &[
            ModuleType::$default_variant,
            $( ModuleType::$variant, )+
        ];

        impl ModuleType {
            /// Lowercase kebab-case identifier for storage and serialization.
            pub const fn as_str(self) -> &'static str {
                match self {
                    Self::$default_variant => $default_storage,
                    $( Self::$variant => $storage, )+
                }
            }

            /// Parse from lowercase kebab-case storage string.
            #[allow(clippy::should_implement_trait)]
            pub fn from_str(value: &str) -> Option<Self> {
                Some(match value {
                    $default_storage => Self::$default_variant,
                    $( $storage => Self::$variant, )+
                    _ => return None,
                })
            }

            /// PascalCase variant name matching the Rust enum variant.
            pub const fn variant_name(self) -> &'static str {
                match self {
                    Self::$default_variant => stringify!($default_variant),
                    $( Self::$variant => stringify!($variant), )+
                }
            }

            /// Parse from PascalCase variant name.
            pub fn from_variant_name(s: &str) -> Option<Self> {
                Some(match s {
                    stringify!($default_variant) => Self::$default_variant,
                    $( stringify!($variant) => Self::$variant, )+
                    _ => return None,
                })
            }

            /// Human-readable display name for UI labels.
            pub const fn display_name(self) -> &'static str {
                match self {
                    Self::$default_variant => module_types!(@display $default_variant, $(as $default_display)?),
                    $( Self::$variant => module_types!(@display $variant, $(as $display)?), )+
                }
            }

            /// UI category grouping.
            pub const fn category(self) -> ModuleCategory {
                match self {
                    Self::$default_variant => ModuleCategory::$default_category,
                    $( Self::$variant => ModuleCategory::$category, )+
                }
            }

            /// Color palette for UI rendering.
            pub const fn color(self) -> ModuleColor {
                match self {
                    Self::$default_variant => ModuleColor {
                        bg: $default_bg, fg: $default_fg, border: $default_border
                    },
                    $( Self::$variant => ModuleColor { bg: $bg, fg: $fg, border: $border }, )+
                }
            }
        }
    };
}

module_types! {
    //        Variant           "storage-key"        Category     (bg, fg, border)              [, as "Display Override"]
    // ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────
    default:
    Drive,              "drive",              Instrument, ("#F97316", "#FFF7ED", "#EA580C");

    // ── Vocal chain ─────────────────────────────────────────────
    Rescue,             "rescue",             Vocal,      ("#EF4444", "#FEF2F2", "#DC2626");
    Correction,         "correction",         Vocal,      ("#F59E0B", "#FFFBEB", "#D97706");
    Tonal,              "tonal",              Vocal,      ("#22C55E", "#F0FDF4", "#16A34A");
    VocalModulation,    "vocal-modulation",   Vocal,      ("#A855F7", "#FAF5FF", "#9333EA"), as "Vocal Modulation";
    Sends,              "sends",              Vocal,      ("#6B7280", "#F9FAFB", "#4B5563");

    // ── Instrument chain ────────────────────────────────────────
    Source,             "source",             Instrument, ("#6B7280", "#F9FAFB", "#4B5563");
    Eq,                 "eq",                 Instrument, ("#22C55E", "#F0FDF4", "#16A34A"), as "EQ";
    Dynamics,           "dynamics",           Instrument, ("#3B82F6", "#EFF6FF", "#2563EB");
    Special,            "special",            Instrument, ("#EC4899", "#FDF2F8", "#DB2777");
    // Drive is the default (above)
    PreFx,              "pre-fx",             Instrument, ("#14B8A6", "#F0FDFA", "#0D9488"), as "Pre-FX";
    Volume,             "volume",             Instrument, ("#6B7280", "#F9FAFB", "#4B5563");
    Amp,                "amp",                Instrument, ("#EAB308", "#FEFCE8", "#CA8A04");
    PostEq,             "post-eq",            Instrument, ("#22C55E", "#F0FDF4", "#16A34A"), as "Post-EQ";
    Modulation,         "modulation",         Instrument, ("#A855F7", "#FAF5FF", "#9333EA");
    Time,               "time",               Instrument, ("#06B6D4", "#ECFEFF", "#0891B2");
    Motion,             "motion",             Instrument, ("#C084FC", "#FAF5FF", "#A855F7");
    Master,             "master",             Instrument, ("#78716C", "#FAFAF9", "#57534E");

    // ── Catch-all ───────────────────────────────────────────────
    Custom,             "custom",             Other,      ("#A8A29E", "#FAFAF9", "#78716C");
}

impl std::fmt::Display for ModuleType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.display_name())
    }
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_type_default_is_drive() {
        assert_eq!(ModuleType::default(), ModuleType::Drive);
    }

    #[test]
    fn module_type_count() {
        assert_eq!(ALL_MODULE_TYPES.len(), 19);
    }

    #[test]
    fn module_type_as_str_round_trip() {
        for &mt in ALL_MODULE_TYPES {
            let s = mt.as_str();
            let parsed = ModuleType::from_str(s);
            assert_eq!(parsed, Some(mt), "round-trip failed for {s:?}");
        }
    }

    #[test]
    fn module_type_variant_name_round_trip() {
        for &mt in ALL_MODULE_TYPES {
            let name = mt.variant_name();
            let parsed = ModuleType::from_variant_name(name);
            assert_eq!(
                parsed,
                Some(mt),
                "variant_name round-trip failed for {name:?}"
            );
        }
    }

    #[test]
    fn module_type_storage_keys_unique() {
        let mut seen = std::collections::HashSet::new();
        for &mt in ALL_MODULE_TYPES {
            assert!(
                seen.insert(mt.as_str()),
                "duplicate storage key: {}",
                mt.as_str()
            );
        }
    }

    #[test]
    fn module_type_every_variant_has_category() {
        for &mt in ALL_MODULE_TYPES {
            let cat = mt.category();
            assert!(!cat.as_str().is_empty(), "empty category for {:?}", mt);
        }
    }

    #[test]
    fn module_type_every_variant_has_color() {
        for &mt in ALL_MODULE_TYPES {
            let c = mt.color();
            assert!(c.bg.starts_with('#'), "bad bg for {:?}: {}", mt, c.bg);
            assert!(c.fg.starts_with('#'), "bad fg for {:?}: {}", mt, c.fg);
            assert!(
                c.border.starts_with('#'),
                "bad border for {:?}: {}",
                mt,
                c.border
            );
        }
    }

    #[test]
    fn module_type_known_storage_keys() {
        assert_eq!(ModuleType::Drive.as_str(), "drive");
        assert_eq!(ModuleType::Time.as_str(), "time");
        assert_eq!(ModuleType::Amp.as_str(), "amp");
        assert_eq!(ModuleType::PreFx.as_str(), "pre-fx");
        assert_eq!(ModuleType::PostEq.as_str(), "post-eq");
        assert_eq!(ModuleType::VocalModulation.as_str(), "vocal-modulation");
        assert_eq!(ModuleType::Eq.as_str(), "eq");
    }

    #[test]
    fn module_type_display_names() {
        assert_eq!(ModuleType::Eq.display_name(), "EQ");
        assert_eq!(ModuleType::PreFx.display_name(), "Pre-FX");
        assert_eq!(ModuleType::PostEq.display_name(), "Post-EQ");
        assert_eq!(
            ModuleType::VocalModulation.display_name(),
            "Vocal Modulation"
        );
        assert_eq!(ModuleType::Drive.display_name(), "Drive");
        assert_eq!(ModuleType::Master.display_name(), "Master");
    }

    #[test]
    fn module_type_display_trait() {
        assert_eq!(format!("{}", ModuleType::Drive), "Drive");
        assert_eq!(format!("{}", ModuleType::Time), "Time");
        assert_eq!(format!("{}", ModuleType::Eq), "EQ");
    }

    #[test]
    fn module_type_categories() {
        assert_eq!(ModuleType::Rescue.category(), ModuleCategory::Vocal);
        assert_eq!(ModuleType::Correction.category(), ModuleCategory::Vocal);
        assert_eq!(
            ModuleType::VocalModulation.category(),
            ModuleCategory::Vocal
        );
        assert_eq!(ModuleType::Drive.category(), ModuleCategory::Instrument);
        assert_eq!(ModuleType::Amp.category(), ModuleCategory::Instrument);
        assert_eq!(ModuleType::Time.category(), ModuleCategory::Instrument);
        assert_eq!(ModuleType::Custom.category(), ModuleCategory::Other);
    }

    #[test]
    fn module_type_from_str_returns_none_for_unknown() {
        assert_eq!(ModuleType::from_str("nope"), None);
        assert_eq!(ModuleType::from_variant_name("Nope"), None);
    }

    #[test]
    fn module_category_all_covers_every_category() {
        let all = ModuleCategory::all();
        assert_eq!(all.len(), 3);
        for &mt in ALL_MODULE_TYPES {
            assert!(
                all.contains(&mt.category()),
                "{:?} category not in all()",
                mt
            );
        }
    }
}
