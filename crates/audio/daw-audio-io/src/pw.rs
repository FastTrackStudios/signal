//! Live PipeWire graph + per-device ALSA latency control.
//!
//! Everything that shells out to `pw-metadata` / `pw-cli` / `systemctl` lives
//! here, so callers (e.g. the live-rig UI) express latency *intent* — a buffer
//! size, a sample rate, a device's headroom/period — and never touch PipeWire
//! directly. Two tiers:
//!
//! - **Live** (global clock, applied instantly): graph quantum
//!   ([`force_quantum`]) and sample rate ([`force_rate`], limited to the
//!   server's `clock.allowed-rates`).
//! - **Device** (per-interface ALSA buffering, NOT live): `period-size`,
//!   `period-num`, `headroom`. WirePlumber owns these and only applies them when
//!   it creates the device node, so [`write_device_latency`] persists a
//!   `monitor.alsa.rules` drop-in and [`restart_session_manager`] re-creates the
//!   nodes to pick it up (a brief audio drop — callers re-open afterwards).
//!
//! All functions are best-effort: on a non-PipeWire system the tools are absent,
//! so the spawns fail and the calls are no-ops / return empty — no `cfg` needed.

use std::process::{Command, Stdio};

/// A `Command` with stdio silenced — these tools (`pw-metadata`, `systemctl`)
/// chatter to stdout/stderr, which would corrupt a TUI that owns the terminal.
fn quiet(program: &str) -> Command {
    let mut c = Command::new(program);
    c.stdout(Stdio::null()).stderr(Stdio::null());
    c
}

// ── Live global clock (pw-metadata) ──────────────────────────────────────────

/// Force the graph quantum (live, graph-wide buffer size in frames). `0` clears
/// the force, returning the graph to automatic quantum.
pub fn force_quantum(frames: u32) {
    set_clock("clock.force-quantum", frames);
}

/// Force the graph sample rate (live). Must be one of the server's
/// `clock.allowed-rates` or PipeWire ignores it; `0` clears the force. Pair with
/// re-opening streams at the new rate.
pub fn force_rate(rate: u32) {
    set_clock("clock.force-rate", rate);
}

fn set_clock(key: &str, value: u32) {
    let _ = quiet("pw-metadata")
        .args(["-n", "settings", "0", key, &value.to_string()])
        .status();
}

/// Global graph clock settings, read from `pw-metadata -n settings`.
#[derive(Clone, Debug, Default)]
pub struct ClockSettings {
    /// Current graph rate (`clock.rate`).
    pub rate: u32,
    /// Current graph quantum (`clock.quantum`).
    pub quantum: u32,
    /// Forced rate, or `0` when automatic (`clock.force-rate`).
    pub force_rate: u32,
    /// Forced quantum, or `0` when automatic (`clock.force-quantum`).
    pub force_quantum: u32,
    /// Rates the server permits forcing (`clock.allowed-rates`).
    pub allowed_rates: Vec<u32>,
    pub min_quantum: u32,
    pub max_quantum: u32,
}

