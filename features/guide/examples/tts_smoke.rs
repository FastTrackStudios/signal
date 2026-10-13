//! Isolated Chatterbox TTS smoke test — model download + ort load + synth.
//! Run: `nix develop -c cargo run -p session-guide --features tts --example tts_smoke`
//! (needs `ORT_DYLIB_PATH` from the flake and `~/.config/fts/tts-voice.wav`)
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let home = std::env::var("HOME")?;
    let voice = format!("{home}/.config/fts/tts-voice.wav");
    eprintln!(
        "[smoke] ORT_DYLIB_PATH={:?}",
        std::env::var("ORT_DYLIB_PATH").ok()
    );
    eprintln!(
        "[smoke] voice={voice} exists={}",
        std::path::Path::new(&voice).exists()
    );
    eprintln!("[smoke] loading Chatterbox (downloads ONNX model from HF)…");
    let cfg = session_guide::ChatterboxTtsConfig::with_voice(&voice, "fts");
    let mut tts = session_guide::ChatterboxTts::load(cfg)?;
    eprintln!(
        "[smoke] model loaded; voice_id={}",
        session_guide::TtsRenderer::voice_id(&tts)
    );
    eprintln!("[smoke] synthesizing 'Chorus'…");
    let audio = session_guide::TtsRenderer::render(&mut tts, "Chorus")?;
    eprintln!(
        "[smoke] OK: {} samples @ {} Hz",
        audio.samples.len(),
        audio.sample_rate
    );
    Ok(())
}
