//! Self-contained, retained scratch projects for practicing on real recordings.
//! All referenced media is copied, never hard-linked or symlinked to the source.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eyre::{Context, Result, bail, eyre};
use regex::Regex;

pub const DEFAULT_ALBUM: &str =
    "/run/media/AudioHaven/Project/Crescendum-Rockstars-SESSION-BACKUP-2026-09-06/Crescendum";

#[derive(Clone, Copy, Debug)]
pub enum Song {
    SetInStone,
    Unbreakable,
}

impl Song {
    pub const BOTH: [Self; 2] = [Self::SetInStone, Self::Unbreakable];

    pub fn name(self) -> &'static str {
        match self {
            Self::SetInStone => "set in stone",
            Self::Unbreakable => "unbreakable",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "set-in-stone" | "set in stone" => Ok(Self::SetInStone),
            "unbreakable" => Ok(Self::Unbreakable),
            _ => bail!("Unknown song {value:?}; choose set-in-stone or unbreakable"),
        }
    }
}

pub fn album_directory() -> PathBuf {
    std::env::var_os("EXPRESSION_EDITOR_PRACTICE_ALBUM")
        .map(PathBuf::from)
        .unwrap_or_else(|| DEFAULT_ALBUM.into())
}

pub struct PracticeProject {
    pub source: PathBuf,
    pub copy: PathBuf,
}

pub struct MediaCopy {
    pub source: PathBuf,
    pub copy: PathBuf,
}

/// Retained after exit, so both manual and test edits can be inspected later.
pub struct PracticeSession {
    pub directory: PathBuf,
    pub projects: Vec<PracticeProject>,
    pub media: Vec<MediaCopy>,
}

