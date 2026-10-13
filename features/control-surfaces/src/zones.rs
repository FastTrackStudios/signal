//! Zone definitions — CSI's `.zon` files, in Styx.
//!
//! A zone maps widget names to [`Action`]s. Keys may be qualified
//! with modifiers (`shift+select`); zones may `include` another zone
//! (child bindings override) and pin a navigator mode that activates
//! with the zone. `@GoZone{zone …}` actions move between zones at
//! runtime — that's the whole layering model: sends mode, FX pages,
//! folder mode are all just zones.
//!
//! ```styx
//! zones {
//!     home {
//!         navigator @Track
//!         strip {
//!             fader  @TrackVolume
//!             vpot   @TrackPan
//!             select @TrackSelect
//!             shift+select @TrackRecordArm
//!             lcd_top @TrackName
//!         }
//!         buttons {
//!             play @Play
//!             bank_right @Bank{amount 8}
//!             global_view @GoZone{zone folder}
//!         }
//!     }
//!     folder {
//!         include home
//!         navigator @Folder
//!         strip {select @FolderSpill, lcd_bottom @FolderIndicator}
//!         buttons {global_view @GoZone{zone home}}
//!     }
//! }
//! ```

use std::collections::HashMap;

use eyre::{WrapErr, eyre};
use facet::Facet;

use crate::action::Action;
use crate::mcu::Button;
use crate::navigator::NavMode;

/// The built-in X-Touch zone set, embedded so the surface works with
/// zero configuration. `FTS_CSI_ZONES=<path>` replaces it wholesale.
pub const DEFAULT_ZONES: &str = include_str!("../config/xtouch.zones.styx");

// ── Styx wire shapes ────────────────────────────────────────────────

/// Top level of a zones file.
#[derive(Facet, Debug)]
pub struct ZonesFile {
    /// Zone activated at startup.
    pub home: Option<String>,
    pub zones: HashMap<String, ZoneDef>,
}

/// One zone as written in the file.
#[derive(Facet, Debug, Default)]
pub struct ZoneDef {
    /// Inherit bindings from these zones, in order (later wins);
    /// local entries override all of them. CSI's `IncludedZones`.
    pub include: Option<Vec<String>>,
    /// Navigator mode pinned while this zone is active.
    pub navigator: Option<NavigatorKind>,
    /// Override every scribble strip's backlight while this zone is
    /// active (CSI's `SetXTouchDisplayColors`); track colors restore
    /// on exit.
    pub display_color: Option<DisplayColor>,
    /// Per-strip widget bindings (apply to all 8 strips).
    pub strip: Option<HashMap<String, Action>>,
    /// Master-section widget bindings.
    pub buttons: Option<HashMap<String, Action>>,
}

#[repr(u8)]
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigatorKind {
    Track,
    Folder,
    Vca,
}

impl From<NavigatorKind> for NavMode {
    fn from(k: NavigatorKind) -> Self {
        match k {
            NavigatorKind::Track => NavMode::Track,
            NavigatorKind::Folder => NavMode::Folder,
            NavigatorKind::Vca => NavMode::Vca,
        }
    }
}

/// The X-Touch scribble palette, as zone config (mirrors
/// [`crate::mcu::StripColor`]).
#[repr(u8)]
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayColor {
    Off,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
}

impl From<DisplayColor> for crate::mcu::StripColor {
    fn from(c: DisplayColor) -> Self {
        use crate::mcu::StripColor as S;
        match c {
            DisplayColor::Off => S::Off,
            DisplayColor::Red => S::Red,
            DisplayColor::Green => S::Green,
            DisplayColor::Yellow => S::Yellow,
            DisplayColor::Blue => S::Blue,
            DisplayColor::Magenta => S::Magenta,
            DisplayColor::Cyan => S::Cyan,
            DisplayColor::White => S::White,
        }
    }
}

// ── Widget + modifier keys ──────────────────────────────────────────

