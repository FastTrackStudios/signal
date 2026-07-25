//! Verify AIFF decode for Stylus-RMX-style 16-bit stereo files.
//!
//! Builds a hand-rolled minimal AIFF in memory: 4 stereo frames at 44.1k,
//! 16-bit big-endian PCM. Exercises the FORM/COMM/SSND parser added for
//! groove packing.

use signal_sampler::engine::cache::load_sample;
use std::io::Write;

/// Encode a 32-bit unsigned int as 80-bit IEEE-754 extended (big-endian).
/// Sufficient for sample-rate fields like 44100.
fn ext80_from_u32(n: u32) -> [u8; 10] {
    if n == 0 {
        return [0; 10];
    }
    let mut mant = n as u64;
    let mut exp = 63i32;
    while mant & (1u64 << 63) == 0 {
        mant <<= 1;
        exp -= 1;
    }
    let biased = (exp + 16383) as u16;
    let mut out = [0u8; 10];
    out[0] = (biased >> 8) as u8;
    out[1] = biased as u8;
    out[2..10].copy_from_slice(&mant.to_be_bytes());
    out
}

#[test]
fn decode_minimal_16bit_stereo_aiff() {
    // 4 stereo frames: L/R interleaved, big-endian i16.
    let frames: [[i16; 2]; 4] = [[0, 0], [16384, -16384], [-32768, 32767], [1, -1]];

    // SSND payload: 8-byte (offset/blockSize zero) + raw BE PCM.
    let mut ssnd_data = Vec::new();
    ssnd_data.extend_from_slice(&0u32.to_be_bytes()); // offset
    ssnd_data.extend_from_slice(&0u32.to_be_bytes()); // blockSize
    for [l, r] in &frames {
        ssnd_data.extend_from_slice(&l.to_be_bytes());
        ssnd_data.extend_from_slice(&r.to_be_bytes());
    }

    // COMM: channels(2), numFrames(4), bits(16), sampleRate(80-bit ext).
    let mut comm = Vec::new();
    comm.extend_from_slice(&2u16.to_be_bytes());
    comm.extend_from_slice(&(frames.len() as u32).to_be_bytes());
    comm.extend_from_slice(&16u16.to_be_bytes());
    comm.extend_from_slice(&ext80_from_u32(44100));

    let mut body = Vec::new();
    body.extend_from_slice(b"AIFF");
    body.extend_from_slice(b"COMM");
    body.extend_from_slice(&(comm.len() as u32).to_be_bytes());
    body.extend_from_slice(&comm);
    body.extend_from_slice(b"SSND");
    body.extend_from_slice(&(ssnd_data.len() as u32).to_be_bytes());
    body.extend_from_slice(&ssnd_data);

    let mut file = Vec::new();
    file.extend_from_slice(b"FORM");
    file.extend_from_slice(&(body.len() as u32).to_be_bytes());
    file.extend_from_slice(&body);

    let dir = std::env::temp_dir().join("signal-sampler-aiff-test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("smoke.aif");
    std::fs::File::create(&path)
        .unwrap()
        .write_all(&file)
        .unwrap();

    let data = load_sample(&path).expect("decode AIFF");
    assert_eq!(data.channels, 2);
    assert_eq!(data.sample_rate, 44100);
    assert_eq!(data.num_frames, 4);
    assert_eq!(data.len(), 8);
    let pcm = data.to_f32();
    assert!((pcm[0]).abs() < 1e-6);
    assert!((pcm[2] - 0.5).abs() < 1e-3);
    assert!((pcm[3] + 0.5).abs() < 1e-3);
    assert!((pcm[4] + 1.0).abs() < 1e-3);
}
