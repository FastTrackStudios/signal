//! The phases a mode moves through, in order.
//!
//! A mode is what the window is set up for; a phase is where in that
//! work you are. Mixing goes Rescue → Balance → Tone → … (see
//! [`crate::mix_phases`]); organising a song goes Import → Tempo Map.
//! A mode without phases has none to switch between, and the window
//! shows no phase switcher for it.

use crate::mix_phases::MixPhase;
use crate::modes::Mode;

/// One phase of a mode: its stable slug and the name it is shown by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Phase {
    pub slug: &'static str,
    pub name: &'static str,
}

/// Organising a song: bring the material in, then lay its tempo map.
pub const ORGANIZE: [Phase; 2] = [
    Phase {
        slug: "import",
        name: "Import",
    },
    Phase {
        slug: "tempo-map",
        name: "Tempo Map",
    },
];

/// The phases of `mode`, in the order they are worked through — empty
/// for a mode that has none.
#[must_use]
pub fn of(mode: Mode) -> Vec<Phase> {
    match mode {
        Mode::Organize => ORGANIZE.to_vec(),
        Mode::Mix => MixPhase::ALL
            .iter()
            .map(|phase| Phase {
                slug: phase.slug(),
                name: phase.display_name(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Which phase each mode is in, chosen from anywhere — the bottom bar's
/// switcher, the mixer's rails — and the version of that choice, which
/// goes up on every change so each place can tell it moved.
struct Chosen {
    by_mode: std::collections::HashMap<Mode, &'static str>,
    version: u64,
}

fn chosen() -> &'static std::sync::Mutex<Chosen> {
    static CHOSEN: std::sync::OnceLock<std::sync::Mutex<Chosen>> = std::sync::OnceLock::new();
    CHOSEN.get_or_init(|| {
        std::sync::Mutex::new(Chosen {
            by_mode: std::collections::HashMap::new(),
            version: 0,
        })
    })
}

/// The phase `mode` is in: the one chosen, else its first.
#[must_use]
pub fn current(mode: Mode) -> Option<Phase> {
    let all = of(mode);
    let slug = chosen()
        .lock()
        .ok()
        .and_then(|c| c.by_mode.get(&mode).copied());
    all.iter()
        .find(|p| Some(p.slug) == slug)
        .or_else(|| all.first())
        .copied()
}

/// Put `mode` in phase `slug` (one of its own; anything else is ignored).
pub fn choose(mode: Mode, slug: &str) {
    let Some(phase) = of(mode).into_iter().find(|p| p.slug == slug) else {
        return;
    };
    if let Ok(mut c) = chosen().lock()
        && c.by_mode.get(&mode) != Some(&phase.slug)
    {
        c.by_mode.insert(mode, phase.slug);
        c.version = c.version.wrapping_add(1);
    }
}

/// The choice's version: different from the last one read means some
/// phase moved.
#[must_use]
pub fn version() -> u64 {
    chosen().lock().map_or(0, |c| c.version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn organize_imports_then_maps_the_tempo() {
        let slugs: Vec<&str> = of(Mode::Organize).iter().map(|p| p.slug).collect();
        assert_eq!(slugs, ["import", "tempo-map"]);
    }

    #[test]
    fn mixing_runs_through_the_mix_phases_and_live_has_none() {
        assert_eq!(of(Mode::Mix).len(), MixPhase::ALL.len());
        assert_eq!(of(Mode::Mix).first().map(|p| p.slug), Some("rescue"));
        assert!(of(Mode::Live).is_empty());
    }

    #[test]
    fn a_choice_moves_the_version_and_a_stranger_does_not() {
        let before = version();
        choose(Mode::Organize, "tempo-map");
        assert_eq!(current(Mode::Organize).map(|p| p.slug), Some("tempo-map"));
        let after = version();
        assert_ne!(before, after);
        choose(Mode::Organize, "not-a-phase");
        assert_eq!(version(), after);
    }
}
