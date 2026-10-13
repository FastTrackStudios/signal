//! `session proxies` — Ogg Vorbis proxies of a session's media, written
//! beside it: `Media/Bass.wav` → `Media/Proxies/Bass.ogg`.
//!
//! A proxy is an ordinary file in the session folder, so it is versioned
//! and synced with the session like everything else in it, and a player
//! that cannot move the originals (the browser demo, a phone) streams the
//! proxy in their place — Task serves it as the take's audio rendition
//! (`files.access.link-proxies`).

use std::path::{Path, PathBuf};

/// Where a media file's proxy lives — the same rule Task's share links
/// apply (`task_server::share::proxy_path`).
pub(crate) fn proxy_of(media: &Path) -> Option<PathBuf> {
    let dir = media.parent()?;
    if dir.file_name().is_some_and(|n| n == "Proxies") {
        return None;
    }
    Some(
        dir.join("Proxies")
            .join(format!("{}.ogg", media.file_stem()?.to_string_lossy())),
    )
}

/// The session's `.RPP`: the path itself, or the one `.RPP` in a folder.
pub(crate) fn project_file(session: &Path) -> eyre::Result<PathBuf> {
    if session.is_file() {
        return Ok(session.to_path_buf());
    }
    let found: Vec<PathBuf> = std::fs::read_dir(session)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("rpp")))
        .collect();
    match found.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(eyre::eyre!("no .RPP in {}", session.display())),
        many => Err(eyre::eyre!(
            "{} holds {} projects — name one",
            session.display(),
            many.len()
        )),
    }
}

/// Every media file the project's sources name (`FILE "…"`), resolved
/// against the project's folder, each once.
pub(crate) fn sources(project: &Path) -> eyre::Result<Vec<PathBuf>> {
    let text = std::fs::read_to_string(project)?;
    let base = project.parent().unwrap_or(Path::new("."));
    let mut out: Vec<PathBuf> = text
        .lines()
        .filter_map(|line| {
            let rest = line.trim_start().strip_prefix("FILE ")?;
            let name = rest.trim().trim_matches('"');
            (!name.is_empty()).then(|| base.join(name))
        })
        .collect();
    out.sort();
    out.dedup();
    Ok(out)
}

/// Whether `proxy` was written after `media` last changed.
fn up_to_date(media: &Path, proxy: &Path) -> bool {
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    matches!((modified(media), modified(proxy)), (Some(m), Some(p)) if p >= m)
}

/// Frames between a proxy's index points: about a second.
const INDEX_STEP: u64 = 48_000;

pub fn write(session: &Path, quality: f32, force: bool) -> eyre::Result<()> {
    let project = project_file(session)?;
    let mut todo = Vec::new();
    for media in sources(&project)? {
        let is_wav = media
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("wav") || x.eq_ignore_ascii_case("wave"));
        let Some(proxy) = proxy_of(&media) else {
            continue;
        };
        if !media.exists() {
            println!("{}  missing — skipped", media.display());
        } else if !is_wav {
            println!("{}  not a WAV — skipped", media.display());
        } else if !force && up_to_date(&media, &proxy) {
            // A proxy made before its page index existed gets one: what a
            // client streaming it from elsewhere fetches by.
            let index = fts_sample::ogg_index::OggIndex::path_for(&proxy);
            if index.exists() {
                println!("{}  up to date", proxy.display());
            } else {
                fts_sample::cache::write_ogg_index(&proxy, INDEX_STEP)
                    .map_err(|e| eyre::eyre!("{e}"))?;
                println!("{}  up to date; indexed", proxy.display());
            }
        } else {
            todo.push((media, proxy));
        }
    }
    let workers = std::thread::available_parallelism()
        .map_or(4, usize::from)
        .min(8);
    let queue = std::sync::Mutex::new(todo.into_iter());
    let failures = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let next = queue.lock().expect("queue").next();
                    let Some((media, proxy)) = next else { break };
                    let result = proxy
                        .parent()
                        .map_or(Ok(()), std::fs::create_dir_all)
                        .map_err(|e| eyre::eyre!("{e}"))
                        .and_then(|()| {
                            // Written aside and moved into place, so a sync
                            // agent never picks up half a proxy.
                            let partial = proxy.with_extension("ogg.partial");
                            let frames =
                                fts_sample::cache::write_ogg_proxy(&media, &partial, quality)
                                    .map_err(|e| eyre::eyre!("{e}"))?;
                            std::fs::rename(&partial, &proxy)?;
                            // Its page index was written beside the partial
                            // name: it moves with the proxy.
                            let index = fts_sample::ogg_index::OggIndex::path_for;
                            std::fs::rename(index(&partial), index(&proxy))?;
                            Ok(frames)
                        });
                    match result {
                        Ok(frames) => {
                            let size = std::fs::metadata(&proxy).map_or(0, |m| m.len());
                            println!("{}  {frames} frames, {} KB", proxy.display(), size / 1024);
                        }
                        Err(e) => failures
                            .lock()
                            .expect("failures")
                            .push(format!("{}: {e}", media.display())),
                    }
                }
            });
        }
    });
    let failures = failures.into_inner().expect("failures");
    if failures.is_empty() {
        Ok(())
    } else {
        Err(eyre::eyre!(
            "{} proxies failed:\n{}",
            failures.len(),
            failures.join("\n")
        ))
    }
}

/// Every `.wav` under `dir`, relative to it.
fn wavs(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) -> eyre::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            wavs(&path, base, out)?;
        } else if path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("wav"))
        {
            out.push(path.strip_prefix(base)?.to_path_buf());
        }
    }
    Ok(())
}

/// An Ogg copy of the guide library at `out`: the same layout, each
/// `.wav` a `.ogg`.
pub fn guide_library(library: &Path, out: &Path, quality: f32) -> eyre::Result<()> {
    let mut files = Vec::new();
    wavs(library, library, &mut files)?;
    files.sort();
    let (mut wav_bytes, mut ogg_bytes) = (0u64, 0u64);
    for rel in &files {
        let dst = out.join(rel).with_extension("ogg");
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        fts_sample::cache::write_ogg_proxy(&library.join(rel), &dst, quality)
            .map_err(|e| eyre::eyre!("{}: {e}", rel.display()))?;
        wav_bytes += std::fs::metadata(library.join(rel))?.len();
        ogg_bytes += std::fs::metadata(&dst)?.len();
    }
    println!(
        "{} samples: {} KB of WAV as {} KB of Ogg, in {}",
        files.len(),
        wav_bytes / 1024,
        ogg_bytes / 1024,
        out.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_proxy_sits_in_a_folder_beside_its_media() {
        assert_eq!(
            proxy_of(Path::new("/s/Media/Bass.wav")),
            Some(PathBuf::from("/s/Media/Proxies/Bass.ogg"))
        );
        assert_eq!(proxy_of(Path::new("/s/Media/Proxies/Bass.ogg")), None);
    }

    #[test]
    fn the_sources_are_the_projects_file_lines() {
        let dir = std::env::temp_dir().join(format!("proxies-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let rpp = dir.join("Song.RPP");
        std::fs::write(
            &rpp,
            "<REAPER_PROJECT\n  <SOURCE WAVE\n    FILE \"Media/Bass.wav\"\n  >\n  \
             <SOURCE WAVE\n    FILE \"Media/Bass.wav\"\n  >\n  <SOURCE MIDI\n    FILE \"\"\n  >\n>\n",
        )
        .expect("rpp");
        assert_eq!(
            sources(&rpp).expect("sources"),
            vec![dir.join("Media/Bass.wav")]
        );
        assert_eq!(project_file(&dir).expect("project"), rpp);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
