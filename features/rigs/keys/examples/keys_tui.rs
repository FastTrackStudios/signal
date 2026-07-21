//! `just keys` — a Nord Stage 4-styled TUI for the composition-tree keys rig.
//!
//! Loads a preset from the [`PresetRegistry`] (code or `.styx`), hosts it on a
//! [`KeysRig`], and plays it from a hardware MIDI keyboard **and** the computer
//! keyboard. The panel shows the program's sections/layers (colored), the
//! **keyboard split zones** (the layering you built), held notes, output level
//! and a MIDI monitor — laid out like a Nord Stage 4.
//!
//! ```text
//! cargo run --release -p signal-sampler --features pipewire --example keys_tui
//! cargo run ... --example keys_tui -- --preset "Layering Demo"
//! ```

use std::collections::HashSet;
use std::time::Duration;

use daw_audio_io::AudioIoPrefs;
use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Gauge, Paragraph};
use signal_keys::KeysRig;
use signal_sampler::{
    Container, MidiEvent, MidiInputHandle, MidiSelection, PresetRegistry, RigNode,
};

const NORD_RED: Color = Color::Rgb(206, 38, 38);
/// Distinct colors for key-split bands / layers, reused on the keyboard + panels.
const BAND_COLORS: [Color; 6] = [
    Color::Rgb(86, 156, 255),
    Color::Rgb(120, 214, 120),
    Color::Rgb(240, 170, 70),
    Color::Rgb(206, 130, 220),
    Color::Rgb(110, 214, 214),
    Color::Rgb(240, 120, 120),
];

fn arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn main() -> eyre::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Default to the sample-realized program when the machine has the piano
    // libraries; the placeholder program otherwise.
    let default_preset = if signal_sampler::nord::nord_stage_piano_preset().is_some() {
        "Nord Stage (Piano)"
    } else {
        "Nord Stage"
    };
    let preset_name = arg(&args, "--preset").unwrap_or_else(|| default_preset.to_string());
    let buffer: u32 = arg(&args, "--buffer")
        .and_then(|s| s.parse().ok())
        .unwrap_or(256);

    // Build the registry (code builtins + any styx library dir passed via --library).
    let mut registry = PresetRegistry::with_builtins();
    if let Some(dir) = arg(&args, "--library") {
        registry.load_styx_dir(&dir);
    }
    // --omni <patch.prt_omn>: import an Omnisphere patch, realize its
    // soundsources against the local extraction, and select it.
    let mut preset_name = preset_name;
    if let Some(patch_path) = arg(&args, "--omni") {
        let index = signal_synth::omni_import::SoundsourceIndex::scan_default();
        let tree =
            signal_synth::omni_import::load_patch_file(std::path::Path::new(&patch_path), &index)
                .map_err(|e| eyre::eyre!("omni import: {e}"))?;
        preset_name = tree.name.clone();
        registry.register_code(tree);
    }
    let tree = registry.tree(&preset_name).cloned().ok_or_else(|| {
        eyre::eyre!(
            "preset {preset_name:?} not found; have: {:?}",
            registry.names()
        )
    })?;

    // Open audio + host the preset BEFORE taking the terminal (errors print normally).
    let prefs = AudioIoPrefs {
        output_device: String::new(),
        sample_rate: 0,
        buffer_size: buffer,
        ..Default::default()
    };
    let mut rig = KeysRig::open(&prefs, &tree)?;

    // MIDI input choices: All merged (default) + each detected port + a virtual
    // port. Cycle live in the TUI with `i`/`I` (same as the strings rig).
    let mut midi_choices: Vec<MidiChoice> = vec![MidiChoice {
        label: "All inputs".to_string(),
        sel: MidiSelection::All,
    }];
    for port in KeysRig::midi_input_ports() {
        midi_choices.push(MidiChoice {
            label: port.clone(),
            sel: MidiSelection::NameContains(port),
        });
    }
    midi_choices.push(MidiChoice {
        label: "Virtual port (FTS-Signal Keys)".to_string(),
        sel: MidiSelection::Virtual("FTS-Signal Keys".into()),
    });
    let mut midi_idx = match arg(&args, "--midi").as_deref() {
        None | Some("all") => 0,
        Some("virtual") => midi_choices.len() - 1,
        Some(name) => midi_choices
            .iter()
            .position(|c| c.label.to_lowercase().contains(&name.to_lowercase()))
            .unwrap_or(0),
    };
    // Don't hard-fail if the device can't open — the TUI still runs so you can
    // switch with `i` and watch the monitor.
    let mut midi: Option<MidiInputHandle> =
        rig.attach_midi(midi_choices[midi_idx].sel.clone()).ok();
    let monitor = rig.midi_monitor();

    redirect_stderr_to_log();

    let mut term = ratatui::init();
    let res = run(
        &mut term,
        &mut rig,
        &registry,
        &tree,
        &monitor,
        &preset_name,
        &midi_choices,
        &mut midi_idx,
        &mut midi,
    );
    ratatui::restore();
    res
}

