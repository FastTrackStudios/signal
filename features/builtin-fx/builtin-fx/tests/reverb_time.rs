//! A reverb set to a note value rings that note at the song's tempo: the
//! engine tells the plugin the tempo (`set_tempo`), the plugin reads its
//! Decay Div against it, the reverb rings the time. Nothing works the time
//! out on the way.

#[cfg(test)]
mod reverb_time {
    use daw_standalone::plugin::{FxFactory, PluginEvents};

    const SR: f64 = 48_000.0;
    const BLOCK: usize = 512;
    /// A second of blocks before the hit: the settings ramp in.
    const SETTLE_BLOCKS: usize = 94;
    /// Eight seconds of tail after it.
    const TAIL_BLOCKS: usize = 750;

    /// The RT60 FTS Reverb rings with `settings` (in its own words) at `bpm`:
    /// an impulse, the tail's Schroeder curve, T20 × 3.
    fn rings(settings: &[(&str, &str)], bpm: f64) -> f64 {
        let mut fx = builtin_fx::BuiltinFx
            .create("FTS Reverb", SR)
            .expect("the reverb");
        fx.prepare(SR, u32::try_from(BLOCK).expect("a block"))
            .expect("prepared");
        let params: Vec<(u32, f64)> = settings
            .iter()
            .map(|(p, t)| {
                builtin_fx::setting("FTS Reverb", p, t).unwrap_or_else(|| panic!("{p}={t}"))
            })
            .collect();
        let mut energy = Vec::new();
        for block in 0..SETTLE_BLOCKS + TAIL_BLOCKS {
            let mut input = vec![0.0_f32; BLOCK];
            if block == SETTLE_BLOCKS
                && let Some(first) = input.first_mut()
            {
                *first = 1.0;
            }
            let (mut l, mut r) = (vec![0.0_f32; BLOCK], vec![0.0_f32; BLOCK]);
            let events = PluginEvents {
                params: if block == 0 { &params } else { &[] },
                ..PluginEvents::EMPTY
            };
            fx.set_tempo(bpm);
            fx.process_block(&input, &input, &mut l, &mut r, &events)
                .expect("a block");
            if block >= SETTLE_BLOCKS {
                energy.extend(l.iter().zip(&r).map(|(a, b)| {
                    let (a, b) = (f64::from(*a), f64::from(*b));
                    a.mul_add(a, b * b)
                }));
            }
        }
        let mut acc = 0.0;
        let mut edc = vec![0.0; energy.len()];
        for i in (0..energy.len()).rev() {
            acc += energy[i];
            edc[i] = acc;
        }
        let db = |i: usize| 10.0 * (edc[i] / edc[0]).log10();
        let t5 = (0..edc.len()).find(|&i| db(i) <= -5.0).expect("a tail");
        let t25 = (t5..edc.len()).find(|&i| db(i) <= -25.0).expect("a decay");
        let samples = u32::try_from(t25.saturating_sub(t5)).expect("a tail under a day");
        f64::from(samples) / SR * 3.0
    }

    const HALF_NOTE_PLATE: &[(&str, &str)] = &[
        ("Space", "Plate"),
        ("Mix", "100%"),
        ("Pre-Delay", "0.0 ms"),
        ("Decay Mode", "Sync"),
        ("Decay Div", "1/2"),
    ];

    #[test]
    fn a_tail_in_notes_rings_the_note_at_the_songs_tempo() {
        // A half note: 1 s at 120 bpm, 2 s at 60.
        let at_120 = rings(HALF_NOTE_PLATE, 120.0);
        let at_60 = rings(HALF_NOTE_PLATE, 60.0);
        assert!(
            (at_120 - 1.0).abs() < 0.1,
            "a half note at 120 rang {at_120} s"
        );
        assert!(
            (at_60 - 2.0).abs() < 0.2,
            "a half note at 60 rang {at_60} s"
        );
    }

    #[test]
    fn a_tail_in_seconds_rings_that_long_whatever_the_tempo() {
        let room = [
            ("Space", "Room"),
            ("Mix", "100%"),
            ("Pre-Delay", "0.0 ms"),
            ("Decay Mode", "Time"),
            ("Decay Time", "0.60"),
        ];
        let (a, b) = (rings(&room, 90.0), rings(&room, 160.0));
        assert!(
            (a - 0.6).abs() < 0.06 && (b - 0.6).abs() < 0.06,
            "0.6 s rang {a} s and {b} s"
        );
    }

    /// The control: a `%` decay is the position it always was — the tempo does
    /// not touch it.
    #[test]
    fn a_percent_decay_ignores_the_tempo() {
        let plate = [
            ("Space", "Plate"),
            ("Mix", "100%"),
            ("Pre-Delay", "0.0 ms"),
            ("Decay", "20%"),
        ];
        let (a, b) = (rings(&plate, 60.0), rings(&plate, 180.0));
        assert!((a / b - 1.0).abs() < 0.02, "{a} s against {b} s");
    }

    /// A gated Non-Linear on a quarter note shuts on the beat: at 120 bpm
    /// its last sound within 40 dB of the burst is half a second after the
    /// hit, and it is silent after that.
    #[test]
    fn a_gate_in_notes_shuts_on_the_beat() {
        let mut fx = builtin_fx::BuiltinFx
            .create("FTS Reverb", SR)
            .expect("the reverb");
        fx.prepare(SR, 512).expect("prepared");
        let params: Vec<(u32, f64)> = [
            ("Space", "Non-Linear"),
            ("Mix", "100%"),
            ("Pre-Delay", "0.0 ms"),
            ("Character A", "35%"),
            ("Decay Mode", "Sync"),
            ("Decay Div", "1/4"),
        ]
        .iter()
        .map(|(p, t)| builtin_fx::setting("FTS Reverb", p, t).expect("a setting"))
        .collect();
        let mut out = Vec::new();
        for block in 0..SETTLE_BLOCKS + 188 {
            let mut input = vec![0.0_f32; BLOCK];
            if block == SETTLE_BLOCKS
                && let Some(first) = input.first_mut()
            {
                *first = 1.0;
            }
            let (mut l, mut r) = (vec![0.0_f32; BLOCK], vec![0.0_f32; BLOCK]);
            let events = PluginEvents {
                params: if block == 0 { &params } else { &[] },
                ..PluginEvents::EMPTY
            };
            fx.set_tempo(120.0);
            fx.process_block(&input, &input, &mut l, &mut r, &events)
                .expect("a block");
            if block >= SETTLE_BLOCKS {
                out.extend(l.iter().map(|v| f64::from(*v).abs()));
            }
        }
        let peak = out.iter().copied().fold(0.0_f64, f64::max);
        let last = out.iter().rposition(|v| *v > peak * 0.01).expect("a burst");
        let shut = f64::from(u32::try_from(last).expect("short")) / SR;
        assert!(
            (shut - 0.5).abs() < 0.025,
            "a quarter at 120 shut at {shut} s"
        );
    }
}
