//! Header peek: print the embedded spec metadata fields from a pack.
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path: PathBuf = std::env::args().nth(1).expect("pack path arg").into();
    let header = signal_sampler::read_pack_header(&path)?;
    let s = &header.spec;
    println!("name        = {}", s.name);
    println!("vendor      = {}", s.vendor);
    println!("instrument  = {}", s.instrument);
    println!("category    = {}", s.category);
    println!("style       = {:?}", s.style);
    println!("tags        = {} entries", s.tags.len());
    for t in &s.tags {
        println!("  {:?} {}", t.category, t.value);
    }
    println!("entries     = {}", header.sample_count);
    println!("size_bytes  = {}", header.size_bytes);
    Ok(())
}