/// One entry in the MIDI input selector.
struct MidiChoice {
    label: String,
    sel: MidiSelection,
}

/// A key-split band collected from the tree (a container with a non-full KEY range).
struct Band {
    name: String,
    lo: u8,
    hi: u8,
    color: Color,
}

/// A velocity-windowed layer (a container with a non-full VELOCITY range).
struct VelLayer {
    name: String,
    lo: u8,
    hi: u8,
    color: Color,
}

/// Walk the tree, collecting key-split bands + velocity layers, assigning colors.
fn collect_routing(tree: &Container) -> (Vec<Band>, Vec<VelLayer>) {
    let mut bands = Vec::new();
    let mut vels = Vec::new();
    let mut ci = 0usize;
    fn walk(c: &Container, bands: &mut Vec<Band>, vels: &mut Vec<VelLayer>, ci: &mut usize) {
        let z = c.zone;
        let key_split = z.key_lo > 0 || z.key_hi < 127;
        let vel_split = z.vel_lo > 1 || z.vel_hi < 127;
        if key_split {
            let color = BAND_COLORS[*ci % BAND_COLORS.len()];
            *ci += 1;
            bands.push(Band {
                name: c.name.clone(),
                lo: z.key_lo,
                hi: z.key_hi,
                color,
            });
        }
        if vel_split {
            let color = BAND_COLORS[*ci % BAND_COLORS.len()];
            *ci += 1;
            vels.push(VelLayer {
                name: c.name.clone(),
                lo: z.vel_lo,
                hi: z.vel_hi,
                color,
            });
        }
        for child in &c.children {
            if let RigNode::Container { container } = child {
                walk(container, bands, vels, ci);
            }
        }
    }
    walk(tree, &mut bands, &mut vels, &mut ci);
    (bands, vels)
}

// ── Computer-keyboard → note mapping (tracker layout, like the sampler TUI) ──

fn key_to_semitone(c: char) -> Option<i32> {
    Some(match c {
        'z' => 0,
        's' => 1,
        'x' => 2,
        'd' => 3,
        'c' => 4,
        'v' => 5,
        'g' => 6,
        'b' => 7,
        'h' => 8,
        'n' => 9,
        'j' => 10,
        'm' => 11,
        ',' => 12,
        'l' => 13,
        '.' => 14,
        ';' => 15,
        '/' => 16,
        'q' => 12,
        '2' => 13,
        'w' => 14,
        '3' => 15,
        'e' => 16,
        'r' => 17,
        '5' => 18,
        't' => 19,
        '6' => 20,
        'y' => 21,
        '7' => 22,
        'u' => 23,
        'i' => 24,
        '9' => 25,
        'o' => 26,
        '0' => 27,
        'p' => 28,
        _ => return None,
    })
}

