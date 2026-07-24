//! Standalone Full ⇄ Proxy pack transcoder — `pack_cli`'s Transcode
//! subcommand without pulling the whole fts-cli build.
//!
//! ```bash
//! cargo run -p signal-sampler --release --example transcode_pack -- \
//!     "<in.signalpack>" "<out.signalpack>" [quality]
//! ```

use signal_sampler::engine::cache::{transcode_signal_pack, PackCodec};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = PathBuf::from(args.next().ok_or("usage: transcode_pack <in> <out> [quality]")?);
    let out = PathBuf::from(args.next().ok_or("usage: transcode_pack <in> <out> [quality]")?);
    let quality: f32 = args.next().map(|q| q.parse()).transpose()?.unwrap_or(0.6);

    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let stats = transcode_signal_pack(&input, &out, PackCodec::OggVorbis { quality })?;
    if stats.failed > 0 {
        return Err(format!("transcode: {} entr(ies) failed", stats.failed).into());
    }
    let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    println!(
        "transcoded {} entr(ies) -> {} ({:.1} MB)",
        stats.prepared,
        out.display(),
        size as f64 / 1e6
    );
    Ok(())
}
