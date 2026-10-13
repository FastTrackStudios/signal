//! `FTS Signal Rig` on a track: made by name, as the engine makes FX; its
//! state says the bass's Metal profile on Clean; it plays the track's
//! blocks; its service — found by the id in its state, as the window and
//! the actions find it — switches it to Drive, heard to change; and its
//! state then says Drive.

#[cfg(test)]
mod signal_rig {
    use daw_standalone::plugin::{FxFactory, PluginEvents};
    use signal_guitar::proto::rig::Rig as _;

    const RATE: f64 = 48_000.0;
    const BLOCK: usize = 480;

    /// The output level over `blocks` blocks of a low E, the last half.
    fn level(
        rig: &mut dyn daw_standalone::plugin::PluginInstance,
        from: usize,
        blocks: usize,
    ) -> f32 {
        let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
        let (mut sum, mut n) = (0.0_f32, 0_u16);
        for b in from..from.saturating_add(blocks) {
            let input: Vec<f32> = (0..BLOCK)
                .map(|i| {
                    let at = b.saturating_mul(BLOCK).saturating_add(i);
                    let t = f64::from(u32::try_from(at).unwrap_or(0)) / RATE;
                    num_traits::ToPrimitive::to_f32(
                        &(0.3 * (std::f64::consts::TAU * 41.2 * t).sin()),
                    )
                    .unwrap_or(0.0)
                })
                .collect();
            rig.process_block(&input, &input, &mut l, &mut r, &PluginEvents::EMPTY)
                .expect("a block");
            if b >= from.saturating_add(blocks / 2) {
                sum += l.iter().map(|s| s * s).sum::<f32>();
                n = n.saturating_add(u16::try_from(BLOCK).unwrap_or(0));
            }
        }
        (sum / f32::from(n.max(1))).sqrt()
    }

    #[test]
    fn a_signal_rig_plays_its_track_and_its_service_switches_it() {
        let root = std::env::temp_dir().join(format!("signal-rig-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // SAFETY: the only test in this binary, before any rig starts.
        unsafe {
            std::env::set_var("FTS_SIGNAL_RIG_ROOT", &root);
            std::env::set_var("XDG_CONFIG_HOME", root.join("xdg"));
            std::env::set_var("SIGNAL_RIG_EPHEMERAL", "0");
            std::env::set_var("SIGNAL_RELOAD_LOG", root.join("config-reload.log"));
            std::env::set_var("SIGNAL_DROP_LOG", root.join("dropouts.log"));
            std::env::remove_var("SIGNAL_RIG_DESIGN");
        }
        let mut rig = signal_fx::SignalFx
            .create(signal_fx::SIGNAL_RIG, RATE)
            .expect("made by name");
        rig.load_state(br#"{"instrument": "bass", "profile": "Metal", "patch": "Clean"}"#)
            .expect("a state");
        rig.prepare(RATE, 512).expect("prepared");

        let saved: signal_fx::signal_rig::RigState =
            serde_json::from_slice(&rig.save_state().expect("saved")).expect("its state");
        assert_eq!(saved.patch.as_deref(), Some("Clean"), "{saved:?}");
        assert_eq!(saved.profile.as_deref(), Some("Metal"));
        let clean = level(rig.as_mut(), 0, 100);
        assert!(clean > 1e-3, "it plays the track's audio: {clean}");

        // Its service, by its id: what the switches and the browser drive.
        let playing = signal_fx::signal_rig::rig(&saved.id).expect("found by its id");
        let drive = playing
            .service
            .patches()
            .iter()
            .position(|p| p.name == "Drive")
            .and_then(|i| u32::try_from(i).ok())
            .expect("Metal has Drive");
        assert!(playing.service.select_patch(drive).ok);
        let driven = level(rig.as_mut(), 100, 100);
        assert!(
            (driven / clean - 1.0).abs() > 0.05,
            "a different sound: {clean} vs {driven}"
        );
        let saved: signal_fx::signal_rig::RigState =
            serde_json::from_slice(&rig.save_state().expect("saved")).expect("its state");
        assert_eq!(
            saved.patch.as_deref(),
            Some("Drive"),
            "the state says what plays"
        );

        // The Patch parameter, as automation sends it: named by the
        // patches, and switching the rig back to Clean.
        let choices: Vec<String> = rig
            .param_choices(signal_fx::signal_rig::PATCH)
            .into_iter()
            .map(|(_, n)| n)
            .collect();
        assert!(choices.iter().any(|c| c == "Clean"), "{choices:?}");
        let clean_at = rig
            .text_to_value(signal_fx::signal_rig::PATCH, "clean")
            .expect("Clean by name");
        let input = vec![0.0_f32; BLOCK];
        let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
        let point = [(signal_fx::signal_rig::PATCH, clean_at)];
        rig.process_block(
            &input,
            &input,
            &mut l,
            &mut r,
            &PluginEvents {
                params: &point,
                ..PluginEvents::EMPTY
            },
        )
        .expect("a block");
        // The switcher takes it off the audio thread within a few ms.
        let switched = (0..100).any(|_| {
            std::thread::sleep(std::time::Duration::from_millis(5));
            playing.playing().1.as_deref() == Some("Clean")
        });
        assert!(
            switched,
            "the automation switched it: {:?}",
            playing.playing()
        );
    }
}
