// Re-encode a session's MIDI via the native writer and write it back out:
//   reinject_midi <in.ptx> <out.ptx>
// Extracts notes per MIDI track, injects them (replacing the MIDI blocks),
// validates the round-trip through our parser, and writes the result.
use dawfile_protools::types::TrackKind;
use dawfile_protools::write::midi::{ChunkNote, MidiTrackInput, inject_midi};

fn collect_inputs(p: &str) -> Vec<MidiTrackInput> {
    let s = dawfile_protools::read_session(p, 48000).unwrap();
    let mut inputs = Vec::new();
    for t in s.all_tracks().filter(|t| matches!(t.kind, TrackKind::Midi)) {
        let mut notes = Vec::new();
        for tr in &t.regions {
            let Some(region) = s.midi_regions.get(tr.region_index as usize) else {
                continue;
            };
            for ev in &region.events {
                if ev.position < tr.clip_lo_ticks || ev.position >= tr.note_trim_ticks {
                    continue;
                }
                notes.push(ChunkNote {
                    position: ev.position,
                    duration: ev.duration.max(1),
                    note: ev.note,
                    velocity: ev.velocity.max(1),
                });
            }
        }
        notes.sort_by_key(|n| n.position);
        inputs.push(MidiTrackInput {
            notes,
            name: t.name.clone(),
        });
    }
    inputs
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let inp = &args[1];
    let outp = &args[2];
    let inputs = collect_inputs(inp);
    let total: usize = inputs.iter().map(|i| i.notes.len()).sum();
    eprintln!(
        "collected {} midi tracks, {} total notes",
        inputs.len(),
        total
    );
    for i in &inputs {
        if !i.notes.is_empty() {
            eprintln!("  {:<20} {} notes", i.name, i.notes.len());
        }
    }

    let raw = std::fs::read(inp).unwrap();
    let mut session = dawfile_protools::parse_raw(raw).unwrap();
    inject_midi(&mut session, &inputs).unwrap();
    let bytes = session.encrypt();
    std::fs::write(outp, &bytes).unwrap();
    eprintln!("wrote {} ({} bytes)", outp, bytes.len());

    // Validate round-trip via our parser.
    let check = dawfile_protools::read_session(outp, 48000).unwrap();
    let got: usize = check.midi_regions.iter().map(|r| r.events.len()).sum();
    eprintln!(
        "re-parsed: {} midi_regions, {} total region events",
        check.midi_regions.len(),
        got
    );
}
