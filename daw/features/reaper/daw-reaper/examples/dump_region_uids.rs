fn main() -> Result<(), Box<dyn std::error::Error>> {
    let s = dawfile_protools::read_session(std::env::args().nth(1).unwrap().as_str(), 48000)?;
    let mut groups: std::collections::BTreeMap<Option<[u8; 6]>, Vec<&str>> = Default::default();
    for r in &s.audio_regions {
        groups
            .entry(r.source_file_uid)
            .or_default()
            .push(r.name.as_str());
    }
    println!(
        "regions: {}, distinct UIDs: {}",
        s.audio_regions.len(),
        groups.len()
    );
    for (uid, names) in groups.iter().take(10) {
        let uid_s = uid
            .map(|u| u.iter().map(|b| format!("{:02x}", b)).collect::<String>())
            .unwrap_or_else(|| "(none)".into());
        println!(
            "  {} -> {} regions: {:?}",
            uid_s,
            names.len(),
            &names[..names.len().min(3)]
        );
    }
    Ok(())
}
