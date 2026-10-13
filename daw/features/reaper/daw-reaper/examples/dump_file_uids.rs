fn main() -> Result<(), Box<dyn std::error::Error>> {
    let s = dawfile_protools::read_session(std::env::args().nth(1).unwrap().as_str(), 48000)?;
    println!("audio files: {}", s.audio_files.len());
    for f in &s.audio_files {
        let uid_s = f
            .source_uid
            .map(|u| u.iter().map(|b| format!("{:02x}", b)).collect::<String>())
            .unwrap_or_else(|| "(none)".into());
        println!("  {:>3} {:<50} uid={}", f.index, f.filename, uid_s);
    }
    Ok(())
}
