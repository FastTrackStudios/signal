//! A song folder served by REAPER: a window driving REAPER (Remote, Cue)
//! mirrors the song's small files and streams its proxies from here, as
//! from a standalone engine — the same `daw_proto::song_files::folder`
//! behind both.
//!
//! Run with: `cargo run -p daw-reaper-xtask -- song_files`

use daw::test::reaper_test;

/// A song folder on disk: its project, a proxy, and the proxy's index.
fn song_folder(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("fts-song-files-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Media/Proxies")).unwrap();
    std::fs::write(
        dir.join("Song.RPP"),
        "<REAPER_PROJECT 0.1 \"7.0\" 1700000000\n  SAMPLERATE 48000 0 0\n  TEMPO 120 4 4\n>\n",
    )
    .unwrap();
    let proxy: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(dir.join("Media/Proxies/Bass.ogg"), &proxy).unwrap();
    std::fs::write(
        dir.join("Media/Proxies/Bass.ogg.idx"),
        "ogg-index 2 300000 0 0 2 48000\n",
    )
    .unwrap();
    dir
}

#[reaper_test(isolated)]
async fn song_files_lists_and_reads_the_folder_reaper_opened(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let dir = song_folder("list");
    let project = ctx
        .daw
        .open_project(dir.join("Song.RPP").to_string_lossy().into_owned())
        .await?;
    let result = async {
        let files = project.song_files().list().await?;
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        eyre::ensure!(
            paths.first() == Some(&"Song.RPP"),
            "the project first: {paths:?}"
        );
        eyre::ensure!(paths.contains(&"Media/Proxies/Bass.ogg"), "{paths:?}");
        let proxy = files
            .iter()
            .find(|f| f.path == "Media/Proxies/Bass.ogg")
            .unwrap();
        eyre::ensure!(proxy.size == 300_000, "its size: {}", proxy.size);

        // A range from the middle, as the fetcher asks.
        let bytes = project
            .song_files()
            .read("Media/Proxies/Bass.ogg", 100_000..100_010)
            .await?;
        let want: Vec<u8> = (100_000..100_010u32).map(|i| (i % 251) as u8).collect();
        eyre::ensure!(bytes == want, "{bytes:?} vs {want:?}");

        // Never out of the folder.
        eyre::ensure!(
            project.song_files().read("../secret", 0..10).await.is_err(),
            "a path out of the song is refused"
        );
        Ok(())
    }
    .await;
    let _ = ctx.daw.close_project(project.guid()).await;
    let _ = std::fs::remove_dir_all(&dir);
    result
}
