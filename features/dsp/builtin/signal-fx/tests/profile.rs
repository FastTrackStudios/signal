//! The bass rig in profile mode, on signal's shipped bass library (seeded
//! into a folder of the test's own): its scenes are the profile's, its
//! Scene parameter is named by them and switches the sound, and its state
//! says which scene plays.

#[cfg(test)]
mod profile {
    use daw_standalone::plugin::{PluginEvents, PluginInstance};
    use signal_fx::rig::{Rig, SCENE};

    const RATE: f64 = 48_000.0;
    const BLOCK: usize = 512;

    /// A low E (41 Hz) played hard, a block of it.
    fn bass(block: usize, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let at = block.saturating_mul(n).saturating_add(i);
                let t = f64::from(u32::try_from(at).unwrap_or(0)) / RATE;
                num_traits::ToPrimitive::to_f32(&((t * 41.2 * std::f64::consts::TAU).sin() * 0.5))
                    .unwrap_or(0.0)
            })
            .collect()
    }

    /// The rig's output level over `blocks` blocks, `params` given in the first.
    fn level(rig: &mut Rig, blocks: usize, params: &[(u32, f64)]) -> f32 {
        let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
        let mut sum = 0.0_f32;
        for b in 0..blocks {
            let input = bass(b, BLOCK);
            let events = PluginEvents {
                params: if b == 0 { params } else { &[] },
                ..PluginEvents::EMPTY
            };
            rig.process_block(&input, &input, &mut l, &mut r, &events)
                .expect("a block");
            assert!(l.iter().chain(&r).all(|s| s.is_finite()), "finite");
            sum += l.iter().map(|s| s * s).sum::<f32>();
        }
        let n = blocks.saturating_mul(BLOCK);
        (sum / f32::from(u16::try_from(n).unwrap_or(u16::MAX))).sqrt()
    }

    #[test]
    fn a_profiles_scenes_are_the_rigs_and_switch_its_sound() {
        let dir = std::env::temp_dir().join(format!("fts-bass-rig-{}", std::process::id()));
        // SAFETY: set before any thread of this test process reads it.
        unsafe { std::env::set_var("FTS_BASS_RIG_DIR", &dir) };

        let mut rig = Rig::new(RATE);
        rig.load_state(br#"{"profile": "Metal", "scene": "Clean"}"#)
            .expect("a state");
        rig.prepare(RATE, u32::try_from(BLOCK).unwrap_or(512))
            .expect("prepared");

        // The Scene parameter is the profile's scenes, by name.
        let choices: Vec<String> = rig
            .param_choices(SCENE)
            .into_iter()
            .map(|(_, n)| n)
            .collect();
        assert_eq!(
            choices,
            ["Clean", "Crunch", "Drive"],
            "the Metal profile's scenes"
        );
        assert_eq!(rig.text_to_value(SCENE, "crunch"), Some(0.5));
        assert_eq!(rig.value_to_text(SCENE, 1.0).as_deref(), Some("Drive"));
        assert_eq!(rig.param_value(SCENE), Some(0.0), "on Clean");

        let clean = level(&mut rig, 40, &[]);
        assert!(clean > 1e-3, "the clean scene plays: {clean}");
        // Switched to Drive by the parameter, as automation switches it.
        let drive = level(&mut rig, 40, &[(SCENE, 1.0)]);
        assert!(drive > 1e-3, "the drive scene plays: {drive}");
        assert!(
            (drive / clean - 1.0).abs() > 0.05,
            "a different sound: {clean} vs {drive}"
        );
        assert_eq!(rig.param_value(SCENE), Some(1.0));
        let saved: serde_json::Value =
            serde_json::from_slice(&rig.save_state().expect("saved")).expect("json");
        assert_eq!(
            saved["scene"], "Drive",
            "the state says which plays: {saved}"
        );
        assert_eq!(saved["profile"], "Metal");
    }
}
