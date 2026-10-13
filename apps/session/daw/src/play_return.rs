//! Where Stop leaves the cursor: where it is, or back where play started.
//!
//! "Return to where play started" is REAPER's "stop moves the edit cursor
//! back to the last start position" — start playing at bar 5, stop, and
//! the cursor is at bar 5 again, so a passage can be played over and over
//! without going back to place the cursor each time. A locate made while
//! playing (a click on the ruler, a jump to a section) is where play
//! starts from now, so it is what Stop returns to.
//!
//! The choice is the window's, process-wide like [`crate::options`], and
//! kept on this device the way [`crate::song_modes`] keeps its modes.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};

/// What Stop does with the cursor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stop {
    /// Stop where it is.
    Stay,
    /// Stop and go back to where play started.
    Return,
}

impl Stop {
    /// Every behaviour, in the order the menu lists them.
    pub const ALL: [Self; 2] = [Self::Stay, Self::Return];

    /// The name the Action List and the saved file use.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Stay => "stay",
            Self::Return => "return",
        }
    }

    /// What the menu calls it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Stay => "Stop where it is",
            Self::Return => "Return to where play started",
        }
    }

    /// The behaviour a slug names.
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|each| each.slug().eq_ignore_ascii_case(slug.trim()))
    }
}

/// What the transport is to do for a press.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Step {
    /// Start playing from where the cursor is.
    Play,
    /// Stop where it is.
    Stop,
    /// Stop, and put the cursor at this time.
    StopAt(f64),
}

/// Where play was last started from, while it is playing.
#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub struct Locate {
    start: Option<f64>,
}

impl Locate {
    /// Play/stop pressed, with the transport `playing` or not and the
    /// cursor at `at` seconds.
    pub const fn press(&mut self, stop: Stop, playing: bool, at: f64) -> Step {
        if playing {
            self.stop(stop, true)
        } else {
            self.start = Some(at);
            Step::Play
        }
    }

    /// Stop asked for, the transport `playing` or not.
    pub const fn stop(&mut self, stop: Stop, playing: bool) -> Step {
        let start = self.start.take();
        match (stop, playing, start) {
            (Stop::Return, true, Some(to)) => Step::StopAt(to),
            _ => Step::Stop,
        }
    }

    /// Play started from `at` seconds on purpose (a move and a play in
    /// one).
    pub const fn started_from(&mut self, at: f64) {
        self.start = Some(at);
    }

    /// The cursor moved to `to`: while playing, that is where play
    /// starts from now. Stopped, nothing is remembered — the next play
    /// starts from wherever the cursor is.
    pub const fn moved(&mut self, playing: bool, to: f64) {
        if playing {
            self.start = Some(to);
        }
    }

    /// Where Stop would return to.
    #[must_use]
    pub const fn start(self) -> Option<f64> {
        self.start
    }
}

/// The window's last locate.
static LOCATE: Mutex<Locate> = Mutex::new(Locate { start: None });

/// Run `f` on the window's [`Locate`].
pub fn with_locate<R>(f: impl FnOnce(&mut Locate) -> R) -> Option<R> {
    LOCATE.lock().ok().map(|mut locate| f(&mut locate))
}

/// Return is the default: stopping goes back to where play started.
static CURRENT: AtomicU8 = AtomicU8::new(1);
static LOADED: std::sync::Once = std::sync::Once::new();

const fn code(stop: Stop) -> u8 {
    match stop {
        Stop::Stay => 0,
        Stop::Return => 1,
    }
}

/// What Stop does now.
#[must_use]
pub fn get() -> Stop {
    LOADED.call_once(|| {
        if let Some(saved) = load().as_deref().and_then(Stop::from_slug) {
            CURRENT.store(code(saved), Ordering::Relaxed);
        }
    });
    if CURRENT.load(Ordering::Relaxed) == code(Stop::Stay) {
        Stop::Stay
    } else {
        Stop::Return
    }
}

/// Choose what Stop does, and keep the choice.
pub fn set(stop: Stop) {
    // Loaded first, so a late first `get` does not overwrite this.
    LOADED.call_once(|| {});
    CURRENT.store(code(stop), Ordering::Relaxed);
    store(stop.slug());
}

#[cfg(feature = "native")]
fn path() -> Option<std::path::PathBuf> {
    Some(dirs::data_dir()?.join("Session").join("stop-behaviour.txt"))
}

#[cfg(feature = "native")]
fn load() -> Option<String> {
    std::fs::read_to_string(path()?).ok()
}

#[cfg(feature = "native")]
fn store(text: &str) {
    let Some(path) = path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&path, text) {
        tracing::warn!(error = %e, "stop behaviour: could not be kept");
    }
}

#[cfg(all(feature = "web", not(feature = "native")))]
const KEY: &str = "fts-session-stop-behaviour";

#[cfg(all(feature = "web", not(feature = "native")))]
fn load() -> Option<String> {
    web_sys::window()?
        .local_storage()
        .ok()??
        .get_item(KEY)
        .ok()?
}

#[cfg(all(feature = "web", not(feature = "native")))]
fn store(text: &str) {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(KEY, text);
    }
}

#[cfg(not(any(feature = "native", feature = "web")))]
fn load() -> Option<String> {
    None
}

#[cfg(not(any(feature = "native", feature = "web")))]
fn store(_text: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_remembers_where_it_started_and_stop_returns_there() {
        let mut locate = Locate::default();
        assert_eq!(locate.press(Stop::Return, false, 8.0), Step::Play);
        assert_eq!(locate.start(), Some(8.0));
        assert_eq!(locate.press(Stop::Return, true, 20.0), Step::StopAt(8.0));
        assert_eq!(locate.start(), None, "a stop forgets the start");
    }

    #[test]
    fn stay_stops_where_it_is() {
        let mut locate = Locate::default();
        locate.press(Stop::Stay, false, 8.0);
        assert_eq!(locate.press(Stop::Stay, true, 20.0), Step::Stop);
    }

    #[test]
    fn a_locate_while_playing_is_the_new_start() {
        let mut locate = Locate::default();
        locate.press(Stop::Return, false, 8.0);
        locate.moved(true, 30.0);
        assert_eq!(locate.stop(Stop::Return, true), Step::StopAt(30.0));
    }

    #[test]
    fn a_locate_while_stopped_remembers_nothing() {
        let mut locate = Locate::default();
        locate.moved(false, 30.0);
        assert_eq!(locate.start(), None);
        assert_eq!(locate.press(Stop::Return, false, 30.0), Step::Play);
        assert_eq!(locate.start(), Some(30.0));
    }

    #[test]
    fn play_from_is_a_start() {
        let mut locate = Locate::default();
        locate.started_from(12.0);
        assert_eq!(locate.stop(Stop::Return, true), Step::StopAt(12.0));
    }

    #[test]
    fn stop_with_no_known_start_stays() {
        let mut locate = Locate::default();
        assert_eq!(locate.stop(Stop::Return, true), Step::Stop);
        assert_eq!(locate.stop(Stop::Return, false), Step::Stop);
    }

    #[test]
    fn slugs_round_trip() {
        for each in Stop::ALL {
            assert_eq!(Stop::from_slug(each.slug()), Some(each));
        }
        assert_eq!(Stop::from_slug(" Return "), Some(Stop::Return));
        assert_eq!(Stop::from_slug("nope"), None);
    }
}
