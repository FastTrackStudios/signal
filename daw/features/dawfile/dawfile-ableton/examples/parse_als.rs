use dawfile_ableton::*;

fn main() {
    for path in &[
        "crates/dawfile-ableton/tests/fixtures/Farmhouse.als",
        "crates/dawfile-ableton/tests/fixtures/LucidDreaming.als",
    ] {
        println!("\n{}", "=".repeat(60));
        println!("Parsing: {path}");
        println!("{}", "=".repeat(60));

        match read_live_set(path) {
            Ok(set) => {
                println!("{}", dawfile_ableton::convert::set_summary(&set));
                println!("Version: {} ({})", set.version, set.version.creator);
                println!(
                    "Tempo: {:.1} BPM  Time Sig: {}/{}",
                    set.tempo, set.time_signature.numerator, set.time_signature.denominator
                );
                if let Some(ref ks) = set.key_signature {
                    println!("Key: {} {}", ks.root_note, ks.scale);
                }

                // Automation summary
                let mut total_track_envelopes = 0;
                let mut total_clip_envelopes = 0;
                let mut total_auto_points = 0;

                let print_track_auto =
                    |common: &TrackCommon, total_env: &mut usize, total_pts: &mut usize| {
                        if !common.automation_envelopes.is_empty() {
                            *total_env += common.automation_envelopes.len();
                            for env in &common.automation_envelopes {
                                *total_pts += env.events.len();
                            }
                            println!(
                                "    automation: {} envelopes, {} total points",
                                common.automation_envelopes.len(),
                                common
                                    .automation_envelopes
                                    .iter()
                                    .map(|e| e.events.len())
                                    .sum::<usize>()
                            );
                        }
                    };

                println!("\n--- Audio Tracks ({}) ---", set.audio_tracks.len());
                for t in &set.audio_tracks {
                    println!(
                        "  [{}] {} | vol={:.2} pan={:.2} devices={} clips={}+{}",
                        t.common.id,
                        t.common.effective_name,
                        t.common.mixer.volume,
                        t.common.mixer.pan,
                        t.common.devices.len(),
                        t.arrangement_clips.len(),
                        t.session_clips.len()
                    );
                    print_track_auto(
                        &t.common,
                        &mut total_track_envelopes,
                        &mut total_auto_points,
                    );

                    // Clip envelopes
                    for clip in &t.arrangement_clips {
                        if !clip.common.envelopes.is_empty() {
                            total_clip_envelopes += clip.common.envelopes.len();
                            println!(
                                "      clip '{}': {} clip envelopes",
                                clip.common.name,
                                clip.common.envelopes.len()
                            );
                        }
                    }

                    // Devices
                    for dev in &t.common.devices {
                        let params_tag = dev
                            .builtin_params
                            .as_ref()
                            .map(|_| " [typed]")
                            .unwrap_or("");
                        let state_tag = if dev.processor_state.is_some() {
                            " [state]"
                        } else {
                            ""
                        };
                        println!(
                            "      fx: {} ({:?}){}{}",
                            dev.name, dev.format, params_tag, state_tag
                        );
                    }
                }

                println!("\n--- MIDI Tracks ({}) ---", set.midi_tracks.len());
                for t in &set.midi_tracks {
                    println!(
                        "  [{}] {} | vol={:.2} pan={:.2} devices={} clips={}+{}",
                        t.common.id,
                        t.common.effective_name,
                        t.common.mixer.volume,
                        t.common.mixer.pan,
                        t.common.devices.len(),
                        t.arrangement_clips.len(),
                        t.session_clips.len()
                    );
                    print_track_auto(
                        &t.common,
                        &mut total_track_envelopes,
                        &mut total_auto_points,
                    );

                    for clip in &t.arrangement_clips {
                        let notes: usize = clip.key_tracks.iter().map(|kt| kt.notes.len()).sum();
                        if notes > 0 || !clip.common.envelopes.is_empty() {
                            println!(
                                "      clip '{}': {} notes, {} clip envelopes",
                                clip.common.name,
                                notes,
                                clip.common.envelopes.len()
                            );
                            total_clip_envelopes += clip.common.envelopes.len();
                        }
                    }
                }

                println!("\n--- Group Tracks ({}) ---", set.group_tracks.len());
                for t in &set.group_tracks {
                    print!("  [{}] {}", t.common.id, t.common.effective_name);
                    if !t.common.automation_envelopes.is_empty() {
                        print!(" | {} envelopes", t.common.automation_envelopes.len());
                        total_track_envelopes += t.common.automation_envelopes.len();
                    }
                    println!();
                }

                println!("\n--- Return Tracks ({}) ---", set.return_tracks.len());
                for t in &set.return_tracks {
                    print!(
                        "  [{}] {} | devices={}",
                        t.common.id,
                        t.common.effective_name,
                        t.common.devices.len()
                    );
                    if !t.common.automation_envelopes.is_empty() {
                        print!(" | {} envelopes", t.common.automation_envelopes.len());
                        total_track_envelopes += t.common.automation_envelopes.len();
                    }
                    println!();
                }

                println!("\n--- Locators ({}) ---", set.locators.len());
                for l in &set.locators {
                    println!("  {:.2}: {}", l.time, l.name);
                }

                println!("\n--- Scenes ({}) ---", set.scenes.len());
                println!("--- Grooves ({}) ---", set.groove_pool.len());

                println!(
                    "\n--- Tempo Automation ({} points) ---",
                    set.tempo_automation.len()
                );
                for pt in &set.tempo_automation {
                    let curve = if pt.curve_control_1.is_some() {
                        " [bezier]"
                    } else {
                        ""
                    };
                    println!("  beat {:.2}: {:.1} BPM{}", pt.time, pt.value, curve);
                }

                println!("\n--- Automation Summary ---");
                println!("  Track envelopes: {total_track_envelopes}");
                println!("  Clip envelopes: {total_clip_envelopes}");
                println!("  Total automation points: {total_auto_points}");

                if let Some(ref master) = set.master_track {
                    println!("\n--- Master Track ---");
                    println!(
                        "  output: {} | vol={:.2} devices={}",
                        master.audio_output.target,
                        master.mixer.volume,
                        master.devices.len()
                    );
                    for dev in &master.devices {
                        let params_tag = dev
                            .builtin_params
                            .as_ref()
                            .map(|_| " [typed]")
                            .unwrap_or("");
                        println!("    fx: {} ({:?}){}", dev.name, dev.format, params_tag);
                    }
                }
            }
            Err(e) => {
                println!("ERROR: {e}");
            }
        }
    }
}
