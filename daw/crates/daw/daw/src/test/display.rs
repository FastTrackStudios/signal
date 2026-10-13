//! A private, screenshottable X display for GUI tests.
//!
//! REAPER cannot open a Dioxus panel headless — creating the window
//! aborts it inside GDK and takes the daw socket down with it, so every
//! later test in the run fails with a socket timeout. One missing
//! display presents as a dozen broken tests.
//!
//! So GUI tests get their own [`VirtualDisplay`]: an Xvfb, a window
//! manager on top of it, and a screenshot method. Nothing touches the
//! developer's own desktop, and the run is reproducible on a machine
//! with no monitor.
//!
//! **The window manager is not optional.** A bare Xvfb has no WM, so
//! windows are unmanaged: nothing positions or stacks them, they get no
//! decorations, and REAPER's Actions List ends up covering the screen
//! with the panel buried underneath. A root-window capture of an
//! unmanaged Xvfb does not show what a user would see.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Window managers tried in order. All are tiny and none need a
/// session bus, a compositor, or configuration.
const WINDOW_MANAGERS: [&str; 4] = [
    "openbox",
    "fluxbox",
    "herbstluftwm",
    "matchbox-window-manager",
];

/// An Xvfb plus a window manager, torn down on drop.
pub struct VirtualDisplay {
    display: String,
    xvfb: Option<Child>,
    wm: Option<Child>,
    /// True when we attached to a display someone else started, in
    /// which case tearing it down would be rude.
    borrowed: bool,
}

