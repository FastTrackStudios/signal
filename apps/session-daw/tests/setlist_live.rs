//! A setlist: several songs in one engine, each from its own folder, each
//! prepared and saved, one current — and picking another makes it current.

use std::path::{Path, PathBuf};

use session_daw::setlist::Setlist;

/// A song folder: `Song/Song.RPP` with one audio item on `Media/Click.wav`
/// — the same relative name in every song, at a loudness of its own — and
/// its chart beside it.
fn song(root: &Path, name: &str, level: f32, chart: &str) -> PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(dir.join("Media")).expect("song folder");
    write_wav(&dir.join("Media/Click.wav"), level);
    std::fs::write(dir.join(format!("{name}.kf")), chart).expect("chart");
    let rpp = dir.join(format!("{name}.RPP"));
    std::fs::write(
        &rpp,
        format!(
            "<REAPER_PROJECT 0.1 \"7.0/test\" 0\n  TEMPO 120 4 4\n  <TRACK {{00000000-0000-0000-0000-00000000000{n}}}\n    NAME Bass\n    <ITEM\n      POSITION 0\n      LENGTH 4\n      IGUID {{00000000-0000-0000-0000-0000000000A{n}}}\n      <SOURCE WAVE\n        FILE \"Media/Click.wav\"\n      >\n    >\n  >\n>\n",
            n = name.len() % 10
        ),
    )
    .expect("rpp");
    rpp
}

/// Four seconds of a square wave at `level`, 48 kHz mono 16-bit.
fn write_wav(path: &Path, level: f32) {
    let rate: u32 = 48_000;
    let frames = rate * 4;
    let mut data = Vec::with_capacity(frames as usize * 2);
    for i in 0..frames {
        let v = if (i / 100) % 2 == 0 { level } else { -level };
        #[expect(clippy::cast_possible_truncation, reason = "a 16-bit sample")]
        data.extend_from_slice(&((v * 32767.0) as i16).to_le_bytes());
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    std::fs::write(path, out).expect("wav");
}

#[test]
fn a_setlist_opens_every_song_into_one_engine_and_switches_between_them() {
    let root = tempfile::tempdir().expect("tempdir");
    let loud = song(
        root.path(),
        "Loud",
        0.8,
        "Loud - Nobody\n90bpm 4/4 #F\nCount 1\nvs 2\n1 4\n",
    );
    let quiet = song(
        root.path(),
        "Quiet",
        0.2,
        "Quiet - Nobody\n120bpm 4/4 #G\nCount 1\nvs 2\n1 5\n",
    );

    let mut setlist = Setlist::open_silent(&[loud.clone(), quiet.clone()]).expect("the set opens");
    assert_eq!(setlist.songs.len(), 2);
    assert_eq!(setlist.at, 0);
    assert_eq!(
        session_daw::open::current_song().as_deref(),
        Some(setlist.songs[0].project.as_str())
    );

    // Each prepared from its own chart, and saved beside itself.
    let bpm = |i: usize| setlist.songs[i].session.project.0.bpm;
    assert!((bpm(0) - 90.0).abs() < 1e-6, "Loud is at 90: {}", bpm(0));
    assert!((bpm(1) - 120.0).abs() < 1e-6, "Quiet is at 120: {}", bpm(1));
    assert!(loud.with_extension("session").is_dir() && quiet.with_extension("session").is_dir());
    assert_eq!(
        setlist.songs[1].saved.as_deref(),
        Some(quiet.with_extension("session").as_path())
    );

    // Each song's Media/Click.wav is its own: the waveforms differ as the
    // files do.
    let loudest = |i: usize| {
        let session = &setlist.songs[i].session;
        session
            .project
            .0
            .items
            .values()
            .flatten()
            .filter_map(|item| session.previews.wave(&item.guid))
            .flat_map(|wave| wave.points.iter().map(|(max, _)| *max).collect::<Vec<_>>())
            .fold(0.0_f32, f32::max)
    };
    assert!(
        (loudest(0) - 0.8).abs() < 0.02,
        "Loud's own file: {}",
        loudest(0)
    );
    assert!(
        (loudest(1) - 0.2).abs() < 0.02,
        "Quiet's own file: {}",
        loudest(1)
    );

    // Picking the second makes it current.
    let project = setlist
        .pick(1, 2.0)
        .map(|s| s.project.clone())
        .expect("a pick");
    session_daw::open::switch_song(&project);
    assert_eq!(session_daw::open::current_song(), Some(project));
    assert!(
        (setlist.songs[0].left_at - 2.0).abs() < 1e-9,
        "where Loud was left is kept"
    );
}