/// Read the live graph clock settings (empty/zero fields when `pw-metadata`
/// isn't available).
pub fn clock_settings() -> ClockSettings {
    let out = Command::new("pw-metadata")
        .args(["-n", "settings"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let mut s = ClockSettings::default();
    // Lines look like: update: id:0 key:'clock.rate' value:'48000' type:''
    for line in out.lines() {
        let Some((key, value)) = parse_metadata_line(line) else {
            continue;
        };
        match key {
            "clock.rate" => s.rate = value.parse().unwrap_or(0),
            "clock.quantum" => s.quantum = value.parse().unwrap_or(0),
            "clock.force-rate" => s.force_rate = value.parse().unwrap_or(0),
            "clock.force-quantum" => s.force_quantum = value.parse().unwrap_or(0),
            "clock.min-quantum" => s.min_quantum = value.parse().unwrap_or(0),
            "clock.max-quantum" => s.max_quantum = value.parse().unwrap_or(0),
            "clock.allowed-rates" => s.allowed_rates = parse_rate_list(value),
            _ => {}
        }
    }
    s
}

/// Extract `(key, value)` from a `pw-metadata` line, stripping the `'…'` quotes.
fn parse_metadata_line(line: &str) -> Option<(&str, &str)> {
    let key = line.split("key:'").nth(1)?.split('\'').next()?;
    let value = line.split("value:'").nth(1)?.split('\'').next()?;
    Some((key, value))
}

/// Parse a `[ 48000, 96000 ]`-style rate list.
fn parse_rate_list(s: &str) -> Vec<u32> {
    s.trim_matches(|c| c == '[' || c == ']' || c == ' ')
        .split([',', ' '])
        .filter_map(|t| t.trim().parse().ok())
        .collect()
}

// ── Per-device ALSA buffering (pw-cli read + WirePlumber config write) ────────

/// One audio interface's ALSA buffering parameters, read from its PipeWire node.
#[derive(Clone, Debug, Default)]
pub struct DeviceLatency {
    pub node_id: u32,
    pub node_name: String,
    /// ALSA period size in frames (`api.alsa.period-size`).
    pub period_size: u32,
    /// Number of periods the device buffers (`api.alsa.period-num`).
    pub period_num: u32,
    /// Extra capture/playback headroom in frames (`api.alsa.headroom`).
    pub headroom: u32,
}

/// Read the ALSA buffering params of the device whose `node.name` contains
/// `name_match` in the requested direction (`capture` → input, else output).
/// Returns `None` when no such node is live (device absent / not opened).
pub fn device_latency(name_match: &str, capture: bool) -> Option<DeviceLatency> {
    let id = device_node_id(name_match, capture)?;
    let info = Command::new("pw-cli")
        .args(["i", &id.to_string()])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let mut d = DeviceLatency {
        node_id: id,
        ..Default::default()
    };
    for line in info.lines() {
        let l = line.trim_start_matches(['*', ' ', '\t']);
        if let Some(v) = prop_after(l, "api.alsa.period-size") {
            d.period_size = v;
        } else if let Some(v) = prop_after(l, "api.alsa.period-num") {
            d.period_num = v;
        } else if let Some(v) = prop_after(l, "api.alsa.headroom") {
            d.headroom = v;
        } else if l.starts_with("node.name = ") {
            d.node_name = l
                .trim_start_matches("node.name = ")
                .trim_matches('"')
                .to_string();
        }
    }
    Some(d)
}

/// Parse `key = "123"` (pw-cli info format), returning the integer value.
fn prop_after(line: &str, key: &str) -> Option<u32> {
    let rest = line.strip_prefix(key)?.trim_start();
    let rest = rest.strip_prefix('=')?.trim();
    rest.trim_matches('"').parse().ok()
}

/// Find the live PipeWire node id for the ALSA device matching `name_match` in
/// the requested direction. Capture nodes are `alsa_input.*`, playback
/// `alsa_output.*`.
fn device_node_id(name_match: &str, capture: bool) -> Option<u32> {
    let listing = Command::new("pw-cli")
        .args(["ls", "Node"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let prefix = if capture {
        "alsa_input."
    } else {
        "alsa_output."
    };
    // cpal device names use spaces ("Yamaha TF"); PipeWire node.name uses
    // underscores ("…Yamaha_TF…"). Match on the underscore form.
    let token = node_token(name_match);
    let mut id: Option<u32> = None;
    for line in listing.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("id ") {
            id = rest.split(',').next().and_then(|s| s.trim().parse().ok());
        } else if t.starts_with("node.name = ") {
            let name = t.trim_start_matches("node.name = ").trim_matches('"');
            if name.starts_with(prefix) && name.contains(&token) {
                return id;
            }
        }
    }
    None
}

/// The `node.name` token for a cpal device name (spaces → underscores), used to
/// match the PipeWire ALSA node both when reading and in the WirePlumber rule.
fn node_token(device_name: &str) -> String {
    device_name.replace(' ', "_")
}

/// The full `node.name` of the live ALSA device matching `name_match` in the
/// requested direction (`capture` → `alsa_input.*`, else `alsa_output.*`).
/// `None` when no such node is present. Used to build `pw-link` endpoints.
///
/// # One device, many nodes
///
/// An interface with several routes is several PipeWire nodes. An Arturia
/// MiniFuse 4 presents eight capture nodes — `Mic1`, `Mic2`, `Line4` (inputs
/// 1+2), `Line5` (inputs 3+4), a loopback, and a `.split` sibling for each —
/// and every one of them reports the same `node.nick`, "MiniFuse 4". So the
/// name a person picks in prefs cannot identify a node on its own, and taking
/// whichever the enumeration happened to list first meant:
///
/// - a `.split` node, whose ports are not `capture_N`, so every link failed
///   and the filter sat at `paused` — the silent-rig case
///   [`default_device_node_name`] warns about; or
/// - `Line5`, which is inputs **3+4** presented under the same name as inputs
///   1+2, so a guitar in input 1 was inaudible and nothing said why.
///
/// Two rules fix that, and both are about being predictable:
///
/// 1. **`.split` nodes are skipped.** They are WirePlumber's internal halves,
///    never a link target.
/// 2. **Candidates are sorted, and the first is taken**, so the choice is the
///    same on every machine and every boot rather than whatever the
///    enumeration happened to emit first.
///
/// Sorted-first is *deterministic*, not *correct*: on a MiniFuse 4 it picks
/// `Line3`, which is the loopback, because "Line3" sorts before "Mic1". A
/// device with one route is unambiguous; a device with several has to be told
/// apart.
///
/// **Every whitespace-separated token must appear in the node name**, which is
/// how it is told apart: `"MiniFuse 4 Mic1"` selects the mono instrument
/// input, `"MiniFuse 4 Line4"` the stereo pair on inputs 1+2. Spaces inside a
/// token become underscores, so the label a player reads still matches.
/// [`device_node_names`] lists every candidate when a guess is wrong.
pub fn device_node_name(name_match: &str, capture: bool) -> Option<String> {
    device_node_names(name_match, capture).into_iter().next()
}

/// Every linkable node matching `name_match`, in the deterministic order
/// [`device_node_name`] picks from.
///
/// Public because "which node did it choose, and what else was there" is the
/// first question when an interface does not appear, and answering it should
/// not require running `pw-cli` by hand.
pub fn device_node_names(name_match: &str, capture: bool) -> Vec<String> {
    let listing = Command::new("pw-cli")
        .args(["ls", "Node"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    matching_nodes(&listing, name_match, capture)
}

/// The selection itself, over a `pw-cli ls Node` listing — separated from
/// running `pw-cli` so the rules can be tested against a real listing rather
/// than against whatever hardware the test machine happens to have.
fn matching_nodes(listing: &str, name_match: &str, capture: bool) -> Vec<String> {
    let prefix = if capture {
        "alsa_input."
    } else {
        "alsa_output."
    };
    // Every token has to appear, so a pref can narrow by route as well as by
    // device: "MiniFuse 4 Mic1" matches the mono input and nothing else.
    let tokens: Vec<String> = name_match
        .split_whitespace()
        .map(node_token)
        .filter(|t| !t.is_empty())
        .collect();

    let mut found: Vec<String> = listing
        .lines()
        .filter_map(|line| {
            let name = line.trim().strip_prefix("node.name = ")?.trim_matches('"');
            let keep = name.starts_with(prefix)
                && !name.ends_with(".split")
                && tokens.iter().all(|t| name.contains(t.as_str()));
            keep.then(|| name.to_string())
        })
        .collect();
    found.sort();
    found.dedup();
    found
}

/// The session manager's current default sink (or source) node name.
///
/// `AudioIoPrefs` documents an empty device name as "system default", and the
/// cpal backend honoured that by asking cpal for the default device. The
/// native backend links ports by name, so it needs the name — without this it
/// links NOTHING and the node sits at `paused`, which presents as a rig that
/// runs, meters dead, in perfect silence.
pub fn default_device_node_name(capture: bool) -> Option<String> {
    let key = if capture {
        "default.audio.source"
    } else {
        "default.audio.sink"
    };
    let out = Command::new("pw-metadata")
        .args(["-n", "default"])
        .output()
        .ok()?;
    let listing = String::from_utf8_lossy(&out.stdout);
    // `update: id:0 key:'default.audio.sink' value:'{"name":"alsa_output.…"}' …`
    let line = listing.lines().find(|l| l.contains(key))?;
    let value = line.split("value:'").nth(1)?;
    let name = value.split("\"name\":\"").nth(1)?.split('"').next()?;
    (!name.is_empty()).then(|| name.to_string())
}

/// The audio port names of `node`, in graph order — `capture_FL`,
/// `capture_1`, `capture_MONO`, whatever this device actually calls them.
///
/// `output` selects the direction as PipeWire sees it: a capture device's
/// ports are *outputs* (it produces audio), a playback device's are *inputs*.
///
/// Callers used to assume `capture_{n}` / `playback_{n}`, which holds for a
/// multichannel interface presented as one node — a Yamaha TF names its ports
/// `capture_1..34`. It does not hold for a device PipeWire presents through a
/// UCM profile: a MiniFuse 4's routes name theirs by channel (`capture_FL`,
/// `capture_FR`, `monitor_FL`), so every link to `capture_1` failed and the
/// engine sat at `paused`, running into nothing.
///
/// Order is the graph's, not sorted: it is channel order, and `FL` before
/// `FR` is the difference between stereo and swapped stereo.
///
/// MIDI ports are excluded — this is for wiring audio.
pub fn node_ports(node: &str, output: bool) -> Vec<String> {
    let flag = if output { "-o" } else { "-i" };
    let listing = Command::new("pw-link")
        .args([flag])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    node_ports_in(&listing, node)
}

/// The port-name extraction, over a `pw-link -o` / `-i` listing.
fn node_ports_in(listing: &str, node: &str) -> Vec<String> {
    listing
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let (owner, port) = line.rsplit_once(':')?;
            (owner == node).then(|| port.trim().to_string())
        })
        .filter(|port| !port.to_lowercase().contains("midi"))
        .collect()
}

/// Link one PipeWire port to another (`"node:port"` endpoints), best-effort.
/// Returns whether the link command reported success.
pub fn link(src: &str, dst: &str) -> bool {
    Command::new("pw-link")
        .args([src, dst])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Outcome of an [`ensure_link`] pass over one port pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkStatus {
    /// The link did not exist and was created.
    Created,
    /// The link already existed (`pw-link` reports "File exists").
    Exists,
    /// The link could not be made (port/node absent, PipeWire down, …).
    Failed,
}

/// Idempotently link `src` → `dst`, reporting whether the link was newly
/// created, already in place, or impossible. `pw-link` exits non-zero for
/// both "already linked" and real failures, so stderr disambiguates — this
/// is what lets a watchdog distinguish a healthy graph from a vanished
/// device using the same cheap enumeration as [`link`].
pub fn ensure_link(src: &str, dst: &str) -> LinkStatus {
    let out = Command::new("pw-link")
        .args([src, dst])
        .stdout(Stdio::null())
        .output();
    match out {
        Ok(o) if o.status.success() => LinkStatus::Created,
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr).to_lowercase();
            if err.contains("exist") {
                LinkStatus::Exists
            } else {
                LinkStatus::Failed
            }
        }
        Err(_) => LinkStatus::Failed,
    }
}

/// Path of the daw-managed WirePlumber drop-in. Named to sort *after* any
/// hand-written `99-*` rule so the rig's chosen values win.
fn device_rule_path() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join("wireplumber/wireplumber.conf.d/99-zz-daw-rig-latency.conf")
}

/// Persist a `monitor.alsa.rules` drop-in setting `period_size` / `period_num` /
/// `headroom` for every ALSA node whose name matches `name_match`. The values
/// take effect when WirePlumber next creates the device node — call
/// [`restart_session_manager`] to apply now. A zero `period_*` is omitted (keep
/// the device default). Returns the written file path.
pub fn write_device_latency(
    name_match: &str,
    period_size: u32,
    period_num: u32,
    headroom: u32,
) -> std::io::Result<std::path::PathBuf> {
    let mut props = Vec::new();
    if period_size > 0 {
        props.push(format!("api.alsa.period-size = {period_size}"));
    }
    if period_num > 0 {
        props.push(format!("api.alsa.period-num = {period_num}"));
    }
    props.push(format!("api.alsa.headroom = {headroom}"));
    let body = format!(
        "# Written by daw-audio-io (live-rig latency tuning). Edit via the rig UI.\n\
         monitor.alsa.rules = [\n  {{ matches = [ {{ node.name = \"~alsa_.*{m}.*\" }} ]\n    \
         actions = {{ update-props = {{ {props} }} }} }}\n]\n",
        m = regex_escape(&node_token(name_match)),
        props = props.join(", "),
    );
    let path = device_rule_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, body)?;
    Ok(path)
}

