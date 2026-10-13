fn main() -> Result<(), Box<dyn std::error::Error>> {
    let s = dawfile_protools::read_session(std::env::args().nth(1).unwrap().as_str(), 48000)?;
    println!("Active routings: {}", s.active_routings().count());
    let mut by_dest = std::collections::BTreeMap::new();
    for r in s.active_routings() {
        let dest_name = s
            .resolve_routing_destination(r)
            .map(|ch| ch.name.clone())
            .unwrap_or_else(|| {
                format!(
                    "?uid={}",
                    r.destination_uid
                        .iter()
                        .map(|b| format!("{:02x}", b))
                        .collect::<String>()
                )
            });
        *by_dest.entry(dest_name).or_insert(0) += 1;
    }
    for (dest, n) in by_dest.iter() {
        println!("  {} <- {} routings", dest, n);
    }
    Ok(())
}
