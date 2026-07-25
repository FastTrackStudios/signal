use signal_sampler::read_pack_header;

#[test]
fn dump_rhodes() {
    let h = read_pack_header(std::path::Path::new(
        "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/Rhodes - Classic.signalpack",
    ))
    .unwrap();
    let s = &h.spec;
    eprintln!("name: {}", s.name);
    eprintln!("articulations ({}):", s.articulations.len());
    for a in &s.articulations {
        eprintln!(
            "  {:?} kind={:?} dynamics={:?} rr={} release_artic={:?}",
            a.id, a.kind, a.dynamics, a.rr, a.release_artic
        );
    }
    eprintln!("mics ({}):", s.mics.len());
    for m in &s.mics {
        eprintln!("  {:?} default={}", m.id, m.default);
    }
    eprintln!("zones: {}", s.zones.len());

    // Dump the sample map to see which (note, artic, dyn) keys actually exist.
    let patch = signal_sampler::PlayerPatch::from_pack(std::path::Path::new(
        "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/Rhodes - Classic.signalpack",
    ))
    .unwrap();
    use std::collections::BTreeMap;
    let mut by_artic: BTreeMap<String, BTreeMap<String, std::collections::BTreeSet<u8>>> =
        BTreeMap::new();
    let mut total = 0usize;
    for (k, _) in patch.map.iter() {
        by_artic
            .entry(k.articulation.clone())
            .or_default()
            .entry(k.dynamic.clone())
            .or_default()
            .insert(k.note);
        total += 1;
    }
    eprintln!("map size: {total}");
    eprintln!("sections ({}):", s.sections.len());
    for sec in &s.sections {
        eprintln!(
            "  {} lo={} hi={} grid_len={}",
            sec.id,
            sec.lowest_note,
            sec.highest_note,
            sec.note_grid.len()
        );
        eprintln!("    grid={:?}", sec.note_grid);
    }
    // First 5 sample keys verbatim, to see exact field values.
    eprintln!("first 5 keys:");
    for (k, p) in patch.map.iter().take(5) {
        eprintln!("  key={k:?} → {}", p.display());
    }
    // Try resolve for note 36 vel 30 with the same args trigger_short uses.
    let r = patch.resolve(&signal_sampler::SampleQuery {
        section_id: "main",
        articulation_id: "lacrm",
        mic_id: "Main",
        dynamic: "35",
        target_note: 36,
        direction: "",
        rr: 0,
    });
    eprintln!("resolve(main, lacrm, Main, 35, 36, 0) = {r:?}");
    let r2 = patch.resolve(&signal_sampler::SampleQuery {
        section_id: "main",
        articulation_id: "lacrm",
        mic_id: "Main",
        dynamic: "35",
        target_note: 60,
        direction: "",
        rr: 0,
    });
    eprintln!("resolve(main, lacrm, Main, 35, 60, 0) = {r2:?}");
    // Compare a few preload-iterator paths vs resolved paths.
    let preload_paths: Vec<_> = patch
        .sample_paths_centered(60)
        .into_iter()
        .take(5)
        .collect();
    eprintln!("preload sample paths first 5:");
    for p in &preload_paths {
        eprintln!("  {}", p.display());
    }
    // Inspect pack-side entries for "RR01 lacrm 36 35.flac".
    let pack = signal_sampler::engine::cache::SignalPcmPack::open(std::path::Path::new(
        "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs/Rhodes - Classic.signalpack",
    ))
    .expect("open pack");
    eprintln!("pack entries: {}", pack.entry_count());
    let r10_entries: Vec<_> = pack
        .entries_iter()
        .filter(|(p, _)| p.to_string_lossy().contains("CLR r10"))
        .map(|(p, _)| p.display().to_string())
        .collect();
    eprintln!("pack entries with 'CLR r10' ({})", r10_entries.len());
    for e in r10_entries.iter().take(25) {
        eprintln!("  {}", e);
    }
    // Decode a couple of lacrm samples and report their durations + peaks.
    let cache = signal_sampler::engine::cache::SampleCache::with_pack(pack.clone());
    for path_str in [
        "RR01 lacrm 60 102.flac",
        "RR01 lacrm 36 35.flac",
        "RR01 lacrmsp 24 102.flac",
    ] {
        let p = std::path::PathBuf::from(path_str);
        match cache.get(&p) {
            Ok(data) => {
                let mut peak = 0.0f32;
                for s in data.to_f32().iter() {
                    peak = peak.max(s.abs());
                }
                let secs = data.num_frames as f32 / data.sample_rate as f32;
                eprintln!(
                    "  {}: dur={:.3}s peak={:.3} channels={} rate={}",
                    path_str, secs, peak, data.channels, data.sample_rate
                );
            }
            Err(e) => eprintln!("  {}: ERROR {}", path_str, e),
        }
    }
    let matches: Vec<_> = pack
        .entries_iter()
        .filter(|(p, _)| p.to_string_lossy().contains("lacrm 36 35"))
        .map(|(p, _)| p.display().to_string())
        .collect();
    eprintln!("pack entries matching 'lacrm 36 35': {}", matches.len());
    for m in matches.iter().take(8) {
        eprintln!("  pack entry: {}", m);
    }
    // Verify resolve at note 36 for the RR cycle.
    for rr in 0..4 {
        let r = patch.resolve(&signal_sampler::SampleQuery {
            section_id: "main",
            articulation_id: "lacrm",
            mic_id: "Main",
            dynamic: "35",
            target_note: 36,
            direction: "",
            rr,
        });
        eprintln!("resolve(36, dyn=35, rr={rr}) = {r:?}");
    }
    for (artic, dyns) in &by_artic {
        eprintln!("  {artic}:");
        for (dyn_id, notes) in dyns {
            let ns: Vec<String> = notes.iter().map(|n| n.to_string()).collect();
            eprintln!("    dyn={dyn_id} notes=[{}]", ns.join(","));
        }
    }
}
