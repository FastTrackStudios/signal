fn main() -> Result<(), Box<dyn std::error::Error>> {
    let s = dawfile_protools::read_session(std::env::args().nth(1).unwrap().as_str(), 48000)?;
    println!("io_channels: {}", s.io_channels.len());
    for (i, ch) in s.io_channels.iter().enumerate() {
        let uid_s = ch
            .uid
            .map(|u| u.iter().map(|b| format!("{:02x}", b)).collect::<String>())
            .unwrap_or_else(|| "(none)".into());
        if uid_s != "(none)"
            || ch.name.contains("Analog")
            || ch.name.contains("Out")
            || ch.io_class != 0
        {
            println!(
                "  [{:>3}] class={} ch={} {:<40} uid={}",
                i, ch.io_class, ch.channel_count, ch.name, uid_s
            );
        }
    }
    Ok(())
}
