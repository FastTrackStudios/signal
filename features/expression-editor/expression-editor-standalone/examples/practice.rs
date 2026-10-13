//! Prepare real-song copies. Stdout contains only project paths for shell use.
//!
//! `--cached` stages each song ONCE into a reused directory and prints
//! the path again on every later call. Benchmarks want that: a fresh
//! copy is several gigabytes of disk and throws away the `.reapeaks`
//! sidecars the last run built, which is most of a run's startup. Manual
//! practice still wants a fresh copy — that is what the default does.
use expression_editor_standalone::practice::{
    PracticeSession, Song, album_directory, cache_directory,
};

fn main() -> eyre::Result<()> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let cached = args.iter().any(|arg| arg == "--cached");
    args.retain(|arg| arg != "--cached");
    let songs = match args.as_slice() {
        [] => vec![Song::SetInStone],
        [song] if song == "both" => Song::BOTH.to_vec(),
        [song] => vec![Song::parse(song)?],
        _ => eyre::bail!("Usage: practice [--cached] [set-in-stone|unbreakable|both]"),
    };
    let session = if cached {
        let cache = cache_directory();
        eprintln!(
            "Reusing the cached practice staging under {}…",
            cache.display()
        );
        PracticeSession::prepare_cached(&album_directory(), &songs, &cache)?
    } else {
        eprintln!(
            "Copying Crescendum projects and their referenced media into a fresh temporary workspace…"
        );
        PracticeSession::prepare(&album_directory(), &songs)?
    };
    eprintln!(
        "Practice directory: {} ({} media copies made now)",
        session.directory.display(),
        session.media.len()
    );
    if !cached {
        eprintln!("Keep this directory to resume later; each run creates a fresh one.");
    }
    for project in session.projects {
        println!("{}", project.copy.display());
    }
    Ok(())
}
