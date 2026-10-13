//! Add a rendered `reference.ogg` (a single mixed-down reference track) to a
//! song's default arrangement, so the streaming player (Now Playing bar) has
//! one file to play instead of all the stems — for songs whose stems have no
//! `original-track`/`reference` of their own.
//!
//! Run it AFTER rendering `reference.ogg` into the song folder (see
//! `scripts`/the render step). Idempotent: skips a song that already
//! references `reference.ogg`.
//!
//! ```text
//! cargo run -p song --example add_reference -- <song-dir> [<song-dir> ...]
//! ```

use song::{AttachmentRef, from_folder, to_folder};

fn main() {
    let dirs: Vec<String> = std::env::args().skip(1).collect();
    if dirs.is_empty() {
        eprintln!("usage: add_reference <song-dir> [<song-dir> ...]");
        std::process::exit(2);
    }
    let mut ok: u32 = 0;
    let mut fail: u32 = 0;
    for d in &dirs {
        let path = std::path::PathBuf::from(d);
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if !path.join("reference.ogg").is_file() {
            eprintln!("skip {name}: no reference.ogg rendered");
            continue;
        }
        let mut song = match from_folder(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("FAIL {name}: read: {e}");
                fail = fail.saturating_add(1);
                continue;
            }
        };
        let arr_id = song.default_arrangement;
        let Some(arr) = song.arrangements.iter_mut().find(|a| a.id == arr_id) else {
            eprintln!("FAIL {name}: no default arrangement");
            fail = fail.saturating_add(1);
            continue;
        };
        if arr
            .attachment_refs
            .iter()
            .any(|a| a.path.as_deref() == Some("reference.ogg"))
        {
            println!("skip {name}: already references reference.ogg");
            continue;
        }
        // Front of the list → the reader treats it as the reference stem.
        arr.attachment_refs.insert(
            0,
            AttachmentRef {
                id: "reference".to_string(),
                path: Some("reference.ogg".to_string()),
                sha256: None,
                kind: Some("reference".to_string()),
            },
        );
        match to_folder(&song, &path) {
            Ok(()) => {
                println!("added reference to {name}");
                ok = ok.saturating_add(1);
            }
            Err(e) => {
                eprintln!("FAIL {name}: write: {e}");
                fail = fail.saturating_add(1);
            }
        }
    }
    println!("\nreferenced {ok}, failed {fail}");
    if fail > 0 {
        std::process::exit(1);
    }
}
