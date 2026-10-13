fn main() -> Result<(), Box<dyn std::error::Error>> {
    let s = dawfile_protools::read_session(std::env::args().nth(1).unwrap().as_str(), 48000)?;
    for (i, r) in s.audio_regions.iter().enumerate().take(15) {
        let uid_s = r
            .source_file_uid
            .map(|u| u.iter().map(|b| format!("{:02x}", b)).collect::<String>())
            .unwrap_or_else(|| "(none)".into());
        println!("{:>3}: {:<45} uid={}", i, r.name, uid_s);
    }
    Ok(())
}
