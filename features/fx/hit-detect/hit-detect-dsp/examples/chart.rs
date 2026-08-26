//! Charts a real recording — what the detector actually finds.
//!
//! Not a test: there is no ground truth for "where are the hits in this
//! song", so this exists to be *looked at* by someone who knows the
//! track. The unit tests pin the behaviour that can be stated exactly;
//! this is how the tuning gets judged.
//!
//! ```text
//! cargo run -p hit-detect-dsp --example chart -- <audio-file> [bpm]
//! ```
//!
//! Given a tempo it also prints each hit's position in bars, which is
//! the form a light cue is written in — and the quickest way to see
//! whether the detector is landing on the beat or between it.

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: chart <audio-file> [bpm]");
    let bpm: Option<f64> = args.next().and_then(|s| s.parse().ok());

    let (samples, sample_rate) = fts_sample::load_mono_f32(
        std::path::Path::new(&path),
        None,
        fts_sample::ResampleQuality::default(),
    )?;
    let config = hit_detect_dsp::Config::for_rate(f64::from(sample_rate));
    let analysis = hit_detect_dsp::analyze(&samples, &config);

    let secs = samples.len() as f64 / f64::from(sample_rate);
    println!(
        "{:.1}s at {} Hz — {} hits, {:.1} per second",
        secs,
        sample_rate,
        analysis.hits.len(),
        analysis.hits.len() as f64 / secs
    );

    let mut counts = [0usize; 3];
    for hit in &analysis.hits {
        counts[match hit.band {
            hit_detect_dsp::Band::Low => 0,
            hit_detect_dsp::Band::Mid => 1,
            hit_detect_dsp::Band::High => 2,
        }] += 1;
    }
    println!("low {} · mid {} · high {}\n", counts[0], counts[1], counts[2]);

    println!("strongest 25:");
    let mut strongest: Vec<_> = analysis.hits.iter().collect();
    strongest.sort_by(|a, b| b.strength.total_cmp(&a.strength));
    strongest.truncate(25);
    strongest.sort_by(|a, b| a.secs.total_cmp(&b.secs));
    for hit in strongest {
        let position = match bpm {
            Some(bpm) => {
                let beats = hit.secs * bpm / 60.0;
                format!("bar {:>3}.{:<4.2}", (beats / 4.0) as u32 + 1, beats % 4.0 + 1.0)
            }
            None => String::new(),
        };
        println!(
            "  {:>7.2}s {position}  {:<5} {:.2}  dyn {:.2}",
            hit.secs,
            format!("{:?}", hit.band),
            hit.strength,
            analysis.dynamics_at(hit.secs)
        );
    }
    Ok(())
}
