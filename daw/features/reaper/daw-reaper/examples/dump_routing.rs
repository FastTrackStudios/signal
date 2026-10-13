fn main() -> Result<(), Box<dyn std::error::Error>> {
    let s = dawfile_protools::read_session(std::env::args().nth(1).unwrap().as_str(), 48000)?;
    let total = s.routing_entries.len();
    let active: Vec<_> = s.routing_entries.iter().filter(|e| e.active).collect();
    println!("routing entries: {} total, {} active", total, active.len());
    for (i, r) in active.iter().enumerate().take(10) {
        let uid_s: String = r
            .destination_uid
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        println!(
            "  active[{}] @ 0x{:x}: flag_33={} flag_36={} dest_uid={}",
            i, r.block_start, r.flag_33, r.flag_36, uid_s
        );
    }
    Ok(())
}
