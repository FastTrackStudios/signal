    use super::*;

    #[test]
    fn ms_to_frames_44100() {
        assert_eq!(ms_to_frames(0, 44100), 0);
        assert_eq!(ms_to_frames(1000, 44100), 44100);
        assert_eq!(ms_to_frames(100, 44100), 4410);
    }

    #[test]
    fn numeric_dynamic_labels_use_nearest_recorded_velocity() {
        let dynamics = [
            "24", "35", "42", "45", "54", "61", "68", "76", "82", "84", "89", "96", "102", "106",
            "110", "114", "118", "121", "124", "126", "127",
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();

        assert_eq!(
            dynamic_label_for_velocity(&dynamics, 40).as_deref(),
            Some("42")
        );
        assert_eq!(
            dynamic_label_for_velocity(&dynamics, 100).as_deref(),
            Some("102")
        );
        assert_eq!(
            dynamic_label_for_velocity(&dynamics, 120).as_deref(),
            Some("121")
        );
    }

    #[test]
    fn named_dynamic_labels_keep_even_velocity_bands() {
        let dynamics = ["pp", "mp", "f", "fff"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();

        assert_eq!(
            dynamic_label_for_velocity(&dynamics, 31).as_deref(),
            Some("pp")
        );
        assert_eq!(
            dynamic_label_for_velocity(&dynamics, 32).as_deref(),
            Some("mp")
        );
        assert_eq!(
            dynamic_label_for_velocity(&dynamics, 96).as_deref(),
            Some("fff")
        );
    }

    fn test_zone(rr_index: u32) -> crate::spec::ZoneSpec {
        crate::spec::ZoneSpec {
            file: format!("rr-{rr_index}.wav"),
            key_min: 60,
            key_max: 60,
            root_key: 60,
            vel_min: 0,
            vel_max: 127,
            rr_index,
            rr_mode: String::new(),
            gain_db: 0.0,
            pan: 0.0,
            tune_cents: 0.0,
            sample_start: 0,
            sample_end: 0,
            loop_start: 0,
            loop_end: 0,
            loop_xfade: 0,
            fade_in: 0,
            release_start: 0,
            playback_mode: String::new(),
            trigger_mode: String::new(),
            trigger_cc: 0,
            trigger_value_min: 0,
            trigger_value_max: 0,
            mic: String::new(),
            articulation: String::new(),
            dynamic: String::new(),
            direction: String::new(),
            interval: 0,
            lead_in_ms: 0.0,
            arrival_ms: 0.0,
            group: String::new(),
            group_polyphony: 0,
            choke_group: String::new(),
            off_by: Vec::new(),
            section: String::new(),
            variant: String::new(),
        }
    }

    #[test]
    fn zone_rr_selection_uses_declared_rr_index() {
        let zones = vec![test_zone(10), test_zone(20), test_zone(30)];
        let indices = vec![0, 1, 2];
        let mut rng = 1;

        assert_eq!(
            select_zone_rr_slot(&zones, &indices, 0, None, &mut rng, None),
            10
        );
        assert_eq!(
            select_zone_rr_slot(&zones, &indices, 1, None, &mut rng, None),
            20
        );
        assert_eq!(
            select_zone_rr_slot(&zones, &indices, 2, None, &mut rng, None),
            30
        );
        assert_eq!(
            select_zone_rr_slot(&zones, &indices, 3, None, &mut rng, None),
            10
        );
        assert_eq!(select_zone_rr_index_by_slot(&zones, &indices, 20), 1);
    }

    #[test]
    fn zone_rr_selection_keeps_multimic_slots_aligned() {
        let zones = vec![test_zone(2), test_zone(0), test_zone(2), test_zone(0)];
        let left_mic = vec![0, 1];
        let right_mic = vec![2, 3];
        let mut rng = 1;
        let first_slot = select_zone_rr_slot(&zones, &left_mic, 0, None, &mut rng, None);
        let second_slot = select_zone_rr_slot(&zones, &left_mic, 1, None, &mut rng, None);

        assert_eq!(first_slot, 0);
        assert_eq!(second_slot, 2);
        assert_eq!(
            select_zone_rr_index_by_slot(&zones, &left_mic, first_slot),
            1
        );
        assert_eq!(
            select_zone_rr_index_by_slot(&zones, &right_mic, first_slot),
            3
        );
        assert_eq!(
            select_zone_rr_index_by_slot(&zones, &left_mic, second_slot),
            0
        );
        assert_eq!(
            select_zone_rr_index_by_slot(&zones, &right_mic, second_slot),
            2
        );
    }

    #[test]
    fn zone_rr_no_repeat_random_avoids_previous_slot() {
        let mut zones = vec![test_zone(0), test_zone(1), test_zone(2)];
        for zone in &mut zones {
            zone.rr_mode = "no-repeat-random".to_string();
        }
        let indices = vec![0, 1, 2];
        let mut rng = 0x9e37_79b9_7f4a_7c15;
        let mut last = None;

        for _ in 0..64 {
            let slot = select_zone_rr_slot(&zones, &indices, 0, last, &mut rng, None);
            assert_ne!(Some(slot), last);
            last = Some(slot);
        }
    }

    #[test]
    fn half_pedal_scales_release_frames() {
        let base = 24_000;

        assert_eq!(half_pedal_release_frames(base, 0, "", 0.0), base);
        assert!(half_pedal_release_frames(base, 32, "", 0.0) > base);
        assert_eq!(
            half_pedal_release_frames(base, 63, "", 0.0),
            (base as f32 * HALF_PEDAL_MAX_RELEASE_MULTIPLIER).round() as usize
        );
        assert_eq!(half_pedal_release_frames(base, 64, "", 0.0), base);
    }

    #[test]
    fn half_pedal_uses_authored_curve_and_multiplier() {
        let base = 24_000;
        let linear = half_pedal_release_frames(base, 32, "linear", 3.0);
        let squared = half_pedal_release_frames(base, 32, "squared", 3.0);
        let sqrt = half_pedal_release_frames(base, 32, "sqrt", 3.0);

        assert!(squared < linear);
        assert!(sqrt > linear);
        assert_eq!(half_pedal_release_frames(base, 63, "linear", 2.0), base * 2);
    }

    #[test]
    fn pedal_zone_triggers_are_exclusive_to_pedal_events() {
        let mut pedal_down = test_zone(0);
        pedal_down.trigger_mode = "pedal-down".to_string();
        let mut pedal_up = test_zone(0);
        pedal_up.trigger_mode = "pedal-up".to_string();

        assert!(zone_trigger_matches(&pedal_down, ZoneTrigger::PedalDown));
        assert!(!zone_trigger_matches(&pedal_down, ZoneTrigger::Attack));
        assert!(!zone_trigger_matches(&pedal_down, ZoneTrigger::Release));
        assert!(zone_trigger_matches(&pedal_up, ZoneTrigger::PedalUp));
        assert!(!zone_trigger_matches(&pedal_up, ZoneTrigger::Attack));
        assert!(!zone_trigger_matches(&pedal_up, ZoneTrigger::Release));
    }

    #[test]
    fn cc_zone_trigger_fires_on_threshold_entry_only() {
        let mut zone = test_zone(0);
        zone.trigger_mode = "cc-threshold".to_string();
        zone.trigger_cc = 11;
        zone.trigger_value_min = 40;
        zone.trigger_value_max = 80;

        assert!(zone_cc_trigger_crossed(&zone, 11, 39, 40));
        assert!(!zone_cc_trigger_crossed(&zone, 11, 40, 60));
        assert!(!zone_cc_trigger_crossed(&zone, 11, 20, 90));
        assert!(!zone_cc_trigger_crossed(&zone, 1, 39, 40));
        assert!(!zone_trigger_matches(&zone, ZoneTrigger::Attack));
    }

    #[test]
    fn aftertouch_zone_trigger_fires_on_threshold_entry_only() {
        let mut zone = test_zone(0);
        zone.trigger_mode = "aftertouch".to_string();
        zone.trigger_value_min = 30;
        zone.trigger_value_max = 90;

        assert!(zone_aftertouch_trigger_crossed(&zone, None, 29, 30));
        assert!(!zone_aftertouch_trigger_crossed(&zone, None, 40, 80));
        assert!(!zone_aftertouch_trigger_crossed(&zone, None, 29, 100));
        assert!(zone_aftertouch_trigger_crossed(&zone, Some(60), 29, 30));
        assert!(!zone_aftertouch_trigger_crossed(&zone, Some(61), 29, 30));
        assert!(!zone_trigger_matches(&zone, ZoneTrigger::Attack));
    }

    #[test]
    fn cc1_layer_selection() {
        // Simulate a 3-layer [p=0-42, mf=33-94, ff=85-127] setup.
        use crate::spec::Cc1Layer;

        // Build a minimal PlayerPatch-less engine via a stub spec.
        // We test current_layers_owned() logic in isolation by constructing
        // a mock layers slice and exercising the algorithm directly.
        let layers: &[Cc1Layer] = &[
            Cc1Layer {
                label: "p".into(),
                cc_range: [0, 42],
            },
            Cc1Layer {
                label: "mf".into(),
                cc_range: [33, 94],
            },
            Cc1Layer {
                label: "ff".into(),
                cc_range: [85, 127],
            },
        ];

        // Inline the algorithm (same logic as current_layers_owned).
        let probe = |cc1: u8| -> (String, String, f32) {
            for i in 0..layers.len().saturating_sub(1) {
                let lo = &layers[i];
                let hi = &layers[i + 1];
                let xs = hi.cc_range[0];
                let xe = lo.cc_range[1];
                if cc1 <= xe {
                    if cc1 < xs {
                        return (lo.label.clone(), lo.label.clone(), 0.0);
                    } else {
                        let span = (xe - xs + 1).max(1) as f32;
                        let blend = (cc1 - xs) as f32 / span;
                        return (lo.label.clone(), hi.label.clone(), blend);
                    }
                }
            }
            let top = &layers[layers.len() - 1];
            (top.label.clone(), top.label.clone(), 0.0)
        };

        let (lo, hi, blend) = probe(10);
        assert_eq!(lo, "p");
        assert_eq!(hi, "p");
        assert_eq!(blend, 0.0);

        let (lo, hi, blend) = probe(33);
        assert_eq!(lo, "p");
        assert_eq!(hi, "mf");
        assert!((0.0..=1.0).contains(&blend));

        let (lo, hi, blend) = probe(50);
        assert_eq!(lo, "mf");
        assert_eq!(hi, "mf");
        assert_eq!(blend, 0.0);

        let (lo, hi, _blend) = probe(127);
        assert_eq!(lo, "ff");
        assert_eq!(hi, "ff");
    }

    #[test]
    fn recent_miss_ring_keeps_bounded_latest_values() {
        let mut recent = VecDeque::new();
        for idx in 0..10 {
            push_recent(&mut recent, format!("miss-{idx}"), 4);
        }
        push_recent(&mut recent, "miss-9".to_string(), 4);

        assert_eq!(recent.len(), 4);
        assert_eq!(recent.front().map(String::as_str), Some("miss-6"));
        assert_eq!(recent.back().map(String::as_str), Some("miss-9"));
    }

    #[test]
    fn con_sordino_remap() {
        // Load the CSS spec and exercise the sordino switch logic using the
        // real articulation list.
        let specs_dir = {
            let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
            std::path::Path::new(&manifest)
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("specs")
        };
        let spec_path = specs_dir.join("cinematic-strings.toml");
        if !spec_path.exists() {
            return;
        }

        let spec = crate::LibrarySpec::from_file(&spec_path).expect("load CSS spec");
        let patch = crate::PlayerPatch::from_spec(spec);

        let mut engine = SampleEngine::new(patch, 44100, "1v", "Mix");
        engine.set_articulation("Vibsus");
        assert!(!engine.con_sordino());

        // Enable Con Sordino — should remap to SordVibsus.
        engine.set_con_sordino(true);
        assert!(engine.con_sordino());
        assert_eq!(engine.articulation(), "SordVibsus");

        // Disable — should revert to Vibsus.
        engine.set_con_sordino(false);
        assert_eq!(engine.articulation(), "Vibsus");

        // Nonvib → SordNonvib and back.
        engine.set_articulation("Nonvib");
        engine.set_con_sordino(true);
        assert_eq!(engine.articulation(), "SordNonvib");
        engine.set_con_sordino(false);
        assert_eq!(engine.articulation(), "Nonvib");
    }

    fn engine_from_styx(styx: &str) -> SampleEngine {
        let spec = crate::LibrarySpec::from_styx(styx).expect("parse styx");
        let patch = crate::PlayerPatch::from_spec(spec);
        SampleEngine::new(patch, 48_000, "", "")
    }

    /// A two-velocity-layer piano note: soft below 64, hard above.
    ///
    /// `from_spec` leaves `zone_paths` empty, and `is_zoned()` keys off that,
    /// so a zone-mode patch has to have them filled in or every `resolve_zone`
    /// silently returns `None`.
    fn two_layer_piano() -> SampleEngine {
        let spec = crate::LibrarySpec::from_styx(
            "name \"p\"\n\
             zones (\n\
               {file \"soft.wav\", key_min 60, key_max 60, root_key 60, vel_min 0, vel_max 63, articulation \"DryTones\"}\n\
               {file \"hard.wav\", key_min 60, key_max 60, root_key 60, vel_min 64, vel_max 127, articulation \"DryTones\"}\n\
             )\n",
        )
        .expect("parse styx");
        let mut patch = crate::PlayerPatch::from_spec(spec);
        patch.zone_paths = patch
            .spec
            .zones
            .iter()
            .map(|z| std::path::PathBuf::from(&z.file))
            .collect();
        SampleEngine::new(patch, 48_000, "main", "Main")
    }

    /// Color must reach zone selection: a note played soft, with Color hard
    /// up, has to pick the hard-struck recording. Selecting the same sample
    /// and merely turning it up is the failure this guards
    /// (`r[keys.piano.color.three-effects]`).
    #[test]
    fn piano_color_shifts_which_velocity_layer_plays() {
        use crate::piano_voice::{PianoOffsets, PianoVoice};
        let mut eng = two_layer_piano();

        let played = 40u8;
        let soft = eng.patch().resolve_zone(60, played, 0).expect("soft zone");
        assert!(soft.path.ends_with("soft.wav"), "as played picks soft");

        let mut pv = PianoVoice::new(PianoOffsets::GRANDEUR);
        pv.color = 50;
        let shifted = pv.apply(played).velocity;
        assert_eq!(shifted, 90);
        let hard = eng.patch().resolve_zone(60, shifted, 0).expect("hard zone");
        assert!(hard.path.ends_with("hard.wav"), "Color reaches the hard layer");

        // And the engine actually runs it on note-on, recording the
        // compensating trim for the voice about to spawn.
        eng.set_piano_voice(Some(pv));
        eng.note_on(60, played);
        // (-40 + 150) * -120 mdB at this velocity — the hard layer is louder,
        // so the trim pulls it back down.
        assert!(
            (eng.piano_trim_db - -13.2).abs() < 0.01,
            "compensating trim applied, got {}",
            eng.piano_trim_db
        );
    }

    #[test]
    fn without_a_piano_voice_nothing_changes() {
        let mut eng = two_layer_piano();
        assert!(eng.piano_voice().is_none());
        eng.note_on(60, 40);
        assert_eq!(eng.piano_trim_db, 0.0, "no trim when no piano voice");
    }

    /// A keyswitch is a command, not a note. Re-aiming it with a tone control
    /// would silently select the wrong articulation.
    #[test]
    fn piano_color_leaves_keyswitches_alone() {
        use crate::piano_voice::{PianoOffsets, PianoVoice};
        let mut eng = engine_from_styx(
            "name \"p\"\n\
             keyswitch {\n\
               notes (\n\
                 {\n\
                   note \"C0\"\n\
                   label \"A\"\n\
                   vel_map { 0-127 \"A\" }\n\
                 }\n\
               )\n\
             }\n\
             zones (\n\
               {file \"a.wav\", key_min 60, key_max 60, root_key 60, vel_min 0, vel_max 127, articulation \"A\"}\n\
               {file \"b.wav\", key_min 60, key_max 60, root_key 60, vel_min 0, vel_max 127, articulation \"B\"}\n\
             )\n",
        );
        let mut pv = PianoVoice::new(PianoOffsets::GRANDEUR);
        pv.color = -50;
        eng.set_piano_voice(Some(pv));
        eng.note_on(12, 100);
        assert_eq!(
            eng.piano_trim_db, 0.0,
            "a keyswitch takes no trim and no velocity shift"
        );
    }

    #[test]
    fn percussion_single_key_fires_on_any_note() {
        // Kick-style pack: drum-kit, one articulation on a single key.
        let eng = engine_from_styx(
            "name \"k\"\n\
             category \"drum-kit\"\n\
             zones (\n\
               {\n\
                 file \"a.wav\"\n\
                 key_min 36\n\
                 key_max 36\n\
                 root_key 36\n\
                 vel_min 0\n\
                 vel_max 127\n\
                 articulation \"Hit\"\n\
               }\n\
             )\n",
        );
        assert!(eng.percussion && eng.single_attack_key);
        let z = &eng.patch().spec.zones[0];
        // Plays on the routed note even though it's off the zone's key.
        assert!(eng.zone_selected(z, 35, 100, ZoneTrigger::Attack));
        assert!(eng.zone_selected(z, 48, 100, ZoneTrigger::Attack));
        // Velocity still gates.
        assert!(!eng.zone_selected(
            &crate::spec::ZoneSpec {
                vel_min: 64,
                vel_max: 127,
                ..z.clone()
            },
            35,
            10,
            ZoneTrigger::Attack
        ));
    }

    #[test]
    fn keyswitch_note_selects_articulation_velocity_sensitive() {
        // Melodic patch (no drum category) with two-articulation zones + a
        // velocity-sensitive keyswitch note (CSS-style).
        let mut eng = engine_from_styx(
            "name \"s\"\n\
             keyswitch {\n\
               notes (\n\
                 {\n\
                   note \"C0\"\n\
                   label \"Sustain\"\n\
                   vel_map { 0-127 \"Leg\" }\n\
                 }\n\
                 {\n\
                   note \"C#0\"\n\
                   label \"Shorts\"\n\
                   vel_map {\n\
                     0-64 \"Spiccato\"\n\
                     65-127 \"Staccato\"\n\
                   }\n\
                 }\n\
               )\n\
             }\n\
             zones (\n\
               {file \"leg.wav\", key_min 60, key_max 60, root_key 60, vel_min 0, vel_max 127, articulation \"Leg\"}\n\
               {file \"spic.wav\", key_min 60, key_max 60, root_key 60, vel_min 0, vel_max 127, articulation \"Spiccato\"}\n\
               {file \"stac.wav\", key_min 60, key_max 60, root_key 60, vel_min 0, vel_max 127, articulation \"Staccato\"}\n\
             )\n",
        );
        let leg = eng.patch().spec.zones[0].clone();
        let spic = eng.patch().spec.zones[1].clone();
        let stac = eng.patch().spec.zones[2].clone();

        // A keyswitch note is consumed (returns true) and selects its artic.
        assert!(eng.try_keyswitch(12, 100)); // C0 = MIDI 12 → Leg
        assert_eq!(eng.articulation(), "Leg");
        assert!(eng.zone_selected(&leg, 60, 100, ZoneTrigger::Attack));
        assert!(!eng.zone_selected(&spic, 60, 100, ZoneTrigger::Attack));

        // Velocity on C#0 picks the variant: soft = Spiccato, hard = Staccato.
        assert!(eng.try_keyswitch(13, 30));
        assert_eq!(eng.articulation(), "Spiccato");
        assert!(eng.zone_selected(&spic, 60, 100, ZoneTrigger::Attack));
        assert!(!eng.zone_selected(&stac, 60, 100, ZoneTrigger::Attack));

        assert!(eng.try_keyswitch(13, 120));
        assert_eq!(eng.articulation(), "Staccato");
        assert!(eng.zone_selected(&stac, 60, 100, ZoneTrigger::Attack));

        // A normal (non-keyswitch) note is NOT consumed — it should sound.
        assert!(!eng.try_keyswitch(60, 100));
    }

    /// Build a zoned NVLeg legato engine with directional zones for a set of
    /// notes (synthetic paths so it reads as zoned; samples never load).
    fn mono_legato_engine(notes: &[u8]) -> SampleEngine {
        let mut styx = String::from(
            "name \"s\"\n\
             articulations ( {id NVLeg, label NVLeg, kind @Legato} )\n\
             zones (\n",
        );
        for &n in notes {
            for dir in ["up", "down"] {
                styx.push_str(&format!(
                    "{{file \"{n}_{dir}.wav\", key_min {n}, key_max {n}, root_key {n}, vel_min 0, vel_max 127, articulation \"NVLeg\", direction \"{dir}\"}}\n"
                ));
            }
        }
        styx.push_str(")\n");
        let spec = crate::LibrarySpec::from_styx(&styx).expect("parse styx");
        let mut patch = crate::PlayerPatch::from_spec(spec);
        patch.zone_paths = (0..patch.spec.zones.len())
            .map(|i| std::path::PathBuf::from(format!("z{i}.wav")))
            .collect();
        let mut eng = SampleEngine::new(patch, 48_000, "", "");
        eng.set_articulation("NVLeg");
        eng
    }

    fn render_ms(eng: &mut SampleEngine, ms: usize) {
        let frames = (eng.sample_rate as usize / 1000) * ms;
        let mut out = vec![0.0f32; frames * 2];
        eng.render(&mut out);
    }

    #[test]
    fn mono_legato_falls_back_to_held_on_release() {
        let mut eng = mono_legato_engine(&[60, 62]);

        // First note sounds immediately and becomes the line head. Soft
        // velocity (≤64) so the Overlap-Delay is non-zero (real O+D is only
        // non-zero for soft+fast playing; loud legato fires immediately).
        eng.note_on(60, 40);
        assert_eq!(eng.lines[0].note, Some(60));

        // Second note transitions (delayed): soft velocity + a fast (20 ms)
        // inter-onset interval → the ~77 ms O+D delay, so it is Pending.
        render_ms(&mut eng, 50);
        eng.note_on(62, 40);
        assert!(matches!(eng.lines[0].state, LegatoState::Pending { .. }));
        render_ms(&mut eng, 200);
        assert_eq!(eng.lines[0].note, Some(62));

        // Releasing the SOUNDING note (62) while 60 is still held falls back to
        // 60 via a legato transition. The fallback velocity (80) is in the
        // upper range → zero O+D → it fires immediately (no Pending).
        eng.note_off(62);
        assert_eq!(eng.lines[0].note, Some(60));

        // Releasing the last held note ends the line.
        eng.note_off(60);
        assert_eq!(eng.lines[0].note, None);
    }

    /// Pacific-style engine: sustain (`sus` w/ atk+rel links) + destination-
    /// rooted interval transitions, `legato_engine { style @Pacific }`.
    fn pacific_legato_engine(notes: &[u8]) -> SampleEngine {
        let mut styx = String::from(
            "name \"p\"\n\
             articulations (\n\
               {id sus, label Sus, kind @Sustain, release_artic rel, attack_artic atk}\n\
               {id atk, label Atk, kind @Short}\n\
               {id rel, label Rel, kind @Release}\n\
               {id leg, label Leg, kind @Legato, legato_role transition, directional true}\n\
             )\n\
             legato_engine {\n\
               style @Pacific\n\
               outgoing_fade_ms 115\n\
               destination_fade_ms 500\n\
               release_overlap { fade_ms 1500 }\n\
             }\n\
             zones (\n",
        );
        for &n in notes {
            for artic in ["sus", "atk", "rel"] {
                styx.push_str(&format!(
                    "{{file \"{artic}_{n}.wav\", key_min {n}, key_max {n}, root_key {n}, vel_min 0, vel_max 127, articulation \"{artic}\"}}\n"
                ));
            }
            // Destination-rooted transitions: every interval 1..=12, both dirs.
            for dir in ["up", "down"] {
                for iv in 1..=12u8 {
                    styx.push_str(&format!(
                        "{{file \"leg_{dir}{iv}_{n}.wav\", key_min {n}, key_max {n}, root_key {n}, vel_min 0, vel_max 127, articulation \"leg\", direction \"{dir}\", interval {iv}}}\n"
                    ));
                }
            }
        }
        styx.push_str(")\n");
        let spec = crate::LibrarySpec::from_styx(&styx).expect("parse pacific styx");
        let mut patch = crate::PlayerPatch::from_spec(spec);
        patch.zone_paths = (0..patch.spec.zones.len())
            .map(|i| std::path::PathBuf::from(format!("p{i}.wav")))
            .collect();
        let mut eng = SampleEngine::new(patch, 48_000, "", "");
        eng.set_articulation("sus");
        eng
    }

    #[test]
    fn pacific_legato_fires_immediately_and_returns_on_release() {
        let mut eng = pacific_legato_engine(&[60, 62, 64]);
        eng.set_legato_fire_log_enabled(true);

        // Sustains are legato-capable under @Pacific with NO vibrato pair.
        assert!(eng.is_legato_capable_artic("sus"));

        eng.note_on(60, 40);
        assert_eq!(eng.lines[0].note, Some(60));

        // Soft velocity + fast IOI would arm the CSS Overlap-Delay countdown;
        // Pacific must fire IMMEDIATELY — never Pending. (Gaps > the 30 ms
        // auto-divisi chord window so the notes read as a line, not a chord.)
        render_ms(&mut eng, 50);
        eng.note_on(62, 40);
        assert!(
            matches!(eng.lines[0].state, LegatoState::Idle),
            "pacific transition must not arm a delay countdown"
        );
        assert_eq!(eng.lines[0].note, Some(62));

        render_ms(&mut eng, 50);
        eng.note_on(64, 40);
        assert_eq!(eng.lines[0].note, Some(64));

        // Return legato: releasing the sounding note falls back to a held one
        // immediately (also style Pacific → no Pending).
        eng.note_off(64);
        assert!(matches!(eng.lines[0].state, LegatoState::Idle));
        assert_eq!(eng.lines[0].note, Some(62));

        // Fire log: two forward transitions + one return, all reactive.
        let log = eng.legato_fire_log();
        assert_eq!(log.len(), 3, "expected 3 legato fires, got {log:?}");
        assert_eq!((log[0].from_note, log[0].to_note), (60, 62));
        assert_eq!((log[1].from_note, log[1].to_note), (62, 64));
        assert_eq!((log[2].from_note, log[2].to_note), (64, 62));
    }

    #[test]
    fn per_line_mono_legato_is_independent() {
        // Two divisi lines on one engine: each keeps its own mono cursor,
        // and a prefire on line 1 must not disturb line 0's sounding note.
        let mut eng = mono_legato_engine(&[60, 62, 64, 67]);

        eng.note_on_line(0, 60, 40);
        eng.note_on_line(1, 64, 40);
        assert_eq!(eng.lines[0].note, Some(60));
        assert_eq!(eng.lines[1].note, Some(64));

        // Reactive transition on line 0 only. The O+D countdown is armed only
        // for soft velocity + a fast inter-onset gap (real O+D model).
        render_ms(&mut eng, 50);
        eng.note_on_line(0, 62, 40);
        assert!(matches!(eng.lines[0].state, LegatoState::Pending { .. }));
        assert!(matches!(eng.lines[1].state, LegatoState::Idle));

        // Document prefire on line 1 fires immediately, line 0 unaffected.
        eng.legato_prefire_line(1, 67, 100);
        assert_eq!(eng.lines[1].note, Some(67));
        assert!(matches!(eng.lines[0].state, LegatoState::Pending { .. }));

        render_ms(&mut eng, 200);
        assert_eq!(eng.lines[0].note, Some(62));
        assert_eq!(eng.lines[1].note, Some(67));

        // Note-offs on line 1 end only line 1 (release the silent key first
        // so the mono line doesn't fall back to it).
        eng.note_off_line(1, 64);
        eng.note_off_line(1, 67);
        assert_eq!(eng.lines[1].note, None);
        assert_eq!(eng.lines[0].note, Some(62));

        // Exactly one reactive trigger: line 0's overlapping note-on. The
        // prefire and the plain note-offs are not reactive.
        assert_eq!(eng.reactive_legato_fires(), 1);
    }

    // ── Live auto-divisi gating (StrictLive) ─────────────────────────────────

    #[test]
    fn live_chord_within_window_fans_out_as_fresh_attacks() {
        let mut eng = mono_legato_engine(&[60, 64, 67]);
        eng.set_legato_fire_log_enabled(true);

        // A triad struck at once: three fresh attacks on three lines, in
        // arrival order — never legato.
        eng.note_on(60, 100);
        eng.note_on(64, 100);
        eng.note_on(67, 100);
        assert_eq!(eng.lines[0].note, Some(60));
        assert_eq!(eng.lines[1].note, Some(64));
        assert_eq!(eng.lines[2].note, Some(67));

        render_ms(&mut eng, 200);
        assert!(
            eng.legato_fire_log().is_empty(),
            "a chord must not fire legato transitions"
        );
        assert_eq!(eng.reactive_legato_fires(), 0);
    }

    #[test]
    fn live_stepwise_line_stays_legato_on_one_line() {
        let mut eng = mono_legato_engine(&[60, 62, 64]);
        eng.set_legato_fire_log_enabled(true);

        eng.note_on(60, 100);
        render_ms(&mut eng, 50);
        eng.note_on(62, 100); // whole step ≤ live_legato_interval_max (2)
        render_ms(&mut eng, 200); // countdown elapses (low_latency)
        eng.note_off(60);
        eng.note_on(64, 100);
        render_ms(&mut eng, 200);

        let log = eng.legato_fire_log();
        assert_eq!(log.len(), 2, "both steps transitioned");
        assert!(log.iter().all(|e| e.line == 0), "one mono line throughout");
        assert_eq!(eng.lines[0].note, Some(64));
        assert!(eng.lines[1].note.is_none(), "no divisi for a mono line");
    }

    #[test]
    fn live_wide_leap_takes_a_fresh_line_not_legato() {
        let mut eng = mono_legato_engine(&[60, 69]);
        eng.set_legato_fire_log_enabled(true);

        eng.note_on(60, 100);
        render_ms(&mut eng, 50);
        eng.note_on(69, 100); // major 6th > interval gate
        render_ms(&mut eng, 200);

        assert_eq!(eng.lines[0].note, Some(60), "held note keeps its line");
        assert_eq!(eng.lines[1].note, Some(69), "leap starts a fresh line");
        assert!(
            eng.legato_fire_log().is_empty(),
            "wide leaps must not legato in strict live mode"
        );
    }

    #[test]
    fn live_held_top_with_moving_bass_keeps_lines_apart() {
        let mut eng = mono_legato_engine(&[72, 50, 52]);
        eng.set_legato_fire_log_enabled(true);

        eng.note_on(72, 100); // held top → line 0
        render_ms(&mut eng, 50);
        eng.note_on(50, 100); // far below → its own line
        render_ms(&mut eng, 50);
        eng.note_on(52, 100); // whole step from the bass → bass line legato
        render_ms(&mut eng, 200);

        assert_eq!(eng.lines[0].note, Some(72), "top never stolen");
        assert_eq!(eng.lines[1].note, Some(52), "bass moved on its own line");
        let log = eng.legato_fire_log();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].line, 1);
        assert_eq!(log[0].from_note, 50);
        assert_eq!(log[0].to_note, 52);
    }

    #[test]
    fn zoned_legato_delays_then_fires_directional() {
        // Legato articulation with directional zones (up/down) for two notes.
        // Build with synthetic zone_paths so the patch reads as zoned (is_zoned
        // checks resolved paths; the samples themselves never load in a test).
        let spec = crate::LibrarySpec::from_styx(
            "name \"s\"\n\
             articulations (\n\
               {id Leg, label Leg, kind @Legato}\n\
             )\n\
             zones (\n\
               {file \"u60.wav\", key_min 60, key_max 60, root_key 60, vel_min 0, vel_max 127, articulation \"Leg\", direction \"up\"}\n\
               {file \"d60.wav\", key_min 60, key_max 60, root_key 60, vel_min 0, vel_max 127, articulation \"Leg\", direction \"down\"}\n\
               {file \"u62.wav\", key_min 62, key_max 62, root_key 62, vel_min 0, vel_max 127, articulation \"Leg\", direction \"up\"}\n\
               {file \"d62.wav\", key_min 62, key_max 62, root_key 62, vel_min 0, vel_max 127, articulation \"Leg\", direction \"down\"}\n\
             )\n",
        )
        .expect("parse styx");
        let mut patch = crate::PlayerPatch::from_spec(spec);
        patch.zone_paths = (0..patch.spec.zones.len())
            .map(|i| std::path::PathBuf::from(format!("z{i}.wav")))
            .collect();
        let mut eng = SampleEngine::new(patch, 48_000, "", "");
        eng.set_articulation("Leg");
        assert!(eng.legato_enabled, "legato on by default");
        assert_eq!(eng.articulation(), "Leg");
        assert!(
            matches!(
                eng.patch().spec.articulation("Leg").map(|a| a.kind.clone()),
                Some(ArticulationKind::Legato)
            ),
            "Leg parsed as @Legato"
        );
        let u62 = eng.patch().spec.zones[2].clone();
        let d62 = eng.patch().spec.zones[3].clone();

        // First note sounds immediately — no pending legato.
        eng.note_on(60, 40);
        assert!(matches!(eng.lines[0].state, LegatoState::Idle));

        // Second note while the first is held → delayed (the CSS latency):
        // pending, not yet fired. Real O+D is non-zero only for soft velocity
        // (≤64) + a fast inter-onset gap (< ~100 ms), giving the ~77 ms delay.
        render_ms(&mut eng, 50);
        eng.note_on(62, 40);
        assert!(
            matches!(eng.lines[0].state, LegatoState::Pending { .. }),
            "soft+fast legato note should delay (latency), not fire instantly"
        );

        // Render past the (default 100ms) delay → fires; direction up (62 > 60).
        let frames = (eng.sample_rate as usize / 1000) * 200; // 200 ms
        let mut out = vec![0.0f32; frames * 2];
        eng.render(&mut out);
        assert!(
            matches!(eng.lines[0].state, LegatoState::Idle),
            "legato fired"
        );
        assert_eq!(eng.play_direction, "up");
        // The up-zone for the target is now selected; the down-zone is not.
        assert!(eng.zone_selected(&u62, 62, 100, ZoneTrigger::Attack));
        assert!(!eng.zone_selected(&d62, 62, 100, ZoneTrigger::Attack));
    }

    #[test]
    fn decoded_short_velocity_layers_and_velvol() {
        // Decoded CSS Spiccato (KSP g25wo=0): %g1qri=[1,25,55,108],
        // %bcez1=[-9,-10,-12,-6], ktuur=1 (4 bands). Verifies band selection and
        // the $arhiq intra-layer velocity→volume law against hand-computed values.
        // `apply_short_velvol` gates the $arhiq trim (off for CSS; on here to test
        // the curve).
        let eng = engine_from_styx(
            "name \"s\"\n\
             dynamics { enable_velocity_layers true\n apply_short_velvol true }\n\
             articulations (\n\
               {id Spiccato, label Spicc, kind @Short, dynamics (pp p mf ff),\n\
                vel_thresholds (25 55 108), vel_layer_db (-9 -10 -12 -6)}\n\
             )\n\
             zones (\n\
               {file \"a.wav\", key_min 60, key_max 60, root_key 60, vel_min 0, vel_max 127, articulation \"Spiccato\"}\n\
             )\n",
        );
        let cases = [
            (10u8, "pp", -5.87f32), // band0: (25-10)*-9/23
            (40, "p", -5.17),       // band1: (55-40)*-10/29
            (80, "mf", -6.46),      // band2: (108-80)*-12/52
            (120, "ff", -2.21),     // top:   (127-120)*-6/19
        ];
        for (vel, want_dyn, db) in cases {
            let (got_dyn, got_db) = eng.short_layer_and_velvol("Spiccato", vel);
            assert_eq!(got_dyn, want_dyn, "dynamic for vel {vel}");
            assert!(
                (got_db - db).abs() < 0.05,
                "velvol for vel {vel}: got {got_db:.2}, want {db:.2}"
            );
        }
        // Gate off → falls back to the even split with no velvol trim.
        let plain = engine_from_styx(
            "name \"s\"\n\
             articulations (\n\
               {id Spiccato, label Spicc, kind @Short, dynamics (pp p mf ff),\n\
                vel_thresholds (25 55 108), vel_layer_db (-9 -10 -12 -6)}\n\
             )\n\
             zones (\n\
               {file \"a.wav\", key_min 60, key_max 60, root_key 60, vel_min 0, vel_max 127, articulation \"Spiccato\"}\n\
             )\n",
        );
        let (_, db) = plain.short_layer_and_velvol("Spiccato", 40);
        assert_eq!(db, 0.0, "gated off → no velocity-volume trim");
    }

    #[test]
    fn short_velocity_selects_layer_and_cc1_selects_type() {
        // KSP-confirmed CSS short model: VELOCITY selects the recorded dynamic
        // LAYER (via the type's `%g1qri` thresholds, mapped 1:1 onto the recorded
        // dynamics) AND scales volume within the band (`$arhiq`); CC1 selects the
        // short TYPE via `short_note_cc1_map`, never the dynamic.
        let styx = "name \"s\"\n\
             dynamics {\n\
               enable_velocity_layers true\n\
               short_note_cc1_map {\n\
                 0-63   Spiccato\n\
                 64-127 Staccato\n\
               }\n\
             }\n\
             articulations (\n\
               {id Spiccato, label Spicc, kind @Short, dyn_ctrl velocity, dynamics (pp p mf ff),\n\
                vel_thresholds (25 55 108), vel_layer_db (-9 -10 -12 -6)}\n\
               {id Staccato, label Stacc, kind @Short, dyn_ctrl velocity, dynamics (pp mp f fff),\n\
                vel_thresholds (51 83 127), vel_layer_db (-17 -8 -13 0)}\n\
             )\n\
             zones (\n\
               {file \"a.wav\", key_min 60, key_max 60, root_key 60, vel_min 0, vel_max 127, articulation \"Spiccato\"}\n\
             )\n";
        let eng = engine_from_styx(styx);

        // VELOCITY → LAYER via %g1qri. Staccato (pp mp f fff) thresholds
        // (51 83 127): trailing 127 is the open-top marker → 3 bands
        // [1,50][51,82][83,127]; 4 dynamics > 3 bands → TOP-align (drop the
        // unreachable pp) → mp/f/fff. Matches the reference render's collapsed
        // Staccato ladder (vel 40/80/120 → mp/f/fff, not pp/mp/f).
        for (vel, want) in [(40u8, "mp"), (80, "f"), (120, "fff"), (127, "fff")] {
            let (dynamic, _) = eng.short_layer_and_velvol("Staccato", vel);
            assert_eq!(dynamic, want, "Staccato vel {vel} → layer");
        }

        // A trailing terminal 127 is the open-top marker, NOT an extra band:
        // Sfz (mf f fff) thresholds (45 65 127) → 3 bands [1,44]mf [45,64]f
        // [65,127]fff, counts match → 1:1.
        let sfz = engine_from_styx(
            "name \"s\"\n\
             dynamics { enable_velocity_layers true }\n\
             articulations (\n\
               {id Sfz, label Sfz, kind @Short, dyn_ctrl velocity, dynamics (mf f fff),\n\
                vel_thresholds (45 65 127), vel_layer_db (-17 -6 -9 0)}\n\
             )\n\
             zones (\n\
               {file \"a.wav\", key_min 60, key_max 60, root_key 60, vel_min 0, vel_max 127, articulation \"Sfz\"}\n\
             )\n",
        );
        for (vel, want) in [(40u8, "mf"), (60, "f"), (80, "fff"), (120, "fff")] {
            let (dynamic, _) = sfz.short_layer_and_velvol("Sfz", vel);
            assert_eq!(dynamic, want, "Sfz vel {vel} → layer");
        }

        // CC1 selects the short TYPE (collapse to the CC1=90 type), never dynamic.
        let mut eng = engine_from_styx(styx);
        eng.set_articulation("Spiccato");
        eng.cc(1, 90); // 90 → Staccato via short_note_cc1_map
        assert_eq!(
            eng.articulation(),
            "Staccato",
            "CC1 selects the short TYPE via short_note_cc1_map"
        );
    }

    // r[verify signal.sampling.articulation.select]
    #[test]
    fn latched_cc_selector_latches_articulation_live() {
        // `selector uacc`: a CC32 value arriving BEFORE a note-on latches the
        // articulation the following notes play — keyswitch semantics on a CC.
        let styx = "name \"u\"\n\
             selector uacc\n\
             articulations (\n\
               {id Vibsus, label Sustain, kind @Sustain}\n\
               {id Spiccato, label Spiccato, kind @Short}\n\
               {id Stac, label Staccato, kind @Short}\n\
             )\n\
             zones (\n\
               {file \"a.wav\", key_min 0, key_max 127, root_key 60, articulation \"Vibsus\"}\n\
               {file \"b.wav\", key_min 0, key_max 127, root_key 60, articulation \"Spiccato\"}\n\
               {file \"c.wav\", key_min 0, key_max 127, root_key 60, articulation \"Stac\"}\n\
             )\n";
        let mut eng = engine_from_styx(styx);
        eng.set_articulation("Vibsus");

        // CC32=42 (Very Short / spiccato) latches Spiccato for the next note.
        eng.cc(32, 42);
        assert_eq!(eng.articulation(), "Spiccato", "CC latch before note-on");
        eng.note_on(60, 100);
        assert_eq!(eng.articulation(), "Spiccato", "note plays the latch");
        eng.note_off(60);

        // Re-latch switches; the latch persists across notes until re-sent.
        eng.cc(32, 40);
        assert_eq!(eng.articulation(), "Stac");
        eng.note_on(62, 100);
        eng.note_off(62);
        eng.note_on(64, 100);
        assert_eq!(eng.articulation(), "Stac", "latch persists across notes");
        eng.note_off(64);

        // Unknown code (a gap in the table) keeps the previous latch.
        eng.cc(32, 99);
        assert_eq!(eng.articulation(), "Stac", "unknown code keeps the latch");

        // Back to Long (1).
        eng.cc(32, 1);
        assert_eq!(eng.articulation(), "Vibsus");

        // No `selector` in the spec → CC32 is inert (defaults untouched).
        let mut plain = engine_from_styx(
            "name \"p\"\n\
             articulations (\n\
               {id Vibsus, label Sustain, kind @Sustain}\n\
               {id Spiccato, label Spiccato, kind @Short}\n\
             )\n\
             zones (\n\
               {file \"a.wav\", key_min 0, key_max 127, root_key 60, articulation \"Vibsus\"}\n\
             )\n",
        );
        plain.set_articulation("Vibsus");
        plain.cc(32, 42);
        assert_eq!(plain.articulation(), "Vibsus", "no selector: CC32 inert");
    }

    #[test]
    fn legato_pairs_nonvib_and_vibrato_for_cc2() {
        // CC2 vibrato crossfades a non-vib / vib pair. CSS has them for legato
        // (Leg ↔ NVLeg) as well as sustains — the pair lookup must find both.
        let eng = engine_from_styx(
            "name \"s\"\n\
             dynamics { vibrato_controller CC2 }\n\
             articulations (\n\
               {id NVLeg, label NVLeg, kind @Legato}\n\
               {id Leg, label Leg, kind @Legato}\n\
             )\n\
             zones (\n\
               {file \"a.wav\", key_min 60, key_max 60, root_key 60, vel_min 0, vel_max 127, articulation \"NVLeg\"}\n\
             )\n",
        );
        assert_eq!(eng.find_vibrato_pair_id("NVLeg").as_deref(), Some("Leg"));
        assert_eq!(eng.find_vibrato_pair_id("Leg").as_deref(), Some("NVLeg"));
    }

    #[test]
    fn pinned_articulation_selects_by_artic_ignoring_key() {
        // Hats-style pack: multiple articulations on different keys.
        let mut eng = engine_from_styx(
            "name \"h\"\n\
             category \"drum-kit\"\n\
             zones (\n\
               {\n\
                 file \"c.wav\"\n\
                 key_min 42\n\
                 key_max 42\n\
                 root_key 42\n\
                 vel_min 0\n\
                 vel_max 127\n\
                 articulation \"Closed Tip\"\n\
               }\n\
               {\n\
                 file \"o.wav\"\n\
                 key_min 46\n\
                 key_max 46\n\
                 root_key 46\n\
                 vel_min 0\n\
                 vel_max 127\n\
                 articulation \"Open 1\"\n\
               }\n\
             )\n",
        );
        assert!(eng.percussion && !eng.single_attack_key);
        let closed = eng.patch().spec.zones[0].clone();
        let open = eng.patch().spec.zones[1].clone();

        // No pin: multi-key drum is addressed by its native keys.
        assert!(eng.zone_selected(&closed, 42, 100, ZoneTrigger::Attack));
        assert!(!eng.zone_selected(&closed, 49, 100, ZoneTrigger::Attack));

        // Pin "Open 1": fires Open on any note, never Closed.
        eng.pin_articulation(Some("open 1".to_string())); // case-insensitive
        assert!(eng.zone_selected(&open, 53, 100, ZoneTrigger::Attack));
        assert!(eng.zone_selected(&open, 99, 100, ZoneTrigger::Attack));
        assert!(!eng.zone_selected(&closed, 53, 100, ZoneTrigger::Attack));

        // Per-trigger (per-route) articulation takes precedence over the pin,
        // so one shared engine can serve many routed notes; cleared after.
        eng.trigger_articulation = Some("Closed Tip".to_string());
        assert!(eng.zone_selected(&closed, 49, 100, ZoneTrigger::Attack));
        assert!(!eng.zone_selected(&open, 49, 100, ZoneTrigger::Attack));
        eng.trigger_articulation = None;
        // Falls back to the pin once the transient clears.
        assert!(eng.zone_selected(&open, 49, 100, ZoneTrigger::Attack));

        // note_on_articulated must leave no residual state.
        eng.note_on_articulated(49, 100, Some("Closed Tip"));
        assert!(eng.trigger_articulation.is_none());
    }

    #[test]
    fn choke_group_setter_and_percussion_one_shot() {
        let mut eng = engine_from_styx(
            "name \"h\"\n\
             category \"drum-kit\"\n\
             zones (\n\
               {\n\
                 file \"c.wav\"\n\
                 key_min 42\n\
                 key_max 42\n\
                 root_key 42\n\
                 vel_min 0\n\
                 vel_max 127\n\
                 articulation \"Closed Tip\"\n\
               }\n\
             )\n",
        );
        // Percussion → voices spawn one-shot (Short) and ignore note-off.
        assert!(eng.percussion);
        // Mono choke (hats): group set, no choke_on → every hit chokes.
        assert!(eng.engine_choke_group.is_none());
        eng.set_choke_group(Some("hats"), &[]);
        assert_eq!(eng.engine_choke_group, Some(stable_group_hash("hats")));
        assert!(eng.should_engine_choke(), "mono: any hit chokes");
        eng.set_choke_group(Some(""), &[]); // empty clears
        assert!(eng.engine_choke_group.is_none());
        assert!(!eng.should_engine_choke());

        // Selective choke (cymbals): only the "Choke" articulation chokes;
        // crashes ring and overlap.
        eng.set_choke_group(Some("ride"), &["Choke".to_string()]);
        eng.trigger_articulation = Some("Crash".to_string());
        assert!(!eng.should_engine_choke(), "crash must not choke the ring");
        eng.trigger_articulation = Some("choke".to_string()); // case-insensitive
        assert!(
            eng.should_engine_choke(),
            "choke articulation stops the ring"
        );
    }

    #[test]
    fn pitched_sampler_still_gates_on_key() {
        // No drum category → pitched: key range gates, no collapse.
        let eng = engine_from_styx(
            "name \"p\"\n\
             instrument \"piano\"\n\
             zones (\n\
               {\n\
                 file \"p.wav\"\n\
                 key_min 60\n\
                 key_max 72\n\
                 root_key 60\n\
                 vel_min 0\n\
                 vel_max 127\n\
               }\n\
             )\n",
        );
        assert!(!eng.percussion);
        let z = &eng.patch().spec.zones[0];
        assert!(eng.zone_selected(z, 65, 100, ZoneTrigger::Attack));
        assert!(!eng.zone_selected(z, 40, 100, ZoneTrigger::Attack));
    }

    /// Build a `SampleEngine` from an inline styx spec plus a synthetic sample
    /// map (parsed from filenames — no files touched).
    fn engine_from_styx_and_paths(styx: &str, paths: Vec<std::path::PathBuf>) -> SampleEngine {
        let spec = crate::LibrarySpec::from_styx(styx).expect("parse styx");
        let mut patch = crate::PlayerPatch::from_spec(spec);
        patch.map = crate::SampleMap::from_paths(paths);
        SampleEngine::new(patch, 48_000, "main", "Main")
    }

    #[test]
    fn pedal_pair_rejects_noise_body_and_release_follows_pedal() {
        // Rhodes-style multi-sample pack: a full-keyboard body (`lacrm`), a
        // directional release (`lacr`, rel/relsl variants), and a pedal-NOISE
        // articulation (`lacrped`) whose only "notes" are the pedal-state
        // index (0 = up, 1 = down).
        let styx = "name \"r\"\n\
             sections ({\n\
               id main\n\
               label m\n\
               note_grid ()\n\
               lowest_note C-1\n\
               highest_note C8\n\
             })\n\
             mics ({\n\
               id Main\n\
               label Main\n\
               kind blended\n\
             })\n\
             articulations (\n\
             {\n\
               id lacr\n\
               label \"lacr\"\n\
               kind @Release\n\
               dynamics (\n\
                 \"100\"\n\
               )\n\
               rr 1\n\
               dyn_ctrl velocity\n\
             } {\n\
               id lacrm\n\
               label \"lacrm\"\n\
               kind @OneShot\n\
               dynamics (\n\
                 \"100\"\n\
               )\n\
               rr 1\n\
               dyn_ctrl velocity\n\
               release_artic lacr\n\
             } {\n\
               id lacrped\n\
               label \"lacrped\"\n\
               kind @OneShot\n\
               dynamics (\n\
                 \"127\"\n\
               )\n\
               rr 1\n\
               dyn_ctrl velocity\n\
             })\n";
        let mut paths: Vec<std::path::PathBuf> = Vec::new();
        for n in 21u8..=108 {
            paths.push(format!("RR01 lacrm {n} 100.flac").into());
            paths.push(format!("RR01 lacr {n} 100 rel.flac").into());
            paths.push(format!("RR01 lacr {n} 100 relsl.flac").into());
        }
        // Pedal noise: two fixed samples at the pedal-state index.
        paths.push("RR01_SL01 LACR Ped_0 r03.flac".into());
        paths.push("RR01_SL01 LACR Ped_1 r03.flac".into());
        let eng = engine_from_styx_and_paths(styx, paths);

        // The body spans the keyboard; the pedal noise spans ≤2 notes.
        assert!(eng.artic_note_span("lacrm") >= 80, "body should span the keyboard");
        assert!(eng.artic_note_span("lacrped") <= 2, "pedal noise spans ≤2 notes");

        // find_pedal_pair must NOT mistake pedal noise for a pedal-down body —
        // otherwise every note held under the pedal plays the clunk instead of
        // the instrument tone.
        assert_eq!(eng.find_pedal_pair("lacrm"), None, "noise must never be the body");
    }
