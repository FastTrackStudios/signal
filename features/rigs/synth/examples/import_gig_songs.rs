//! `import_gig_songs` — turn a Gig Performer gig's song list into a song
//! library: one portable song folder per song, under one root.
//!
//! ```text
//! import_gig_songs <file.gig> <library_root> [--dry-run] [--force]
//! ```
//!
//! A "song library" is just a directory of song folders — the `song` crate's
//! folder format is already self-contained and portable, so the library needs
//! no index of its own and nothing is lost by moving a folder between roots.
//!
//! What carries across, per song: title, key, tempo, time signature, and the
//! parts in running order with the patch each one calls for. What does not:
//! chords and lyrics (`songCordsLyrics` is empty throughout the reference
//! gig), and the rackspace each part names — that is Gig Performer rig state,
//! and the part keeps the *patch name* instead, which is the portable half.
//!
//! ## The key is read from the title, not from `rootNote`
//!
//! Gig Performer stores a `rootNote` per song, and in the reference gig it
//! disagrees with the title's key suffix on 12 of 29 songs (`Center - D`
//! stores G) while `transpose` is 0 throughout, so transposition does not
//! explain the gap. The title suffix is the field a human maintains, so it
//! wins; a song with no suffix and a stale-looking `rootNote` is reported
//! rather than guessed at.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use signal_synth::gig::{read_presets, read_songs, GigSong};
use song::{Arrangement, Part, PartsManifest, Song, TimeSignature};
use uuid::Uuid;

/// Patch names per rackspace, in the order Gig Performer indexes them.
///
/// A `SONG_PART` names neither its rackspace nor its patch — it stores two
/// 0-based indices (`rackspace="1" variation="6"`). Resolving them is what
/// turns a part list into something a human can read, and it is checkable:
/// the observed variation ranges are 0..10, 0..16 and 0..1 against rackspaces
/// holding 13, 18 and 2 patches, which fits 0-based indexing and nothing else.
struct PatchIndex {
    /// One entry per non-global rackspace, in document order.
    rackspaces: Vec<Vec<String>>,
}

impl PatchIndex {
    fn build(xml: &str) -> Self {
        let mut rackspaces: Vec<Vec<String>> = Vec::new();
        let mut current = String::new();
        for p in read_presets(xml) {
            // The global rackspace holds its own patch but is not addressable
            // by a song part, so it is not in the index.
            if p.rackspace == "GLOBAL RACKSPACE" {
                continue;
            }
            if p.rackspace != current {
                current = p.rackspace.clone();
                rackspaces.push(Vec::new());
            }
            if let Some(last) = rackspaces.last_mut() {
                last.push(p.name);
            }
        }
        Self { rackspaces }
    }

    fn patch(&self, rackspace: &str, variation: &str) -> Option<&str> {
        let r: usize = rackspace.trim().parse().ok()?;
        let v: usize = variation.trim().parse().ok()?;
        self.rackspaces.get(r)?.get(v).map(String::as_str)
    }
}

/// Filesystem-safe folder name for a song title.
fn slug(title: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !out.is_empty() && !dash {
            out.push('-');
            dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
}

/// Songs that are rig scratch rather than repertoire. They are real entries in
/// the gig, but a song library of "Testing" and "Jam" is a worse library.
fn is_scratch(title: &str) -> bool {
    const SCRATCH: [&str; 6] = [
        "testing",
        "jam",
        "make it work",
        "dry piano",
        "underscore vibes",
        "gospel",
    ];
    let t = title.trim().to_ascii_lowercase();
    SCRATCH.contains(&t.as_str())
}

fn to_song(g: &GigSong, patches: &PatchIndex) -> (Song, Option<String>) {
    let title = g.title().to_string();
    let mut warning = None;

    // The title wins where it has a key. Where it does not, `rootNote` is the
    // only signal there is — distrusting it is only warranted when it
    // *contradicts* a title, not when it stands alone, and falling back to
    // C Major there would discard real information.
    let key = match g.key_from_name().map(str::parse) {
        Some(Ok(k)) => k,
        _ => match g.stored_key().parse() {
            Ok(k) => {
                warning = Some(format!(
                    "no key in the title; used rootNote {}",
                    g.stored_key()
                ));
                k
            }
            Err(_) => {
                warning = Some("no usable key — defaulting to C Major".to_string());
                song::Key::c_major()
            }
        },
    };

    let parts: Vec<Part> = g
        .parts
        .iter()
        .map(|(name, rackspace, variation)| Part {
            name: name.clone(),
            // The gig names a patch per part but never a bar count.
            patch: patches.patch(rackspace, variation).map(str::to_string),
            ..Part::default()
        })
        .collect();

    let arrangement = Arrangement {
        id: Uuid::new_v4(),
        name: "Default".to_string(),
        key,
        tempo_bpm: (g.bpm > 0.0).then_some(g.bpm),
        time_signature: Some(TimeSignature::new(g.sig_num as u8, g.sig_den as u8)),
        chart_ref: None,
        parts: PartsManifest { parts },
        attachment_refs: Vec::new(),
    };

    let song = Song {
        id: Uuid::new_v4(),
        title,
        tags: vec!["worship".to_string()],
        default_arrangement: arrangement.id,
        arrangements: vec![arrangement],
    };
    (song, warning)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let gig_path = args
        .next()
        .ok_or("usage: import_gig_songs <file.gig> <library_root> [--dry-run] [--force]")?;
    let root = PathBuf::from(
        args.next()
            .ok_or("usage: import_gig_songs <file.gig> <library_root> [--dry-run] [--force]")?,
    );
    let flags: Vec<String> = args.collect();
    let dry_run = flags.iter().any(|f| f == "--dry-run");
    let force = flags.iter().any(|f| f == "--force");

    let xml = std::fs::read_to_string(&gig_path)?;
    let patches = PatchIndex::build(&xml);
    // Songs recur across setlists; the library wants one folder each, and the
    // first occurrence is as good as any (they carry the same fields).
    let mut unique: BTreeMap<String, GigSong> = BTreeMap::new();
    for s in read_songs(&xml) {
        if s.name.trim().is_empty() {
            continue;
        }
        unique.entry(s.name.clone()).or_insert(s);
    }

    let (mut written, mut skipped, mut scratch) = (0usize, 0usize, 0usize);
    for g in unique.values() {
        let (s, warning) = to_song(g, &patches);
        if is_scratch(&s.title) {
            scratch += 1;
            continue;
        }
        let dir: &Path = &root.join(slug(&s.title));
        let arr = &s.arrangements[0];
        let meter = arr
            .time_signature
            .map(|t| t.to_string())
            .unwrap_or_default();
        println!(
            "  {:<38} {:<8} {:>5.1} {:<5} {} part(s){}",
            s.title,
            arr.key.to_string(),
            arr.tempo_bpm.unwrap_or(0.0),
            meter,
            arr.parts.parts.len(),
            warning.map(|w| format!("   [{w}]")).unwrap_or_default()
        );
        if dry_run {
            continue;
        }
        if dir.exists() && !force {
            skipped += 1;
            continue;
        }
        song::to_folder(&s, dir)?;
        written += 1;
    }

    println!(
        "\n{} song(s) written, {skipped} already present, {scratch} scratch entries skipped",
        written
    );
    if dry_run {
        println!("(dry run — nothing written)");
    } else {
        println!("library root: {}", root.display());
    }
    Ok(())
}
