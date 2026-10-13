fn main() -> Result<(), Box<dyn std::error::Error>> {
    let s = dawfile_protools::read_session(std::env::args().nth(1).unwrap().as_str(), 48000)?;
    for t in s.all_tracks() {
        println!(
            "track {}: vol_auto={} mute_auto={}",
            t.name,
            t.volume_automation.len(),
            t.mute_automation.len()
        );
        for bp in &t.volume_automation {
            println!(
                "  vol bp: time={} value={} cb",
                bp.time_samples, bp.value_centibel
            );
        }
        for bp in &t.mute_automation {
            println!("  mute bp: time={} muted={}", bp.time_samples, bp.muted);
        }
    }
    Ok(())
}
