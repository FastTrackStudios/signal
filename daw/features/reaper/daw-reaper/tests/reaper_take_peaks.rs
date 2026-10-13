//! Take-peaks parity: the same fixture take answers the same
//! `Peaks::take_peaks` frame from both backends (session#54).
//!
//! The proto's contract is standalone's: one `(min ≤ 0, max ≥ 0)` pair
//! per channel per block, blocks of `block_size` SOURCE samples of take
//! playback time — start offset and play rate applied — covering the
//! item's length. REAPER reads its `.reapeaks` mipmap in source time
//! through `PCM_Source_GetPeaks`, so its answer can differ from an exact
//! scan by the cache's i16 quantisation; the fixtures align every block
//! on REAPER's 2400-sample mipmap grid so that is the ONLY difference.
//!
//! - `standalone_*`: plain tests, the reference frame against an exact
//!   scan of the generated samples — no DAW.
//! - `peaks_parity_*`: `#[reaper_test]`, the same file through REAPER.
//!
//! Run the REAPER half with: `cargo run -p daw-reaper-xtask -- peaks_parity`

use daw::test::reaper_test;
use daw_proto::midi::Midi;
use daw_proto::{
    Duration, Peaks, ProjectContext, ProjectInfo, TakePeakData, TakeRef, Takes, TrackRef, Tracks,
};
use std::path::PathBuf;

/// Two `.reapeaks` least-significant bits. REAPER answers from its i16
/// peak cache (`(v × 32767) as i16`, truncating toward zero) while
/// standalone scans the decoded samples exactly, so the two can sit an
/// LSB apart — and the fixture's own i16 samples round once more on the
/// way into the WAV. Anything larger is a real disagreement.
const REAPEAKS_TOLERANCE: f64 = 2.0 / 32767.0;

/// A deterministic fixture: the `(channel, frame) → sample` recipe plus
/// the take placement to read it through.
#[derive(Clone, Copy)]
struct Fixture {
    name: &'static str,
    sample_rate: u32,
    channels: u16,
    /// Source length in seconds.
    source_secs: f64,
    /// `samples_per_peak` asked of both backends.
    block: u32,
    play_rate: f64,
    /// Take start offset in SOURCE samples.
    start_offset_frames: u32,
    /// Item length in blocks of take time.
    blocks: usize,
}

impl Fixture {
    /// The source frames one block covers — `block × play_rate` for
    /// every fixture here, and REAPER's own mipmap window, so both
    /// backends fold whole blocks and the comparison stays exact.
    const SPAN: usize = 2400;
    /// Where the loud burst sits inside a block, in frames: past the
    /// half a play-rate-ignoring read would stop at, and well clear of
    /// both edges so a cache reading the block slightly narrow or
    /// shifted still finds it.
    const BURST_AT: usize = 1440;
    const BURST_LEN: usize = 360;

    /// Mono, no placement: the plain case. 48 kHz like every fixture
    /// here — see [`OFF_RATE_SOURCE`] for why a source at another rate
    /// gets its own, weaker test.
    const PLAIN: Fixture = Fixture {
        name: "plain",
        sample_rate: 48_000,
        channels: 1,
        source_secs: 1.0,
        block: 2400,
        play_rate: 1.0,
        start_offset_frames: 0,
        blocks: 10,
    };

    /// Stereo (not the 2 channels the old shim hardcoded — mono is
    /// PLAIN's job), trimmed 4800 frames in and played at double speed,
    /// so every take-time block of 1200 samples reads 2400 source frames
    /// from a 2400-aligned start.
    const OFFSET_RATE: Fixture = Fixture {
        name: "offset-rate",
        sample_rate: 48_000,
        channels: 2,
        source_secs: 1.5,
        block: 1200,
        play_rate: 2.0,
        start_offset_frames: 4800,
        blocks: 14,
    };

    /// A source at a rate the project is NOT running at. REAPER serves
    /// such a take's peaks on the project's own grid rather than the
    /// file's — a 44.1 kHz file in a 48 kHz project answers 20 peaks a
    /// second whatever peak rate it is asked for — so its VALUES cannot
    /// line up with an exact scan of the file, and this fixture only
    /// proves what the shim itself got wrong: that the rate and the
    /// channel count come from the source (they were hardcoded to
    /// 44.1 kHz and 2), and that real audio comes back.
    const OFF_RATE_SOURCE: Fixture = Fixture {
        name: "off-rate-source",
        sample_rate: 44_100,
        channels: 1,
        source_secs: 1.0,
        block: 2400,
        play_rate: 1.0,
        start_offset_frames: 0,
        blocks: 10,
    };

