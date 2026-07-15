//! Probe a running `fasttrackstudio --engine`'s SetlistService over the
//! network — the same wire path the browser session remote uses (one typed
//! vox client per service over its own WebSocket link).
//!
//! ```bash
//! cargo run -p fasttrackstudio --example setlist_probe -- ws://127.0.0.1:4040/vox
//! ```
//!
//! Prints the served setlist, the active song/section cursor from the
//! `active_indices` `#[subscribe]` stream, and one `events` stream frame —
//! proving RPC + both subscription streams reach the engine's session
//! router end-to-end.
//!
//! It then probes the chart path the browser chart pane rides on:
//! `song_chart(0)` must return the demo song's keyflow chart text, the text
//! must parse + lay out with the CPU engraver, and after a
//! `goto_measure(0, 4)` seek the transport's musical position must map to a
//! chart cursor whose highlighted measure tracks the seek target.
//!
//! NEVER point this at the deployed rig on :4040 when seeking — start a
//! scratch engine on another port (`SIGNAL_ENGINE_ADDR=127.0.0.1:14041
//! fasttrackstudio --engine`).

use session_proto::services::setlist_service::SetlistServiceStreamClient;
use session_proto::{ActiveIndices, SetlistEvent, SetlistServiceClient};

async fn establish<C: vox_core::FromVoxLane>(url: &str) -> C {
    let link = vox_websocket::WsLink::connect(url).await.expect("ws connect");
    vox_core::initiator_on(link)
        .establish::<C>()
        .await
        .expect("vox handshake")
}

fn main() {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ws://127.0.0.1:4040/vox".to_string());

    // 16 MiB worker stacks: vox 0.10's debug-build channel encode recurses
    // deeply on Setlist payloads (see session_engine.rs).
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .expect("tokio runtime");

    rt.block_on(async move {
        // ── RPC: fetch the setlist ────────────────────────────────────────
        let client: SetlistServiceClient = establish(&url).await;
        let setlist = client.setlist().await.expect("setlist() rpc");
        println!("setlist '{}': {} songs", setlist.name, setlist.songs.len());
        for (i, song) in setlist.songs.iter().enumerate() {
            println!("  {i:02} {} ({} sections)", song.name, song.sections.len());
        }

        // ── `#[subscribe]` streams: active_indices + events ──────────────
        let stream: SetlistServiceStreamClient = establish(&url).await;
        let (atx, mut arx) = vox::channel::<ActiveIndices>();
        let ai_stream = stream.clone();
        tokio::spawn(async move {
            let _ = ai_stream.active_indices(atx).await;
        });
        let ai = tokio::time::timeout(std::time::Duration::from_secs(5), arx.recv())
            .await
            .expect("active_indices frame within 5s")
            .expect("active_indices stream")
            .expect("active_indices closed");
        let ai = ai.get();
        println!(
            "active cursor: song {:?} / section {:?}",
            ai.song_index, ai.section_index
        );

        // The events hub has no replay (remotes snapshot the setlist via
        // RPC), so an idle transport legitimately produces no frame here —
        // subscribing without error is the assertion; a frame is a bonus.
        let (etx, mut erx) = vox::channel::<SetlistEvent>();
        let ev_stream = stream.clone();
        tokio::spawn(async move {
            let _ = ev_stream.events(etx).await;
        });
        match tokio::time::timeout(std::time::Duration::from_secs(3), erx.recv()).await {
            Ok(Ok(Some(ev))) => {
                println!("events stream: first frame = {:?}", variant_name(ev.get()))
            }
            Ok(_) => panic!("events stream closed"),
            Err(_) => println!("events stream: subscribed (no traffic while idle — ok)"),
        }

        // ── Chart RPC + playhead→highlight mapping (the chart pane's path) ──
        probe_chart(&client, &stream).await;

        println!("OK: SetlistService reachable over {url}");
    });
}

