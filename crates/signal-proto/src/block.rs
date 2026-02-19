use facet::Facet;
use serde::{Deserialize, Serialize};

// ─── Block category ─────────────────────────────────────────────

/// Type-safe grouping of block types for UI selectors and filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(C)]
pub enum BlockCategory {
    Utility,
    Dynamics,
    Drive,
    Amp,
    Eq,
    Modulation,
    Motion,
    Time,
    Special,
    Other,
}

impl BlockCategory {
    /// All categories in display order.
    pub const fn all() -> &'static [BlockCategory] {
        &[
            Self::Utility,
            Self::Dynamics,
            Self::Drive,
            Self::Amp,
            Self::Eq,
            Self::Modulation,
            Self::Motion,
            Self::Time,
            Self::Special,
            Self::Other,
        ]
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Utility => "utility",
            Self::Dynamics => "dynamics",
            Self::Drive => "drive",
            Self::Amp => "amp",
            Self::Eq => "eq",
            Self::Modulation => "modulation",
            Self::Motion => "motion",
            Self::Time => "time",
            Self::Special => "special",
            Self::Other => "other",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Utility => "Utility",
            Self::Dynamics => "Dynamics",
            Self::Drive => "Drive",
            Self::Amp => "Amp",
            Self::Eq => "EQ",
            Self::Modulation => "Modulation",
            Self::Motion => "Motion",
            Self::Time => "Time",
            Self::Special => "Special",
            Self::Other => "Other",
        }
    }
}

// ─── Block color ────────────────────────────────────────────────

/// Color configuration for a block type (Tailwind-compatible hex values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockColor {
    /// Background color.
    pub bg: &'static str,
    /// Text/foreground color.
    pub fg: &'static str,
    /// Border/accent color (slightly darker than bg).
    pub border: &'static str,
}

// ─── Block type (table-driven macro) ────────────────────────────

/// Declares the `BlockType` enum and generates all accessor methods from a
/// single table of variant metadata.
///
/// Each row needs only a variant name, storage key, category, and color.
/// Display name defaults to `stringify!(Variant)` — override with `as "Custom Name"`
/// for variants where the Rust identifier doesn't match the UI label
/// (e.g. `Eq` → `"EQ"`, `DeEsser` → `"De-Esser"`).
///
/// The `default:` prefix on a variant makes it `impl Default`.
macro_rules! block_types {
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
        /// Functional category of a DSP processing block.
        ///
        /// Used for UI grouping, icon selection, color coding, and signal-chain
        /// validation. Covers the full range of guitar/synth/vocal effects.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Facet)]
        #[repr(C)]
        pub enum BlockType {
            $default_variant,
            $( $variant, )+
        }

        impl Default for BlockType {
            fn default() -> Self { Self::$default_variant }
        }

        /// All block type variants for iteration.
        pub const ALL_BLOCK_TYPES: &[BlockType] = &[
            BlockType::$default_variant,
            $( BlockType::$variant, )+
        ];

        impl BlockType {
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
            ///
            /// Defaults to the variant name; override with `as "Custom"` in the
            /// macro invocation for non-obvious mappings.
            pub const fn display_name(self) -> &'static str {
                match self {
                    Self::$default_variant => block_types!(@display $default_variant, $(as $default_display)?),
                    $( Self::$variant => block_types!(@display $variant, $(as $display)?), )+
                }
            }

            /// UI category grouping.
            pub const fn category(self) -> BlockCategory {
                match self {
                    Self::$default_variant => BlockCategory::$default_category,
                    $( Self::$variant => BlockCategory::$category, )+
                }
            }

            /// Quad Cortex / Helix inspired color palette for UI rendering.
            pub const fn color(self) -> BlockColor {
                match self {
                    Self::$default_variant => BlockColor {
                        bg: $default_bg, fg: $default_fg, border: $default_border
                    },
                    $( Self::$variant => BlockColor { bg: $bg, fg: $fg, border: $border }, )+
                }
            }
        }
    };
}