    /// The sample at `frame` on `channel`, as the i16 the WAV holds.
    ///
    /// The content has to tell the three original bugs apart without
    /// depending on exactly where inside a block a peak cache looks —
    /// both backends round a block's edges to whole frames, and
    /// REAPER's mipmap windows are its own. So the amplitude is a
    /// smooth ramp across the whole source (continuous at every block
    /// edge, so a frame of slop costs nothing, while blocks a whole
    /// span apart are far apart in level — a read at the wrong start
    /// offset lands on another block's numbers), carrying a short loud
    /// burst deep inside each [`Self::SPAN`]-frame block: clear of both
    /// edges, and past the half a play-rate-ignoring read would stop
    /// at. The negative half is scaled differently from the positive
    /// one so a min/max swap cannot pass, and the channels carry
    /// different tones so a channel swap cannot.
    fn sample(&self, channel: usize, frame: usize) -> i16 {
        let t = frame as f64 / self.sample_rate as f64;
        let (hz, base) = match channel {
            0 => (440.0, 0.75),
            _ => (97.0, 0.5),
        };
        let ramp = 0.4 + 0.5 * frame as f64 / self.frames().max(1) as f64;
        let amp = base * ramp;
        let into = frame % Self::SPAN;
        let v = if (Self::BURST_AT..Self::BURST_AT + Self::BURST_LEN).contains(&into) {
            0.95 * amp
        } else {
            let wave = (t * hz * std::f64::consts::TAU).sin();
            0.6 * amp * if wave >= 0.0 { wave } else { wave * 0.55 }
        };
        (v * i16::MAX as f64) as i16
    }

    fn frames(&self) -> usize {
        (self.sample_rate as f64 * self.source_secs) as usize
    }

    fn start_offset(&self) -> Duration {
        Duration::from_seconds(self.start_offset_frames as f64 / self.sample_rate as f64)
    }

    /// Item length in take seconds: a hair under a whole number of
    /// blocks, so `ceil(length / block)` cannot round up to an extra
    /// (silent) block on either backend.
    fn item_length(&self) -> Duration {
        let block_secs = self.block as f64 / self.sample_rate as f64;
        Duration::from_seconds(self.blocks as f64 * block_secs - 1e-6)
    }

    /// A PCM-16 WAV of the fixture, written to a fresh temp path.
    fn write_wav(&self, tag: &str) -> PathBuf {
        let frames = self.frames();
        let block_align = self.channels * 2;
        let data_size = frames as u32 * block_align as u32;
        let mut d = Vec::with_capacity(44 + data_size as usize);
        d.extend_from_slice(b"RIFF");
        d.extend_from_slice(&(36 + data_size).to_le_bytes());
        d.extend_from_slice(b"WAVE");
        d.extend_from_slice(b"fmt ");
        d.extend_from_slice(&16u32.to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&self.channels.to_le_bytes());
        d.extend_from_slice(&self.sample_rate.to_le_bytes());
        d.extend_from_slice(&(self.sample_rate * block_align as u32).to_le_bytes());
        d.extend_from_slice(&block_align.to_le_bytes());
        d.extend_from_slice(&16u16.to_le_bytes());
        d.extend_from_slice(b"data");
        d.extend_from_slice(&data_size.to_le_bytes());
        for frame in 0..frames {
            for ch in 0..self.channels as usize {
                d.extend_from_slice(&self.sample(ch, frame).to_le_bytes());
            }
        }
        let path = std::env::temp_dir().join(format!(
            "daw-take-peaks-{}-{tag}-{}.wav",
            self.name,
            std::process::id()
        ));
        std::fs::write(&path, &d).expect("write fixture wav");
        path
    }

    /// The frame the proto documents, from an exact scan of the recipe:
    /// block `b` of take time reads source frames
    /// `start + b × block × play_rate ..+ block × play_rate`.
    fn expected(&self) -> TakePeakData {
        let span = (self.block as f64 * self.play_rate) as usize;
        let frames = self.frames();
        let mut peaks = Vec::with_capacity(self.blocks * self.channels as usize * 2);
        for b in 0..self.blocks {
            let lo = self.start_offset_frames as usize + b * span;
            let hi = (lo + span).min(frames);
            for ch in 0..self.channels as usize {
                let (mut mn, mut mx) = (0.0f64, 0.0f64);
                for f in lo..hi {
                    let v = self.sample(ch, f) as f64 / i16::MAX as f64;
                    mn = mn.min(v);
                    mx = mx.max(v);
                }
                peaks.push(mn);
                peaks.push(mx);
            }
        }
        TakePeakData {
            sample_rate: self.sample_rate as f64,
            num_channels: self.channels as u32,
            peaks,
            samples_per_peak: self.block,
        }
    }
}

