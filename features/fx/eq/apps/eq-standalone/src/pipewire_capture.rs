//! Native PipeWire capture for the spectrum analyzer.
//!
//! Creates an audio *input* stream with `stream.capture.sink = true`, which
//! tells PipeWire to auto-connect it to the default sink's monitor. The result:
//! the analyzer shows whatever is playing on the system with zero manual routing
//! (this is the mechanism cava uses). Output is never touched.
//!
//! The PipeWire main loop is not `Send`, so the whole client lives on its own
//! thread; only the (Send) analyzer feed + handle are moved in.

use std::sync::Arc;

use eq_ui::params::EqUiState;
use pipewire as pw;
use pw::spa;
use spectrum_analyzer::dsp::{Analyzer, AudioFeed};

/// Spawn the PipeWire capture thread. Returns an error only on setup that can be
/// observed synchronously (feed already taken, thread spawn failure); the actual
/// PipeWire connection happens on the thread and logs its own errors.
pub fn spawn(ui: &Arc<EqUiState>) -> anyhow::Result<()> {
    let feed = ui
        .take_audio_feed()
        .ok_or_else(|| anyhow::anyhow!("analyzer feed already taken"))?;
    let analyzer = ui.analyzer.clone();

    std::thread::Builder::new()
        .name("pw-capture".into())
        .spawn(move || {
            if let Err(e) = run(feed, analyzer) {
                eprintln!("pipewire capture stopped: {e}");
            }
        })?;
    Ok(())
}

struct UserData {
    feed: AudioFeed,
    analyzer: Arc<Analyzer>,
    format: spa::param::audio::AudioInfoRaw,
    mono: Vec<f32>,
}

fn run(feed: AudioFeed, analyzer: Arc<Analyzer>) -> Result<(), pw::Error> {
    pw::init();

    let mainloop = pw::main_loop::MainLoop::new(None)?;
    let context = pw::context::Context::new(&mainloop)?;
    let core = context.connect(None)?;

    // `stream.capture.sink = true` + AUTOCONNECT → capture the default output's
    // monitor automatically.
    let mut props = pw::properties::properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Music",
        *pw::keys::NODE_NAME => "FTS-EQ Analyzer",
    };
    props.insert("stream.capture.sink", "true");

    let stream = pw::stream::Stream::new(&core, "fts-eq-analyzer", props)?;

    let data = UserData {
        feed,
        analyzer,
        format: Default::default(),
        mono: Vec::new(),
    };

    let _listener = stream
        .add_local_listener_with_user_data(data)
        .param_changed(|_, ud, id, param| {
            let Some(param) = param else { return };
            if id != pw::spa::param::ParamType::Format.as_raw() {
                return;
            }
            let Ok((media_type, media_subtype)) = pw::spa::param::format_utils::parse_format(param)
            else {
                return;
            };
            if media_type != spa::param::format::MediaType::Audio
                || media_subtype != spa::param::format::MediaSubtype::Raw
            {
                return;
            }
            if ud.format.parse(param).is_err() {
                return;
            }
            let rate = ud.format.rate();
            if rate > 0 {
                ud.analyzer.set_sample_rate(rate as f32);
            }
            eprintln!(
                "pipewire capture: monitor of default sink — {} ch @ {} Hz",
                ud.format.channels(),
                rate
            );
        })
        .process(|stream, ud| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let datas = buffer.datas_mut();
            if datas.is_empty() {
                return;
            }
            let chans = ud.format.channels().max(1) as usize;
            let stride = chans * std::mem::size_of::<f32>();
            let data0 = &mut datas[0];
            let n_valid = data0.chunk().size() as usize;
            let Some(bytes) = data0.data() else { return };
            let n = n_valid.min(bytes.len());
            let n = n - (n % stride.max(1));

            ud.mono.clear();
            ud.mono.reserve(n / stride.max(1));
            for frame in bytes[..n].chunks_exact(stride) {
                let mut sum = 0.0f32;
                for c in 0..chans {
                    let b = &frame[c * 4..c * 4 + 4];
                    sum += f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                }
                ud.mono.push(sum / chans as f32);
            }
            // No EQ is applied to the captured signal, so pre == post.
            ud.feed.push_pre(&ud.mono);
            ud.feed.push_post(&ud.mono);
        })
        .register()?;

    // Request 32-bit float; let PipeWire choose rate/channels (read back in
    // `param_changed`).
    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(spa::param::audio::AudioFormat::F32LE);
    let obj = pw::spa::pod::Object {
        type_: pw::spa::sys::SPA_TYPE_OBJECT_Format,
        id: pw::spa::sys::SPA_PARAM_EnumFormat,
        properties: audio_info.into(),
    };
    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(obj),
    )
    .map_err(|_| pw::Error::CreationFailed)?
    .0
    .into_inner();
    let mut params = [pw::spa::pod::Pod::from_bytes(&values).ok_or(pw::Error::CreationFailed)?];

    stream.connect(
        spa::utils::Direction::Input,
        None,
        pw::stream::StreamFlags::AUTOCONNECT
            | pw::stream::StreamFlags::MAP_BUFFERS
            | pw::stream::StreamFlags::RT_PROCESS,
        &mut params,
    )?;

    // Blocks for the lifetime of the app.
    mainloop.run();
    Ok(())
}
