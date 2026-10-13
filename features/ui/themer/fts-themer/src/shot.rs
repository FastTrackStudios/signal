//! Screenshotting a real REAPER wearing the theme.
//!
//! The browser preview in the editor is fast but approximate — it draws from
//! the palette and WALTER layout, with vector fallbacks where the theme uses
//! PNG artwork. This is the ground truth: a real REAPER, on a private X
//! display, with real tracks and a real mixer, captured to a file.
//!
//! Two things make it quick enough to run in a loop:
//!
//! - **No plugin scan.** A cold REAPER scanning a few hundred VSTs takes
//!   longer than everything else here combined and shows a modal progress
//!   dialog over the UI we came to photograph. [`Overrides::for_screenshot`]
//!   blanks the scan paths for the duration of the run.
//! - **The overrides are temporary.** When shooting against a real profile
//!   (so the screenshot shows *your* layout), the ini is restored on drop —
//!   the run must not leave the profile without its plugin paths.
//!
//! CLAP paths are deliberately left alone: they're what we actually load, and
//! the CLAP scan is cheap.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::color::Rgb;

/// Which REAPER profile to shoot against.
#[derive(Clone, Debug)]
pub enum Profile {
    /// Build a throwaway profile. Reproducible, but shows REAPER's defaults
    /// rather than your window layout.
    Isolated(PathBuf),
    /// Use an existing resource dir (e.g. `~/fts-dev`), with its ini
    /// temporarily overridden and restored afterwards.
    Existing(PathBuf),
}

impl Profile {
    fn dir(&self) -> &Path {
        match self {
            Self::Isolated(p) | Self::Existing(p) => p,
        }
    }
}

/// A track to put in the generated project.
#[derive(Clone, Debug)]
pub struct TrackSpec {
    pub name: String,
    pub color: Rgb,
}

impl TrackSpec {
    pub fn new(name: impl Into<String>, color: Rgb) -> Self {
        Self {
            name: name.into(),
            color,
        }
    }
}

/// A representative band: a folder's worth of drums plus the usual suspects,
/// in colors far enough apart to judge how the theme tints panels.
pub fn default_tracks() -> Vec<TrackSpec> {
    [
        ("Kick", "#e0567a"),
        ("Snare", "#e0567a"),
        ("OH", "#e0567a"),
        ("Bass", "#5b8def"),
        ("Gtr", "#46b9fe"),
        ("Keys", "#f2b134"),
        ("Lead Vox", "#3ddc97"),
    ]
    .into_iter()
    .map(|(n, c)| TrackSpec::new(n, Rgb::parse_hex(c).expect("literal hex")))
    .collect()
}

/// What a shot is aimed at.
#[derive(Clone, Debug)]
pub enum Capture {
    /// The first window whose title contains this string.
    Window(String),
    /// The whole display, including every floating window.
    Screen,
}

/// What to capture.
#[derive(Clone, Debug)]
pub struct ShotOptions {
    /// Theme directory to load (the one holding `<name>.ReaperTheme`).
    pub theme: PathBuf,
    /// Profile to run against.
    pub profile: Profile,
    /// Tracks in the generated project.
    pub tracks: Vec<TrackSpec>,
    /// Where the PNG goes.
    pub out: PathBuf,
    /// Xvfb screen spec.
    pub geometry: String,
    /// X display to use.
    pub display: String,
    /// How long to let REAPER settle before capturing.
    pub settle: Duration,
    /// Extension `.so`/`.dylib`s to install into the profile before launch.
    ///
    /// A theme is art REAPER blits; a *panel* is a window an extension
    /// opens, and photographing one means the extension has to be loaded.
    /// Copied rather than symlinked: REAPER holds the library open, and a
    /// symlink into a build directory turns a rebuild into a crash.
    pub plugins: Vec<PathBuf>,
    /// Which window to photograph, by title, or the whole screen.
    ///
    /// A floating panel is its own X window, so a capture aimed at
    /// "REAPER" misses it entirely and looks exactly like a panel that
    /// never opened.
    pub window: Capture,
    /// Actions to run once REAPER has started, by named-command id.
    ///
    /// Written into the profile as a `__startup.lua`, which REAPER runs on
    /// its own. The alternative — driving the Actions dialog with xdotool —
    /// is a keystroke race against a window that may not have focus yet,
    /// and it fails differently every time.
    pub startup_actions: Vec<String>,
}