/// Modifier bitmask: the four MCU modifier keys plus the synthetic
/// HOLD bit — `hold+select` binds a long-press (CSI's `Hold+`); the
/// driver sets the bit when a press outlasts the hold threshold.
pub type Modifiers = u8;
pub const SHIFT: Modifiers = 1 << 0;
pub const OPTION: Modifiers = 1 << 1;
pub const CONTROL: Modifiers = 1 << 2;
pub const ALT: Modifiers = 1 << 3;
pub const HOLD: Modifiers = 1 << 4;
/// Double-press (driver resolves taps within the double window).
pub const DOUBLE: Modifiers = 1 << 5;
/// Latching FLIP layer (fader ↔ v-pot swap lives in zone bindings).
pub const FLIP: Modifiers = 1 << 6;
/// Latching TOGGLE layer (CSI's `Toggle+` second function).
pub const TOGGLE: Modifiers = 1 << 7;

/// Widgets that exist once per channel strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StripWidget {
    Fader,
    VPot,
    VPotPress,
    Rec,
    Solo,
    Mute,
    Select,
    LcdTop,
    LcdBottom,
}

impl StripWidget {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "fader" => Self::Fader,
            "vpot" => Self::VPot,
            "vpot_press" => Self::VPotPress,
            "rec" => Self::Rec,
            "solo" => Self::Solo,
            "mute" => Self::Mute,
            "select" => Self::Select,
            "lcd_top" => Self::LcdTop,
            "lcd_bottom" => Self::LcdBottom,
            _ => return None,
        })
    }
}

/// Master-section widgets bindable from the `buttons` block. Named
/// buttons map onto the MCU note map; `master_fader` and `jog` are
/// the two non-button gestures that live here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlobalWidget {
    MasterFader,
    Jog,
    Button(Button),
}

fn parse_global_widget(s: &str) -> Option<GlobalWidget> {
    Some(match s {
        "master_fader" => GlobalWidget::MasterFader,
        "jog" => GlobalWidget::Jog,
        "play" => GlobalWidget::Button(Button::Play),
        "stop" => GlobalWidget::Button(Button::Stop),
        "record" => GlobalWidget::Button(Button::Record),
        "rewind" => GlobalWidget::Button(Button::Rewind),
        "fast_forward" => GlobalWidget::Button(Button::FastForward),
        "cycle" => GlobalWidget::Button(Button::Cycle),
        "bank_left" => GlobalWidget::Button(Button::BankLeft),
        "bank_right" => GlobalWidget::Button(Button::BankRight),
        "channel_left" => GlobalWidget::Button(Button::ChannelLeft),
        "channel_right" => GlobalWidget::Button(Button::ChannelRight),
        "flip" => GlobalWidget::Button(Button::Flip),
        "global_view" => GlobalWidget::Button(Button::GlobalView),
        "name_value" => GlobalWidget::Button(Button::NameValue),
        "smpte_beats" => GlobalWidget::Button(Button::SmpteBeats),
        "marker" => GlobalWidget::Button(Button::Marker),
        "nudge" => GlobalWidget::Button(Button::Nudge),
        "drop" => GlobalWidget::Button(Button::Drop),
        "replace" => GlobalWidget::Button(Button::Replace),
        "click" => GlobalWidget::Button(Button::Click),
        "solo_global" => GlobalWidget::Button(Button::SoloGlobal),
        "up" => GlobalWidget::Button(Button::Up),
        "down" => GlobalWidget::Button(Button::Down),
        "left" => GlobalWidget::Button(Button::Left),
        "right" => GlobalWidget::Button(Button::Right),
        "zoom" => GlobalWidget::Button(Button::Zoom),
        "scrub" => GlobalWidget::Button(Button::Scrub),
        "auto_read" => GlobalWidget::Button(Button::AutoRead),
        "auto_write" => GlobalWidget::Button(Button::AutoWrite),
        "auto_trim" => GlobalWidget::Button(Button::AutoTrim),
        "auto_touch" => GlobalWidget::Button(Button::AutoTouch),
        "auto_latch" => GlobalWidget::Button(Button::AutoLatch),
        "auto_group" => GlobalWidget::Button(Button::AutoGroup),
        "save" => GlobalWidget::Button(Button::Save),
        "undo" => GlobalWidget::Button(Button::Undo),
        "cancel" => GlobalWidget::Button(Button::Cancel),
        "enter" => GlobalWidget::Button(Button::Enter),
        // MCU encoder-assign keys (Track/Send/Pan/Plugin/EQ/Inst) —
        // the canonical zone-switch buttons.
        "assign_track" => GlobalWidget::Button(Button::Assign(crate::mcu::AssignKey::Track)),
        "assign_send" => GlobalWidget::Button(Button::Assign(crate::mcu::AssignKey::Send)),
        "assign_pan" => GlobalWidget::Button(Button::Assign(crate::mcu::AssignKey::Pan)),
        "assign_plugin" => GlobalWidget::Button(Button::Assign(crate::mcu::AssignKey::Plugin)),
        "assign_eq" => GlobalWidget::Button(Button::Assign(crate::mcu::AssignKey::Eq)),
        "assign_inst" => GlobalWidget::Button(Button::Assign(crate::mcu::AssignKey::Inst)),
        s if s.starts_with('f') => {
            let n: u8 = s[1..].parse().ok()?;
            (1..=8)
                .contains(&n)
                .then(|| GlobalWidget::Button(Button::Function(n - 1)))?
        }
        _ => return None,
    })
}

