//! Generate a minimal RPP isolating ONE feature, convert backwards via the
//! PT Reaper Converter (`--convert generated.rpp result.ptx`), and report
//! the PTX block-type counts.
//!
//! Usage: cargo run -p daw-reaper --example rpp_to_ptx_probe -- <probe>
//!
//! Probes: baseline | mute | color | solo | folder | vol | pan | notes
//!
//! Requires `scripts/pt-convert.sh` to work (voyager reachable, app installed).
//!
//! Generates RPP into `/tmp/probe_<name>.rpp`, asks the converter to produce
//! `/tmp/probe_<name>.ptx`, then dumps the block counts. Compare against
//! `baseline` to see which block types appear/grow per feature.

use dawfile_protools::raw_block::RawBlock;
use dawfile_reaper::RppSerialize;
use std::collections::BTreeMap;
use std::process::Command;

fn build_rpp(probe: &str) -> String {
    use dawfile_reaper::builder::ReaperProjectBuilder;
    let mut b = ReaperProjectBuilder::new().sample_rate(48000);

    match probe {
        "baseline" => {
            b = b.track("ProbeTrack", |t| t);
        }
        "mute" => {
            b = b.track("ProbeTrack", |t| t.muted());
        }
        "color" => {
            b = b.track("ProbeTrack", |t| t.color(0xd86e41));
        }
        "solo" => {
            b = b.track("ProbeTrack", |t| t.soloed());
        }
        "folder" => {
            b = b
                .track("ProbeFolder", |t| t.folder_start())
                .track("Child", |t| t.folder_end(1));
        }
        "vol" => {
            b = b.track("ProbeTrack", |t| t.volume(0.5));
        }
        "pan" => {
            b = b.track("ProbeTrack", |t| t.pan(0.5));
        }
        "two_tracks" => {
            b = b.track("Alpha", |t| t).track("Beta", |t| t);
        }
        "one_track_aaa" => {
            // Single-track baseline with 3-char name, to compare against
            // two_tracks_eq trk1 (also "AAA").
            b = b.track("AAA", |t| t);
        }
        "two_tracks_eq" => {
            // Equal-length names to eliminate the name-length byte-shift
            // when diffing track1-vs-track2 byte ranges.
            b = b.track("AAA", |t| t).track("BBB", |t| t);
        }
        "three_tracks" => {
            b = b
                .track("Alpha", |t| t)
                .track("Beta", |t| t)
                .track("Gamma", |t| t);
        }
        "marker" => {
            b = b.track("ProbeTrack", |t| t).marker(1, 1.0, "M");
        }
        "marker_two" => {
            b = b
                .track("ProbeTrack", |t| t)
                .marker(1, 1.0, "M1")
                .marker(2, 2.5, "M2");
        }
        "marker_colored" => {
            use dawfile_reaper::builder::MarkerBuilder;
            b = b.track("ProbeTrack", |t| t).add_marker(
                MarkerBuilder::marker(1, 1.0, "MColor")
                    .color(0xd86e41_i32)
                    .build(),
            );
        }
        "region" => {
            b = b.track("ProbeTrack", |t| t).region(1, 0.5, 2.5, "R1");
        }
        "ten_plain" => {
            for i in 1..=10 {
                let n = format!("T{i:02}");
                b = b.track(&n, |t| t);
            }
        }
        "ten_muted" => {
            for i in 1..=10 {
                let n = format!("T{i:02}");
                b = b.track(&n, |t| t.muted());
            }
        }
        "solo_defeat" => {
            b = b.track("ProbeTrack", |t| t.solo_defeated());
        }
        "fx_disabled" => {
            b = b.track("ProbeTrack", |t| t.fx_disabled());
        }
        "mute_envelope" => {
            // 2 breakpoints: mute on at t=0, off at t=1.0.
            use dawfile_reaper::types::envelope::EnvelopePointShape;
            b = b.track("ProbeTrack", |t| {
                t.envelope("MUTEENV", |e| {
                    e.active()
                        .visible()
                        .point(0.0, 0.0, EnvelopePointShape::Square)
                        .point(1.0, 1.0, EnvelopePointShape::Square)
                })
            });
        }
        "mute_envelope_1pt" => {
            use dawfile_reaper::types::envelope::EnvelopePointShape;
            b = b.track("ProbeTrack", |t| {
                t.envelope("MUTEENV", |e| {
                    e.active()
                        .visible()
                        .point(0.0, 0.0, EnvelopePointShape::Square)
                })
            });
        }
        "mute_envelope_3pt" => {
            use dawfile_reaper::types::envelope::EnvelopePointShape;
            b = b.track("ProbeTrack", |t| {
                t.envelope("MUTEENV", |e| {
                    e.active()
                        .visible()
                        .point(0.0, 0.0, EnvelopePointShape::Square)
                        .point(1.0, 1.0, EnvelopePointShape::Square)
                        .point(2.0, 0.0, EnvelopePointShape::Square)
                })
            });
        }
        "mute_envelope_4pt" => {
            use dawfile_reaper::types::envelope::EnvelopePointShape;
            b = b.track("ProbeTrack", |t| {
                t.envelope("MUTEENV", |e| {
                    e.active()
                        .visible()
                        .point(0.0, 0.0, EnvelopePointShape::Square)
                        .point(1.0, 1.0, EnvelopePointShape::Square)
                        .point(2.0, 0.0, EnvelopePointShape::Square)
                        .point(3.0, 1.0, EnvelopePointShape::Square)
                })
            });
        }
        "vol_envelope" => {
            use dawfile_reaper::types::envelope::EnvelopePointShape;
            b = b.track("ProbeTrack", |t| {
                t.envelope("VOLENV2", |e| {
                    e.active()
                        .visible()
                        .point(0.0, 1.0, EnvelopePointShape::Linear)
                        .point(1.0, 0.5, EnvelopePointShape::Linear)
                })
            });
        }
        "pan_envelope" => {
            use dawfile_reaper::types::envelope::EnvelopePointShape;
            b = b.track("ProbeTrack", |t| {
                t.envelope("PANENV", |e| {
                    e.active()
                        .visible()
                        .point(0.0, 0.0, EnvelopePointShape::Linear)
                        .point(1.0, 0.5, EnvelopePointShape::Linear)
                })
            });
        }
        "pan_envelope_2" => {
            use dawfile_reaper::types::envelope::EnvelopePointShape;
            b = b.track("ProbeTrack", |t| {
                t.envelope("PANENV2", |e| {
                    e.active()
                        .visible()
                        .point(0.0, 0.0, EnvelopePointShape::Linear)
                        .point(1.0, 0.5, EnvelopePointShape::Linear)
                })
            });
        }
        "pan_envelope_lr" => {
            // Some REAPER versions use separate L/R envelopes for stereo pan
            use dawfile_reaper::types::envelope::EnvelopePointShape;
            b = b.track("ProbeTrack", |t| {
                t.envelope("PANENV2L", |e| {
                    e.active()
                        .visible()
                        .point(0.0, 0.0, EnvelopePointShape::Linear)
                        .point(1.0, 0.5, EnvelopePointShape::Linear)
                })
                .envelope("PANENV2R", |e| {
                    e.active()
                        .visible()
                        .point(0.0, 0.0, EnvelopePointShape::Linear)
                        .point(1.0, -0.5, EnvelopePointShape::Linear)
                })
            });
        }
        "send" => {
            b = b.track("Source", |t| t).track("Dest", |t| t.receive(0));
        }
        "folder3" => {
            // Folder with 3 children — to map the children list in 0x200d
            b = b
                .track("Parent", |t| t.folder_start())
                .track("Child1", |t| t)
                .track("Child2", |t| t)
                .track("Child3", |t| t.folder_end(1));
        }
        "item_basic" => {
            // Track with one item
            b = b.track("ProbeTrack", |t| t.item(0.0, 1.0, |i| i.name("Clip")));
        }
        "clip_with_wav" => {
            // Track with one item referencing a real WAV file. Converter
            // requires real audio source to actually emit clip metadata.
            b = b.track("ProbeTrack", |t| {
                t.wave_item(0.0, 1.0, "/tmp/pt-re/input/clip_probe.wav")
            });
        }
        "clip_muted" => {
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 1.0, |i| {
                    i.source_wave("/tmp/pt-re/input/clip_probe.wav").muted()
                })
            });
        }
        "clip_colored" => {
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 1.0, |i| {
                    i.source_wave("/tmp/pt-re/input/clip_probe.wav")
                        .color(0x6e41d8)
                })
            });
        }
        "clip_named" => {
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 1.0, |i| {
                    i.source_wave("/tmp/pt-re/input/clip_probe.wav")
                        .name("MyClip")
                })
            });
        }
        "clip_long_name" => {
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 1.0, |i| {
                    i.source_wave("/tmp/pt-re/input/clip_probe.wav")
                        .name("ThisIsALongerClipName")
                })
            });
        }
        "clip_fadein" => {
            use dawfile_reaper::types::item::FadeCurveType;
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 1.0, |i| {
                    i.source_wave("/tmp/pt-re/input/clip_probe.wav")
                        .fade_in(0.25, FadeCurveType::Linear)
                })
            });
        }
        "clip_fadeout" => {
            use dawfile_reaper::types::item::FadeCurveType;
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 1.0, |i| {
                    i.source_wave("/tmp/pt-re/input/clip_probe.wav")
                        .fade_out(0.25, FadeCurveType::Linear)
                })
            });
        }
        "clip_fadein_long" => {
            use dawfile_reaper::types::item::FadeCurveType;
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 1.0, |i| {
                    i.source_wave("/tmp/pt-re/input/clip_probe.wav")
                        .fade_in(0.5, FadeCurveType::Linear)
                })
            });
        }
        "clip_fadein_xlong" => {
            use dawfile_reaper::types::item::FadeCurveType;
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 1.0, |i| {
                    i.source_wave("/tmp/pt-re/input/clip_probe.wav")
                        .fade_in(0.75, FadeCurveType::Linear)
                })
            });
        }
        "clip_pitch_up_2" => {
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 1.0, |i| {
                    i.source_wave("/tmp/pt-re/input/clip_probe.wav").pitch(2.0)
                })
            });
        }
        "clip_pitch_up_7" => {
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 1.0, |i| {
                    i.source_wave("/tmp/pt-re/input/clip_probe.wav").pitch(7.0)
                })
            });
        }
        "clip_pitch_down_3" => {
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 1.0, |i| {
                    i.source_wave("/tmp/pt-re/input/clip_probe.wav").pitch(-3.0)
                })
            });
        }
        "clip_slip_quarter" => {
            // Item plays last 0.75s of source, starting at source-offset 0.25s
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 0.75, |i| {
                    i.source_wave("/tmp/pt-re/input/clip_probe.wav")
                        .slip_offset(0.25)
                })
            });
        }
        "clip_slip_half" => {
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 0.5, |i| {
                    i.source_wave("/tmp/pt-re/input/clip_probe.wav")
                        .slip_offset(0.5)
                })
            });
        }
        "midi_one_note" => {
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 1.0, |i| {
                    i.midi(|m| m.ticks_per_qn(960).note(0, 0, 60, 96, 480))
                })
            });
        }
        "midi_cc1_only" => {
            // MIDI item with only a single CC#1 (modwheel) event, no notes
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 1.0, |i| {
                    i.midi(|m| m.ticks_per_qn(960).cc(0, 0, 1, 64))
                })
            });
        }
        "midi_cc1_value127" => {
            // Same as midi_cc1_only but value 127
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 1.0, |i| {
                    i.midi(|m| m.ticks_per_qn(960).cc(0, 0, 1, 127))
                })
            });
        }
        "midi_cc7_volume" => {
            // CC#7 (volume), value 100
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 1.0, |i| {
                    i.midi(|m| m.ticks_per_qn(960).cc(0, 0, 7, 100))
                })
            });
        }
        "clip_slip_eighth" => {
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 0.875, |i| {
                    i.source_wave("/tmp/pt-re/input/clip_probe.wav")
                        .slip_offset(0.125)
                })
            });
        }
        "clip_playrate_half" => {
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 2.0, |i| {
                    i.source_wave("/tmp/pt-re/input/clip_probe.wav")
                        .playrate(0.5)
                })
            });
        }
        "clip_playrate_quarter" => {
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 4.0, |i| {
                    i.source_wave("/tmp/pt-re/input/clip_probe.wav")
                        .playrate(0.25)
                })
            });
        }
        "clip_playrate_double" => {
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 0.5, |i| {
                    i.source_wave("/tmp/pt-re/input/clip_probe.wav")
                        .playrate(2.0)
                })
            });
        }
        "clip_selected" => {
            b = b.track("ProbeTrack", |t| {
                t.item(0.0, 1.0, |i| {
                    i.source_wave("/tmp/pt-re/input/clip_probe.wav").selected()
                })
            });
        }
        "clip_at_offset" => {
            b = b.track("ProbeTrack", |t| {
                t.item(2.5, 1.0, |i| {
                    i.source_wave("/tmp/pt-re/input/clip_probe.wav")
                })
            });
        }
        "track_selected" => {
            b = b.track("ProbeTrack", |t| t.selected());
        }
        "track_locked" => {
            b = b.track("ProbeTrack", |t| t.locked());
        }
        "track_show_mixer" => {
            // SHOWINMIX = 1: track visible in mixer
            b = b.track("ProbeTrack", |t| t);
        }
        _ => panic!("unknown probe: {probe}"),
    }

    b.build().to_rpp_string()
}