impl ShotOptions {
    pub fn new(theme: impl Into<PathBuf>, out: impl Into<PathBuf>) -> Self {
        let scratch = std::env::temp_dir().join("fts-themer-shot");
        Self {
            theme: theme.into(),
            profile: Profile::Isolated(scratch),
            tracks: default_tracks(),
            out: out.into(),
            geometry: "1920x1200x24".into(),
            display: ":97".into(),
            settle: Duration::from_secs(14),
            window: Capture::Window("REAPER".into()),
            plugins: Vec::new(),
            startup_actions: Vec::new(),
        }
    }
}

// ── ini overrides ────────────────────────────────────────────────────────

/// Temporary `[REAPER]` ini edits, restored when this drops.
///
/// The restore is the whole point: a screenshot run against a real profile
/// blanks its plugin paths, and leaving them blank would silently break the
/// next real REAPER launch.
pub struct Overrides {
    path: PathBuf,
    original: Option<String>,
    restore: bool,
}

impl Overrides {
    /// The settings that make REAPER start fast and photograph cleanly.
    ///
    /// `clap_path` is intentionally absent — CLAP is what we load, and its
    /// scan is cheap.
    /// `empty` is a directory containing no plugins — see below.
    pub fn for_screenshot(empty: &Path) -> Vec<(&'static str, String)> {
        let empty = empty.display().to_string();
        vec![
            // The expensive one: a few hundred VSTs, behind a modal dialog
            // that lands on top of whatever we came to photograph.
            //
            // Point the search at a directory with nothing in it. Two
            // things that look like they should work do not: blanking
            // `vstpath` makes REAPER treat it as *unset* and scan its
            // defaults, and `vst_noscan=1` — which this used to rely on —
            // does not appear in a real profile at all and did not stop a
            // cold run scanning 134 plugins. What actually keeps a warm
            // profile quiet is its plugin *cache*, which a throwaway
            // profile by definition does not have.
            //
            // This on its own is still not enough: on a cold profile
            // REAPER appends its own default to whatever we wrote, so the
            // ini comes back out as `<empty>;~/.vst3` and it scans that.
            // [`fake_home`] is the other half — it makes `~/.vst3` resolve
            // somewhere empty too.
            ("vstpath", empty.clone()),
            ("vstpath64", empty.clone()),
            ("lv2path_linux", empty),
            // "A new REAPER is available" pops over the arrange view.
            ("verchk", "0".into()),
            // No device to open — and no chance of stealing the live rig's.
            ("audiosys", "0".into()),
            ("undo_max_mem", "0".into()),
        ]
    }

    /// Apply `values` to the `[REAPER]` section of `path`.
    ///
    /// `restore` false is for a throwaway profile, where putting the old
    /// values back is pointless work.
    pub fn apply(path: &Path, values: &[(&str, String)], restore: bool) -> Result<Self> {
        let original = std::fs::read_to_string(path).ok();
        let text = original.clone().unwrap_or_else(|| "[REAPER]\n".into());
        std::fs::write(path, patch_reaper_section(&text, values))
            .with_context(|| format!("write {}", path.display()))?;
        Ok(Self {
            path: path.to_path_buf(),
            original,
            restore,
        })
    }
}

impl Drop for Overrides {
    fn drop(&mut self) {
        if !self.restore {
            return;
        }
        if let Some(original) = &self.original {
            // Best effort: a failed restore must not mask the run's own error,
            // but it does need saying — the profile is left modified.
            if let Err(e) = std::fs::write(&self.path, original) {
                eprintln!(
                    "WARNING: could not restore {}: {e}. Its plugin paths are \
                     still blank — fix before launching REAPER normally.",
                    self.path.display()
                );
            }
        }
    }
}