/// Fetch song 0's chart over the new `song_chart` RPC, lay it out with the
/// CPU engraver (exactly what the browser pane does), seek to a measure, and
/// verify the transport's musical position maps to a chart cursor tracking
/// the seek.
async fn probe_chart(client: &SetlistServiceClient, stream: &SetlistServiceStreamClient) {
    use keyflow::engraver::layout::ChartLayoutMode;
    use keyflow::engraver::layout::chart::cursor::{ChartCursor, CursorConfig, CursorStyle};
    use keyflow::engraver::layout::chart::{Breakpoint, ChartLayoutConfig};
    use keyflow::engraver::fonts::ChartFontBundle;
    use keyflow::engraver::style::MStyle;

    let chart = client
        .song_chart(0)
        .await
        .expect("song_chart(0) rpc")
        .expect("song 0 must have a hydrated chart (demo charts ride ext-state)");
    println!(
        "song_chart(0): {} bytes of chart text, fingerprint {}",
        chart.chart_text.len(),
        chart.chart_fingerprint
    );
    assert!(!chart.chart_text.trim().is_empty(), "chart text empty");

    // Parse + layout — the same continuous-scroll pipeline as the pane.
    let parsed = keyflow::parse(chart.chart_text.as_str()).expect("chart text parses");
    let font_bundle = ChartFontBundle::new().expect("font bundle");
    let style: &'static MStyle = Box::leak(Box::new(MStyle::new()));
    let mut engine = font_bundle.create_layout_engine(style);
    let width = 640.0;
    let layout = engine.layout_chart_with_config(
        &parsed,
        &ChartLayoutMode::ContinuousScroll { width },
        &ChartLayoutConfig::responsive_for(Breakpoint::from_viewport_pt(width)),
    );
    let total_measures = layout
        .beat_positions
        .iter()
        .map(|bp| bp.measure)
        .max()
        .map(|m| m + 1)
        .unwrap_or(0);
    println!(
        "chart layout: {} beat positions over {} measures",
        layout.beat_positions.len(),
        total_measures
    );
    assert!(total_measures > 4, "demo chart should span many measures");

    // The pane's static layer: fontless SVG serialization of the scene.
    let svg = {
        use keyflow::engraver::export::{SvgExportConfig, SvgSerializer};
        let (w, h) = (layout.total_width.max(width), layout.total_height.max(60.0));
        let mut serializer = SvgSerializer::new(SvgExportConfig::for_page(0.0, 0.0, w, h));
        serializer.serialize(&layout.scene)
    };
    assert!(svg.starts_with("<?xml") && svg.contains("<svg "), "svg header");
    assert!(svg.contains("viewBox="), "svg viewBox");
    println!("chart svg: {} bytes", svg.len());

    // Seek to project measure 4. `goto_measure` resolves through the
    // project tempo map (`musical_to_time(measure, …)`), i.e. measures are
    // 0-indexed from the PROJECT start with the count-in included — and the
    // demo charts lead with the same 2-measure Count section stamped at
    // t=0, so project measures and chart layout measures align 1:1.
    let (atx, mut arx) = vox::channel::<ActiveIndices>();
    let ai_stream = stream.clone();
    tokio::spawn(async move {
        let _ = ai_stream.active_indices(atx).await;
    });
    client.goto_measure(0, 4).await.expect("goto_measure(0,4)");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut landed = None;
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(std::time::Duration::from_millis(500), arx.recv()).await {
            Ok(Ok(Some(ai))) => {
                let ai: ActiveIndices = ai.get().clone();
                if ai.song_index == Some(0) {
                    landed = Some(ai);
                    break;
                }
            }
            _ => break,
        }
    }
    let landed = landed.expect("active_indices lands on song 0 after seek");
    println!(
        "after goto_measure(0,4): song {:?} section {:?}",
        landed.song_index, landed.section_index
    );

    // Static mapping check: the sought project measure is the chart measure.
    let ticks_per_measure = 4 * 480; // demo charts are 4/4 at 480 PPQ
    let sought_measure = 4usize;
    let tick = (sought_measure * ticks_per_measure) as i64;
    let cursor = ChartCursor::new(CursorConfig {
        style: CursorStyle::MeasureHighlight,
        ..CursorConfig::default()
    });
    let state = cursor
        .compute(&layout, tick)
        .expect("cursor state for the sought measure");
    println!(
        "cursor: page {} system {} measure {} ({} draw commands)",
        state.page,
        state.system,
        state.measure,
        state.commands.len()
    );
    assert_eq!(
        state.measure, sought_measure,
        "highlighted chart measure tracks the seek"
    );
    assert!(!state.commands.is_empty(), "measure highlight draws");

    // ── The pane's live mapping: cursor-stream progress → chart time ──────
    // Roll the transport briefly and capture `song_progress` frames from
    // the active-indices stream (the ~10 Hz cursor stream every backend
    // serves), then map them exactly like the browser pane does:
    // `chart seconds = count_in + progress × song duration` →
    // `ChartCursor::compute_at_time`. The highlighted measure must sit at
    // or past the measure we sought, and advance while playing.
    let song0 = client.song(0).await.expect("song(0) rpc");
    let count_in = song0.count_in_seconds.unwrap_or(0.0);
    let duration = song0.duration();
    assert!(duration > 0.0, "song 0 must have a duration");

    client.play().await.expect("play");
    let (dtx, mut drx) = vox::channel::<ActiveIndices>();
    let live_stream = stream.clone();
    tokio::spawn(async move {
        let _ = live_stream.active_indices(dtx).await;
    });
    let mut live_measures = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
    while std::time::Instant::now() < deadline && live_measures.len() < 12 {
        match tokio::time::timeout(std::time::Duration::from_millis(500), drx.recv()).await {
            Ok(Ok(Some(ai))) => {
                let ai: ActiveIndices = ai.get().clone();
                let (Some(0), Some(p), true) = (ai.song_index, ai.song_progress, ai.is_playing)
                else {
                    continue;
                };
                let chart_seconds = count_in + p.clamp(0.0, 1.0) * duration;
                if let Some(state) = cursor.compute_at_time(&layout, chart_seconds) {
                    live_measures.push(state.measure);
                }
            }
            Ok(_) => break,
            Err(_) => continue,
        }
    }
    client.pause().await.expect("pause");
    assert!(
        !live_measures.is_empty(),
        "cursor stream must yield mappable progress frames while playing"
    );
    let first = *live_measures.first().unwrap();
    let last = *live_measures.last().unwrap();
    println!(
        "live cursor: {} frames, chart measure {first} → {last} (sought measure {sought_measure}, \
         count_in {count_in:.2}s, duration {duration:.1}s)",
        live_measures.len(),
    );
    assert!(
        first >= sought_measure,
        "live highlight starts at/past the sought measure"
    );
    assert!(
        live_measures.windows(2).all(|w| w[1] >= w[0]),
        "live highlight only moves forward while playing"
    );
    println!("OK: chart RPC + highlight mapping verified");
}

fn variant_name(ev: &SetlistEvent) -> &'static str {
    match ev {
        SetlistEvent::SetlistChanged(_) => "SetlistChanged",
        SetlistEvent::SongHydrated { .. } => "SongHydrated",
        SetlistEvent::SongEntered { .. } => "SongEntered",
        _ => "(other)",
    }
}