/// Escape regex metacharacters in a device name for the `~`-prefixed WirePlumber
/// match (the interface name can contain `.`, `+`, etc.).
fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\.+*?()|[]{}^$".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Restart the WirePlumber user service so device nodes are re-created with the
/// latest `monitor.alsa.rules`. Briefly drops audio — callers re-open after.
/// Returns whether the restart command succeeded.
pub fn restart_session_manager() -> bool {
    quiet("systemctl")
        .args(["--user", "restart", "wireplumber"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod node_selection_tests {
    use super::matching_nodes;

    /// A real `pw-cli ls Node` excerpt: a MiniFuse 4 as PipeWire's UCM profile
    /// presents it — five capture routes, three playback, a `.split` sibling
    /// for each — plus a single-route interface for contrast.
    const LISTING: &str = r#"
		node.name = "alsa_input.usb-ARTURIA_MiniFuse_4_8860400264110319-00.HiFi__Line5__source"
		node.name = "alsa_input.usb-ARTURIA_MiniFuse_4_8860400264110319-00.HiFi__Line5__source.split"
		node.name = "alsa_input.usb-ARTURIA_MiniFuse_4_8860400264110319-00.HiFi__Mic1__source.split"
		node.name = "alsa_input.usb-ARTURIA_MiniFuse_4_8860400264110319-00.HiFi__Mic1__source"
		node.name = "alsa_input.usb-ARTURIA_MiniFuse_4_8860400264110319-00.HiFi__Line4__source"
		node.name = "alsa_input.usb-ARTURIA_MiniFuse_4_8860400264110319-00.HiFi__Line3__source"
		node.name = "alsa_input.usb-ARTURIA_MiniFuse_4_8860400264110319-00.HiFi__Mic2__source"
		node.name = "alsa_output.usb-ARTURIA_MiniFuse_4_8860400264110319-00.HiFi__Line1__sink"
		node.name = "alsa_output.usb-ARTURIA_MiniFuse_4_8860400264110319-00.HiFi__Line2__sink.split"
		node.name = "alsa_output.usb-ARTURIA_MiniFuse_4_8860400264110319-00.HiFi__Line2__sink"
		node.name = "alsa_input.usb-Yamaha_Corporation_Yamaha_TF-00.capture.0.0"
		node.name = "alsa_output.usb-Yamaha_Corporation_Yamaha_TF-00.playback.0.0"
"#;

    /// `.split` nodes are WirePlumber's internal halves. Their ports are not
    /// `capture_N`, so linking to one fails every link and leaves the filter
    /// paused — a rig that runs, meters dead, in silence.
    #[test]
    fn split_nodes_are_never_candidates() {
        let found = matching_nodes(LISTING, "MiniFuse 4", true);
        assert!(!found.is_empty());
        assert!(
            found.iter().all(|n| !n.ends_with(".split")),
            "got {found:?}"
        );
    }

    /// The choice is the same on every machine and every boot. Sorted, not
    /// enumeration order — which for this device listed inputs 3+4 first.
    #[test]
    fn selection_is_deterministic() {
        let a = matching_nodes(LISTING, "MiniFuse 4", true);
        let mut shuffled: Vec<&str> = LISTING.lines().collect();
        shuffled.reverse();
        let b = matching_nodes(&shuffled.join("\n"), "MiniFuse 4", true);
        assert_eq!(a, b, "order of the listing must not change the answer");
    }

    /// Deterministic is not the same as right: on this device the first sorted
    /// capture node is the loopback. Pinned so the honest limit stays visible
    /// — a multi-route device has to be told apart, and the doc comment says
    /// so.
    #[test]
    fn the_default_is_first_sorted_not_first_input() {
        let first = matching_nodes(LISTING, "MiniFuse 4", true)
            .into_iter()
            .next()
            .expect("a candidate");
        assert!(first.contains("Line3"), "got {first}");
    }

    /// Every token must appear, which is how one route is named: the mono
    /// instrument input, or the stereo pair on inputs 1+2.
    #[test]
    fn a_route_can_be_named() {
        assert_eq!(
            matching_nodes(LISTING, "MiniFuse 4 Mic1", true),
            vec![
                "alsa_input.usb-ARTURIA_MiniFuse_4_8860400264110319-00.HiFi__Mic1__source"
                    .to_string()
            ]
        );
        assert_eq!(
            matching_nodes(LISTING, "MiniFuse 4 Line4", true),
            vec![
                "alsa_input.usb-ARTURIA_MiniFuse_4_8860400264110319-00.HiFi__Line4__source"
                    .to_string()
            ]
        );
    }

    /// Direction is part of the match: an input name finds no playback node,
    /// so a mistyped pref reads as "device absent" rather than linking audio
    /// backwards.
    #[test]
    fn direction_is_respected() {
        assert!(matching_nodes(LISTING, "MiniFuse 4 Mic1", false).is_empty());
        let out = matching_nodes(LISTING, "MiniFuse 4", false);
        assert!(out.iter().all(|n| n.starts_with("alsa_output.")), "{out:?}");
        assert!(out[0].contains("Line1"), "main output first: {out:?}");
    }

    /// A single-route interface needs no disambiguation, and a space in the
    /// label still matches the underscored node name.
    #[test]
    fn a_single_route_device_is_unambiguous() {
        assert_eq!(
            matching_nodes(LISTING, "Yamaha TF", true),
            vec!["alsa_input.usb-Yamaha_Corporation_Yamaha_TF-00.capture.0.0".to_string()]
        );
    }
}

#[cfg(test)]
mod node_port_tests {
    use super::node_ports_in;

    /// A real `pw-link -o` excerpt: a numbered multichannel interface, a UCM
    /// device that names ports by channel, and a MIDI bridge.
    const LISTING: &str = r#"
Midi-Bridge:MiniFuse 4: MIDI 1 (capture)
alsa_input.usb-ARTURIA_MiniFuse_4-00.HiFi__Line4__source:capture_FL
alsa_input.usb-ARTURIA_MiniFuse_4-00.HiFi__Line4__source:capture_FR
alsa_input.usb-ARTURIA_MiniFuse_4-00.HiFi__Mic1__source:capture_MONO
alsa_input.usb-Yamaha_Corporation_Yamaha_TF-00.capture.0.0:capture_1
alsa_input.usb-Yamaha_Corporation_Yamaha_TF-00.capture.0.0:capture_2
alsa_input.usb-Yamaha_Corporation_Yamaha_TF-00.capture.0.0:capture_3
"#;

    /// The names come from the device, whatever shape they are. Assuming
    /// `capture_1` is what left a UCM device linked to nothing.
    #[test]
    fn ports_are_read_not_assumed() {
        assert_eq!(
            node_ports_in(
                LISTING,
                "alsa_input.usb-ARTURIA_MiniFuse_4-00.HiFi__Line4__source"
            ),
            vec!["capture_FL".to_string(), "capture_FR".to_string()]
        );
        assert_eq!(
            node_ports_in(
                LISTING,
                "alsa_input.usb-Yamaha_Corporation_Yamaha_TF-00.capture.0.0"
            ),
            vec![
                "capture_1".to_string(),
                "capture_2".to_string(),
                "capture_3".to_string()
            ]
        );
    }

    /// Channel order, not sorted: `FL` before `FR` is the difference between
    /// stereo and swapped stereo.
    #[test]
    fn order_is_the_graphs() {
        let ports = node_ports_in(
            LISTING,
            "alsa_input.usb-ARTURIA_MiniFuse_4-00.HiFi__Line4__source",
        );
        assert_eq!(ports.first().map(String::as_str), Some("capture_FL"));
    }

    /// A mono input has one port, so a stereo assumption would have linked a
    /// port that does not exist.
    #[test]
    fn a_mono_input_has_one_port() {
        assert_eq!(
            node_ports_in(
                LISTING,
                "alsa_input.usb-ARTURIA_MiniFuse_4-00.HiFi__Mic1__source"
            )
            .len(),
            1
        );
    }

    /// MIDI is not audio wiring.
    #[test]
    fn midi_ports_are_excluded() {
        assert!(node_ports_in(LISTING, "Midi-Bridge").is_empty());
    }

    /// An absent node yields nothing rather than a partial match on a node
    /// whose name merely starts the same way.
    #[test]
    fn a_prefix_is_not_a_match() {
        assert!(
            node_ports_in(LISTING, "alsa_input.usb-ARTURIA_MiniFuse_4-00.HiFi__Line4").is_empty()
        );
        assert!(node_ports_in(LISTING, "nothing-like-this").is_empty());
    }
}
