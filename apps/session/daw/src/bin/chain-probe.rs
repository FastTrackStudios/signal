//! Does the standalone engine host the processor's plugins?
//!
//! The rack draws a placeholder because `daw_proto::Track` carries no
//! chain. Before wiring it to a real one there is a question worth
//! answering with a running process rather than a guess: can the
//! backend this window drives load `FTS EQ`, and does it report
//! parameters with names a rack could map?
//!
//! **The answer, as of this probe: not yet.** `Effects::add` creates a
//! chain ENTRY by name without loading a binary, so
//! `Standalone::slot_parameters` finds nothing in `plugin_instances`
//! and `Effects::parameters` falls through to its stored path — which
//! names parameters `Param 1..N` and defaults them to 0.5. Adding
//! `FTS EQ` to the kit's first track gives eight of those. daw-standalone
//! says as much in its own source: the built-in FX list is "aspirational
//! — the audio-graph integration replaces this with real installed FX
//! once available".
//!
//! So the rack still draws `tone::placeholder`. There is nothing to map
//! a frequency, a gain or a Q onto, and a rack fed `Param 3 = 0.5` would
//! be a curve that looks like information and is not. Re-run this when
//! the backend loads plugins; the moment names come back, the mapping is
//! the only piece missing.
//!
//! Run: `chain-probe <project.rpp> [plugin name]`.

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: chain-probe <project.rpp> [plugin name]");
        std::process::exit(2);
    };
    let want = args.next().unwrap_or_else(|| "FTS EQ".to_owned());

    if let Err(error) = session_daw::open::open_and_serve(std::path::Path::new(&path)) {
        eprintln!("the project did not open: {error}");
        std::process::exit(1);
    }
    let Some(runtime) = session_daw::open::runtime() else {
        eprintln!("no runtime");
        std::process::exit(1);
    };

    runtime.block_on(async move {
        let Some(daw) = daw::rpc::Daw::try_get() else {
            eprintln!("no facade");
            return;
        };
        let project = daw.current_project().await.expect("a project");
        let tracks = project.tracks().all().await.expect("the tracks");
        let Some(first) = tracks.first() else {
            eprintln!("no tracks");
            return;
        };
        println!("track: {} ({})", first.name, first.guid);

        let track = project
            .tracks()
            .by_guid(&first.guid)
            .await
            .expect("a lookup")
            .expect("the track");
        let chain = track.fx_chain();
        println!(
            "fx before: {}",
            chain.all().await.map(|f| f.len()).unwrap_or(0)
        );
        match chain.add(&want).await {
            Ok(added) => println!("added {want}: {added:?}"),
            Err(error) => println!("add {want} failed: {error}"),
        }
        let all = chain.all().await.unwrap_or_default();
        println!("fx after: {}", all.len());
        for (i, entry) in all.iter().enumerate() {
            let index = u32::try_from(i).unwrap_or(0);
            let Ok(Some(fx)) = chain.by_index(index).await else {
                continue;
            };
            let params = fx.parameters().await.unwrap_or_default();
            println!("  [{i}] {} — {} parameters", entry.name, params.len());
            for p in params.iter().take(24) {
                println!(
                    "      {:>3} {:<28} {:.4}  {}",
                    p.index, p.name, p.value, p.formatted
                );
            }
        }
    });
}
