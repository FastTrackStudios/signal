//! Generate the #162 demo as a REAPER project.
//!
//! The `.daw` document the demo tests assemble is the native form; this
//! example writes the same material out as an `.rpp` so the whole thing
//! can be opened *in REAPER* and driven through the extension — select
//! any item, press `e`, and the expression editor opens on it.
//!
//! ```text
//! cargo run -p expression-editor-demo --example gen_reaper [-- /path/to/out.rpp]
//! ```
//!
//! What goes in, per scenario:
//!
//! - **Vocal + context**: the PNG session stems as wave items, full
//!   length, absolute paths (the material is never copied).
//! - **Drums**: the close mics of the drum multitrack, plus the song's
//!   own MIDI export split into one MIDI item per named track — so both
//!   audio *and* MIDI drums are selectable.
//! - **MPE**: the #177 synthesized fixture, written event-for-event —
//!   per-note bends, channel pressure, CC74 and the MPE configuration
//!   RPNs all survive into the take.
//! - **Guitar**: the Guitar Pro transcription's notes as a MIDI item
//!   (string→channel, as the importer assigns them).
//!
//! Nothing is committed and the output lands next to the material by
//! default; a machine without the material exits saying so.

use std::path::{Path, PathBuf};

use dawfile_reaper::builder::{MidiSourceBuilder, ReaperProjectBuilder};
use expression_editor_demo::{Material, TrackRole, build};