impl VirtualDisplay {
    /// Start a display, reusing `display` if something is already on it.
    ///
    /// `geometry` is Xvfb's screen spec, e.g. `"1920x1200x24"`.
    pub fn start(display: &str, geometry: &str) -> std::io::Result<Self> {
        let socket = PathBuf::from(format!(
            "/tmp/.X11-unix/X{}",
            display.trim_start_matches(':')
        ));
        if socket.exists() && !Self::display_responds(display) {
            // A killed Xvfb leaves its socket behind, so "the file exists"
            // is not "a server is listening". Borrowing a dead display looks
            // like success right up until the capture, which then fails with
            // imagemagick's unhelpful "missing an image filename" — a long
            // way from the actual cause. Clear it and start our own.
            let _ = std::fs::remove_file(&socket);
        }
        if socket.exists() {
            // Someone else's display: use it, but do not own it.
            let mut vd = Self {
                display: display.to_string(),
                xvfb: None,
                wm: None,
                borrowed: true,
            };
            vd.start_window_manager();
            return Ok(vd);
        }

        let xvfb = Command::new("Xvfb")
            .args([display, "-screen", "0", geometry, "-nolisten", "tcp"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        let mut vd = Self {
            display: display.to_string(),
            xvfb: Some(xvfb),
            wm: None,
            borrowed: false,
        };

        // Wait for the socket rather than sleeping a fixed amount: on a
        // loaded machine Xvfb can take well over a second.
        let deadline = Instant::now() + Duration::from_secs(10);
        while !socket.exists() {
            if Instant::now() > deadline {
                return Err(std::io::Error::other(format!(
                    "Xvfb did not come up on {display} within 10s"
                )));
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        vd.start_window_manager();
        Ok(vd)
    }

    /// Does an X server actually answer on this display?
    ///
    /// `xwininfo` is already a hard dependency of this harness, so probing
    /// costs no new tooling.
    fn display_responds(display: &str) -> bool {
        Command::new("xwininfo")
            .args(["-display", display, "-root"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Convenience: the conventional test display.
    pub fn start_default() -> std::io::Result<Self> {
        let display = std::env::var("FTS_TEST_DISPLAY").unwrap_or_else(|_| ":99".into());
        let geometry = std::env::var("FTS_TEST_GEOMETRY").unwrap_or_else(|_| "1920x1200x24".into());
        Self::start(&display, &geometry)
    }

    fn start_window_manager(&mut self) {
        for wm in WINDOW_MANAGERS {
            let spawned = Command::new(wm)
                .env("DISPLAY", &self.display)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            if let Ok(child) = spawned {
                // Give it a moment to take over as WM before REAPER maps
                // anything, or the first windows come up unmanaged.
                std::thread::sleep(Duration::from_millis(500));
                println!("  Window manager: {wm} on {}", self.display);
                self.wm = Some(child);
                return;
            }
        }
        println!(
            "  WARNING: no window manager found ({}) — windows will be \
             unmanaged and screenshots will not reflect real layout",
            WINDOW_MANAGERS.join(", ")
        );
    }

    /// The `DISPLAY` value to hand to a child process.
    pub fn display(&self) -> &str {
        &self.display
    }

    /// Capture the whole screen.
    pub fn screenshot(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        self.capture(&["-window", "root"], path.as_ref())
    }

    /// Capture one window, found by its `WIDTHxHEIGHT` geometry.
    ///
    /// Useful when another window is on top: a panel is far easier to
    /// identify by size than by title, since embedded views often carry
    /// no useful name.
    pub fn screenshot_window_sized(
        &self,
        width: u32,
        height: u32,
        path: impl AsRef<Path>,
    ) -> std::io::Result<bool> {
        let Some(id) = self.find_window_sized(width, height) else {
            return Ok(false);
        };
        self.capture(&["-window", &id], path.as_ref())?;
        Ok(true)
    }

    /// Capture one window found by title.
    ///
    /// `xdotool search` prints ids in **decimal**, and `import -window`
    /// only accepts hex — handing one straight to the other fails with a
    /// bare "missing an image filename", which reads like a path problem
    /// rather than a window one. Converting here keeps that trap in one
    /// place.
    pub fn screenshot_window_named(
        &self,
        pattern: &str,
        path: impl AsRef<Path>,
    ) -> std::io::Result<bool> {
        // `--onlyvisible`: an unmapped window matches the title search but
        // cannot be captured, and `import` fails on it rather than skipping
        // it.
        //
        // Then take the **largest** window, not the newest. An app's splash
        // and its main window share a title, and which one is "newest"
        // depends entirely on how far into startup the capture lands — so
        // picking by id yields a splash screenshot on a slow boot and the
        // real thing on a fast one. Area is stable.
        let ids: Vec<u64> = self
            .xdotool(&["search", "--onlyvisible", "--name", pattern])
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.trim().parse::<u64>().ok())
            .collect();
        let Some(id) = ids.into_iter().max_by_key(|id| self.window_area(*id)) else {
            return Ok(false);
        };
        self.capture(&["-window", &format!("0x{id:x}")], path.as_ref())?;
        Ok(true)
    }

    /// Pixel area of a window, or 0 if it can't be measured.
    fn window_area(&self, id: u64) -> u64 {
        let out = self
            .xdotool(&["getwindowgeometry", "--shell", &id.to_string()])
            .unwrap_or_default();
        let field = |name: &str| -> u64 {
            out.lines()
                .find_map(|l| l.strip_prefix(name)?.parse::<u64>().ok())
                .unwrap_or(0)
        };
        field("WIDTH=") * field("HEIGHT=")
    }

    /// X window id of the first window matching a geometry.
    pub fn find_window_sized(&self, width: u32, height: u32) -> Option<String> {
        let out = Command::new("xwininfo")
            .args(["-display", &self.display, "-root", "-tree"])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let needle = format!("{width}x{height}");
        text.lines()
            .find(|l| l.contains(&needle))
            .and_then(|l| l.split_whitespace().next())
            .filter(|id| id.starts_with("0x"))
            .map(|id| id.to_string())
    }

    fn capture(&self, args: &[&str], path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let status = Command::new("import")
            .args(["-display", &self.display])
            .args(args)
            .arg(path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(
                "import failed — is imagemagick on PATH?",
            ))
        }
    }

    /// Capture on an interval until the returned handle is dropped.
    ///
    /// A panel opened by a test is up for a second or two, so a
    /// one-shot capture almost always misses it.
    pub fn record(&self, dir: impl AsRef<Path>, interval: Duration) -> Recording {
        let dir = dir.as_ref().to_path_buf();
        let _ = std::fs::create_dir_all(&dir);
        let display = self.display.clone();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = stop.clone();
        let vd = Self {
            display: display.clone(),
            xvfb: None,
            wm: None,
            // A borrowed handle for the capture thread: it must never
            // tear down a display it does not own.
            borrowed: true,
        };
        let handle = std::thread::spawn(move || {
            let mut i = 0usize;
            while !flag.load(std::sync::atomic::Ordering::Relaxed) {
                i += 1;
                // REAPER restores whatever dialogs were open when its
                // config was saved, and it does so *after* it starts —
                // so closing them once up front is too early. Keep
                // sweeping for the first few seconds instead.
                if i <= 20 {
                    vd.close_stray_dialogs();
                }
                let path = dir.join(format!("{i:04}.png"));
                let _ = Command::new("import")
                    .args(["-display", &display, "-window", "root"])
                    .arg(&path)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                std::thread::sleep(interval);
            }
        });
        Recording {
            stop,
            handle: Some(handle),
        }
    }

    /// Whether the tooling this needs is actually installed.
    ///
    /// Checked up front so a GUI run fails with a clear message instead
    /// of an empty screenshot directory.
    pub fn tooling_available() -> Result<(), String> {
        let mut missing = Vec::new();
        for (bin, what) in [
            ("Xvfb", "xorg-server"),
            ("import", "imagemagick"),
            ("xdotool", "xdotool"),
            ("xwininfo", "xwininfo"),
        ] {
            if which(bin).is_none() {
                missing.push(format!("{bin} ({what})"));
            }
        }
        if missing.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "missing: {}. In this repo: `nix develop` provides them.",
                missing.join(", ")
            ))
        }
    }
}

impl Drop for VirtualDisplay {
    fn drop(&mut self) {
        if let Some(mut wm) = self.wm.take() {
            let _ = wm.kill();
            let _ = wm.wait();
        }
        if self.borrowed {
            return;
        }
        if let Some(mut x) = self.xvfb.take() {
            let _ = x.kill();
            let _ = x.wait();
        }
        let _ = std::fs::remove_file(format!(
            "/tmp/.X{}-lock",
            self.display.trim_start_matches(':')
        ));
    }
}

/// A running screenshot loop. Stops on drop.
pub struct Recording {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Recording {
    /// Stop capturing and delete frames that are effectively blank.
    ///
    /// Most frames are blank — REAPER takes a while to map anything —
    /// and a directory of black PNGs is not worth opening.
    pub fn finish(mut self, dir: impl AsRef<Path>) -> usize {
        self.stop_now();
        let mut kept = 0;
        let Ok(entries) = std::fs::read_dir(dir.as_ref()) else {
            return 0;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "png") {
                continue;
            }
            if mean_brightness(&path).unwrap_or(0.0) < 300.0 {
                let _ = std::fs::remove_file(&path);
            } else {
                kept += 1;
            }
        }
        kept
    }

    fn stop_now(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        self.stop_now();
    }
}

fn mean_brightness(path: &Path) -> Option<f64> {
    let out = Command::new("identify")
        .args(["-format", "%[mean]"])
        .arg(path)
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

fn which(bin: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(bin))
            .find(|p| p.is_file())
    })
}

