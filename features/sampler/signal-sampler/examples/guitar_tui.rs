//! Live guitar-rig TUI — input/output meters + patch switching + audio device
//! selection (ratatui).
//!
//! ```text
//! cargo run -p signal-sampler --features pipewire --example guitar_tui -- --rig "Guitar Rig"
//! # (via just: `just guitar`)
//! ```
//!
//! Keys: `1`-`9` pick patch · `←`/`→` or `p`/`n` prev/next · `b`/space bypass ·
//! `[`/`]` buffer · `s` audio settings · `q`/Esc quit. The INPUT meter shows
//! your guitar level reaching the rig; the OUTPUT meter shows what the rig is
//! sending out. If INPUT stays flat while you play, open `s` and check the
//! interface + input channel are the ones your guitar is plugged into.
//!
//! All audio-backend mechanics (host selection, device targeting, per-channel
//! routing, buffer/quantum control) live in `daw` — this TUI only expresses
//! intent (which device / channel / output) through [`RigManager`] prefs and
//! asks daw to (re)open.

use std::path::PathBuf;
use std::time::Duration;

use daw_audio_io::pw::{self, ClockSettings, DeviceLatency};
use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph};
use signal_sampler::{DeviceInfo, GuitarRig, Library, ProfileRig, RigManager};

fn arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn main() -> eyre::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let name = arg(&args, "--rig").unwrap_or_else(|| "Guitar Rig".to_string());

    // Open the rig BEFORE taking over the terminal so any error prints normally.
    // The rig requests its low latency PER-NODE (the duplex node's `node.latency`,
    // set from `buffer_size`), NOT a graph-wide `force_quantum`. PipeWire then
    // runs only the rig's driver graph (its selected device) hot, while other
    // devices — everyday audio over USB — stay at their own relaxed quantum.
    let mut mgr = RigManager::load(&name);
    let mut prig = mgr.open()?;

    // Library root for the Preset / Profile / Song browser — the same root the
    // rig loaded its profile from (so browser + rig agree). `--library <dir>`
    // overrides.
    let lib_root = arg(&args, "--library")
        .map(PathBuf::from)
        .unwrap_or_else(|| mgr.library_root());
    let mut browser = Browser::load(lib_root);

    // If the rig opened with no profile (e.g. a fresh "Guitar Rig"), seed it with
    // the library's first profile so there's something to play + browse from.
    if prig.patches().is_empty() {
        browser.load_first_profile(&mut prig);
    }

    let mut settings = Settings::load(&mgr);

    // ratatui draws to stdout; send any stray stderr (engine/library logs that
    // fire on a re-open) to a log file so it can't corrupt the render.
    redirect_stderr_to_log();

    let mut term = ratatui::init();
    let res = run(
        &mut term,
        &mut prig,
        &mut mgr,
        &mut settings,
        &mut browser,
        &name,
    );
    ratatui::restore();
    res
}

// ── Preset / Profile / Song browser ──────────────────────────────────────────

/// Which browser column has focus.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pane {
    Presets,
    Profiles,
    Songs,
}

impl Pane {
    fn title(self) -> &'static str {
        match self {
            Pane::Presets => "Presets",
            Pane::Profiles => "Profiles",
            Pane::Songs => "Songs",
        }
    }
    fn next(self) -> Self {
        match self {
            Pane::Presets => Pane::Profiles,
            Pane::Profiles => Pane::Songs,
            Pane::Songs => Pane::Presets,
        }
    }
    fn prev(self) -> Self {
        match self {
            Pane::Presets => Pane::Songs,
            Pane::Profiles => Pane::Presets,
            Pane::Songs => Pane::Profiles,
        }
    }
}

/// The library browser overlay: the in-memory catalog plus per-pane selection.
/// Selecting a Preset / Profile / Song and pressing Enter resolves it to a
/// playable `RigProfile` and loads it into the live rig.
struct Browser {
    lib: Library,
    open: bool,
    pane: Pane,
    presets: ListState,
    profiles: ListState,
    songs: ListState,
    /// One-line status (what was loaded / any error).
    status: Option<String>,
}

impl Browser {
    fn load(root: PathBuf) -> Self {
        let lib = Library::load_dir(&root);
        let sel = |n: usize| {
            let mut s = ListState::default();
            if n > 0 {
                s.select(Some(0));
            }
            s
        };
        Self {
            presets: sel(lib.presets.len()),
            profiles: sel(lib.profiles.len()),
            songs: sel(lib.songs.len()),
            status: (!lib.errors.is_empty()).then(|| format!("{} load error(s)", lib.errors.len())),
            lib,
            open: false,
            pane: Pane::Profiles,
        }
    }

    /// Load the library's first profile into the rig (startup seeding).
    fn load_first_profile(&mut self, prig: &mut ProfileRig) {
        if self.lib.profiles.is_empty() {
            return;
        }
        self.profiles.select(Some(0));
        self.pane = Pane::Profiles;
        self.load_selected(prig);
    }

    fn state_for(&mut self, pane: Pane) -> &mut ListState {
        match pane {
            Pane::Presets => &mut self.presets,
            Pane::Profiles => &mut self.profiles,
            Pane::Songs => &mut self.songs,
        }
    }

    fn len_for(&self, pane: Pane) -> usize {
        match pane {
            Pane::Presets => self.lib.presets.len(),
            Pane::Profiles => self.lib.profiles.len(),
            Pane::Songs => self.lib.songs.len(),
        }
    }

