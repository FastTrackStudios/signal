fn main() -> Result<(), Box<dyn std::error::Error>> {
    let s = dawfile_protools::read_session(std::env::args().nth(1).unwrap().as_str(), 48000)?;
    let mut total = 0usize;
    let mut flagged = 0usize;
    let mut muted = 0usize;
    let mut colored = 0usize;
    for t in s.all_tracks() {
        for r in &t.regions {
            total += 1;
            if r.clip_flag_53 {
                flagged += 1;
            }
            if r.clip_muted {
                muted += 1;
            }
            if let Some(c) = r.clip_color
                && c != -2
            {
                colored += 1;
            }
        }
    }
    println!(
        "total clips: {} | flag_53: {} | muted: {} | colored: {}",
        total, flagged, muted, colored
    );
    for t in s.all_tracks() {
        for (i, r) in t.regions.iter().enumerate() {
            if r.clip_flag_53 || r.clip_muted || r.clip_color.is_some_and(|c| c != -2 && c != 0) {
                println!(
                    "  track={} idx={} flag={} muted={} color={:?}",
                    t.name, i, r.clip_flag_53, r.clip_muted, r.clip_color
                );
            }
        }
    }
    Ok(())
}
