use dawfile_reaper::RppSerialize;
use dawfile_reaper::builder::ReaperProjectBuilder;
fn main() {
    let out = std::env::args().nth(1).unwrap();
    // Stepped gains across the dB range, labeled by name so they're verifiable in PT.
    let steps: &[(f64, &str)] = &[
        (0.251189, "-12dB"),
        (0.501187, "-6dB"),
        (0.707946, "-3dB"),
        (1.0, "0dB"),
        (1.412538, "+3dB"),
        (1.995262, "+6dB"),
        (3.981072, "+12dB"),
    ];
    let wavs = [
        "/Users/codywright/Downloads/PNG WORSHIP COLLECTIVE SESSION FILES/10 REASON WHY/Audio Files/10 REASON WHY demo (Bass)_1.1.wav",
        "/Users/codywright/Downloads/PNG WORSHIP COLLECTIVE SESSION FILES/10 REASON WHY/Audio Files/10 REASON WHY demo (Drums)_1.1.wav",
        "/Users/codywright/Downloads/PNG WORSHIP COLLECTIVE SESSION FILES/10 REASON WHY/Audio Files/10 REASON WHY demo (Guitar)_1.1.wav",
    ];
    let mut b = ReaperProjectBuilder::new().tempo_with_time_sig(120.0, 4, 4);
    // 3 tracks, each with all 7 gain steps as sequential clips (2s clip, 2.5s stride)
    for tk in 0..3 {
        let wav = wavs[tk].to_string();
        b = b.track(format!("Gain Track {}", tk + 1), move |mut t| {
            for (i, (g, lbl)) in steps.iter().enumerate() {
                let pos = i as f64 * 2.5;
                let g = *g;
                let nm = lbl.to_string();
                let w = wav.clone();
                t = t.item(pos, 2.0, move |it| it.name(&nm).source_wave(&w).gain(g));
            }
            t
        });
    }
    std::fs::write(&out, b.build().to_rpp_string()).unwrap();
    eprintln!("wrote {}", out);
}