    fn move_sel(&mut self, delta: i32) {
        let pane = self.pane;
        let len = self.len_for(pane);
        if len == 0 {
            return;
        }
        let st = self.state_for(pane);
        let cur = st.selected().unwrap_or(0) as i32;
        let next = (cur + delta).rem_euclid(len as i32) as usize;
        st.select(Some(next));
    }

    /// Resolve + load the focused entity into the live rig.
    fn load_selected(&mut self, prig: &mut ProfileRig) {
        let base = Some(self.lib.root.as_path());
        let (label, profile) = match self.pane {
            Pane::Presets => {
                let Some(p) = self
                    .presets
                    .selected()
                    .and_then(|i| self.lib.presets.get(i))
                else {
                    return;
                };
                (format!("preset {}", p.name), self.lib.preset_as_profile(p))
            }
            Pane::Profiles => {
                let Some(p) = self
                    .profiles
                    .selected()
                    .and_then(|i| self.lib.profiles.get(i))
                else {
                    return;
                };
                (format!("profile {}", p.name), self.lib.resolve_profile(p))
            }
            Pane::Songs => {
                let Some(s) = self.songs.selected().and_then(|i| self.lib.songs.get(i)) else {
                    return;
                };
                (format!("song {}", s.name), self.lib.song_as_profile(s))
            }
        };
        match prig.load_profile(profile, base) {
            Ok(()) => self.status = Some(format!("loaded {label}")),
            Err(e) => self.status = Some(format!("load {label} failed: {e}")),
        }
    }
}

/// Point the process's stderr at `$TMPDIR/fts-signal-rig.log` for the rest of
/// the run. ratatui owns stdout; without this, an engine/library line written
/// to stderr on a device/latency re-open would scribble over the TUI. Tracing
/// (which the rig uses instead of prints) lands in this log too.
fn redirect_stderr_to_log() {
    use std::os::fd::AsRawFd;
    let path = std::env::temp_dir().join("fts-signal-rig.log");
    if let Ok(f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        // SAFETY: dup2 onto STDERR_FILENO is the standard fd-redirect; `f`'s fd
        // is valid here. Leak `f` so the fd stays open for the process lifetime.
        unsafe { libc::dup2(f.as_raw_fd(), libc::STDERR_FILENO) };
        std::mem::forget(f);
    }
}

/// Buffer sizes selectable at runtime (frames).
const BUFFERS: [u32; 6] = [32, 64, 128, 256, 512, 1024];

/// Step the buffer size up (`+1`) or down (`-1`) and persist. The new size is
/// the rig's per-node latency request, applied on the next [`reopen`] (the
/// caller re-opens) — not a graph-wide quantum force.
fn step_buffer(mgr: &mut RigManager, dir: i32) {
    let cur = mgr.audio.buffer_size;
    let idx = BUFFERS.iter().position(|&b| b >= cur).unwrap_or(2) as i32;
    let nidx = (idx + dir).clamp(0, BUFFERS.len() as i32 - 1) as usize;
    mgr.audio.buffer_size = BUFFERS[nidx];
    let _ = mgr.save();
}

// ── Audio device settings overlay ────────────────────────────────────────────

/// The audio-settings picker: enumerated devices + the current selection. The
/// device lists come from daw (`GuitarRig::input_devices`/`output_devices`); the
/// `"(default)"` head entry maps to an empty device string (system default).
struct Settings {
    open: bool,
    inputs: Vec<DeviceChoice>,
    outputs: Vec<DeviceChoice>,
    sel_input: usize,
    sel_channel: usize,
    sel_output: usize,
    /// Index into [`BUFFERS`] — the graph buffer (quantum / period) size.
    sel_buffer: usize,
    /// Sample rates the server allows forcing (`clock.allowed-rates`).
    rates: Vec<u32>,
    sel_rate: usize,
    /// Staged per-device ALSA buffering (applied on Enter via a WirePlumber
    /// drop-in + session-manager restart — not live).
    headroom: u32,
    period_num: u32,
    period_size: u32,
    /// True once a device-buffering field was changed, so Enter knows to write
    /// the drop-in + restart WirePlumber (a brief audio drop).
    device_dirty: bool,
    /// Live readouts for the latency breakdown (the input/output ALSA nodes and
    /// the global clock), refreshed when the panel opens.
    clock: ClockSettings,
    cap: Option<DeviceLatency>,
    play: Option<DeviceLatency>,
    /// Focused row (see [`Field`]).
    field: usize,
}

/// Settings rows, in display order.
mod field {
    pub const INPUT_DEV: usize = 0;
    pub const INPUT_CH: usize = 1;
    pub const OUTPUT_DEV: usize = 2;
    pub const BUFFER: usize = 3;
    pub const RATE: usize = 4;
    pub const HEADROOM: usize = 5;
    pub const PERIOD_NUM: usize = 6;
    pub const PERIOD_SIZE: usize = 7;
    pub const COUNT: usize = 8;
}
use field::COUNT as FIELDS;

/// Selectable values for the staged device-buffering fields. `0` on the period
/// rows means "device default" (the rule omits it).
const HEADROOMS: [u32; 9] = [0, 32, 64, 128, 256, 512, 1024, 2048, 4096];
const PERIOD_NUMS: [u32; 8] = [0, 2, 3, 4, 8, 16, 32, 64];
const PERIOD_SIZES: [u32; 8] = [0, 16, 32, 64, 128, 256, 512, 1024];

