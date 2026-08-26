//! Network probe for the engine's PackLibrary service — list the host's
//! packs, optionally download one (with resume) and verify its sha256.
//!
//! ```bash
//! # list
//! cargo run -p fasttrackstudio --example pack_probe \
//!     --no-default-features --features signal-guitar -- ws://127.0.0.1:4040/vox
//! # download
//! cargo run -p fasttrackstudio --example pack_probe \
//!     --no-default-features --features signal-guitar -- \
//!     ws://127.0.0.1:4040/vox "Wurlitzer 200A" /tmp/packs
//! ```

use std::io::Write as _;

use signal_packs_proto::PackChunk;
use signal_packs_proto::packs::PackLibraryClient;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    let mut args = std::env::args().skip(1);
    let url = args
        .next()
        .unwrap_or_else(|| "ws://127.0.0.1:4040/vox".into());
    let want = args.next();
    let dest = args.next().map(std::path::PathBuf::from);

    // "ws://…" dials the WebSocket; anything else is an iroh endpoint id
    // (the p2p path the phone takes by default). The iroh endpoint must
    // outlive the whole session — dropping it closes every connection —
    // so it lives at fn scope (the app keeps its own in a static).
    let mut _ep_keepalive: Option<architect::iroh_link::iroh::Endpoint> = None;
    let client: PackLibraryClient = if url.starts_with("ws") {
        let link = vox_websocket::WsLink::connect(&url)
            .await
            .map_err(|e| eyre::eyre!("ws connect {url}: {e:?}"))?;
        vox_core::initiator_on(link)
            .establish()
            .await
            .map_err(|e| eyre::eyre!("vox handshake: {e:?}"))?
    } else {
        use architect::iroh_link::iroh;
        let id: iroh::EndpointId = url
            .trim()
            .parse()
            .map_err(|e| eyre::eyre!("bad iroh id: {e:?}"))?;
        let ep = architect::iroh_link::bind_endpoint(iroh::SecretKey::generate())
            .await
            .map_err(|e| eyre::eyre!("iroh bind: {e:?}"))?;
        let link = architect::iroh_link::connect(&ep, id)
            .await
            .map_err(|e| eyre::eyre!("iroh connect: {e:?}"))?;
        _ep_keepalive = Some(ep);
        vox_core::initiator_on(link)
            .establish()
            .await
            .map_err(|e| eyre::eyre!("vox handshake (iroh): {e:?}"))?
    };

    // `planfirst:<name>` — call pack_plan as the FIRST method on this
    // fresh connection (the browser's call pattern), before packs().
    if let Some(pname) = std::env::args()
        .nth(2)
        .as_deref()
        .and_then(|w| w.strip_prefix("planfirst:").map(str::to_string))
    {
        let (ptx, mut prx) = vox::channel::<PackChunk>();
        let plan_call = client.pack_plan(pname.clone(), "proxy".to_string(), 0, ptx);
        let mut json_bytes = Vec::new();
        let plan_drain = async {
            while let Ok(Some(chunk)) = prx.recv().await {
                json_bytes.extend_from_slice(&chunk.get().bytes);
            }
        };
        let (plan_result, ()) = futures_util::join!(plan_call, plan_drain);
        plan_result.map_err(|e| eyre::eyre!("pack_plan (first call): {e:?}"))?;
        println!(
            "pack_plan as FIRST call: {} bytes of json, {} segments",
            json_bytes.len(),
            String::from_utf8_lossy(&json_bytes)
                .matches("\"start\"")
                .count()
        );
        return Ok(());
    }

    let packs = client
        .packs()
        .await
        .map_err(|e| eyre::eyre!("packs: {e:?}"))?;
    println!("{} packs on {url}:", packs.len());
    for p in &packs {
        println!(
            "  [{}] {} / {} — {:.2} GB  sha256={}",
            p.variant,
            p.category,
            p.name,
            p.size_bytes as f64 / 1e9,
            if p.sha256.is_empty() {
                "<pending>"
            } else {
                &p.sha256[..16]
            },
        );
    }

    let Some(name) = want else { return Ok(()) };

    // `plan:<name>` — probe the W7 pack_plan + a rank-0 read_range instead
    // of downloading. Diagnoses the range-streaming path from native.
    if let Some(pname) = name.strip_prefix("plan:") {
        let (ptx, mut prx) = vox::channel::<PackChunk>();
        let plan_call = client.pack_plan(pname.to_string(), "proxy".to_string(), 0, ptx);
        let mut json_bytes = Vec::new();
        let plan_drain = async {
            while let Ok(Some(chunk)) = prx.recv().await {
                json_bytes.extend_from_slice(&chunk.get().bytes);
            }
        };
        let (plan_result, ()) = futures_util::join!(plan_call, plan_drain);
        plan_result.map_err(|e| eyre::eyre!("pack_plan: {e:?}"))?;
        let json = String::from_utf8(json_bytes)?;
        eyre::ensure!(!json.trim().is_empty(), "empty plan for {pname:?}");
        let segment_count = json.matches("\"start\"").count();
        println!(
            "plan for {pname}: {} bytes of json, {} segments",
            json.len(),
            segment_count
        );
        println!("  head: {}", &json[..json.len().min(200)]);
        // Rank-0 header segment is always [0, 64) — probe read_range on it.
        let (tx, mut rx) = vox::channel::<PackChunk>();
        let call = client.read_range(
            pname.to_string(),
            "proxy".to_string(),
            signal_packs_proto::PackRange { start: 0, len: 64 }.to_string(),
            tx,
        );
        let drain = async {
            let mut got = 0u64;
            while let Ok(Some(chunk)) = rx.recv().await {
                got += chunk.get().bytes.len() as u64;
            }
            got
        };
        let (call_result, got) = futures_util::join!(call, drain);
        call_result.map_err(|e| eyre::eyre!("read_range: {e:?}"))?;
        println!("read_range header seg: {got} of 64 bytes");
        return Ok(());
    }

    let info = packs
        .iter()
        .find(|p| p.name == name && p.variant == "proxy")
        .or_else(|| packs.iter().find(|p| p.name == name))
        .ok_or_else(|| eyre::eyre!("no pack named {name:?} on host"))?;
    let dest = dest.unwrap_or_else(|| "/tmp/packs".into());
    std::fs::create_dir_all(&dest)?;
    let part = dest.join(format!("{}.signalpack.part", info.name));
    let final_path = dest.join(format!("{}.signalpack", info.name));
    let start = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    println!(
        "downloading [{}] {} from byte {start}…",
        info.variant, info.name
    );
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&part)?;

    let (tx, mut rx) = vox::channel::<PackChunk>();
    let read_call = client.read(info.name.clone(), info.variant.clone(), start, tx);
    let total = info.size_bytes;
    let mut done = start;
    let started = std::time::Instant::now();
    let drain = async {
        while let Ok(Some(chunk)) = rx.recv().await {
            let chunk = chunk.get();
            file.write_all(&chunk.bytes)?;
            done += chunk.bytes.len() as u64;
            if done % (64 * 1024 * 1024) < chunk.bytes.len() as u64 {
                println!(
                    "  {:.1}% — {:.1} MB/s",
                    done as f64 * 100.0 / total as f64,
                    (done - start) as f64 / 1e6 / started.elapsed().as_secs_f64(),
                );
            }
        }
        Ok::<_, std::io::Error>(())
    };
    let (read_result, drain_result) = futures_util::join!(read_call, drain);
    read_result.map_err(|e| eyre::eyre!("read: {e:?}"))?;
    drain_result?;
    file.flush()?;
    drop(file);

    eyre::ensure!(done == total, "incomplete: {done} of {total} bytes");
    if !info.sha256.is_empty() {
        use sha2::Digest as _;
        use std::io::Read as _;
        let mut hasher = sha2::Sha256::new();
        let mut f = std::fs::File::open(&part)?;
        let mut buf = vec![0u8; 4 * 1024 * 1024];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let hex = format!("{:x}", hasher.finalize());
        eyre::ensure!(
            hex == info.sha256,
            "sha256 mismatch: {hex} != {}",
            info.sha256
        );
        println!("sha256 verified ✓");
    }
    std::fs::rename(&part, &final_path)?;
    println!(
        "done → {} ({:.2} GB in {:.0?})",
        final_path.display(),
        total as f64 / 1e9,
        started.elapsed(),
    );
    Ok(())
}