/// Split `shift+option+select` into (modifier mask, widget name).
fn parse_qualified(key: &str) -> eyre::Result<(Modifiers, &str)> {
    let mut mods: Modifiers = 0;
    let mut rest = key;
    loop {
        let Some((head, tail)) = rest.split_once('+') else {
            return Ok((mods, rest));
        };
        mods |= match head {
            "shift" => SHIFT,
            "option" => OPTION,
            "control" => CONTROL,
            "alt" => ALT,
            "hold" => HOLD,
            "double" => DOUBLE,
            "flip" => FLIP,
            "toggle" => TOGGLE,
            other => return Err(eyre!("unknown modifier '{other}' in binding '{key}'")),
        };
        rest = tail;
    }
}

// ── Compiled zones ──────────────────────────────────────────────────

/// One zone after include-resolution and key parsing.
#[derive(Debug, Default, Clone)]
pub struct CompiledZone {
    pub navigator: Option<NavMode>,
    pub display_color: Option<crate::mcu::StripColor>,
    pub strip: HashMap<(Modifiers, StripWidget), Action>,
    pub global: HashMap<(Modifiers, GlobalWidget), Action>,
}

impl CompiledZone {
    /// Look up a strip binding: exact modifier match first, then the
    /// unmodified binding (CSI's fallback rule — a missing
    /// `shift+mute` falls back to `mute`).
    pub fn strip_action(&self, mods: Modifiers, widget: StripWidget) -> Option<&Action> {
        self.strip
            .get(&(mods, widget))
            .or_else(|| self.strip.get(&(0, widget)))
    }

    pub fn global_action(&self, mods: Modifiers, widget: GlobalWidget) -> Option<&Action> {
        self.global
            .get(&(mods, widget))
            .or_else(|| self.global.get(&(0, widget)))
    }

    /// Whether an EXACT binding exists (no unmodified fallback). The
    /// driver uses this to decide if a press must wait for hold
    /// resolution.
    pub fn strip_has_exact(&self, mods: Modifiers, widget: StripWidget) -> bool {
        self.strip.contains_key(&(mods, widget))
    }

    pub fn global_has_exact(&self, mods: Modifiers, widget: GlobalWidget) -> bool {
        self.global.contains_key(&(mods, widget))
    }
}

/// Every zone, compiled, plus the home zone name.
#[derive(Debug)]
pub struct ZoneSet {
    pub home: String,
    zones: HashMap<String, CompiledZone>,
}

impl ZoneSet {
    /// Parse + compile a zones file.
    pub fn parse(source: &str) -> eyre::Result<Self> {
        let file: ZonesFile =
            facet_styx::from_str(source).map_err(|e| eyre!("zones file parse: {e}"))?;
        Self::compile(file)
    }

    /// The built-in X-Touch set.
    pub fn builtin() -> Self {
        Self::parse(DEFAULT_ZONES).expect("embedded default zones must compile")
    }