/// Nearest [`BUFFERS`] index at or above `frames` (clamped to the table).
fn buffer_index(frames: u32) -> usize {
    BUFFERS
        .iter()
        .position(|&b| b >= frames)
        .unwrap_or(BUFFERS.len() - 1)
}

/// Step `cur` to the next/previous value in `table` (snaps to nearest first, so
/// an off-table device default still moves sensibly).
fn step_table(cur: u32, table: &[u32], dir: i32) -> u32 {
    let idx = table
        .iter()
        .position(|&v| v >= cur)
        .unwrap_or(table.len() - 1) as i32;
    let n = table[(idx + dir).clamp(0, table.len() as i32 - 1) as usize];
    n
}

/// One pickable device — display name + channel count, plus the device-substring
/// to persist (empty for the system default).
#[derive(Clone)]
struct DeviceChoice {
    label: String,
    device: String,
    channels: u16,
}

impl Settings {
    fn load(mgr: &RigManager) -> Self {
        let inputs = device_choices(&GuitarRig::input_devices(), 2);
        let outputs = device_choices(&GuitarRig::output_devices(), 2);
        let sel_input = inputs
            .iter()
            .position(|c| c.device == mgr.audio.input_device)
            .unwrap_or(0);
        let sel_output = outputs
            .iter()
            .position(|c| c.device == mgr.audio.output_device)
            .unwrap_or(0);
        let mut s = Self {
            open: false,
            sel_channel: mgr.audio.input_channel.min(channel_max(&inputs, sel_input)),
            sel_buffer: buffer_index(mgr.audio.buffer_size),
            rates: Vec::new(),
            sel_rate: 0,
            headroom: 0,
            period_num: 0,
            period_size: 0,
            device_dirty: false,
            clock: ClockSettings::default(),
            cap: None,
            play: None,
            inputs,
            outputs,
            sel_input,
            sel_output,
            field: 0,
        };
        s.reload_pw(mgr);
        s
    }

    /// Re-read the live PipeWire state: clock settings + allowed rates, and the
    /// input/output devices' ALSA buffering (which seeds the editable headroom /
    /// period fields). Clears the dirty flag.
    fn reload_pw(&mut self, mgr: &RigManager) {
        self.clock = pw::clock_settings();
        self.rates = if self.clock.allowed_rates.is_empty() {
            vec![self.clock.rate.max(48_000)]
        } else {
            self.clock.allowed_rates.clone()
        };
        let want_rate = if mgr.audio.sample_rate != 0 {
            mgr.audio.sample_rate
        } else {
            self.clock.rate
        };
        self.sel_rate = self.rates.iter().position(|&r| r == want_rate).unwrap_or(0);
        self.cap = pw::device_latency(&mgr.audio.input_device, true);
        self.play = pw::device_latency(&mgr.audio.output_device, false);
        if let Some(c) = &self.cap {
            self.headroom = c.headroom;
            self.period_num = c.period_num;
            self.period_size = c.period_size;
        }
        self.device_dirty = false;
    }

    /// Re-enumerate devices (interfaces may have been plugged in/out), resync the
    /// buffer from the manager, and re-read the live PipeWire state.
    fn refresh(&mut self, mgr: &RigManager) {
        self.inputs = device_choices(&GuitarRig::input_devices(), 2);
        self.outputs = device_choices(&GuitarRig::output_devices(), 2);
        self.sel_input = self.sel_input.min(self.inputs.len().saturating_sub(1));
        self.sel_output = self.sel_output.min(self.outputs.len().saturating_sub(1));
        self.sel_channel = self
            .sel_channel
            .min(channel_max(&self.inputs, self.sel_input));
        self.sel_buffer = buffer_index(mgr.audio.buffer_size);
        self.reload_pw(mgr);
    }

    /// The currently selected buffer size in frames.
    fn buffer(&self) -> u32 {
        BUFFERS[self.sel_buffer]
    }

    /// The currently selected sample rate in Hz (0 if none enumerated).
    fn rate(&self) -> u32 {
        self.rates.get(self.sel_rate).copied().unwrap_or(0)
    }

    /// Channel count of the selected input device (≥ 1).
    fn input_channels(&self) -> usize {
        channel_max(&self.inputs, self.sel_input) + 1
    }

    /// Adjust the focused field by `dir` (−1/+1).
    fn adjust(&mut self, dir: i32) {
        match self.field {
            field::INPUT_DEV => {
                self.sel_input = wrap(self.sel_input, self.inputs.len(), dir);
                self.sel_channel = self
                    .sel_channel
                    .min(channel_max(&self.inputs, self.sel_input));
            }
            field::INPUT_CH => {
                let max = channel_max(&self.inputs, self.sel_input);
                self.sel_channel = wrap(self.sel_channel, max + 1, dir);
            }
            field::OUTPUT_DEV => self.sel_output = wrap(self.sel_output, self.outputs.len(), dir),
            field::BUFFER => self.sel_buffer = wrap(self.sel_buffer, BUFFERS.len(), dir),
            field::RATE => self.sel_rate = wrap(self.sel_rate, self.rates.len(), dir),
            field::HEADROOM => {
                self.headroom = step_table(self.headroom, &HEADROOMS, dir);
                self.device_dirty = true;
            }
            field::PERIOD_NUM => {
                self.period_num = step_table(self.period_num, &PERIOD_NUMS, dir);
                self.device_dirty = true;
            }
            field::PERIOD_SIZE => {
                self.period_size = step_table(self.period_size, &PERIOD_SIZES, dir);
                self.device_dirty = true;
            }
            _ => {}
        }
    }