fn main() {
    let Some(material) = Material::discover() else {
        eprintln!(
            "no demo material on this machine — set ${} to the directory \
             holding 'PNG WORSHIP COLLECTIVE SESSION FILES'",
            expression_editor_demo::material::ROOT_ENV
        );
        std::process::exit(1);
    };

    let out: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| material.root().join("FTS Expression Editor Demo.rpp"));

    let demo = build(&material);
    println!("song: {}", demo.song);

    // The .mid export's tempo is the project tempo — every stem in the
    // session was played to it.
    let song = material.songs().into_iter().find(|s| s.name == demo.song);
    let smf_bytes = song
        .as_ref()
        .and_then(|s| s.midi.as_ref())
        .and_then(|p| std::fs::read(p).ok());
    let smf = smf_bytes.as_deref().and_then(|b| midly::Smf::parse(b).ok());
    let bpm = smf.as_ref().and_then(first_tempo).unwrap_or(120.0);

    let mut project = ReaperProjectBuilder::new()
        .version_string("7.0/linux-x86_64")
        .tempo(bpm)
        .sample_rate(48_000);

    // ── Audio: every demo track that is a wav on disk ────────────────
    for t in &demo.tracks {
        let Some(path) = &t.source else { continue };
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
        {
            continue;
        }
        let Some(len) = wav_seconds(path) else {
            eprintln!("skipping unreadable wav: {}", path.display());
            continue;
        };
        let name = format!("{} [{}]", t.name, role_tag(t.role));
        let file = path.to_string_lossy().into_owned();
        let item_name = t.name.clone();
        project = project.track(&name, |tr| {
            tr.item(0.0, len, |item| item.name(&item_name).source_wave(&file))
        });
        println!("  audio  {name}  ({len:.1}s)");
    }

    // ── MIDI: the song's export, one item per named track ────────────
    if let Some(smf) = &smf {
        let ppq = match smf.header.timing {
            midly::Timing::Metrical(t) => t.as_int() as u32,
            // SMPTE timing never appears in these exports; fall back
            // rather than modelling it.
            midly::Timing::Timecode(..) => 960,
        };
        for (i, track) in smf.tracks.iter().enumerate() {
            let (name, end_ticks, has_notes) = survey(track);
            if !has_notes {
                continue;
            }
            let name = name.unwrap_or_else(|| format!("MIDI {i}"));
            let len = ticks_to_seconds(end_ticks, ppq, bpm);
            project = project.track(format!("{name} [MIDI]"), |tr| {
                tr.item(0.0, len, |item| {
                    item.name(&name)
                        .midi(|m| smf_track_events(m.ticks_per_qn(ppq), track))
                })
            });
            println!("  midi   {name}  ({len:.1}s)");
        }
    } else {
        eprintln!("no MIDI export found for the song — skipping MIDI tracks");
    }

    // ── MPE: the synthesized fixture, every dimension on the wire ────
    let snap = expression_editor_daw::fixture::snapshot();
    let mpe_len = ticks_to_seconds(snap.length_ppq as u64, snap.ppq as u32, bpm);
    project = project.track("MPE Fixture [MPE]", |tr| {
        tr.item(0.0, mpe_len, |item| {
            item.name("MPE Fixture")
                .midi(|m| snapshot_events(m.ticks_per_qn(snap.ppq as u32), &snap))
        })
    });
    println!(
        "  mpe    MPE Fixture  ({mpe_len:.1}s, {} notes)",
        snap.notes.len()
    );

    // ── Guitar: the GP transcription's notes as a MIDI item ──────────
    if let Some(gp) = demo
        .by_role(TrackRole::Guitar)
        .first()
        .and_then(|t| t.source.clone())
    {
        match expression_editor_guitarpro::import_file(&gp.to_string_lossy()) {
            Ok(imported) => {
                let ppq = 960u32;
                let end = imported
                    .doc
                    .notes
                    .iter()
                    .map(|n| n.end)
                    .fold(0.0_f64, f64::max);
                let len = ticks_to_seconds(end as u64, ppq, bpm);
                let count = imported.doc.notes.len();
                project = project.track("Guitar (GP import) [GUITAR]", |tr| {
                    tr.item(0.0, len, |item| {
                        item.name("Guitar (GP import)").midi(|m| {
                            let mut m = m.ticks_per_qn(ppq);
                            for n in &imported.doc.notes {
                                let vel = (n.velocity.clamp(1.0, 127.0)) as u8;
                                let ch = n.channel.unwrap_or(0).min(15);
                                let dur = (n.end - n.start).max(1.0) as u32;
                                m = m.at(n.start as u64).note(
                                    0,
                                    ch,
                                    n.row.clamp(0, 127) as u8,
                                    vel,
                                    dur,
                                );
                            }
                            m
                        })
                    })
                });
                println!("  gtr    Guitar (GP import)  ({len:.1}s, {count} notes)");
            }
            Err(e) => eprintln!("guitar pro import failed: {e}"),
        }
    }

    // `Display` on `ReaperProject` is a human summary; `RppSerialize`
    // is the actual file format.
    use dawfile_reaper::RppSerialize;
    let text = project.build().to_rpp_string();
    std::fs::write(&out, &text).expect("write .rpp");

    // Read our own output back through the parser — a project REAPER
    // would refuse should fail here, not at the desk.
    match dawfile_reaper::io::parse_project_text(&text) {
        Ok(parsed) => println!(
            "\nwrote {} ({} tracks, parses clean)",
            out.display(),
            parsed.tracks.len()
        ),
        Err(e) => {
            eprintln!("generated project does not parse back: {e}");
            std::process::exit(1);
        }
    }
}

fn role_tag(role: TrackRole) -> &'static str {
    match role {
        TrackRole::Vocal => "VOCAL",
        TrackRole::Drum => "DRUM",
        TrackRole::Mpe => "MPE",
        TrackRole::Guitar => "GUITAR",
        TrackRole::Context => "CTX",
    }
}

/// A wav's length in seconds, by its own header.
fn wav_seconds(path: &Path) -> Option<f64> {
    let reader = hound::WavReader::open(path).ok()?;
    let spec = reader.spec();
    Some(reader.duration() as f64 / spec.sample_rate as f64)
}

/// The first set-tempo event in the file, as BPM.
fn first_tempo(smf: &midly::Smf) -> Option<f64> {
    for track in &smf.tracks {
        for ev in track {
            if let midly::TrackEventKind::Meta(midly::MetaMessage::Tempo(us_per_qn)) = ev.kind {
                return Some(60_000_000.0 / us_per_qn.as_int() as f64);
            }
        }
    }
    None
}