/// Same shape, every value within `tolerance`.
fn assert_peaks_match(what: &str, got: &TakePeakData, want: &TakePeakData, tolerance: f64) {
    assert_eq!(got.sample_rate, want.sample_rate, "{what}: sample_rate");
    assert_eq!(got.num_channels, want.num_channels, "{what}: num_channels");
    assert_eq!(
        got.samples_per_peak, want.samples_per_peak,
        "{what}: samples_per_peak"
    );
    assert_eq!(got.peaks.len(), want.peaks.len(), "{what}: peak count");
    let worst = got
        .peaks
        .iter()
        .zip(&want.peaks)
        .enumerate()
        .map(|(i, (g, w))| ((g - w).abs(), i))
        .fold((0.0f64, 0usize), |a, b| if b.0 > a.0 { b } else { a });
    assert!(
        worst.0 <= tolerance,
        "{what}: peaks[{}] differs by {} (> {tolerance}): got {} want {}",
        worst.1,
        worst.0,
        got.peaks[worst.1],
        want.peaks[worst.1]
    );
    // The standalone clamp the proto documents: min ≤ 0 ≤ max.
    assert!(
        got.peaks.chunks(2).all(|p| p[0] <= 0.0 && p[1] >= 0.0),
        "{what}: a pair is not (min ≤ 0, max ≥ 0)"
    );
}