/// Set `values` in the `[REAPER]` section, preserving every other line.
fn patch_reaper_section(text: &str, values: &[(&str, String)]) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut in_reaper = false;
    let mut section_end = lines.len();
    let mut seen: Vec<&str> = Vec::new();

    for i in 0..lines.len() {
        let t = lines[i].trim().to_string();
        if let Some(section) = t.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            if in_reaper {
                section_end = i;
                in_reaper = false;
            } else if section.eq_ignore_ascii_case("REAPER") {
                in_reaper = true;
                section_end = i + 1;
            }
            continue;
        }
        if in_reaper
            && let Some((k, _)) = t.split_once('=')
            && let Some((key, v)) = values
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(k.trim()))
        {
            lines[i] = format!("{key}={v}");
            seen.push(key);
            section_end = i + 1;
        } else if in_reaper && t.contains('=') {
            section_end = i + 1;
        }
    }

    // Keys the file didn't have yet.
    let missing: Vec<String> = values
        .iter()
        .filter(|(k, _)| !seen.contains(k))
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    lines.splice(section_end..section_end, missing);

    let mut out = lines.join("\n");
    out.push('\n');
    out
}

// ── project generation ───────────────────────────────────────────────────

/// A minimal `.RPP` with the requested tracks.
///
/// Hand-written rather than routed through the RPP writer: this needs a few
/// named, colored tracks and nothing else, and keeping it dependency-free
/// keeps the screenshot tool light.
pub fn project_rpp(tracks: &[TrackSpec]) -> String {
    let mut out = String::from("<REAPER_PROJECT 0.1 \"7.0\" 0\n  TEMPO 120 4 4\n");
    for t in tracks {
        // PEAKCOL is a native color: 0x01000000 marks "custom colour set",
        // and the low bytes are BGR like every other REAPER colour.
        let col = 0x0100_0000_u32 | (t.color.to_colorref() as u32 & 0x00ff_ffff);
        out.push_str(&format!(
            "  <TRACK\n    NAME \"{}\"\n    PEAKCOL {col}\n    TRACKHEIGHT 0 0\n  >\n",
            t.name.replace('"', "'")
        ));
    }
    out.push_str(">\n");
    out
}

// ── the run ──────────────────────────────────────────────────────────────

/// Set up a profile, launch REAPER on a private display, and capture it.
pub fn capture(opts: &ShotOptions) -> Result<PathBuf> {
    use daw::test::VirtualDisplay;

    let theme = crate::ThemeDir::open(&opts.theme)?;
    let dir = opts.profile.dir().to_path_buf();
    let restore = matches!(opts.profile, Profile::Existing(_));

    if !restore {
        // Throwaway profile: (re)build it from scratch, theme linked in.
        let themes = dir.join("ColorThemes");
        std::fs::create_dir_all(&themes)?;
        link_force(&theme.images_dir(), &themes.join(&theme.name))?;
        link_force(
            &theme.ini_path(),
            &themes.join(format!("{}.ReaperTheme", theme.name)),
        )?;

        match install_license(&dir) {
            Some(from) => println!("  license: copied from {}", from.display()),
            None => eprintln!(
                "  NOTE: no {LICENSE_FILE} found in any known resource dir — \
                 REAPER's evaluation splash will appear in the capture."
            ),
        }
    }

    // A real directory with nothing in it, so the plugin scan finishes
    // instantly instead of being skipped by a flag that does not work.
    let empty = dir.join("no-plugins");
    std::fs::create_dir_all(&empty)?;

    let ini = dir.join("reaper.ini");
    let mut values = Overrides::for_screenshot(&empty);
    let theme_ini = std::fs::canonicalize(theme.ini_path())?;
    values.push(("lastthemefn5", theme_ini.display().to_string()));
    let _guard = Overrides::apply(&ini, &values, restore)?;

    // SWELL's theme lives in the resource dir, not the theme dir, and paints
    // the menu bar, dialogs and list controls. Writing it here means a shot
    // shows the whole window rather than a dark REAPER under a grey menu bar.
    #[cfg(feature = "apply")]
    {
        let swell = dir.join("libSwell.colortheme");
        if !swell.exists()
            && let Ok(source) = crate::apply::load_theme(None)
        {
            let _ = std::fs::write(&swell, source.swell_colortheme());
        }
    }

    let project = dir.join("fts-themer-shot.rpp");
    std::fs::write(&project, project_rpp(&opts.tracks))?;
    install_plugins(&dir, &opts.plugins)?;
    write_startup_actions(&dir, &opts.startup_actions)?;

    if let Err(missing) = VirtualDisplay::tooling_available() {
        eprintln!("  NOTE: {missing}");
    }
    let display = VirtualDisplay::start(&opts.display, &opts.geometry)
        .map_err(|e| anyhow::anyhow!("start display {}: {e}", opts.display))?;
    let home = fake_home(&dir)?;
    let mut reaper = launch_reaper(&ini, &project, display.display(), &home)?;

    // REAPER restores dialogs *after* it starts, so sweep for a while
    // rather than once — the update nag can appear seconds in.
    let deadline = Instant::now() + opts.settle;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(700));
        display.close_stray_dialogs();
    }
    display.focus_window_named("REAPER");
    std::thread::sleep(Duration::from_millis(600));

    if let Some(parent) = opts.out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Prefer REAPER's own window: without a window manager the root capture
    // is mostly black surround, since nothing maximises anything.
    // Falling back on *error* as well as on "not found": a window can match
    // the title search and still refuse to be captured, and a whole-screen
    // shot is far better than no shot.
    let result = match &opts.window {
        Capture::Screen => display.screenshot(&opts.out),
        Capture::Window(title) => match display.screenshot_window_named(title, &opts.out) {
            Ok(true) => Ok(()),
            Ok(false) => {
                eprintln!("  NOTE: no window titled {title:?}; capturing the screen instead");
                display.screenshot(&opts.out)
            }
            Err(e) => {
                eprintln!("  NOTE: window capture failed ({e}); falling back to the full screen");
                display.screenshot(&opts.out)
            }
        },
    }
    .map_err(|e| anyhow::anyhow!("capture: {e}"));

    let _ = reaper.kill();
    let _ = reaper.wait();
    result?;

    Ok(opts.out.clone())
}