fn run_converter(rpp_path: &str, ptx_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Resolve repo root: crates/daw-reaper/examples/ → ../../..
    let manifest = env!("CARGO_MANIFEST_DIR");
    let script = std::path::Path::new(manifest)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts/pt-convert.sh");
    let out = Command::new(&script).args([rpp_path, ptx_path]).output()?;
    if !out.status.success() {
        return Err(format!(
            "convert failed: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
        .into());
    }
    eprintln!("{}", String::from_utf8_lossy(&out.stdout));
    Ok(())
}

fn block_summary(ptx_path: &str) -> Result<BTreeMap<u16, usize>, Box<dyn std::error::Error>> {
    let raw = std::fs::read(ptx_path)?;
    let session = dawfile_protools::parse_raw(raw)?;
    let mut counts: BTreeMap<u16, usize> = BTreeMap::new();
    fn walk(blocks: &[RawBlock], counts: &mut BTreeMap<u16, usize>) {
        for b in blocks {
            *counts.entry(b.content_type_raw).or_default() += 1;
            walk(&b.children, counts);
        }
    }
    walk(&session.blocks, &mut counts);
    Ok(counts)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let probe = std::env::args().nth(1).expect("probe name");

    let tmp = std::env::temp_dir();
    let rpp = tmp.join(format!("probe_{probe}.rpp"));
    let ptx = tmp.join(format!("probe_{probe}.ptx"));

    let rpp_content = build_rpp(&probe);
    std::fs::write(&rpp, &rpp_content)?;
    eprintln!("wrote {} ({} bytes)", rpp.display(), rpp_content.len());

    run_converter(rpp.to_str().unwrap(), ptx.to_str().unwrap())?;

    let ptx_size = std::fs::metadata(&ptx)?.len();
    eprintln!("got {} ({} bytes)", ptx.display(), ptx_size);

    let counts = block_summary(ptx.to_str().unwrap())?;
    println!("=== block counts for probe={probe} ({}b) ===", ptx_size);
    for (ct, n) in &counts {
        println!("  0x{ct:04x} × {n:>3}");
    }
    Ok(())
}