block_types! {
    //        Variant         "storage-key"   Category    (bg, fg, border)         [, as "Display Override"]
    // ──────────────────────────────────────────────────────────────────────────────────────────────────────
    default:
    Drive,         "drive",          Drive,    ("#F97316", "#FFF7ED", "#EA580C");

    // ── Utility ─────────────────────────────────────────────────
    Input,         "input",          Utility,  ("#6B7280", "#F9FAFB", "#4B5563");
    Volume,        "volume",         Utility,  ("#6B7280", "#F9FAFB", "#4B5563");
    Send,          "send",           Utility,  ("#6B7280", "#F9FAFB", "#4B5563");
    Tuner,         "tuner",          Utility,  ("#78716C", "#FAFAF9", "#57534E");

    // ── Dynamics ────────────────────────────────────────────────
    Compressor,    "compressor",     Dynamics, ("#3B82F6", "#EFF6FF", "#2563EB");
    Gate,          "gate",           Dynamics, ("#3B82F6", "#EFF6FF", "#2563EB");
    Limiter,       "limiter",        Dynamics, ("#3B82F6", "#EFF6FF", "#2563EB");
    DeEsser,       "de-esser",       Dynamics, ("#60A5FA", "#EFF6FF", "#3B82F6"), as "De-Esser";

    // ── Drive (continued) ───────────────────────────────────────
    Saturator,     "saturator",      Drive,    ("#EF4444", "#FEF2F2", "#DC2626");
    Boost,         "boost",          Drive,    ("#FB923C", "#FFF7ED", "#F97316");

    // ── Amp ─────────────────────────────────────────────────────
    Amp,           "amp",            Amp,      ("#EAB308", "#FEFCE8", "#CA8A04");
    Cabinet,       "cabinet",        Amp,      ("#B45309", "#FEF3C7", "#92400E");

    // ── EQ ──────────────────────────────────────────────────────
    Eq,            "eq",             Eq,       ("#22C55E", "#F0FDF4", "#16A34A"), as "EQ";
    Crossover,     "crossover",      Eq,       ("#22C55E", "#F0FDF4", "#16A34A");

    // ── Modulation ──────────────────────────────────────────────
    Modulation,    "modulation",     Modulation, ("#A855F7", "#FAF5FF", "#9333EA");
    Chorus,        "chorus",         Modulation, ("#A855F7", "#FAF5FF", "#9333EA");
    Flanger,       "flanger",        Modulation, ("#A855F7", "#FAF5FF", "#9333EA");
    Phaser,        "phaser",         Modulation, ("#A855F7", "#FAF5FF", "#9333EA");
    RingModulator, "ring-modulator", Modulation, ("#9333EA", "#FAF5FF", "#7E22CE"), as "Ring Modulator";

    // ── Motion ──────────────────────────────────────────────────
    Tremolo,       "tremolo",        Motion,   ("#C084FC", "#FAF5FF", "#A855F7");
    Panner,        "panner",         Motion,   ("#C084FC", "#FAF5FF", "#A855F7");
    Vibrato,       "vibrato",        Motion,   ("#C084FC", "#FAF5FF", "#A855F7");
    Rotary,        "rotary",         Motion,   ("#C084FC", "#FAF5FF", "#A855F7");

    // ── Time ────────────────────────────────────────────────────
    Delay,         "delay",          Time,     ("#06B6D4", "#ECFEFF", "#0891B2");
    Reverb,        "reverb",         Time,     ("#0EA5E9", "#F0F9FF", "#0284C7");
    Freeze,        "freeze",         Time,     ("#22D3EE", "#ECFEFF", "#06B6D4");

    // ── Special ─────────────────────────────────────────────────
    Special,       "special",        Special,  ("#EC4899", "#FDF2F8", "#DB2777");
    Wah,           "wah",            Special,  ("#EC4899", "#FDF2F8", "#DB2777");
    Filter,        "filter",         Special,  ("#EC4899", "#FDF2F8", "#DB2777");
    Doubler,       "doubler",        Special,  ("#EC4899", "#FDF2F8", "#DB2777");
    Pitch,         "pitch",          Special,  ("#8B5CF6", "#FAF5FF", "#7C3AED");

    // ── Catch-all ───────────────────────────────────────────────
    Custom,        "custom",         Other,    ("#A8A29E", "#FAFAF9", "#78716C");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_type_default_is_drive() {
        assert_eq!(BlockType::default(), BlockType::Drive);
    }

    #[test]
    fn block_type_count() {
        assert_eq!(ALL_BLOCK_TYPES.len(), 33);
    }

    #[test]
    fn block_type_as_str_round_trip() {
        for &bt in ALL_BLOCK_TYPES {
            let s = bt.as_str();
            let parsed = BlockType::from_str(s);
            assert_eq!(parsed, Some(bt), "round-trip failed for {s:?}");
        }
    }

    #[test]
    fn block_type_variant_name_round_trip() {
        for &bt in ALL_BLOCK_TYPES {
            let name = bt.variant_name();
            let parsed = BlockType::from_variant_name(name);
            assert_eq!(
                parsed,
                Some(bt),
                "variant_name round-trip failed for {name:?}"
            );
        }
    }

    #[test]
    fn block_type_storage_keys_unique() {
        let mut seen = std::collections::HashSet::new();
        for &bt in ALL_BLOCK_TYPES {
            assert!(
                seen.insert(bt.as_str()),
                "duplicate storage key: {}",
                bt.as_str()
            );
        }
    }

    #[test]
    fn block_type_every_variant_has_category() {
        for &bt in ALL_BLOCK_TYPES {
            let cat = bt.category();
            assert!(!cat.as_str().is_empty(), "empty category for {:?}", bt);
        }
    }

    #[test]
    fn block_type_every_variant_has_color() {
        for &bt in ALL_BLOCK_TYPES {
            let c = bt.color();
            assert!(c.bg.starts_with('#'), "bad bg for {:?}: {}", bt, c.bg);
            assert!(c.fg.starts_with('#'), "bad fg for {:?}: {}", bt, c.fg);
            assert!(
                c.border.starts_with('#'),
                "bad border for {:?}: {}",
                bt,
                c.border
            );
        }
    }

    #[test]
    fn block_type_known_storage_keys() {
        assert_eq!(BlockType::Amp.as_str(), "amp");
        assert_eq!(BlockType::Drive.as_str(), "drive");
        assert_eq!(BlockType::DeEsser.as_str(), "de-esser");
        assert_eq!(BlockType::RingModulator.as_str(), "ring-modulator");
        assert_eq!(BlockType::Eq.as_str(), "eq");
    }

    #[test]
    fn block_type_display_names() {
        assert_eq!(BlockType::DeEsser.display_name(), "De-Esser");
        assert_eq!(BlockType::RingModulator.display_name(), "Ring Modulator");
        assert_eq!(BlockType::Eq.display_name(), "EQ");
        assert_eq!(BlockType::Amp.display_name(), "Amp");
    }

    #[test]
    fn block_type_categories() {
        assert_eq!(BlockType::Input.category(), BlockCategory::Utility);
        assert_eq!(BlockType::Compressor.category(), BlockCategory::Dynamics);
        assert_eq!(BlockType::Drive.category(), BlockCategory::Drive);
        assert_eq!(BlockType::Amp.category(), BlockCategory::Amp);
        assert_eq!(BlockType::Eq.category(), BlockCategory::Eq);
        assert_eq!(BlockType::Chorus.category(), BlockCategory::Modulation);
        assert_eq!(BlockType::Tremolo.category(), BlockCategory::Motion);
        assert_eq!(BlockType::Delay.category(), BlockCategory::Time);
        assert_eq!(BlockType::Wah.category(), BlockCategory::Special);
        assert_eq!(BlockType::Custom.category(), BlockCategory::Other);
    }

    #[test]
    fn block_type_from_str_returns_none_for_unknown() {
        assert_eq!(BlockType::from_str("nope"), None);
        assert_eq!(BlockType::from_variant_name("Nope"), None);
    }

    #[test]
    fn block_category_all_covers_every_category() {
        let all = BlockCategory::all();
        assert_eq!(all.len(), 10);
        for &bt in ALL_BLOCK_TYPES {
            assert!(
                all.contains(&bt.category()),
                "{:?} category not in all()",
                bt
            );
        }
    }
}