/// Copy extension libraries into the profile's `UserPlugins`.
fn install_plugins(profile: &Path, plugins: &[PathBuf]) -> Result<()> {
    if plugins.is_empty() {
        return Ok(());
    }
    let dest = profile.join("UserPlugins");
    std::fs::create_dir_all(&dest)?;
    for plugin in plugins {
        let name = plugin
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("plugin path has no filename: {}", plugin.display()))?;
        // Cargo builds a cdylib as `libfoo.so`; REAPER loads extensions by
        // filename and wants `reaper_foo.so`. Copying the file under its
        // build name is the failure that looks like a working setup: the
        // plugin is present, REAPER starts fine, and the actions it would
        // have registered simply never appear.
        let installed = name.strip_prefix("lib").unwrap_or(name);
        if !installed.starts_with("reaper_") {
            eprintln!(
                "  NOTE: {installed} does not start with `reaper_` — REAPER \
                 will ignore it, and its actions will not register."
            );
        }
        std::fs::copy(plugin, dest.join(installed))
            .with_context(|| format!("install {}", plugin.display()))?;
        println!("  plugin:  {} -> UserPlugins/{installed}", plugin.display());
    }
    Ok(())
}

/// Write a `__startup.lua` that runs `actions` when REAPER opens.
///
/// REAPER runs this script itself at startup, which is why nothing has to
/// be typed at a dialog. But it runs it *early* — an extension that
/// registers its actions asynchronously has not finished by then, and the
/// lookup returns 0 for an action that is about to exist. So the script
/// retries on `defer` (once per frame) until the action appears or the
/// budget runs out, and says so on the console either way.
///
/// Both spellings are tried: extensions register a bare name and REAPER
/// exposes it prefixed with `_`, and which one a caller passes is not worth
/// getting wrong over.
fn write_startup_actions(profile: &Path, actions: &[String]) -> Result<()> {
    if actions.is_empty() {
        return Ok(());
    }
    let scripts = profile.join("Scripts");
    std::fs::create_dir_all(&scripts)?;

    let mut lua = String::from(
        "local pending = {}\n\
         local tries = 0\n",
    );
    for action in actions {
        let bare = action.trim_start_matches('_');
        lua.push_str(&format!("pending[#pending+1] = \"{bare}\"\n"));
        println!("  action:  {bare}");
    }
    // REAPER's own commands are numbers, not names — `40078` is "View:
    // toggle mixer visible" — and `NamedCommandLookup` does not resolve
    // them. Without this the tool can only drive our own extension, which
    // is exactly the wrong half when what you need photographed is
    // REAPER's mixer.
    lua.push_str(
        "local function run()\n\
        \x20 local left = {}\n\
        \x20 for _, name in ipairs(pending) do\n\
        \x20   local id = tonumber(name) or reaper.NamedCommandLookup(\"_\" .. name)\n\
        \x20   if id == 0 then id = reaper.NamedCommandLookup(name) end\n\
        \x20   if id ~= 0 then reaper.Main_OnCommand(id, 0)\n\
        \x20   else left[#left+1] = name end\n\
        \x20 end\n\
        \x20 pending = left\n\
        \x20 if #pending == 0 then return end\n\
        \x20 tries = tries + 1\n\
        \x20 if tries > 400 then\n\
        \x20   for _, name in ipairs(pending) do\n\
        \x20     reaper.ShowConsoleMsg(\"no such action: \" .. name .. \"\\n\")\n\
        \x20   end\n\
        \x20   return\n\
        \x20 end\n\
        \x20 reaper.defer(run)\n\
        end\n\
        run()\n",
    );
    std::fs::write(scripts.join("__startup.lua"), lua)?;
    Ok(())
}

