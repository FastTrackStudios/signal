//! Generate a fade-probe RPP: many items with distinct fade-in/out lengths and
//! curve types, for the RPP→PTX→RPP round-trip fade validation.
//! Usage: cargo run -p daw-reaper --example gen_fades -- <out.rpp>
use dawfile_reaper::RppSerialize;
use dawfile_reaper::builder::ReaperProjectBuilder;
use dawfile_reaper::types::item::FadeCurveType as F;

const WAV: &str = "/Users/codywright/Downloads/PNG WORSHIP COLLECTIVE SESSION FILES/01 ALL THAT I AM/Audio Files/01 ALL THAT I AM (Vocals)_1.wav";

fn main() {
    let out = std::env::args().nth(1).expect("out.rpp");
    let mut b = ReaperProjectBuilder::new()
        .sample_rate(48000)
        .tempo_with_time_sig(120.0, 4, 4);

    // Track 1: isolated items, each a DISTINCT fade config (unique lengths +
    // curve so each is identifiable after the round-trip).
    // (label, pos, len, fade_in, fi_curve, fade_out, fo_curve)
    let cases: &[(&str, f64, f64, f64, F, f64, F)] = &[
        ("fi_lin_0.50", 0.0, 3.0, 0.50, F::Linear, 0.0, F::Linear),
        ("fo_lin_0.70", 4.0, 3.0, 0.0, F::Linear, 0.70, F::Linear),
        ("fio_0.30_0.40", 8.0, 3.0, 0.30, F::Linear, 0.40, F::Linear),
        ("fi_sqr_1.00", 12.0, 3.0, 1.00, F::Square, 0.0, F::Linear),
        (
            "fi_sse_0.60",
            16.0,
            3.0,
            0.60,
            F::SlowStartEnd,
            0.0,
            F::Linear,
        ),
        ("fo_fst_0.80", 20.0, 3.0, 0.0, F::Linear, 0.80, F::FastStart),
        ("fo_fend_0.90", 24.0, 3.0, 0.0, F::Linear, 0.90, F::FastEnd),
        ("fi_bez_1.10", 28.0, 3.0, 1.10, F::Bezier, 0.0, F::Linear),
        ("fio_sqr", 32.0, 3.0, 0.25, F::Square, 0.35, F::Square),
        ("nofade", 36.0, 3.0, 0.0, F::Linear, 0.0, F::Linear),
    ];
    b = b.track("Fades", |mut t| {
        for (name, pos, len, fi, fic, fo, foc) in cases.iter().copied() {
            t = t.item(pos, len, move |mut it| {
                it = it.name(name).source_wave(WAV);
                if fi > 0.0 {
                    it = it.fade_in(fi, fic);
                }
                if fo > 0.0 {
                    it = it.fade_out(fo, foc);
                }
                it
            });
        }
        t
    });

    // Track 2: overlapping items → crossfades (item A fade-out meets item B
    // fade-in over a 0.5s overlap).
    b = b.track("Crossfades", |mut t| {
        let mut pos = 0.0;
        for i in 0..5 {
            let p = pos;
            t = t.item(p, 3.0, move |it| {
                it.name(format!("xf{i}"))
                    .source_wave(WAV)
                    .fade_in(0.5, F::Linear)
                    .fade_out(0.5, F::Linear)
            });
            pos += 2.5; // 0.5s overlap with the previous 3.0s item
        }
        t
    });

    std::fs::write(&out, b.build().to_rpp_string()).unwrap();
    eprintln!("wrote {out}");
}