#[allow(clippy::too_many_arguments)]
fn run(
    term: &mut ratatui::DefaultTerminal,
    rig: &mut KeysRig,
    registry: &PresetRegistry,
    tree: &Container,
    monitor: &signal_sampler::MidiMonitor,
    preset_name: &str,
    midi_choices: &[MidiChoice],
    midi_idx: &mut usize,
    midi: &mut Option<MidiInputHandle>,
) -> eyre::Result<()> {
    let (bands, vels) = collect_routing(tree);
    let mut octave: i32 = 60; // C4 base for the computer keyboard
    let mut held_kbd: HashSet<char> = HashSet::new();
    let mut status: Option<String> = None;
    let preset_names = registry
        .names()
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

    loop {
        // Held notes for the viz: computer keys + a fold of recent hardware MIDI.
        let mut held: HashSet<u8> = held_kbd
            .iter()
            .filter_map(|c| note_for_key(*c, octave))
            .collect();
        fold_monitor_held(monitor, &mut held);

        let peak = rig.output_peak();
        let midi_label = midi_choices[*midi_idx].label.as_str();
        let midi_open = midi.is_some();
        term.draw(|f| {
            ui(
                f,
                preset_name,
                rig,
                &bands,
                &vels,
                &held,
                peak,
                monitor,
                octave,
                midi_label,
                midi_open,
                status.as_deref(),
            )
        })?;

        if !event::poll(Duration::from_millis(20))? {
            continue;
        }
        if let Event::Key(k) = event::read()? {
            match k.kind {
                KeyEventKind::Press => match k.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char(' ') => {
                        rig.panic();
                        held_kbd.clear();
                        status = Some("panic — all notes off".into());
                    }
                    KeyCode::Char('[') => octave = (octave - 12).max(0),
                    KeyCode::Char(']') => octave = (octave + 12).min(108),
                    // Cycle the MIDI input device (i forward / I back) — re-attach.
                    KeyCode::Char('i') | KeyCode::Char('I') => {
                        let n = midi_choices.len();
                        *midi_idx = if k.code == KeyCode::Char('I') {
                            (*midi_idx + n - 1) % n
                        } else {
                            (*midi_idx + 1) % n
                        };
                        *midi = None; // close current before opening next
                        *midi = rig.attach_midi(midi_choices[*midi_idx].sel.clone()).ok();
                        status = Some(format!("MIDI in: {}", midi_choices[*midi_idx].label));
                    }
                    // Tab cycles presets in the registry (re-host without restart).
                    KeyCode::Tab => {
                        if let Some(cur) = preset_names.iter().position(|n| n == preset_name) {
                            let next = &preset_names[(cur + 1) % preset_names.len()];
                            if let Some(t) = registry.tree(next) {
                                rig.load_preset(t);
                                status = Some(format!("loaded {next} (restart to re-color)"));
                            }
                        }
                    }
                    KeyCode::Char(c) => {
                        if !held_kbd.contains(&c) {
                            if let Some(note) = note_for_key(c, octave) {
                                held_kbd.insert(c);
                                rig.note_on(note, 100);
                            }
                        }
                    }
                    _ => {}
                },
                KeyEventKind::Release => {
                    if let KeyCode::Char(c) = k.code {
                        if held_kbd.remove(&c) {
                            if let Some(note) = note_for_key(c, octave) {
                                rig.note_off(note);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    rig.all_notes_off();
    Ok(())
}

fn note_for_key(c: char, octave: i32) -> Option<u8> {
    let n = octave + key_to_semitone(c)?;
    (0..=127).contains(&n).then_some(n as u8)
}

/// Approximate currently-held hardware notes by folding the monitor's recent log.
fn fold_monitor_held(monitor: &signal_sampler::MidiMonitor, held: &mut HashSet<u8>) {
    for msg in monitor.recent() {
        match msg {
            MidiEvent::NoteOn { key, velocity, .. } if velocity.get() > 0 => {
                held.insert(key.get());
            }
            MidiEvent::NoteOn { key, .. } | MidiEvent::NoteOff { key, .. } => {
                held.remove(&key.get());
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn ui(
    f: &mut Frame,
    preset_name: &str,
    rig: &KeysRig,
    bands: &[Band],
    vels: &[VelLayer],
    held: &HashSet<u8>,
    peak: f32,
    monitor: &signal_sampler::MidiMonitor,
    octave: i32,
    midi_label: &str,
    midi_open: bool,
    status: Option<&str>,
) {
    let rows = Layout::vertical([
        Constraint::Length(1), // header (red)
        Constraint::Length(3), // output meter
        Constraint::Min(6),    // sections + velocity
        Constraint::Length(5), // keyboard
        Constraint::Length(1), // footer
    ])
    .split(f.area());

    // ── Header (Nord red) ──
    let header = Line::from(vec![
        Span::styled(
            "  NORD STAGE 4  ",
            Style::new().fg(Color::White).bg(NORD_RED).bold(),
        ),
        Span::raw("  "),
        Span::styled(
            preset_name.to_string(),
            Style::new().fg(Color::White).bold(),
        ),
        Span::styled(
            format!("   {} Hz", rig.sample_rate()),
            Style::new().fg(Color::DarkGray),
        ),
        Span::raw("   in: "),
        Span::styled(
            midi_label.to_string(),
            Style::new().fg(if midi_open { Color::Cyan } else { Color::Red }),
        ),
        Span::styled(
            format!("  ({} msgs)", monitor.count()),
            Style::new().fg(Color::DarkGray),
        ),
    ]);
    f.render_widget(Paragraph::new(header), rows[0]);

    // ── Output meter ──
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
    } else {
        Color::Green
    };
    f.render_widget(
        Gauge::default()
            .block(Block::bordered().title("OUTPUT"))
            .gauge_style(Style::new().fg(color))
            .ratio(ratio)
            .label(if db <= -119.0 {
                "—".into()
            } else {
                format!("{db:.1} dB")
            }),
        rows[1],
    );

    // ── Sections (left) + Velocity layers (right) ──
    let mid =
        Layout::horizontal([Constraint::Percentage(64), Constraint::Percentage(36)]).split(rows[2]);
    render_sections(f, mid[0], bands);
    render_velocity(f, mid[1], vels, held);

    // ── Keyboard with split zones + held notes ──
    render_keyboard(f, rows[3], bands, held);

    // ── Footer ──
    let footer = if let Some(s) = status {
        Line::from(Span::styled(format!(" {s}"), Style::new().fg(Color::Green)))
    } else {
        Line::from(Span::styled(
            format!(
                " play: z–m / q–p   [ ] octave (C{})   i: MIDI device   space: panic   Tab: preset   q: quit",
                octave / 12 - 1
            ),
            Style::new().fg(Color::DarkGray),
        ))
    };
    f.render_widget(Paragraph::new(footer), rows[4]);
}

/// The key-split bands as labelled colored rows (the "sections").
fn render_sections(f: &mut Frame, area: Rect, bands: &[Band]) {
    let mut lines = Vec::new();
    if bands.is_empty() {
        lines.push(Line::from(Span::styled(
            "(no key splits — full-range layers)",
            Style::new().fg(Color::DarkGray),
        )));
    }
    for b in bands {
        lines.push(Line::from(vec![
            Span::styled("██ ", Style::new().fg(b.color)),
            Span::styled(
                format!("{:<14}", b.name),
                Style::new().fg(Color::White).bold(),
            ),
            Span::styled(
                format!("keys {}–{}", note_name(b.lo), note_name(b.hi)),
                Style::new().fg(Color::Gray),
            ),
        ]));
    }
    f.render_widget(
        Paragraph::new(lines).block(Block::bordered().title("SECTIONS / SPLITS")),
        area,
    );
}

/// Velocity-windowed layers as horizontal velocity bars (Omnisphere-style xfade).
fn render_velocity(f: &mut Frame, area: Rect, vels: &[VelLayer], _held: &HashSet<u8>) {
    let mut lines = Vec::new();
    if vels.is_empty() {
        lines.push(Line::from(Span::styled(
            "(no velocity layers)",
            Style::new().fg(Color::DarkGray),
        )));
    }
    let width = 16usize;
    for v in vels {
        let lo = (v.lo as usize * width / 127).min(width);
        let hi = (v.hi as usize * width / 127).min(width);
        let bar: String = (0..width)
            .map(|i| if i >= lo && i <= hi { '█' } else { '·' })
            .collect();
        lines.push(Line::from(vec![
            Span::styled(format!("{:<10}", v.name), Style::new().fg(Color::White)),
            Span::styled(bar, Style::new().fg(v.color)),
            Span::styled(
                format!(" {}–{}", v.lo, v.hi),
                Style::new().fg(Color::DarkGray),
            ),
        ]));
    }
    f.render_widget(
        Paragraph::new(lines).block(Block::bordered().title("VELOCITY LAYERS")),
        area,
    );
}

/// The keyboard strip: each key colored by its covering split band; held notes lit.
fn render_keyboard(f: &mut Frame, area: Rect, bands: &[Band], held: &HashSet<u8>) {
    const LOW: u8 = 36; // C2
    const HIGH: u8 = 96; // C7
    let mut spans = Vec::new();
    for n in LOW..=HIGH {
        // Deepest band covering this key wins its color.
        let band_color = bands
            .iter()
            .rev()
            .find(|b| n >= b.lo && n <= b.hi)
            .map(|b| b.color);
        let is_black = matches!(n % 12, 1 | 3 | 6 | 8 | 10);
        let held_here = held.contains(&n);
        let (ch, mut style) = if held_here {
            (
                '▀',
                Style::new()
                    .fg(Color::White)
                    .bg(band_color.unwrap_or(Color::Gray))
                    .bold(),
            )
        } else if let Some(c) = band_color {
            ('▄', Style::new().fg(c))
        } else if is_black {
            ('▖', Style::new().fg(Color::DarkGray))
        } else {
            ('▄', Style::new().fg(Color::Rgb(60, 60, 60)))
        };
        if is_black && !held_here {
            style = style.add_modifier(Modifier::DIM);
        }
        spans.push(Span::styled(ch.to_string(), style));
    }
    // Octave ticks under the keys.
    let mut ticks = String::new();
    for n in LOW..=HIGH {
        ticks.push(if n % 12 == 0 { '|' } else { ' ' });
    }
    let body = vec![
        Line::from(spans),
        Line::from(Span::styled(ticks, Style::new().fg(Color::DarkGray))),
    ];
    f.render_widget(
        Paragraph::new(body).block(Block::bordered().title("KEYBOARD — split zones + held notes")),
        area,
    );
}

fn note_name(n: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!("{}{}", NAMES[(n % 12) as usize], n as i32 / 12 - 1)
}

/// Send stray stderr (engine logs) to a file so they can't corrupt the TUI. (fn below)
fn redirect_stderr_to_log() {
    use std::os::fd::AsRawFd;
    let path = std::env::temp_dir().join("fts-signal-keys.log");
    if let Ok(f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        unsafe { libc::dup2(f.as_raw_fd(), libc::STDERR_FILENO) };
        std::mem::forget(f);
    }
}