/// Resource dirs searched for an existing REAPER licence, in order.
///
/// A throwaway profile has no licence, so REAPER shows its "Still
/// Evaluating" splash over the arrange view — in the middle of the
/// screenshot. Copying the licence the user already has into their own
/// scratch profile is what makes an unattended capture clean.
fn license_search_paths() -> Vec<PathBuf> {
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let mut dirs = Vec::new();
    if let Ok(dev) = std::env::var("FTS_DEV") {
        dirs.push(PathBuf::from(dev));
    }
    dirs.extend([
        home.join("fts-dev"),
        home.join(".config/REAPER"),
        home.join(".fasttrackstudio/Reaper"),
        home.join("fasttrackstudio"),
    ]);
    dirs
}

/// The name REAPER stores its licence under, in the resource dir.
const LICENSE_FILE: &str = "reaper-license.rk";

/// Find an installed REAPER licence, if there is one.
pub fn find_license() -> Option<PathBuf> {
    license_search_paths()
        .into_iter()
        .map(|d| d.join(LICENSE_FILE))
        .find(|p| p.is_file())
}

/// Copy an existing licence into `profile` so the evaluation splash stays
/// out of the shot. Returns where it came from.
///
/// Copied, not symlinked: REAPER rewrites this file, and a symlink would
/// let a throwaway profile write through to the real one.
fn install_license(profile: &Path) -> Option<PathBuf> {
    let src = find_license()?;
    let dst = profile.join(LICENSE_FILE);
    if dst.is_file() {
        return Some(dst);
    }
    std::fs::copy(&src, &dst).ok()?;
    Some(src)
}

/// Replace `dst` with a symlink to `src`, whatever `dst` currently is.
fn link_force(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() || std::fs::symlink_metadata(dst).is_ok() {
        let _ = std::fs::remove_file(dst);
        let _ = std::fs::remove_dir_all(dst);
    }
    let src = std::fs::canonicalize(src).with_context(|| format!("resolve {}", src.display()))?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(&src, dst)
        .with_context(|| format!("link {} -> {}", dst.display(), src.display()))?;
    Ok(())
}

/// Spawn REAPER through the FHS wrapper when one is configured.
///
/// Without it SWELL cannot dlopen its GUI libraries and REAPER dies inside
/// GDK — which looks exactly like a missing display.
/// A home directory whose plugin folders exist but are empty.
///
/// REAPER hardcodes `~/.vst`, `~/.vst3`, `~/.lv2` and `~/.clap` as scan
/// defaults and re-adds them to `vstpath` behind our back, so the only way
/// to make those paths cheap is to change what `~` means for the child.
fn fake_home(under: &Path) -> Result<PathBuf> {
    let home = under.join("home");
    for plugins in [".vst", ".vst3", ".lv2", ".clap"] {
        std::fs::create_dir_all(home.join(plugins))?;
    }
    Ok(home)
}