    /// Load from `FTS_CSI_ZONES` when set, else the built-in set.
    pub fn load() -> eyre::Result<Self> {
        match std::env::var("FTS_CSI_ZONES") {
            Ok(path) if !path.is_empty() => {
                let text = std::fs::read_to_string(&path)
                    .wrap_err_with(|| format!("read zones file {path}"))?;
                Self::parse(&text).wrap_err_with(|| format!("compile zones file {path}"))
            }
            _ => Ok(Self::builtin()),
        }
    }

    pub fn zone(&self, name: &str) -> Option<&CompiledZone> {
        self.zones.get(name)
    }

    fn compile(file: ZonesFile) -> eyre::Result<Self> {
        let home = file.home.clone().unwrap_or_else(|| "home".to_string());
        if !file.zones.contains_key(&home) {
            return Err(eyre!("home zone '{home}' is not defined"));
        }
        let mut compiled: HashMap<String, CompiledZone> = HashMap::new();
        for name in file.zones.keys() {
            compile_zone(name, &file.zones, &mut compiled, &mut Vec::new())?;
        }
        Ok(Self {
            home,
            zones: compiled,
        })
    }
}

/// Depth-first include resolution with cycle detection. Parent
/// bindings land first; the zone's own entries override.
fn compile_zone(
    name: &str,
    defs: &HashMap<String, ZoneDef>,
    out: &mut HashMap<String, CompiledZone>,
    visiting: &mut Vec<String>,
) -> eyre::Result<CompiledZone> {
    if let Some(done) = out.get(name) {
        return Ok(done.clone());
    }
    if visiting.iter().any(|v| v == name) {
        return Err(eyre!("include cycle through zone '{name}'"));
    }
    let def = defs
        .get(name)
        .ok_or_else(|| eyre!("zone '{name}' referenced but not defined"))?;

    visiting.push(name.to_string());
    let mut zone = CompiledZone::default();
    for parent in def.include.as_deref().unwrap_or_default() {
        let parent = compile_zone(parent, defs, out, visiting)?;
        // Later includes override earlier ones; locals override all.
        if parent.navigator.is_some() {
            zone.navigator = parent.navigator;
        }
        if parent.display_color.is_some() {
            zone.display_color = parent.display_color;
        }
        zone.strip.extend(parent.strip);
        zone.global.extend(parent.global);
    }
    visiting.pop();

    if let Some(nav) = def.navigator {
        zone.navigator = Some(nav.into());
    }
    if let Some(color) = def.display_color {
        zone.display_color = Some(color.into());
    }
    if let Some(strip) = &def.strip {
        for (key, action) in strip {
            let (mods, widget) = parse_qualified(key)?;
            let widget = StripWidget::parse(widget)
                .ok_or_else(|| eyre!("zone '{name}': unknown strip widget '{key}'"))?;
            zone.strip.insert((mods, widget), action.clone());
        }
    }
    if let Some(buttons) = &def.buttons {
        for (key, action) in buttons {
            let (mods, widget) = parse_qualified(key)?;
            let widget = parse_global_widget(widget)
                .ok_or_else(|| eyre!("zone '{name}': unknown button widget '{key}'"))?;
            zone.global.insert((mods, widget), action.clone());
        }
    }
    out.insert(name.to_string(), zone.clone());
    Ok(zone)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_zones_compile() {
        let set = ZoneSet::builtin();
        assert_eq!(set.home, "folder");
        let home = set.zone("home").expect("home zone");
        assert_eq!(
            home.strip_action(0, StripWidget::Fader),
            Some(&Action::TrackVolume)
        );
        assert_eq!(
            home.global_action(0, GlobalWidget::Button(Button::Play)),
            Some(&Action::Play)
        );
        // Folder zone inherits home's fader binding and overrides select.
        let folder = set.zone("folder").expect("folder zone");
        assert_eq!(
            folder.strip_action(0, StripWidget::Fader),
            Some(&Action::TrackVolume)
        );
        assert_eq!(
            folder.strip_action(0, StripWidget::Select),
            Some(&Action::FolderSpill)
        );
        assert_eq!(folder.navigator, Some(NavMode::Folder));
    }

    #[test]
    fn converted_csi_official_zones_compile() {
        // The zon2styx conversion of CSI's official X-Touch zone set
        // must stay parseable (regenerate with:
        // cargo run -p daw-csi --bin zon2styx -- <CSI/Surfaces/X_Touch/Zones>).
        let set = ZoneSet::parse(include_str!("../config/xtouch-csi.zones.styx"))
            .expect("converted CSI zones must compile");
        // Home merges CSI's IncludedZones (Track + MasterTrack).
        let home = set.zone("home").expect("home zone");
        assert_eq!(
            home.strip_action(0, StripWidget::Fader),
            Some(&Action::TrackVolume)
        );
        assert_eq!(
            home.global_action(0, GlobalWidget::MasterFader),
            Some(&Action::MasterVolume)
        );
        // CSI's official folder spill is HOLD+Select.
        let folder = set.zone("folder").expect("folder zone");
        assert_eq!(
            folder.strip_action(HOLD, StripWidget::Select),
            Some(&Action::FolderSpill)
        );
        assert_eq!(
            folder.strip_action(0, StripWidget::Select),
            Some(&Action::TrackSelect)
        );
        assert_eq!(folder.display_color, Some(crate::mcu::StripColor::Yellow));
        // The sends zone landed with send actions + cyan scribbles.
        let sends = set.zone("selected_track_send").expect("sends zone");
        assert_eq!(
            sends.strip_action(0, StripWidget::Fader),
            Some(&Action::SendVolume)
        );
        assert_eq!(sends.display_color, Some(crate::mcu::StripColor::Cyan));
        // VCA zone: hold+select spills.
        let vca = set.zone("vca").expect("vca zone");
        assert_eq!(
            vca.strip_action(HOLD, StripWidget::Select),
            Some(&Action::VcaSpill)
        );
    }

    #[test]
    fn modifier_bindings_and_fallback() {
        let set = ZoneSet::parse(
            r#"
zones {
    home {
        strip {
            select @TrackSelect
            shift+select @TrackRecordArm
        }
        buttons {
            play @Play
        }
    }
}
"#,
        )
        .unwrap();
        let home = set.zone("home").unwrap();
        assert_eq!(
            home.strip_action(0, StripWidget::Select),
            Some(&Action::TrackSelect)
        );
        assert_eq!(
            home.strip_action(SHIFT, StripWidget::Select),
            Some(&Action::TrackRecordArm)
        );
        // Unbound modifier combo falls back to the plain binding.
        assert_eq!(
            home.strip_action(SHIFT | ALT, StripWidget::Select),
            Some(&Action::TrackSelect)
        );
        // Modified button with no modified binding → plain binding.
        assert_eq!(
            home.global_action(SHIFT, GlobalWidget::Button(Button::Play)),
            Some(&Action::Play)
        );
    }

    #[test]
    fn include_cycle_is_an_error() {
        let err = ZoneSet::parse(
            r#"
zones {
    home {include (other)}
    other {include (home)}
}
"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("cycle"), "{err}");
    }

    #[test]
    fn unknown_widget_is_an_error() {
        let err = ZoneSet::parse(
            r#"
zones {
    home {
        strip {frobnicator @TrackVolume}
    }
}
"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("frobnicator"), "{err}");
    }

    #[test]
    fn parameterized_actions_parse() {
        let set = ZoneSet::parse(
            r#"
zones {
    home {
        buttons {
            bank_right @Bank{amount 8}
            global_view @GoZone{zone folder}
            rewind @NudgePosition{seconds -5}
        }
    }
    folder {include (home)}
}
"#,
        )
        .unwrap();
        let home = set.zone("home").unwrap();
        assert_eq!(
            home.global_action(0, GlobalWidget::Button(Button::BankRight)),
            Some(&Action::Bank { amount: 8 })
        );
        assert_eq!(
            home.global_action(0, GlobalWidget::Button(Button::GlobalView)),
            Some(&Action::GoZone {
                zone: "folder".into()
            })
        );
    }
}