    /// Write the routing + clock selection into the manager's prefs (device
    /// buffering is applied separately via the WirePlumber drop-in).
    fn apply_to(&self, mgr: &mut RigManager) {
        if let Some(c) = self.inputs.get(self.sel_input) {
            mgr.audio.input_device = c.device.clone();
        }
        mgr.audio.input_channel = self.sel_channel;
        if let Some(c) = self.outputs.get(self.sel_output) {
            mgr.audio.output_device = c.device.clone();
        }
        mgr.audio.buffer_size = self.buffer();
        if self.rate() != 0 {
            mgr.audio.sample_rate = self.rate();
        }
    }
}

/// Wrap `i` by `dir` within `[0, len)` (no-op when empty).
fn wrap(i: usize, len: usize, dir: i32) -> usize {
    if len == 0 {
        return 0;
    }
    let n = len as i32;
    (((i as i32 + dir) % n + n) % n) as usize
}

/// Highest 0-based channel index for the input device at `idx` (≥ 0).
fn channel_max(inputs: &[DeviceChoice], idx: usize) -> usize {
    inputs
        .get(idx)
        .map(|c| c.channels.max(1) as usize - 1)
        .unwrap_or(0)
}

/// Collapse daw's raw device list into pickable choices: a `"(system default)"`
/// head entry (empty device string) followed by named devices, deduped by name
/// keeping the highest channel count, dropping PipeWire's synthetic `*default`
/// nodes. `default_channels` is the channel count assumed for the default entry.
fn device_choices(devs: &[DeviceInfo], default_channels: u16) -> Vec<DeviceChoice> {
    let mut out = vec![DeviceChoice {
        label: "(system default)".to_string(),
        device: String::new(),
        channels: default_channels,
    }];
    for d in devs {
        let lname = d.name.to_lowercase();
        if d.name.is_empty() || lname.contains("default") || lname == "unknown" {
            continue; // PipeWire's synthetic default/unknown nodes
        }
        // Dedupe by name, keep the variant exposing the most channels.
        if let Some(existing) = out.iter_mut().find(|c| c.label == d.name) {
            existing.channels = existing.channels.max(d.channels);
        } else {
            out.push(DeviceChoice {
                label: d.name.clone(),
                device: d.name.clone(),
                channels: d.channels,
            });
        }
    }
    out
}

/// Re-open the rig with the manager's current audio prefs. The new buffer/rate
/// take effect here as the duplex node's own `node.latency` request (set when
/// the engine rebuilds) — NOT a graph-wide quantum force, so only the rig's
/// device runs at this latency and other devices are untouched. Replaces `prig`
/// in place; the old engine is torn down on drop.
fn reopen(prig: &mut ProfileRig, mgr: &RigManager) -> eyre::Result<()> {
    *prig = mgr.open()?;
    Ok(())
}

/// Commit the settings panel: persist routing/buffer/rate prefs; if a device
/// buffering field changed, write the WirePlumber drop-in and restart the
/// session manager (re-creating the device nodes — a brief audio drop) before
/// re-opening the rig. Returns a one-line status for the footer.
fn apply_settings(settings: &Settings, prig: &mut ProfileRig, mgr: &mut RigManager) -> String {
    settings.apply_to(mgr);
    let _ = mgr.save();

    let mut note = String::new();
    if settings.device_dirty && !mgr.audio.input_device.is_empty() {
        match pw::write_device_latency(
            &mgr.audio.input_device,
            settings.period_size,
            settings.period_num,
            settings.headroom,
        ) {
            Ok(_) => {
                if pw::restart_session_manager() {
                    // WirePlumber re-creates the device nodes; give them a beat
                    // to reappear before the engine grabs them.
                    std::thread::sleep(Duration::from_millis(1500));
                    note = format!(
                        " · device hr{} pn{} ps{} applied",
                        settings.headroom, settings.period_num, settings.period_size
                    );
                } else {
                    note = " · wrote config (restart wireplumber to apply)".into();
                }
            }
            Err(e) => note = format!(" · device config write failed: {e}"),
        }
    }

    match reopen(prig, mgr) {
        Ok(()) => format!(
            "opened {} ch{}  @ {} Hz  buf {}{note}",
            dev_label(&mgr.audio.input_device),
            mgr.audio.input_channel + 1,
            mgr.audio.sample_rate,
            mgr.audio.buffer_size,
        ),
        Err(e) => format!("open failed: {e}{note}"),
    }
}