/// Track name, end tick, and whether any note-on exists.
fn survey(track: &[midly::TrackEvent]) -> (Option<String>, u64, bool) {
    let mut name = None;
    let mut tick = 0u64;
    let mut has_notes = false;
    for ev in track {
        tick += ev.delta.as_int() as u64;
        match &ev.kind {
            midly::TrackEventKind::Meta(midly::MetaMessage::TrackName(n)) => {
                if name.is_none() {
                    name = Some(String::from_utf8_lossy(n).into_owned());
                }
            }
            midly::TrackEventKind::Midi {
                message: midly::MidiMessage::NoteOn { vel, .. },
                ..
            } if vel.as_int() > 0 => has_notes = true,
            _ => {}
        }
    }
    (name, tick, has_notes)
}

fn ticks_to_seconds(ticks: u64, ppq: u32, bpm: f64) -> f64 {
    (ticks as f64 / ppq as f64) * 60.0 / bpm
}

/// Replay one SMF track's channel events into a REAPER MIDI source.
fn smf_track_events<'a>(
    mut m: MidiSourceBuilder,
    track: &[midly::TrackEvent<'a>],
) -> MidiSourceBuilder {
    let mut tick = 0u64;
    for ev in track {
        tick += ev.delta.as_int() as u64;
        let midly::TrackEventKind::Midi { channel, message } = ev.kind else {
            continue;
        };
        let ch = channel.as_int();
        m = m.at(tick);
        m = match message {
            midly::MidiMessage::NoteOn { key, vel } => m.note_on(0, ch, key.as_int(), vel.as_int()),
            midly::MidiMessage::NoteOff { key, vel } => {
                m.note_off(0, ch, key.as_int(), vel.as_int())
            }
            midly::MidiMessage::Controller { controller, value } => {
                m.cc(0, ch, controller.as_int(), value.as_int())
            }
            midly::MidiMessage::PitchBend { bend } => {
                // midly is -8192..8191; the source wants the raw 14-bit word.
                m.pitch_bend(
                    0,
                    ch,
                    (bend.0.as_int() as i32 + 8192).clamp(0, 16383) as u16,
                )
            }
            midly::MidiMessage::ProgramChange { program } => {
                m.program_change(0, ch, program.as_int())
            }
            midly::MidiMessage::ChannelAftertouch { vel } => {
                m.channel_pressure(0, ch, vel.as_int())
            }
            midly::MidiMessage::Aftertouch { key, vel } => {
                m.aftertouch(0, ch, key.as_int(), vel.as_int())
            }
        };
    }
    m
}

/// Replay the MPE fixture snapshot into a REAPER MIDI source.
///
/// Everything the snapshot carries goes on the wire in tick order —
/// the configuration RPNs open the stream (they sit at tick 0 in the
/// snapshot's `ccs`), and each voice's bends, pressures and CC74 land
/// on that voice's member channel.
fn snapshot_events(
    mut m: MidiSourceBuilder,
    snap: &daw::service::midi::MidiTakeSnapshot,
) -> MidiSourceBuilder {
    for n in &snap.notes {
        m = m.at(n.start_ppq as u64).note(
            0,
            n.channel.min(15),
            n.pitch,
            n.velocity.max(1),
            n.length_ppq.max(1.0) as u32,
        );
    }
    for cc in &snap.ccs {
        m = m
            .at(cc.position_ppq as u64)
            .cc(0, cc.channel.min(15), cc.controller, cc.value);
    }
    for pb in &snap.pitch_bends {
        m = m.at(pb.position_ppq as u64).pitch_bend(
            0,
            pb.channel.min(15),
            (pb.value as i32 + 8192).clamp(0, 16383) as u16,
        );
    }
    for cp in &snap.channel_pressures {
        m = m
            .at(cp.position_ppq as u64)
            .channel_pressure(0, cp.channel.min(15), cp.pressure);
    }
    m
}
