//! Dump zone key/velocity/mic/articulation coverage from a pack, to see why
//! note_on doesn't match a zone.
use std::collections::BTreeMap;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path: PathBuf = std::env::args().nth(1).expect("pack path arg").into();
    let h = signal_sampler::read_pack_header(&path)?;
    let s = &h.spec;
    println!(
        "name={} category={:?} instrument={:?}",
        s.name, s.category, s.instrument
    );
    println!(
        "mics: {:?}",
        s.mics
            .iter()
            .map(|m| (&m.id, &m.kind, m.default))
            .collect::<Vec<_>>()
    );
    println!("zones: {}", s.zones.len());
    // key range histogram
    let mut keys: BTreeMap<(u8, u8), usize> = BTreeMap::new();
    let mut vels: BTreeMap<(u8, u8), usize> = BTreeMap::new();
    let mut artics: BTreeMap<String, usize> = BTreeMap::new();
    let mut mics: BTreeMap<String, usize> = BTreeMap::new();
    let mut roots: BTreeMap<u8, usize> = BTreeMap::new();
    for z in &s.zones {
        *keys.entry((z.key_min, z.key_max)).or_default() += 1;
        *vels.entry((z.vel_min, z.vel_max)).or_default() += 1;
        *artics.entry(z.articulation.clone()).or_default() += 1;
        *mics.entry(z.mic.clone()).or_default() += 1;
        *roots.entry(z.root_key).or_default() += 1;
    }
    println!("\nkey ranges (min,max)->count:");
    for (k, c) in &keys {
        println!("  {:?} x{c}", k);
    }
    println!("\nvel ranges (min,max)->count:");
    for (k, c) in &vels {
        println!("  {:?} x{c}", k);
    }
    println!("\narticulations:");
    for (a, c) in &artics {
        println!("  {:?} x{c}", a);
    }
    println!("\nmic tags:");
    for (m, c) in &mics {
        println!("  {:?} x{c}", m);
    }
    println!("\nroot keys:");
    for (r, c) in &roots {
        println!("  {r} x{c}");
    }
    Ok(())
}
