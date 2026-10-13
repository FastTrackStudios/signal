//! Per-user preferences: settings that follow *you*, not the song.
//!
//! The split this implements is #192's: a preference is something you
//! want in every project and that must **not** travel when a project
//! does. Your grid and snap settings are yours; the lane layout belongs
//! to the song.
//!
//! Two properties are load-bearing and easy to lose:
//!
//! **The web build keeps its preferences.** Native stores one file per
//! key under `$XDG_CONFIG_HOME/fts` (or `~/.config/fts`); wasm stores
//! the same keys in `localStorage` under `fts.<key>`. This lived in the
//! app crate, where the editor could not reach it, which is the only
//! reason it moved here.
//!
//! **One key per preference struct, each independently versioned.** A
//! corrupt or unreadable key costs you that preference and nothing else.
//! Bundling every setting into one blob would make a single bad field
//! reset all of them.
//!
//! Values are styx via facet. Parsing is tolerant and failure is always
//! `None` — a preference that cannot be read falls back to its default,
//! because no preference is worth refusing to start over.

use facet::Facet;

/// Bumped only when a field keeps its name and changes its meaning —
/// the one change tolerant parsing cannot survive.
pub const CURRENT_VERSION: u32 = 1;

/// A stored preference and the version of the shape it was written in.
#[derive(Debug, Clone, Facet)]
pub struct Versioned<T> {
    pub version: u32,
    pub value: T,
}

// ── The key/value floor ─────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
fn config_path(key: &str) -> Option<std::path::PathBuf> {
    // Honour XDG_CONFIG_HOME: on iOS the app roots it under
    // Documents/FastTrackStudio, since the container's ~/.config is not
    // writable.
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(std::path::Path::new(&xdg).join("fts").join(key));
    }
    let home = std::env::var_os("HOME")?;
    Some(std::path::Path::new(&home).join(".config/fts").join(key))
}

/// Read a raw value.
#[cfg(not(target_arch = "wasm32"))]
pub fn get_raw(key: &str) -> Option<String> {
    let val = std::fs::read_to_string(config_path(key)?).ok()?;
    let val = val.trim().to_string();
    (!val.is_empty()).then_some(val)
}

/// Write a raw value.
#[cfg(not(target_arch = "wasm32"))]
pub fn set_raw(key: &str, value: &str) {
    let Some(path) = config_path(key) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, value);
}

