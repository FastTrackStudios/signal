fn main() -> Result<(), Box<dyn std::error::Error>> {
    let s = dawfile_protools::read_session(std::env::args().nth(1).unwrap().as_str(), 48000)?;
    println!("=== {} regions ===", s.audio_regions.len());
    for r in &s.audio_regions {
        println!(
            "  idx={} name={:?} start_pos={} sample_offset={} length={}",
            r.index, r.name, r.start_pos, r.sample_offset, r.length
        );
    }
    println!("=== tracks ===");
    for t in s.all_tracks() {
        for (ci, c) in t.regions.iter().enumerate() {
            let ar = s.audio_regions.iter().find(|r| r.index == c.region_index);
            println!(
                "  track={} clip={} region_index={} -> region={:?} clip_muted={}",
                t.name,
                ci,
                c.region_index,
                ar.map(|r| format!(
                    "name={:?} samp_off={} len={}",
                    r.name, r.sample_offset, r.length
                )),
                c.clip_muted
            );
        }
    }
    Ok(())
}
