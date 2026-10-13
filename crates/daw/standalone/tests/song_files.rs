//! A song folder served to a peer: listed (the project first, the DAW's
//! backups not at all), read by range, never outside the song.

#![cfg(feature = "bootstrap")]
#![allow(clippy::unwrap_used)]

use daw_proto::ProjectInfo;
use daw_standalone::bootstrap::build_in_process_daw;
use daw_standalone::sync::Standalone;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_peer_lists_and_reads_the_song_folder() -> eyre::Result<()> {
    let dir = tempfile::tempdir()?;
    let song = dir.path().join("Washed");
    std::fs::create_dir_all(song.join("Media/Proxies"))?;
    std::fs::create_dir_all(song.join("Backups"))?;
    std::fs::write(song.join("Washed.RPP"), "<REAPER_PROJECT\n>\n")?;
    std::fs::write(song.join("Washed.kf"), "Washed\n#B\n")?;
    let proxy: Vec<u8> = (0..=255u8).cycle().take(5000).collect();
    std::fs::write(song.join("Media/Proxies/Bass.ogg"), &proxy)?;
    std::fs::write(song.join("Backups/Washed-old.RPP"), "old")?;
    std::fs::write(dir.path().join("secret.txt"), "not the song's")?;

    let standalone = Standalone::new();
    standalone.seed_project(ProjectInfo {
        guid: "washed".into(),
        name: "Washed".into(),
        path: song.join("Washed.RPP").to_string_lossy().into_owned(),
    });
    let bundle = build_in_process_daw(standalone).await?;
    let files = bundle.daw.project("washed").await?.song_files();

    let list = files.list().await?;
    assert_eq!(list[0].path, "Washed.RPP", "the project first");
    let names: Vec<&str> = list.iter().map(|f| f.path.as_str()).collect();
    assert!(names.contains(&"Media/Proxies/Bass.ogg") && names.contains(&"Washed.kf"));
    assert!(!names.iter().any(|n| n.starts_with("Backups")), "{names:?}");
    assert_eq!(
        list.iter()
            .find(|f| f.path == "Media/Proxies/Bass.ogg")
            .unwrap()
            .size,
        5000
    );

    let got = files.read("Media/Proxies/Bass.ogg", 1000..1300).await?;
    assert_eq!(got, proxy[1000..1300]);
    assert!(
        files.read("../secret.txt", 0..10).await.is_err(),
        "nothing outside the song"
    );
    Ok(())
}