/// The fixture through the standalone backend: one project, one track,
/// one item of the fixture's length whose active take points at `wav`
/// with the fixture's placement.
fn standalone_peaks(fx: &Fixture, wav: &std::path::Path) -> TakePeakData {
    let daw = daw::standalone::sync::Standalone::new();
    daw.seed_project(ProjectInfo {
        guid: format!("peaks-{}", fx.name),
        name: fx.name.into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Current;
    let track = Tracks::add(&daw, ctx.clone(), "Peaks", None).expect("track");
    let loc = Midi::create_midi_item(
        &daw,
        ctx.clone(),
        TrackRef::Guid(track),
        0.0,
        fx.item_length().as_seconds(),
    )
    .expect("item");
    let item = loc.item.clone();
    // Through the service, the same door the REAPER half goes through:
    // a test that hand-patched the project struct instead would pass
    // while the standalone source writer was broken.
    Takes::set_source_file(
        &daw,
        ctx.clone(),
        item.clone(),
        TakeRef::Active,
        wav.to_string_lossy().into_owned(),
    )
    .expect("source file");
    Takes::set_start_offset(
        &daw,
        ctx.clone(),
        item.clone(),
        TakeRef::Active,
        fx.start_offset(),
    )
    .expect("start offset");
    Takes::set_play_rate(
        &daw,
        ctx.clone(),
        item.clone(),
        TakeRef::Active,
        fx.play_rate,
    )
    .expect("play rate");
    Peaks::take_peaks(&daw, ctx, item, TakeRef::Active, fx.block)
}

fn standalone_matches_exact_scan(fx: Fixture) {
    let wav = fx.write_wav("standalone");
    let got = standalone_peaks(&fx, &wav);
    let _ = std::fs::remove_file(&wav);
    assert_peaks_match(
        "standalone vs exact scan",
        &got,
        &fx.expected(),
        REAPEAKS_TOLERANCE,
    );
    assert!(
        got.peaks.iter().any(|p| p.abs() > 0.25),
        "real peaks, not silence"
    );
}

#[test]
fn standalone_plain_take_matches_exact_scan() {
    standalone_matches_exact_scan(Fixture::PLAIN);
}

#[test]
fn standalone_offset_rate_take_matches_exact_scan() {
    standalone_matches_exact_scan(Fixture::OFFSET_RATE);
}

/// The fixture through REAPER: a real item + take over `wav` with the
/// fixture's placement, read back over the daw RPC.
async fn reaper_peaks(
    ctx: &daw::test::ReaperTestContext,
    fx: &Fixture,
    wav: &std::path::Path,
) -> eyre::Result<TakePeakData> {
    let project = ctx.project().clone();
    let track = project.tracks().add("Peaks", None).await?;
    let item = track
        .items()
        .add(
            daw_proto::PositionInSeconds::from_seconds(0.0),
            fx.item_length(),
        )
        .await?;
    let take = item.takes().active().await?;
    take.set_source_file(&wav.to_string_lossy()).await?;
    take.set_start_offset(fx.start_offset()).await?;
    take.set_play_rate(fx.play_rate).await?;
    // Guard the setup, so a take that never took the file fails HERE
    // and not as a mystery frame of defaults further down.
    let kind = take.source_type().await?;
    eyre::ensure!(
        matches!(kind, daw_proto::SourceType::Audio),
        "take did not take the fixture source ({kind:?}): {}",
        wav.display()
    );
    Ok(take.peaks(fx.block).await?)
}

async fn parity(ctx: &daw::test::ReaperTestContext, fx: Fixture) -> eyre::Result<()> {
    let wav = fx.write_wav("parity");
    let reaper = reaper_peaks(ctx, &fx, &wav).await;
    let standalone = standalone_peaks(&fx, &wav);
    let _ = std::fs::remove_file(&wav);
    let _ = std::fs::remove_file(wav.with_extension("wav.reapeaks"));
    let reaper = reaper?;
    assert_peaks_match(
        "REAPER vs standalone",
        &reaper,
        &standalone,
        REAPEAKS_TOLERANCE,
    );
    assert_peaks_match(
        "REAPER vs exact scan",
        &reaper,
        &fx.expected(),
        REAPEAKS_TOLERANCE,
    );
    Ok(())
}

// r[verify drums.open.peaks]
#[reaper_test(isolated)]
async fn peaks_parity_plain(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    parity(ctx, Fixture::PLAIN).await
}

#[reaper_test(isolated)]
async fn peaks_parity_offset_and_rate(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    parity(ctx, Fixture::OFFSET_RATE).await
}

/// The fixtures are only worth running if their content can tell the
/// three original bugs apart: a read that ignored the start offset, or
/// the play rate, must not be able to answer the right frame anyway.
/// Pure arithmetic over the recipe — the negative control for the
/// parity tests above.
#[test]
fn fixture_content_discriminates_offset_and_rate() {
    for fx in [Fixture::PLAIN, Fixture::OFFSET_RATE] {
        let want = fx.expected();

        let ignores_rate = Fixture {
            play_rate: 1.0,
            ..fx
        }
        .expected();
        let ignores_offset = Fixture {
            start_offset_frames: 0,
            ..fx
        }
        .expected();

        // PLAIN has neither, so only the rate-changed fixture can
        // disagree with itself — but every fixture must have blocks
        // that differ from each other, or any misread lands on a
        // neighbour's numbers.
        let first = &want.peaks[..fx.channels as usize * 2];
        let second = &want.peaks[fx.channels as usize * 2..fx.channels as usize * 4];
        assert_ne!(first, second, "{}: blocks are indistinguishable", fx.name);

        if fx.play_rate != 1.0 {
            assert_ne!(want.peaks, ignores_rate.peaks, "{}: play rate", fx.name);
        }
        if fx.start_offset_frames != 0 {
            assert_ne!(
                want.peaks, ignores_offset.peaks,
                "{}: start offset",
                fx.name
            );
        }
        // Asymmetric content: a min/max swap is visible.
        assert!(
            want.peaks
                .chunks(2)
                .any(|p| (p[0].abs() - p[1].abs()).abs() > 1e-3),
            "{}: pairs are mirror images",
            fx.name
        );
    }
}

/// The rate and channel count come from the SOURCE, not from the
/// project and not from the constants the old shim carried — even when
/// the source disagrees with the project, the case that cannot be
/// checked by value (see [`Fixture::OFF_RATE_SOURCE`]).
#[reaper_test(isolated)]
async fn peaks_report_the_sources_own_rate_and_channels(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let fx = Fixture::OFF_RATE_SOURCE;
    let wav = fx.write_wav("off-rate");
    let got = reaper_peaks(ctx, &fx, &wav).await;
    let _ = std::fs::remove_file(&wav);
    let got = got?;

    assert_eq!(got.sample_rate, 44_100.0, "source rate, not 44.1k by luck");
    assert_eq!(got.num_channels, 1, "mono source, not the old hardcoded 2");
    assert_eq!(got.samples_per_peak, fx.block);
    assert_eq!(
        got.peaks.len(),
        fx.blocks * 2,
        "one (min, max) pair per channel per block"
    );
    assert!(
        got.peaks.chunks(2).all(|p| p[0] <= 0.0 && p[1] >= 0.0),
        "a pair is not (min ≤ 0, max ≥ 0)"
    );
    assert!(
        got.peaks.iter().any(|p| p.abs() > 0.25),
        "real peaks, not silence"
    );
    Ok(())
}