fn launch_reaper(ini: &Path, project: &Path, display: &str, home: &Path) -> Result<Child> {
    let exe = reaper_binary()?;
    let fhs = reaper_fhs();

    let mut cmd = match &fhs {
        Some(wrapper) => {
            let mut c = Command::new(wrapper);
            c.arg(&exe);
            c
        }
        None => {
            eprintln!(
                "WARNING: no reaper-env FHS wrapper found — REAPER's GUI libs \
                 may fail to load, which presents as a missing display."
            );
            Command::new(&exe)
        }
    };

    cmd.args(["-cfgfile"])
        .arg(ini)
        .args(["-nosplash", "-ignoreerrors"])
        .arg(project)
        .env("DISPLAY", display)
        .env("HOME", home);

    // REAPER's own output, and every extension's with it. Discarded by
    // default because it is noisy; kept when asked, because "the panel did
    // not appear" is unanswerable without it — an extension that fails to
    // load, or a window that fails to create, says so here and nowhere else.
    match std::env::var("FTS_SHOT_LOG") {
        Ok(path) if !path.is_empty() => {
            let log =
                std::fs::File::create(&path).with_context(|| format!("open shot log {path}"))?;
            let dup = log.try_clone()?;
            cmd.stdout(Stdio::from(log)).stderr(Stdio::from(dup));
            eprintln!("  log:     {path}");
        }
        _ => {
            cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }
    }

    cmd.spawn()
        .with_context(|| format!("launch {}", exe.display()))
}

/// Locate REAPER: the fts-dev test profile's pinned build, else `$PATH`.
fn reaper_binary() -> Result<PathBuf> {
    let launch = PathBuf::from(
        std::env::var("FTS_DEV")
            .unwrap_or_else(|_| format!("{}/fts-dev", std::env::var("HOME").unwrap_or_default())),
    )
    .join("launch.json");

    if let Ok(text) = std::fs::read_to_string(&launch)
        && let Some(exe) = json_string(&text, "reaper_executable")
    {
        let p = PathBuf::from(exe);
        // launch.json can name a garbage-collected store path.
        if p.is_file() {
            return Ok(p);
        }
    }
    which("reaper").context("no REAPER found (not in launch.json, not on PATH)")
}

/// The FHS wrapper, from `$FTS_REAPER_FHS` or scraped from the `fts-dev` script.
fn reaper_fhs() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("FTS_REAPER_FHS") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let script = which("fts-dev")?;
    let text = std::fs::read_to_string(script).ok()?;
    let marker = "FTS_REAPER_FHS=\"";
    let start = text.find(marker)? + marker.len();
    let end = start + text[start..].find('"')?;
    let p = PathBuf::from(&text[start..end]);
    p.is_file().then_some(p)
}

/// Naive JSON string-field lookup — enough for launch.json, no dep needed.
fn json_string(text: &str, key: &str) -> Option<String> {
    let at = text.find(&format!("\"{key}\""))?;
    let rest = &text[at + key.len() + 2..];
    let open = rest.find('"')? + 1;
    let close = open + rest[open..].find('"')?;
    Some(rest[open..close].to_string())
}

fn which(bin: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(bin))
            .find(|p| p.is_file())
    })
}