// ── driving the UI ───────────────────────────────────────────────────
//
// REAPER restores whatever windows were open when its config was last
// saved, so a dialog somebody left open — the Actions list is the usual
// culprit — reappears on every spawn and covers the screen. Rather than
// hand-editing `reaper.ini`, close it the way a person would.

impl VirtualDisplay {
    fn xdotool(&self, args: &[&str]) -> std::io::Result<String> {
        let out = Command::new("xdotool")
            .env("DISPLAY", &self.display)
            .args(args)
            .output()?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Window ids whose title matches `pattern` (an xdotool regex).
    pub fn find_windows(&self, pattern: &str) -> Vec<String> {
        self.xdotool(&["search", "--name", pattern])
            .map(|s| s.lines().map(str::to_string).collect())
            .unwrap_or_default()
    }

    /// Ask a window to close, politely (`WM_DELETE_WINDOW`).
    ///
    /// Returns how many were asked. Politely matters: killing the X
    /// client would take REAPER with it, since these are all its
    /// windows.
    pub fn close_windows_named(&self, pattern: &str) -> usize {
        let ids = self.find_windows(pattern);
        for id in &ids {
            let _ = self.xdotool(&["windowclose", id]);
        }
        if !ids.is_empty() {
            std::thread::sleep(Duration::from_millis(300));
        }
        ids.len()
    }

    /// Close the dialogs REAPER tends to restore over the arrange view.
    ///
    /// Called before a screenshot so the capture shows the thing under
    /// test rather than a leftover dialog.
    pub fn close_stray_dialogs(&self) -> usize {
        [
            "Actions",
            "Preferences",
            "Routing Matrix",
            "Region/Marker Manager",
        ]
        .iter()
        .map(|name| self.close_windows_named(name))
        .sum()
    }

    /// Accept REAPER's "Project Load Warning" modal (pops when a real
    /// project was saved by a different REAPER build/version than the one
    /// running the test — observed opening a real production session with
    /// "11163 elements ... not understood", likely newer fixed-lane/
    /// comping state) by pressing Return, not `windowclose`.
    ///
    /// This dialog only has an "OK" button, so accepting it is safe: the
    /// test never saves the project back to disk, so nothing about
    /// dismissal touches the real file. `windowclose` (WM_DELETE) was
    /// tried first and crashed REAPER's GDK backend outright ("GdkWindow
    /// unexpectedly destroyed" / X BadDrawable) — a WM-level destroy
    /// request racing GDK's own surface teardown, apparently unlike a
    /// plain keypress the dialog's own event loop handles itself. Without
    /// a display to dismiss it from at all (pure headless), the dialog
    /// blocks REAPER's main thread until an operator clicks it, which
    /// starves every RPC and looks exactly like a hung connection — hence
    /// `--virtual`/`--gui` plus the background poll loop in
    /// `TestRunner::run_reaper_tests` that calls this.
    pub fn accept_project_load_warning(&self) -> bool {
        if !self.focus_window_named("Project Load Warning") {
            return false;
        }
        self.key("Return");
        true
    }

    /// Raise and focus a window by title.
    pub fn focus_window_named(&self, pattern: &str) -> bool {
        let Some(id) = self.find_windows(pattern).into_iter().next() else {
            return false;
        };
        let _ = self.xdotool(&["windowactivate", &id]);
        let _ = self.xdotool(&["windowraise", &id]);
        std::thread::sleep(Duration::from_millis(200));
        true
    }

    /// Send a key chord, e.g. `"Escape"`, `"ctrl+s"`.
    pub fn key(&self, chord: &str) {
        let _ = self.xdotool(&["key", "--clearmodifiers", chord]);
        std::thread::sleep(Duration::from_millis(120));
    }

    /// Type text into the focused window.
    pub fn type_text(&self, text: &str) {
        let _ = self.xdotool(&["type", "--clearmodifiers", "--delay", "20", text]);
        std::thread::sleep(Duration::from_millis(120));
    }

    /// Click at screen coordinates. `button` is 1=left, 2=middle,
    /// 3=right.
    pub fn click(&self, x: i32, y: i32, button: u8) {
        let _ = self.xdotool(&["mousemove", &x.to_string(), &y.to_string()]);
        let _ = self.xdotool(&["click", &button.to_string()]);
        std::thread::sleep(Duration::from_millis(150));
    }

    /// Click at coordinates *within* a window, found by geometry.
    ///
    /// The offset is relative to the window, so a test can aim at a
    /// panel's own controls without knowing where the WM put it.
    pub fn click_in_window_sized(&self, w: u32, h: u32, dx: i32, dy: i32, button: u8) -> bool {
        let Some(id) = self.find_window_sized(w, h) else {
            return false;
        };
        let Ok(geom) = self.xdotool(&["getwindowgeometry", "--shell", &id]) else {
            return false;
        };
        let get = |key: &str| -> Option<i32> {
            geom.lines().find_map(|l| l.strip_prefix(key)?.parse().ok())
        };
        let (Some(x), Some(y)) = (get("X="), get("Y=")) else {
            return false;
        };
        self.click(x + dx, y + dy, button);
        true
    }

    /// Drag within a window, for gestures a click cannot express.
    pub fn drag_in_window_sized(&self, w: u32, h: u32, from: (i32, i32), to: (i32, i32)) -> bool {
        let Some(id) = self.find_window_sized(w, h) else {
            return false;
        };
        let Ok(geom) = self.xdotool(&["getwindowgeometry", "--shell", &id]) else {
            return false;
        };
        let get = |key: &str| -> Option<i32> {
            geom.lines().find_map(|l| l.strip_prefix(key)?.parse().ok())
        };
        let (Some(ox), Some(oy)) = (get("X="), get("Y=")) else {
            return false;
        };
        let _ = self.xdotool(&[
            "mousemove",
            &(ox + from.0).to_string(),
            &(oy + from.1).to_string(),
        ]);
        let _ = self.xdotool(&["mousedown", "1"]);
        // Intermediate moves: a single jump reads as a teleport and many
        // drag handlers never see it as a drag at all.
        for i in 1..=8 {
            let f = i as f32 / 8.0;
            let x = ox + from.0 + ((to.0 - from.0) as f32 * f) as i32;
            let y = oy + from.1 + ((to.1 - from.1) as f32 * f) as i32;
            let _ = self.xdotool(&["mousemove", &x.to_string(), &y.to_string()]);
            std::thread::sleep(Duration::from_millis(30));
        }
        let _ = self.xdotool(&["mouseup", "1"]);
        std::thread::sleep(Duration::from_millis(150));
        true
    }

    /// Whether xdotool is present, for callers that want to skip
    /// interaction rather than fail.
    pub fn can_send_input(&self) -> bool {
        which("xdotool").is_some()
    }
}
