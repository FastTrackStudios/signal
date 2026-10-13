//! The golden template, built at prepare from a session's own tracks:
//! the kit's pieces with their Sum, Fund, Sub and Verb banks, the toms as
//! pieces, each piece's gate keyed from its trigger, the vocals' Vox FX
//! fed by every lead — and the FTS processors on them (`mix_chains`).

#![cfg(feature = "native")]

use std::sync::Arc;

use daw::service::{Effects, FxChainContext, ProjectContext, Routing, TrackRef, Tracks};
use daw_proto::routing::SendMode;

const SONG: &str = r#"<REAPER_PROJECT 0.1 "7.75/linux-x86_64" 1790015830 0
  TEMPO 120 4 4 0
  SAMPLERATE 48000 0 0
  <TRACK {A0000000-0000-0000-0000-000000000001}
    NAME Drums
    ISBUS 1 1
  >
  <TRACK {A0000000-0000-0000-0000-000000000002}
    NAME Kick
    ISBUS 1 1
  >
  <TRACK {A0000000-0000-0000-0000-000000000003}
    NAME In
  >
  <TRACK {A0000000-0000-0000-0000-000000000004}
    NAME Trig
    MUTESOLO 1 0 0
    ISBUS 2 -1
  >
  <TRACK {A0000000-0000-0000-0000-000000000005}
    NAME Snare
    ISBUS 1 1
  >
  <TRACK {A0000000-0000-0000-0000-000000000006}
    NAME Top
  >
  <TRACK {A0000000-0000-0000-0000-000000000007}
    NAME Trig
    MUTESOLO 1 0 0
    ISBUS 2 -1
  >
  <TRACK {A0000000-0000-0000-0000-000000000008}
    NAME Toms
    ISBUS 1 1
  >
  <TRACK {A0000000-0000-0000-0000-000000000009}
    NAME T1
  >
  <TRACK {A0000000-0000-0000-0000-000000000011}
    NAME T2
  >
  <TRACK {A0000000-0000-0000-0000-00000000000A}
    NAME Trig
    MUTESOLO 1 0 0
    ISBUS 1 1
  >
  <TRACK {A0000000-0000-0000-0000-00000000000B}
    NAME "T1 Trig"
    MUTESOLO 1 0 0
  >
  <TRACK {A0000000-0000-0000-0000-000000000012}
    NAME "T2 Trig"
    MUTESOLO 1 0 0
    ISBUS 2 -2
  >
  <TRACK {A0000000-0000-0000-0000-00000000000C}
    NAME Cymbals
    ISBUS 1 1
  >
  <TRACK {A0000000-0000-0000-0000-00000000000D}
    NAME OHs
    ISBUS 2 -2
  >
  <TRACK {A0000000-0000-0000-0000-00000000000E}
    NAME Vocals
    ISBUS 1 1
  >
  <TRACK {A0000000-0000-0000-0000-00000000000F}
    NAME Main
    ISBUS 1 1
  >
  <TRACK {A0000000-0000-0000-0000-000000000010}
    NAME "Lead Vocal"
    ISBUS 2 -1
    <ITEM
      POSITION 0
      LENGTH 0.1
      <SOURCE WAVE
        FILE "PAIR_WAV"
      >
    >
  >
  <TRACK {A0000000-0000-0000-0000-000000000013}
    NAME Scream
    ISBUS 2 -1
    <ITEM
      POSITION 0
      LENGTH 0.1
      <SOURCE WAVE
        FILE "MONO_WAV"
      >
    >
  >
>
"#;

/// A tenth of a second of silence on `channels` channels, as a WAV at
/// `path`.
fn wav(path: &std::path::Path, channels: u16) {
    let frames: u32 = 4_800;
    let data = frames * u32::from(channels) * 2;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&48_000_u32.to_le_bytes());
    bytes.extend_from_slice(&(48_000 * u32::from(channels) * 2).to_le_bytes());
    bytes.extend_from_slice(&(channels * 2).to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    bytes.resize(bytes.len() + usize::try_from(data).unwrap_or(0), 0);
    std::fs::write(path, bytes).expect("a wav");
}