/// Forget a value.
#[cfg(not(target_arch = "wasm32"))]
pub fn remove(key: &str) {
    if let Some(path) = config_path(key) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

#[cfg(target_arch = "wasm32")]
pub fn get_raw(key: &str) -> Option<String> {
    let val = local_storage()?.get_item(&format!("fts.{key}")).ok()??;
    let val = val.trim().to_string();
    (!val.is_empty()).then_some(val)
}

#[cfg(target_arch = "wasm32")]
pub fn set_raw(key: &str, value: &str) {
    if let Some(s) = local_storage() {
        let _ = s.set_item(&format!("fts.{key}"), value);
    }
}

#[cfg(target_arch = "wasm32")]
pub fn remove(key: &str) {
    if let Some(s) = local_storage() {
        let _ = s.remove_item(&format!("fts.{key}"));
    }
}

// ── The typed layer ─────────────────────────────────────────────────

/// Read a preference, or `None` if it is unset, unreadable, or was
/// written by a version this build does not understand.
///
/// Never an error. A preference that cannot be read falls back to its
/// default, which is always a survivable outcome — unlike refusing to
/// start.
pub fn load<T>(key: &str) -> Option<T>
where
    // `'static`, not a free lifetime: the parsed value must outlive the
    // text it came from, since that text is read here and dropped.
    T: Facet<'static>,
{
    let raw = get_raw(key)?;
    let stored: Versioned<T> = facet_styx::from_str(&raw).ok()?;
    // A *newer* version is discarded rather than half-read. Downgrading
    // costs you a preference; misreading one silently is worse.
    (stored.version <= CURRENT_VERSION).then_some(stored.value)
}

/// Write a preference, stamped with the current version.
pub fn store<T>(key: &str, value: T)
where
    T: Facet<'static>,
{
    let wrapped = Versioned {
        version: CURRENT_VERSION,
        value,
    };
    if let Ok(text) = facet_styx::to_string(&wrapped) {
        set_raw(key, &text);
    }
}

/// How many lanes fill the viewport before the stack starts scrolling.
///
/// A **count**, not a pixel height: 200px is five lanes on a laptop and
/// eleven on a studio monitor, so a stored pixel floor behaves
/// differently on every screen. The floor is derived as
/// `viewport height / lanes_visible`.
///
/// A preference rather than project state, by #192's test: it is a
/// habit, not a property of the song, and it should not travel when a
/// project does.
pub const LANES_VISIBLE_KEY: &str = "expression-editor.lanes-visible";

/// Default lanes on screen. Five is tall enough to work in; someone
/// with an orchestra changes a setting rather than the code.
pub const LANES_VISIBLE_DEFAULT: u8 = 5;

/// The stored lane count, or the default when unset or unreadable.
pub fn lanes_visible() -> u8 {
    load::<u8>(LANES_VISIBLE_KEY)
        .filter(|n| *n > 0)
        .unwrap_or(LANES_VISIBLE_DEFAULT)
}

pub fn set_lanes_visible(n: u8) {
    store(LANES_VISIBLE_KEY, n.max(1));
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Facet)]
    struct Grid {
        division: u32,
        snap: bool,
    }

    /// Point the store at a scratch directory so tests never touch a
    /// real config.
    fn scoped(name: &str) -> (tempdir::Guard, String) {
        let guard = tempdir::Guard::new(name);
        (guard, format!("test-{name}"))
    }

    mod tempdir {
        // XDG_CONFIG_HOME is process-global, so these tests cannot run
        // concurrently with each other. Serialising here is cheaper and
        // clearer than threading a root through the public API purely
        // for tests.
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

        pub struct Guard {
            _held: std::sync::MutexGuard<'static, ()>,
            prev: Option<std::ffi::OsString>,
            dir: std::path::PathBuf,
        }
        impl Guard {
            pub fn new(name: &str) -> Self {
                let _held = LOCK.lock().unwrap_or_else(|e| e.into_inner());
                let dir =
                    std::env::temp_dir().join(format!("fts-prefs-{name}-{}", std::process::id()));
                let _ = std::fs::create_dir_all(&dir);
                let prev = std::env::var_os("XDG_CONFIG_HOME");
                unsafe { std::env::set_var("XDG_CONFIG_HOME", &dir) };
                Self { _held, prev, dir }
            }
        }
        impl Drop for Guard {
            fn drop(&mut self) {
                match &self.prev {
                    Some(p) => unsafe { std::env::set_var("XDG_CONFIG_HOME", p) },
                    None => unsafe { std::env::remove_var("XDG_CONFIG_HOME") },
                }
                let _ = std::fs::remove_dir_all(&self.dir);
            }
        }
    }

    #[test]
    fn a_preference_round_trips() {
        let (_g, key) = scoped("roundtrip");
        let grid = Grid {
            division: 16,
            snap: true,
        };
        store(&key, grid.clone());
        assert_eq!(load::<Grid>(&key), Some(grid));
    }

    #[test]
    fn an_unset_preference_is_none_not_an_error() {
        let (_g, key) = scoped("unset");
        assert_eq!(load::<Grid>(&key), None);
    }

    #[test]
    fn a_corrupt_key_costs_only_that_key() {
        let (_g, key) = scoped("corrupt");
        let other = format!("{key}-other");
        store(
            &other,
            Grid {
                division: 8,
                snap: false,
            },
        );
        set_raw(&key, "this is not styx {{{");

        assert_eq!(load::<Grid>(&key), None, "the bad one is gone");
        assert!(
            load::<Grid>(&other).is_some(),
            "and it did not take the others with it"
        );
    }

    #[test]
    fn a_version_from_the_future_falls_back_to_the_default() {
        let (_g, key) = scoped("future");
        set_raw(
            &key,
            &format!(
                "version {}\nvalue {{ division 4 snap true }}\n",
                CURRENT_VERSION + 1
            ),
        );
        assert_eq!(load::<Grid>(&key), None, "discarded rather than half-read");
    }

    #[test]
    fn lanes_visible_defaults_and_round_trips() {
        let (_g, _k) = scoped("lanes");
        assert_eq!(lanes_visible(), LANES_VISIBLE_DEFAULT, "unset means five");
        set_lanes_visible(11);
        assert_eq!(lanes_visible(), 11);
        // Zero lanes on screen is not a setting, it is a divide by zero
        // waiting to happen in the floor calculation.
        set_lanes_visible(0);
        assert_eq!(lanes_visible(), 1);
    }

    #[test]
    fn removing_forgets_it() {
        let (_g, key) = scoped("remove");
        store(
            &key,
            Grid {
                division: 4,
                snap: true,
            },
        );
        remove(&key);
        assert_eq!(load::<Grid>(&key), None);
    }
}