// ── the display ──────────────────────────────────────────────────────────
//
// `daw::test::VirtualDisplay` is the shared harness (see the reaper-testing
// skill): a private Xvfb, a window manager on it, dialog sweeping, xdotool
// input and a screenshot recorder. This module had its own copy briefly;
// having two implementations of "run REAPER on a private display" is how
// hard-won details like the FHS wrapper and the WM requirement end up fixed
// in one of them and not the other.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_replace_existing_keys_in_place() {
        let ini = "[REAPER]\nvstpath=~/.vst\nother=keep\n\n[other]\nvstpath=notme\n";
        let out = patch_reaper_section(&ini, &[("vstpath", String::new())]);
        assert!(out.contains("vstpath=\n"));
        assert!(out.contains("other=keep"));
        // The same key in another section is not ours to touch.
        assert!(out.contains("vstpath=notme"));
    }

    #[test]
    fn overrides_append_missing_keys_inside_the_section() {
        let ini = "[REAPER]\nother=keep\n\n[tail]\nx=1\n";
        let out = patch_reaper_section(&ini, &[("verchk", "0".into())]);
        let verchk = out.lines().position(|l| l.starts_with("verchk")).unwrap();
        let tail = out.lines().position(|l| l == "[tail]").unwrap();
        assert!(verchk < tail, "key escaped its section:\n{out}");
    }

    #[test]
    fn the_fake_home_has_every_default_plugin_dir() {
        // Each of these has to *exist* and be empty. A missing directory is
        // not the same as an empty one — REAPER falls back to its own
        // default when the path does not resolve, which is the scan we are
        // trying to avoid.
        let tmp = std::env::temp_dir().join(format!("fts-shot-home-{}", std::process::id()));
        let home = fake_home(&tmp).unwrap();
        for plugins in [".vst", ".vst3", ".lv2", ".clap"] {
            let dir = home.join(plugins);
            assert!(dir.is_dir(), "{plugins} must exist");
            assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn screenshot_points_vst_at_an_empty_dir_and_leaves_clap_alone() {
        // CLAP is what we load, and its scan is cheap.
        //
        // The VST paths must be *set to somewhere empty*, not blanked: an
        // empty value reads as unset and REAPER scans its defaults, which
        // is how a cold run ended up photographing a scan dialog.
        let values = Overrides::for_screenshot(Path::new("/tmp/empty"));
        for key in ["vstpath", "vstpath64", "lv2path_linux"] {
            let v = values
                .iter()
                .find(|(k, _)| *k == key)
                .unwrap_or_else(|| panic!("{key} not overridden"));
            assert_eq!(v.1, "/tmp/empty", "{key} must point somewhere empty");
        }
        assert!(!values.iter().any(|(k, _)| k.contains("clap")));
    }

    #[test]
    fn restore_puts_the_original_back() {
        let dir = std::env::temp_dir().join(format!("fts-shot-ini-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ini = dir.join("reaper.ini");
        let original = "[REAPER]\nvstpath=~/.vst;~/.vst3\n";
        std::fs::write(&ini, original).unwrap();

        {
            let _g = Overrides::apply(
                &ini,
                &Overrides::for_screenshot(Path::new("/tmp/empty")),
                true,
            )
            .unwrap();
            assert!(
                std::fs::read_to_string(&ini)
                    .unwrap()
                    .contains("vstpath=/tmp/empty\n")
            );
        }
        // Dropped — a run must never leave a real profile without its paths.
        assert_eq!(std::fs::read_to_string(&ini).unwrap(), original);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn no_restore_leaves_the_throwaway_profile_alone() {
        let dir = std::env::temp_dir().join(format!("fts-shot-nore-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ini = dir.join("reaper.ini");
        std::fs::write(&ini, "[REAPER]\nvstpath=x\n").unwrap();
        {
            let _g = Overrides::apply(
                &ini,
                &Overrides::for_screenshot(Path::new("/tmp/empty")),
                false,
            )
            .unwrap();
        }
        assert!(
            std::fs::read_to_string(&ini)
                .unwrap()
                .contains("vstpath=/tmp/empty\n")
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn project_has_a_track_per_spec_with_its_colour() {
        let rpp = project_rpp(&[TrackSpec::new("Kick", Rgb::new(0xe0, 0x56, 0x7a))]);
        assert_eq!(rpp.matches("<TRACK").count(), 1);
        assert!(rpp.contains("NAME \"Kick\""));
        // 0x01000000 | BGR(0x7a56e0)
        let want = 0x0100_0000_u32 | 0x007a_56e0;
        assert!(rpp.contains(&format!("PEAKCOL {want}")), "{rpp}");
    }

    #[test]
    fn project_quotes_survive_a_track_name() {
        let rpp = project_rpp(&[TrackSpec::new("The \"Big\" One", Rgb::new(0, 0, 0))]);
        // An unescaped quote would truncate the NAME field and corrupt the RPP.
        assert!(rpp.contains("NAME \"The 'Big' One\""), "{rpp}");
    }
}
