fn main() -> Result<(), Box<dyn std::error::Error>> {
    let s = dawfile_protools::read_session(std::env::args().nth(1).unwrap().as_str(), 48000)?;
    println!("audio_tracks: {}", s.audio_tracks.len());
    for t in &s.audio_tracks {
        println!(
            "  idx={} name={:?} kind={:?} regions={} alts={}",
            t.index,
            t.name,
            t.kind,
            t.regions.len(),
            t.alternate_playlists.len()
        );
    }
    println!("midi_tracks: {}", s.midi_tracks.len());
    for t in &s.midi_tracks {
        println!("  idx={} name={:?}", t.index, t.name);
    }
    Ok(())
}