/// The names of `parent`'s children, in order.
fn children(tracks: &[daw_proto::Track], parent: &daw_proto::Track) -> Vec<String> {
    tracks
        .iter()
        .filter(|t| t.parent_guid.as_deref() == Some(parent.guid.as_str()))
        .map(|t| t.name.clone())
        .collect()
}

/// The track named `name` under a parent named `parent`.
fn find<'a>(tracks: &'a [daw_proto::Track], parent: &str, name: &str) -> &'a daw_proto::Track {
    tracks
        .iter()
        .find(|t| {
            t.name == name
                && t.parent_guid
                    .as_deref()
                    .and_then(|p| tracks.iter().find(|u| u.guid == p))
                    .is_some_and(|p| p.name == parent)
        })
        .unwrap_or_else(|| panic!("{parent}/{name}"))
}

/// Whether the track `guid` is fed by a send from `from`.
fn fed_by(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    guid: &str,
    from: &str,
) -> bool {
    Routing::receives(daw, project.clone(), TrackRef::Guid(guid.to_owned()))
        .iter()
        .any(|r| r.dest_track_guid.as_deref() == Some(from))
}

#[test]
fn prepare_builds_the_golden_template() {
    let daw = daw::standalone::Standalone::new();
    daw.set_fx_factory(Arc::new(builtin_fx::BuiltinFx));
    // The lead sung into a dual-capsule mic (front and rear, two
    // channels), the scream into an ordinary one.
    let dir = std::env::temp_dir().join(format!("golden-mix-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a folder");
    let (pair, mono) = (dir.join("pair.wav"), dir.join("mono.wav"));
    wav(&pair, 2);
    wav(&mono, 1);
    let song = SONG
        .replace("PAIR_WAV", &pair.to_string_lossy())
        .replace("MONO_WAV", &mono.to_string_lossy());
    let loaded =
        daw::standalone::project_loader::load_rpp_text(&daw, "song", "/tmp/song.RPP", &song)
            .expect("the song");
    let project = ProjectContext::Project(loaded.project_guid);
    let mut target = dynamic_template::apply::DawTarget::on(daw.clone(), project.clone());

    assert!(!target.add_drum_process().expect("process").is_empty());
    assert!(!target.apply_golden_kit().expect("kit").is_empty());
    assert!(!target.apply_vox_fx().expect("vox fx").is_empty());
    assert_eq!(
        target.key_drum_gates().expect("keyed"),
        ["Kick", "Snare", "Tom 1", "Tom 2"]
    );
    let chains = session_daw::mix_chains::add(&daw, &project).expect("chains");
    let tracks = Tracks::all(&daw, project.clone());

    // The kick: its tracks under a Sum, the template's Sub and Verb bank
    // beside it, the bank fed by the Sum.
    let kick = find(&tracks, "Drums", "Kick");
    assert_eq!(children(&tracks, kick), ["Sum", "Sub", "Verb"]);
    let kick_sum = find(&tracks, "Kick", "Sum");
    assert_eq!(children(&tracks, kick_sum), ["In", "Trig", "Sample"]);
    let kick_room = find(&tracks, "Verb", "Room");
    assert!(fed_by(&daw, &project, &kick_room.guid, &kick_sum.guid));
    // The snare: a Fund in its Sum, three rooms with the first on.
    let snare_sum = find(&tracks, "Snare", "Sum");
    assert_eq!(
        children(&tracks, snare_sum),
        ["Top", "Trig", "Sample", "Fund"]
    );
    let snare_verb = tracks
        .iter()
        .find(|t| {
            t.name == "Verb"
                && t.parent_guid.as_deref() == Some(find(&tracks, "Drums", "Snare").guid.as_str())
        })
        .expect("the snare's bank");
    assert_eq!(children(&tracks, snare_verb), ["Plate", "Hall", "Nonlin"]);
    let in_bank = |name: &str| {
        tracks
            .iter()
            .find(|t| t.name == name && t.parent_guid.as_deref() == Some(snare_verb.guid.as_str()))
            .expect(name)
    };
    assert!(!in_bank("Plate").muted && in_bank("Hall").muted && in_bank("Nonlin").muted);
    assert!(fed_by(
        &daw,
        &project,
        &in_bank("Hall").guid,
        &snare_sum.guid
    ));
    // The toms: each a piece of its mic, its trigger and a Fund; the
    // triggers' folder gone; the shared bank fed by every tom.
    let toms = find(&tracks, "Drums", "Toms");
    assert_eq!(children(&tracks, toms), ["Tom 1", "Tom 2", "Verb"]);
    let tom_1 = find(&tracks, "Toms", "Tom 1");
    assert_eq!(
        children(&tracks, tom_1),
        ["T1", "T1 Trig", "T1 Sample", "Fund"]
    );
    assert!(
        !tracks.iter().any(|t| t.name == "Trig" && t.is_folder),
        "the emptied trigger folder"
    );
    let toms_verb = tracks
        .iter()
        .find(|t| t.name == "Verb" && t.parent_guid.as_deref() == Some(toms.guid.as_str()))
        .expect("the toms' bank");
    let toms_room = tracks
        .iter()
        .find(|t| t.name == "Room" && t.parent_guid.as_deref() == Some(toms_verb.guid.as_str()))
        .expect("Room");
    assert!(fed_by(&daw, &project, &toms_room.guid, &tom_1.guid));
    assert!(fed_by(
        &daw,
        &project,
        &toms_room.guid,
        &find(&tracks, "Toms", "Tom 2").guid
    ));

    // Each tom keyed from its own trigger, pre-FX onto 3/4.
    let t1_trig = find(&tracks, "Tom 1", "T1 Trig");
    let key = Routing::sends(&daw, project.clone(), TrackRef::Guid(t1_trig.guid.clone()));
    let key = key.first().expect("the key send");
    assert_eq!(key.dest_track_guid.as_deref(), Some(tom_1.guid.as_str()));
    assert_eq!(
        (key.send_mode, key.dest_channels.start_channel),
        (SendMode::PreFx, 2)
    );
    assert!(!t1_trig.muted, "a muted track's sends are muted");
    let cymbals = find(&tracks, "Drums", "Cymbals");
    assert!(
        Routing::receives(&daw, project.clone(), TrackRef::Guid(cymbals.guid.clone())).is_empty()
    );

    // The vocals: the template's Vox FX, every lead into every return.
    let vox_fx = find(&tracks, "Vocals", "Vox FX");
    assert_eq!(
        children(&tracks, vox_fx),
        ["Delay", "Verb", "Wide", "Pitch"]
    );
    assert_eq!(
        children(&tracks, find(&tracks, "Vox FX", "Delay")),
        ["Slap", "Short", "Long", "Throw"]
    );
    let slap = find(&tracks, "Delay", "Slap");
    for lead in ["Main", "Scream"] {
        assert!(
            fed_by(
                &daw,
                &project,
                &slap.guid,
                &find(&tracks, "Vocals", lead).guid
            ),
            "{lead} into Slap"
        );
    }
    assert!(
        !slap.muted && find(&tracks, "Delay", "Throw").muted,
        "a start, the rest a mute away"
    );

    // The processors.
    let fx = |track: &daw_proto::Track| {
        Effects::list(
            &daw,
            project.clone(),
            FxChainContext::Track(track.guid.clone()),
        )
    };
    let first = |track: &daw_proto::Track| {
        fx(track)
            .first()
            .map(|f| (f.plugin_name.clone(), f.enabled))
    };
    // A gate first; bypassed, since this fixture's triggers have no audio
    // to read a threshold off (a dead trigger would open it on noise).
    assert_eq!(first(tom_1), Some(("FTS Gate".to_owned(), false)));
    assert!(
        chains
            .gates
            .iter()
            .all(|(_, threshold)| threshold.is_none())
    );
    assert_eq!(
        first(in_bank("Hall")).map(|f| f.0).as_deref(),
        Some("FTS Reverb")
    );
    assert_eq!(
        first(find(&tracks, "Vox FX", "Wide"))
            .map(|f| f.0)
            .as_deref(),
        Some("FTS Unison")
    );
    assert_eq!(
        first(find(&tracks, "Pitch", "Oct-"))
            .map(|f| f.0)
            .as_deref(),
        Some("FTS Pitch")
    );
    assert_eq!(
        first(find(&tracks, "Delay", "Long"))
            .map(|f| f.0)
            .as_deref(),
        Some("FTS Delay")
    );
    // The one-note chain in Dan Worrall's order — the band-pass rings,
    // the gate after it tightens the ring — bypassed here, with no hits
    // to tune it to. Fed by its mic, keyed by its trigger.
    let fund = find(&tracks, "Tom 1", "Fund");
    let names: Vec<String> = fx(fund).into_iter().map(|f| f.plugin_name).collect();
    assert_eq!(names, ["FTS EQ", "FTS Gate", "FTS Comp", "FTS Saturate"]);
    assert_eq!(first(fund), Some(("FTS EQ".to_owned(), false)));
    assert!(fed_by(
        &daw,
        &project,
        &fund.guid,
        &find(&tracks, "Tom 1", "T1").guid
    ));
    let fund_keys = Routing::receives(&daw, project.clone(), TrackRef::Guid(fund.guid.clone()));
    assert!(fund_keys.iter().any(|r| r.dest_channels.start_channel == 2
        && r.dest_track_guid.as_deref() == Some(t1_trig.guid.as_str())));
    let sub = find(&tracks, "Kick", "Sub");
    assert!(
        fed_by(&daw, &project, &sub.guid, &kick_sum.guid),
        "the Sub, fed by the kick's Sum"
    );
    // A vocal on a capsule pair: FTS Mic first, on, making it one mic,
    // then its Tone chain; a vocal on one capsule, its Tone chain alone.
    let lead = find(&tracks, "Main", "Lead Vocal");
    let names: Vec<String> = fx(lead).into_iter().map(|f| f.plugin_name).collect();
    assert_eq!(names, ["FTS Mic", "FTS EQ", "FTS Comp", "FTS Saturate"]);
    assert_eq!(first(lead), Some(("FTS Mic".to_owned(), true)));
    assert_eq!(
        first(find(&tracks, "Vocals", "Scream"))
            .map(|f| f.0)
            .as_deref(),
        Some("FTS EQ")
    );
    assert_eq!(chains.mics, ["Lead Vocal"]);
    assert!(fx(t1_trig).is_empty(), "a trigger is a key, not a channel");
    assert!(fx(vox_fx).is_empty(), "a returns folder is no bus");
    assert!(chains.unknown.is_empty(), "{:?}", chains.unknown);

    // One kit is no folder of its own: the pieces are Drums', and Process
    // is in Drums with them — in the mixer, out of the track panel — fed
    // by every piece.
    let drums = tracks.iter().find(|t| t.name == "Drums").expect("Drums");
    assert!(!tracks.iter().any(|t| t.name == "Drum Kit"));
    assert_eq!(
        children(&tracks, drums),
        ["Kick", "Snare", "Toms", "Cymbals", "Process"]
    );
    let process = find(&tracks, "Drums", "Process");
    assert!(!process.visible_in_tcp && process.visible_in_mixer);
    let dry = find(&tracks, "Compress", "Dry");
    assert!(!dry.visible_in_tcp && dry.visible_in_mixer && dry.muted);
    for piece in ["Kick", "Snare", "Toms", "Cymbals"] {
        assert!(
            fed_by(
                &daw,
                &project,
                &dry.guid,
                &find(&tracks, "Drums", piece).guid
            ),
            "{piece} into Dry"
        );
    }

    // Again: nothing more.
    assert!(target.add_drum_process().expect("process").is_empty());
    assert!(target.apply_golden_kit().expect("kit").is_empty());
    assert!(target.apply_vox_fx().expect("vox fx").is_empty());
    assert!(target.key_drum_gates().expect("keyed").is_empty());
    assert_eq!(Tracks::all(&daw, project).len(), tracks.len());
}