/// Where a cached staging keeps its songs, one directory per song.
///
/// Set `EXPRESSION_EDITOR_PRACTICE_CACHE` to move it; the default sits
/// under the system temporary directory, so it survives between runs
/// without being anyone's idea of permanent storage.
pub fn cache_directory() -> PathBuf {
    std::env::var_os("EXPRESSION_EDITOR_PRACTICE_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("fts-drum-practice-cache"))
}

impl PracticeSession {
    /// Each call creates a new directory below the system temporary directory
    /// (`TMPDIR` on Unix). A failed preparation removes only its partial copy.
    pub fn prepare(album: &Path, songs: &[Song]) -> Result<Self> {
        if songs.is_empty() {
            bail!("Choose at least one practice song");
        }
        let scratch = tempfile::Builder::new()
            .prefix("fts-drum-practice-")
            .tempdir()?;
        let root = scratch.path().canonicalize()?;
        std::fs::create_dir(root.join("Media"))?;
        std::fs::create_dir(root.join("Output"))?;
        let session = Self::stage(album, songs, root)?;
        // Retained only once staging succeeded; a failure takes its
        // partial copy with it.
        let _ = scratch.keep();
        Ok(session)
    }

    /// One staging per song, kept and reused.
    ///
    /// The copy exists so experiments never write to the originals, and
    /// that is just as true of a copy made once as of a copy made every
    /// run — but a *fresh* copy also throws away everything the last run
    /// learned about the audio. The `.reapeaks` sidecars are the ones
    /// that hurt: peaks are written next to the media, so a new
    /// directory means rescanning every take's PCM before the window can
    /// say it is ready. Reusing the staging is what makes a benchmark
    /// loop a minute instead of several.
    ///
    /// A staging is complete when its manifest is on disk — that is the
    /// last thing [`Self::stage`] writes — and its project copy still
    /// exists. Anything less is restaged from scratch.
    pub fn prepare_cached(album: &Path, songs: &[Song], cache: &Path) -> Result<Self> {
        if songs.is_empty() {
            bail!("Choose at least one practice song");
        }
        let mut projects = Vec::new();
        let mut media = Vec::new();
        let mut directory = cache.to_path_buf();
        for song in songs {
            let root = cache.join(song.name().replace(' ', "-"));
            let copy = root.join(format!("{}.practice.RPP", song.name()));
            if root.join("MANIFEST.txt").is_file() && copy.is_file() {
                let source = album
                    .join(song.name())
                    .join(format!("{}.organized.RPP", song.name()));
                projects.push(PracticeProject { source, copy });
                directory = root;
                continue;
            }
            // Partial or absent: start it over rather than trust it.
            if root.exists() {
                std::fs::remove_dir_all(&root)
                    .wrap_err_with(|| format!("Clearing {}", root.display()))?;
            }
            std::fs::create_dir_all(root.join("Media"))?;
            std::fs::create_dir_all(root.join("Output"))?;
            let root = root.canonicalize()?;
            let staged = Self::stage(album, std::slice::from_ref(song), root)?;
            directory = staged.directory;
            projects.extend(staged.projects);
            media.extend(staged.media);
        }
        Ok(Self {
            directory,
            projects,
            media,
        })
    }

    /// Copy the songs' media into `root` and rewrite their projects to
    /// point at it. `root` must exist and hold empty `Media`/`Output`.
    fn stage(album: &Path, songs: &[Song], root: PathBuf) -> Result<Self> {
        let files =
            Regex::new(r#"(?m)^([ \t]*FILE[ \t]+)(?:"([^"\r\n]*)"|([^ \t\r\n"]+))([^\r\n]*)"#)?;
        let outputs =
            Regex::new(r"(?m)^([ \t]*)(RECORD_PATH|RENDER_FILE|PEAKSFILE)[ \t]+[^\r\n]*")?;
        let mut copied = HashMap::<PathBuf, PathBuf>::new();
        let mut projects = Vec::new();
        let mut media = Vec::new();
        let mut manifest =
            String::from("Fresh practice copies. Originals are never editing targets.\n\n");

        for song in songs {
            let source = album
                .join(song.name())
                .join(format!("{}.organized.RPP", song.name()));
            let text = std::fs::read_to_string(&source).wrap_err_with(|| {
                format!(
                    "Reading {} (set EXPRESSION_EDITOR_PRACTICE_ALBUM to override the album)",
                    source.display()
                )
            })?;
            let directory = source
                .parent()
                .ok_or_else(|| eyre!("Project has no directory"))?;
            let mut rewritten = String::with_capacity(text.len());
            let mut cursor = 0;
            let mut references = 0;
            for captures in files.captures_iter(&text) {
                let span = captures.get(0).expect("regex match");
                let reference = captures
                    .get(2)
                    .or_else(|| captures.get(3))
                    .expect("file path")
                    .as_str();
                let resolved = resolve_media(album, directory, reference)?;
                if resolved
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("rpp"))
                {
                    bail!(
                        "Nested project source needs recursive staging: {}",
                        resolved.display()
                    );
                }
                let target = match copied.get(&resolved) {
                    Some(path) => path.clone(),
                    None => {
                        let filename = resolved
                            .file_name()
                            .ok_or_else(|| eyre!("Media has no file name"))?;
                        let target = root.join("Media").join(format!(
                            "{:04}-{}",
                            media.len(),
                            filename.to_string_lossy()
                        ));
                        std::fs::copy(&resolved, &target)
                            .wrap_err_with(|| format!("Copying {}", resolved.display()))?;
                        manifest.push_str(&format!(
                            "MEDIA\t{}\t{}\n",
                            resolved.display(),
                            target.display()
                        ));
                        copied.insert(resolved.clone(), target.clone());
                        media.push(MediaCopy {
                            source: resolved,
                            copy: target.clone(),
                        });
                        target
                    }
                };
                let relative = target.strip_prefix(&root)?;
                rewritten.push_str(&text[cursor..span.start()]);
                rewritten.push_str(&captures[1]);
                rewritten.push('"');
                rewritten.push_str(&relative.to_string_lossy());
                rewritten.push('"');
                rewritten.push_str(&captures[4]);
                cursor = span.end();
                references += 1;
            }
            let directives = text
                .lines()
                .filter(|line| {
                    line.trim_start()
                        .strip_prefix("FILE")
                        .is_some_and(|rest| rest.starts_with([' ', '\t']))
                })
                .count();
            if references == 0 || references != directives {
                bail!(
                    "{} contains missing or unsupported FILE syntax",
                    source.display()
                );
            }
            rewritten.push_str(&text[cursor..]);
            let rewritten = outputs.replace_all(&rewritten, |caps: &regex::Captures<'_>| {
                let value = match &caps[2] {
                    "RECORD_PATH" => "\"Output\" \"\"",
                    "RENDER_FILE" => "\"Output/practice\"",
                    _ => "\"\"",
                };
                format!("{}{} {value}", &caps[1], &caps[2])
            });
            let copy = root.join(format!("{}.practice.RPP", song.name()));
            std::fs::write(&copy, rewritten.as_bytes())?;
            manifest.push_str(&format!(
                "PROJECT\t{}\t{}\n",
                source.display(),
                copy.display()
            ));
            projects.push(PracticeProject { source, copy });
        }
        std::fs::write(root.join("MANIFEST.txt"), manifest)?;
        let directory = root;
        Ok(Self {
            directory,
            projects,
            media,
        })
    }
}

fn resolve_media(album: &Path, project: &Path, reference: &str) -> Result<PathBuf> {
    let normalized = reference.replace('\\', "/");
    let path = Path::new(&normalized);
    let mut candidates = vec![project.join(path)];
    // The album was moved from macOS. Keep the song component so shared
    // recordings resolve to their own song, not a same-named local file.
    if let Some((_, relative)) = normalized.split_once("/Crescendum/") {
        candidates.push(album.join(relative));
    }
    if let Some(name) = path.file_name() {
        candidates.push(project.join("Media").join(name));
    }
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            eyre!(
                "Missing media {reference:?} referenced by {}",
                project.display()
            )
        })?
        .canonicalize()
        .map_err(Into::into)
}

#[cfg(test)]
mod tests;