fn run(
    term: &mut ratatui::DefaultTerminal,
    prig: &mut ProfileRig,
    mgr: &mut RigManager,
    settings: &mut Settings,
    browser: &mut Browser,
    rig_name: &str,
) -> eyre::Result<()> {
    let mut status: Option<String> = None;
    loop {
        term.draw(|f| ui(f, prig, mgr, settings, browser, rig_name, status.as_deref()))?;

        if event::poll(Duration::from_millis(33))? {
            if let Event::Key(k) = event::read()? {
                if k.kind != KeyEventKind::Press {
                    continue;
                }
                // Library browser swallows input while open.
                if browser.open {
                    match k.code {
                        KeyCode::Char('l') | KeyCode::Esc => browser.open = false,
                        KeyCode::Char('q') => break,
                        KeyCode::Tab | KeyCode::Right => browser.pane = browser.pane.next(),
                        KeyCode::BackTab | KeyCode::Left => browser.pane = browser.pane.prev(),
                        KeyCode::Up | KeyCode::Char('k') => browser.move_sel(-1),
                        KeyCode::Down | KeyCode::Char('j') => browser.move_sel(1),
                        KeyCode::Enter => {
                            term.draw(|f| {
                                ui(f, prig, mgr, settings, browser, rig_name, Some("loading…"))
                            })?;
                            browser.load_selected(prig);
                            status = browser.status.clone();
                        }
                        _ => {}
                    }
                    continue;
                }
                if settings.open {
                    match k.code {
                        KeyCode::Char('s') | KeyCode::Esc => settings.open = false,
                        KeyCode::Char('q') => break,
                        KeyCode::Up | KeyCode::Char('k') => {
                            settings.field = settings.field.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            settings.field = (settings.field + 1).min(FIELDS - 1);
                        }
                        // All fields stage their change; Enter commits + re-opens
                        // (buffer/rate become the duplex node's latency request).
                        KeyCode::Left | KeyCode::Char('h') => settings.adjust(-1),
                        KeyCode::Right | KeyCode::Char('l') => settings.adjust(1),
                        KeyCode::Char('r') => settings.refresh(mgr),
                        KeyCode::Enter => {
                            term.draw(|f| {
                                ui(f, prig, mgr, settings, browser, rig_name, Some("applying…"))
                            })?;
                            status = Some(apply_settings(settings, prig, mgr));
                            settings.refresh(mgr);
                            settings.open = false;
                        }
                        _ => {}
                    }
                    continue;
                }
                match k.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('l') => {
                        browser.open = true;
                    }
                    KeyCode::Char('s') => {
                        settings.refresh(mgr);
                        settings.open = true;
                    }
                    KeyCode::Char('b') | KeyCode::Char(' ') => {
                        let b = !prig.rig().is_bypassed();
                        prig.rig().set_bypass(b);
                    }
                    // 0 = global time-bypass (kills the Time module on the active
                    // patch — delay/reverb). The "Funk switch". (`t` alias.)
                    KeyCode::Char('0') | KeyCode::Char('t') => {
                        let on = prig.toggle_fx_bypass();
                        status = Some(if on {
                            "time bypass ON".into()
                        } else {
                            "time bypass off".into()
                        });
                    }
                    // 1–8 (and F1–F8) are the footswitch stacks: press to engage
                    // the stack's current patch, press again to rotate within it.
                    KeyCode::Char(c) if ('1'..='8').contains(&c) => {
                        prig.activate_stack(c as usize - '1' as usize);
                    }
                    KeyCode::F(n) if (1..=12).contains(&n) => {
                        prig.activate_stack((n - 1) as usize);
                    }
                    KeyCode::Char('n') | KeyCode::Right => {
                        prig.next_patch();
                    }
                    KeyCode::Char('p') | KeyCode::Left => {
                        prig.prev_patch();
                    }
                    // Buffer = the rig node's latency request; changing it
                    // re-opens (only the rig's device follows, not the graph).
                    KeyCode::Char('[') | KeyCode::Char(']') => {
                        step_buffer(mgr, if k.code == KeyCode::Char('[') { -1 } else { 1 });
                        term.draw(|f| {
                            ui(f, prig, mgr, settings, browser, rig_name, Some("applying…"))
                        })?;
                        status = Some(match reopen(prig, mgr) {
                            Ok(()) => format!("buffer {} fr", mgr.audio.buffer_size),
                            Err(e) => format!("open failed: {e}"),
                        });
                    }
                    KeyCode::Char('r') => prig.rig().reset_render_peak(),
                    KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                        prig.activate(c as usize - '1' as usize);
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

/// Short device label for status text (`"system default"` when empty).
fn dev_label(device: &str) -> &str {
    if device.is_empty() {
        "system default"
    } else {
        device
    }
}

/// One-line audio summary for the header.
fn audio_line(mgr: &RigManager, prig: &ProfileRig) -> String {
    format!(
        "{}  ch{}  @ {} Hz",
        dev_label(&mgr.audio.input_device),
        mgr.audio.input_channel + 1,
        prig.rig().sample_rate
    )
}

fn ui(
    f: &mut Frame,
    prig: &ProfileRig,
    mgr: &RigManager,
    settings: &Settings,
    browser: &mut Browser,
    rig_name: &str,
    status: Option<&str>,
) {
    let rig = prig.rig();
    let has_stacks = !prig.stacks().is_empty();
    let rows = Layout::vertical([
        Constraint::Length(2),                              // header
        Constraint::Length(2),                              // patches
        Constraint::Length(if has_stacks { 2 } else { 0 }), // stacks (footswitches)
        Constraint::Length(3),                              // input meter
        Constraint::Length(3),                              // output meter
        Constraint::Length(3),                              // dsp load
        Constraint::Length(2),                              // stats (buffer/latency + xruns)
        Constraint::Min(0),                                 // spacer
        Constraint::Length(1),                              // footer
    ])
    .split(f.area());

    // Header
    let bypassed = rig.is_bypassed();
    let header = Line::from(vec![
        Span::styled(
            format!(" {rig_name} "),
            Style::new()
                .fg(Color::Black)
                .bg(Color::Rgb(232, 129, 58))
                .bold(),
        ),
        Span::raw("  "),
        Span::styled(
            prig.profile_name().unwrap_or("(no profile)").to_string(),
            Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Span::raw("   in: "),
        Span::styled(audio_line(mgr, prig), Style::new().fg(Color::DarkGray)),
        if bypassed {
            Span::styled("   [BYPASS]", Style::new().fg(Color::Yellow).bold())
        } else {
            Span::raw("")
        },
        if prig.fx_bypass() {
            Span::styled("   [TIME OFF]", Style::new().fg(Color::Magenta).bold())
        } else {
            Span::raw("")
        },
    ]);
    f.render_widget(Paragraph::new(header), rows[0]);

    // Patches
    let mut spans = vec![Span::styled("patch: ", Style::new().fg(Color::DarkGray))];
    for (i, p) in prig.patches().iter().enumerate() {
        let active = Some(i) == prig.active_index();
        let avail = prig.is_patch_available(i);
        let label = format!(" {}:{} ", i + 1, p.name);
        let style = if active {
            Style::new()
                .fg(Color::Black)
                .bg(Color::Rgb(232, 129, 58))
                .bold()
        } else if avail {
            Style::new().fg(Color::Gray)
        } else {
            Style::new().fg(Color::Red).add_modifier(Modifier::DIM)
        };
        spans.push(Span::styled(label, style));
        spans.push(Span::raw(" "));
    }
    if prig.patches().is_empty() {
        spans.push(Span::styled(
            "(no patches loaded)",
            Style::new().fg(Color::Red),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), rows[1]);

    // Stacks (footswitches) — F1..Fn. The active stack is highlighted; its
    // rotation position shows as "(2/4)".
    if has_stacks {
        let active_stack = prig.active_stack();
        let mut sspans = vec![Span::styled("stack: ", Style::new().fg(Color::DarkGray))];
        for (i, st) in prig.stacks().iter().enumerate() {
            let on = Some(i) == active_stack;
            let pos = prig.stack_position(i);
            let count = st.patches.len();
            let rot = if count > 1 {
                format!(" ({}/{})", pos + 1, count)
            } else {
                String::new()
            };
            let label = format!(" {}:{}{} ", i + 1, st.name, rot);
            let style = if on {
                Style::new()
                    .fg(Color::Black)
                    .bg(Color::Rgb(232, 129, 58))
                    .bold()
            } else {
                Style::new().fg(Color::Gray)
            };
            sspans.push(Span::styled(label, style));
        }
        f.render_widget(Paragraph::new(Line::from(sspans)), rows[2]);
    }

    // Meters
    meter(f, rows[3], "INPUT  (guitar)", rig.input_peak());
    meter(f, rows[4], "OUTPUT (to speakers)", rig.output_peak());

    // DSP load — render time vs the per-block budget (buffer / sample-rate).
    let bf = rig.block_frames().max(1) as f64;
    let sr = rig.sample_rate.max(1) as f64;
    let budget_us = bf / sr * 1e6;
    let render_us = rig.render_us() as f64;
    let peak_us = rig.peak_render_us() as f64;
    let load = (render_us / budget_us).clamp(0.0, 1.0);
    let load_color = if load >= 0.85 {
        Color::Red
    } else if load >= 0.6 {
        Color::Yellow
    } else {
        Color::Green
    };
    let dsp = Gauge::default()
        .block(Block::bordered().title("DSP LOAD"))
        .gauge_style(Style::new().fg(load_color))
        .ratio(load)
        .label(format!(
            "{:.0}%   {:.2}/{:.2} ms   peak {:.2} ms",
            load * 100.0,
            render_us / 1000.0,
            budget_us / 1000.0,
            peak_us / 1000.0
        ));
    f.render_widget(dsp, rows[5]);

    // Stats — buffer + honest latency estimate + xruns.
    let ms = |frames: f64| frames / sr * 1000.0;
    let ring = rig.ring_frames() as f64;
    let sw_ms = ms(bf + ring + bf); // in-block + ring + out-block
    let xruns = rig.underruns() + rig.overruns();
    let stats = vec![
        Line::from(Span::styled(
            format!(
                "buffer {} fr ({:.2} ms)   ring {:.0} fr ({:.2} ms)   {} Hz",
                rig.block_frames(),
                ms(bf),
                ring,
                ms(ring),
                rig.sample_rate
            ),
            Style::new().fg(Color::Gray),
        )),
        Line::from(vec![
            Span::styled(
                format!("≈ {sw_ms:.1} ms software round-trip"),
                Style::new().fg(Color::Cyan),
            ),
            Span::styled(
                "  (+ USB & interface converters on top)   ",
                Style::new().fg(Color::DarkGray),
            ),
            Span::styled(
                format!("xruns u{} o{}", rig.underruns(), rig.overruns()),
                Style::new().fg(if xruns > 0 {
                    Color::Red
                } else {
                    Color::DarkGray
                }),
            ),
        ]),
    ];
    f.render_widget(Paragraph::new(stats), rows[6]);

    // Footer (status line, then key hints)
    let footer = if let Some(s) = status {
        Line::from(Span::styled(format!(" {s}"), Style::new().fg(Color::Green)))
    } else {
        Line::from(Span::styled(
            " [l] browse  [1-8] stack (re-press rotates)  [0] time-bypass  ←/→ patch  [b] bypass  [ / ] buffer  [s] settings  [q] quit",
            Style::new().fg(Color::DarkGray),
        ))
    };
    f.render_widget(Paragraph::new(footer), rows[8]);

    if settings.open {
        settings_overlay(f, settings, rig.sample_rate, rig.ring_frames());
    }
    if browser.open {
        browser_overlay(f, browser);
    }
}

/// The Preset / Profile / Song browser modal: three columns + a detail panel
/// for the focused entity's children (scenes / patches+stacks / sections).
fn browser_overlay(f: &mut Frame, b: &mut Browser) {
    let area = centered(f.area(), 96, 30);
    f.render_widget(Clear, area);
    let outer = Block::bordered()
        .title(" Library — [Tab] pane  ↑/↓ select  [Enter] load → rig  [l/Esc] close ")
        .border_style(Style::new().fg(Color::Rgb(232, 129, 58)));
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let rows = Layout::vertical([Constraint::Min(6), Constraint::Length(10)]).split(inner);
    let cols = Layout::horizontal([
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
    ])
    .split(rows[0]);

    // Snapshot the names/focus before borrowing list state mutably.
    let preset_names: Vec<String> = b.lib.presets.iter().map(|p| p.name.clone()).collect();
    let profile_names: Vec<String> = b.lib.profiles.iter().map(|p| p.name.clone()).collect();
    let song_names: Vec<String> = b.lib.songs.iter().map(|s| s.name.clone()).collect();
    let focus = b.pane;
    render_browser_col(
        f,
        cols[0],
        Pane::Presets,
        &preset_names,
        focus,
        &mut b.presets,
    );
    render_browser_col(
        f,
        cols[1],
        Pane::Profiles,
        &profile_names,
        focus,
        &mut b.profiles,
    );
    render_browser_col(f, cols[2], Pane::Songs, &song_names, focus, &mut b.songs);

    // Detail panel for the focused entity.
    let detail = browser_detail(b);
    let title = format!(" {} ", focus.title());
    f.render_widget(
        Paragraph::new(detail).block(Block::bordered().title(title)),
        rows[1],
    );
}

fn render_browser_col(
    f: &mut Frame,
    area: Rect,
    pane: Pane,
    names: &[String],
    focus: Pane,
    state: &mut ListState,
) {
    let focused = pane == focus;
    let items: Vec<ListItem> = if names.is_empty() {
        vec![ListItem::new(Span::styled(
            "(none)",
            Style::new().fg(Color::DarkGray),
        ))]
    } else {
        names
            .iter()
            .map(|n| ListItem::new(Span::raw(n.clone())))
            .collect()
    };
    let border = if focused {
        Color::Rgb(232, 129, 58)
    } else {
        Color::DarkGray
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(pane.title())
                .border_style(Style::new().fg(border)),
        )
        .highlight_style(
            Style::new()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");
    f.render_stateful_widget(list, area, state);
}

/// Lines describing the focused entity's children.
fn browser_detail(b: &Browser) -> Vec<Line<'static>> {
    let dim = Style::new().fg(Color::DarkGray);
    match b.pane {
        Pane::Presets => {
            let Some(p) = b.presets.selected().and_then(|i| b.lib.presets.get(i)) else {
                return vec![Line::from(Span::styled("no preset selected", dim))];
            };
            let mut lines = vec![Line::from(vec![
                Span::styled(
                    format!("{}  ", p.name),
                    Style::new().fg(Color::White).bold(),
                ),
                Span::styled(format!("{} · {}", p.vendor, p.category), dim),
            ])];
            for (i, s) in p.scenes.iter().enumerate() {
                let mark = if i == p.default_scene { "● " } else { "  " };
                lines.push(Line::from(format!(
                    "{mark}{}  ({} block)",
                    s.name,
                    s.chain.len()
                )));
            }
            lines
        }
        Pane::Profiles => {
            let Some(p) = b.profiles.selected().and_then(|i| b.lib.profiles.get(i)) else {
                return vec![Line::from(Span::styled("no profile selected", dim))];
            };
            let mut lines = vec![Line::from(vec![
                Span::styled(
                    format!("{}  ", p.name),
                    Style::new().fg(Color::White).bold(),
                ),
                Span::styled(
                    format!("{} patches · {} stacks", p.patches.len(), p.stacks.len()),
                    dim,
                ),
            ])];
            for (i, st) in p.stacks.iter().enumerate() {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{}:{}  ", i + 1, st.name),
                        Style::new().fg(Color::Rgb(232, 129, 58)),
                    ),
                    Span::raw(st.patches.join(" · ")),
                ]));
            }
            lines
        }
        Pane::Songs => {
            let Some(s) = b.songs.selected().and_then(|i| b.lib.songs.get(i)) else {
                return vec![Line::from(Span::styled("no song selected", dim))];
            };
            let mut lines = vec![Line::from(vec![
                Span::styled(
                    format!("{}  ", s.name),
                    Style::new().fg(Color::White).bold(),
                ),
                Span::styled(s.artist.clone(), dim),
            ])];
            for sec in &s.sections {
                let target = if sec.patch.is_empty() {
                    format!("{} / {}", sec.preset, sec.scene)
                } else {
                    format!("{} · {}", sec.profile, sec.patch)
                };
                lines.push(Line::from(format!("  {} → {}", sec.name, target)));
            }
            lines
        }
    }
}

/// Format `frames` as `"N fr (X.XX ms)"` at `rate`.
fn fr_ms(frames: u32, rate: u32) -> String {
    format!("{frames} fr  ({:.2} ms)", block_ms(frames, rate))
}

/// Modal audio-settings + latency panel centered over the UI. `rate` is the
/// rig's running sample rate (drives the latency math); `ring` is the rig's
/// internal ring depth in frames.
fn settings_overlay(f: &mut Frame, s: &Settings, rate: u32, ring: u32) {
    let area = centered(f.area(), 74, 21);
    f.render_widget(Clear, area);

    let in_dev = s.inputs.get(s.sel_input);
    let out_dev = s.outputs.get(s.sel_output);
    let row = |fld: usize, label: &str, value: String| {
        let focused = s.field == fld;
        let marker = if focused { "▸ " } else { "  " };
        let lstyle = if focused {
            Style::new()
                .fg(Color::Black)
                .bg(Color::Rgb(232, 129, 58))
                .bold()
        } else {
            Style::new().fg(Color::Gray)
        };
        Line::from(vec![
            Span::styled(format!("{marker}{label:<14}"), lstyle),
            Span::raw(" "),
            Span::styled(value, Style::new().fg(Color::White)),
        ])
    };
    let head = |t: &str| {
        Line::from(Span::styled(
            t.to_string(),
            Style::new().fg(Color::DarkGray),
        ))
    };
    let period_str = |v: u32| {
        if v == 0 {
            "device default".to_string()
        } else {
            v.to_string()
        }
    };

    // Latency breakdown — each stage's frames → ms, summed. Device stages use
    // (headroom + period-size); the graph quantum and rig ring add on top. The
    // interface's USB transport + converters are real but unmeasurable here.
    let cap_fr = s
        .cap
        .as_ref()
        .map(|c| c.headroom + c.period_size)
        .unwrap_or(0);
    let play_fr = s
        .play
        .as_ref()
        .map(|p| p.headroom + p.period_size)
        .unwrap_or(0);
    let total_fr = cap_fr + s.buffer() + ring + play_fr;
    let cap_d = s.cap.clone().unwrap_or_default();
    let play_d = s.play.clone().unwrap_or_default();

    let lines = vec![
        head(" ── Routing ──"),
        row(
            field::INPUT_DEV,
            "input device",
            in_dev
                .map(|c| format!("{}  ({} ch)", c.label, c.channels))
                .unwrap_or_default(),
        ),
        row(
            field::INPUT_CH,
            "input channel",
            format!("{} / {}", s.sel_channel + 1, s.input_channels()),
        ),
        row(
            field::OUTPUT_DEV,
            "output device",
            out_dev.map(|c| c.label.clone()).unwrap_or_default(),
        ),
        head(" ── Clock (live) ──"),
        row(field::BUFFER, "buffer", fr_ms(s.buffer(), rate)),
        row(
            field::RATE,
            "sample rate",
            if s.rate() == 0 {
                "—".into()
            } else {
                format!("{} Hz   (allowed: {:?})", s.rate(), s.rates)
            },
        ),
        head(" ── Device buffering (applies on Enter: writes drop-in + restarts WirePlumber) ──"),
        row(field::HEADROOM, "headroom", fr_ms(s.headroom, rate)),
        row(field::PERIOD_NUM, "period count", period_str(s.period_num)),
        row(field::PERIOD_SIZE, "period size", period_str(s.period_size)),
        head(" ── Latency graph ──"),
        Line::from(Span::styled(
            format!(
                "   capture dev   hr {} + period {}  →  {}",
                cap_d.headroom,
                cap_d.period_size,
                fr_ms(cap_fr, rate)
            ),
            Style::new().fg(Color::Gray),
        )),
        Line::from(Span::styled(
            format!("   graph quantum {}", fr_ms(s.buffer(), rate)),
            Style::new().fg(Color::Gray),
        )),
        Line::from(Span::styled(
            format!("   rig ring      {}", fr_ms(ring, rate)),
            Style::new().fg(Color::Gray),
        )),
        Line::from(Span::styled(
            format!(
                "   playback dev  hr {} + period {}  →  {}",
                play_d.headroom,
                play_d.period_size,
                fr_ms(play_fr, rate)
            ),
            Style::new().fg(Color::Gray),
        )),
        Line::from(vec![
            Span::styled(
                format!(
                    "   ≈ {:.1} ms software round-trip",
                    block_ms(total_fr, rate)
                ),
                Style::new().fg(Color::Cyan).bold(),
            ),
            Span::styled("  (+ USB & converters)", Style::new().fg(Color::DarkGray)),
        ]),
        Line::from(Span::styled(
            " ↑/↓ field   ←/→ change   Enter apply + re-open   r rescan   s/Esc close",
            Style::new().fg(Color::DarkGray),
        )),
    ];
    let block = Block::bordered()
        .title(" Audio Settings & Latency ")
        .border_style(Style::new().fg(Color::Rgb(232, 129, 58)));
    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// One processing block's duration in milliseconds (`frames / rate`).
fn block_ms(frames: u32, sample_rate: u32) -> f64 {
    frames as f64 / sample_rate.max(1) as f64 * 1000.0
}

/// A centered `w`×`h` rect within `area` (clamped).
fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

/// Render one horizontal dB meter (−60…0 dB) as a colored gauge.
fn meter(f: &mut Frame, area: Rect, title: &str, peak: f32) {
    let db = if peak <= 1e-6 {
        -120.0
    } else {
        20.0 * peak.log10()
    };
    let ratio = (((db + 60.0) / 60.0) as f64).clamp(0.0, 1.0);
    let color = if db >= -1.0 {
        Color::Red
    } else if db >= -6.0 {
        Color::Yellow
    } else if db <= -119.0 {
        Color::DarkGray
    } else {
        Color::Green
    };
    let label = if db <= -119.0 {
        "  —  ".to_string()
    } else {
        format!("{db:.1} dB")
    };
    let gauge = Gauge::default()
        .block(Block::bordered().title(title))
        .gauge_style(Style::new().fg(color))
        .ratio(ratio)
        .label(label);
    f.render_widget(gauge, area);
}
