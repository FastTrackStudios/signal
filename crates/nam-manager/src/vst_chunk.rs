use crate::NamError;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

/// Decoded NAM VST3 plugin state.
///
/// The NAM plugin stores its state as a binary chunk with length-prefixed strings.
/// Layout: header(0x48 bytes) → plugin_id → version → model_path → ir_path → param doubles
#[derive(Debug, Clone)]
pub struct NamVstChunk {
    /// Raw header bytes (preserved verbatim)
    pub header: Vec<u8>,
    /// Plugin identifier string
    pub plugin_id: String,
    /// Plugin version string
    pub version: String,
    /// Absolute path to the .nam model file
    pub model_path: String,
    /// Absolute path to the .wav IR file
    pub ir_path: String,
    /// Trailing parameter data (preserved verbatim)
    pub tail: Vec<u8>,
}

const HEADER_SIZE: usize = 0x48;

/// Decode a NAM VST3 state chunk from base64.
pub fn decode_chunk(base64_data: &str) -> Result<NamVstChunk, NamError> {
    let data = BASE64
        .decode(base64_data.trim())
        .map_err(|e| NamError::Parse(format!("base64 decode: {}", e)))?;

    if data.len() < HEADER_SIZE {
        return Err(NamError::Parse(format!(
            "chunk too short: {} bytes (need at least {})",
            data.len(),
            HEADER_SIZE
        )));
    }

    let header = data[..HEADER_SIZE].to_vec();
    let mut cursor = HEADER_SIZE;

    let plugin_id = read_length_prefixed_string(&data, &mut cursor)?;
    let version = read_length_prefixed_string(&data, &mut cursor)?;
    let model_path = read_length_prefixed_string(&data, &mut cursor)?;
    let ir_path = read_length_prefixed_string(&data, &mut cursor)?;

    let tail = data[cursor..].to_vec();

    Ok(NamVstChunk {
        header,
        plugin_id,
        version,
        model_path,
        ir_path,
        tail,
    })
}

/// Encode a NAM VST3 state chunk back to base64.
pub fn encode_chunk(chunk: &NamVstChunk) -> String {
    let mut data = Vec::with_capacity(
        HEADER_SIZE + 16 + chunk.plugin_id.len() + chunk.version.len()
            + chunk.model_path.len() + chunk.ir_path.len() + chunk.tail.len(),
    );

    data.extend_from_slice(&chunk.header);
    write_length_prefixed_string(&mut data, &chunk.plugin_id);
    write_length_prefixed_string(&mut data, &chunk.version);
    write_length_prefixed_string(&mut data, &chunk.model_path);
    write_length_prefixed_string(&mut data, &chunk.ir_path);
    data.extend_from_slice(&chunk.tail);

    BASE64.encode(&data)
}

/// Rewrite the model and/or IR paths in a VST chunk.
pub fn rewrite_paths(
    chunk: &mut NamVstChunk,
    new_model_path: Option<&str>,
    new_ir_path: Option<&str>,
) {
    if let Some(p) = new_model_path {
        chunk.model_path = p.to_string();
    }
    if let Some(p) = new_ir_path {
        chunk.ir_path = p.to_string();
    }
}

/// Read a length-prefixed string: [4-byte LE length][UTF-8 bytes]
fn read_length_prefixed_string(data: &[u8], cursor: &mut usize) -> Result<String, NamError> {
    if *cursor + 4 > data.len() {
        return Err(NamError::Parse(format!(
            "unexpected end of chunk at offset {} reading string length",
            cursor
        )));
    }

    let len = u32::from_le_bytes([
        data[*cursor],
        data[*cursor + 1],
        data[*cursor + 2],
        data[*cursor + 3],
    ]) as usize;
    *cursor += 4;

    if *cursor + len > data.len() {
        return Err(NamError::Parse(format!(
            "string length {} exceeds chunk size at offset {}",
            len,
            *cursor - 4
        )));
    }

    let s = String::from_utf8_lossy(&data[*cursor..*cursor + len]).to_string();
    *cursor += len;
    Ok(s)
}

/// Write a length-prefixed string: [4-byte LE length][UTF-8 bytes]
fn write_length_prefixed_string(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_chunk(model_path: &str, ir_path: &str) -> Vec<u8> {
        let mut data = vec![0u8; HEADER_SIZE];
        // Mark header with recognizable bytes
        data[0] = 0xAB;
        data[1] = 0xCD;

        write_length_prefixed_string(&mut data, "NeuralAmpModeler");
        write_length_prefixed_string(&mut data, "0.7.8");
        write_length_prefixed_string(&mut data, model_path);
        write_length_prefixed_string(&mut data, ir_path);
        // Some trailing param data
        data.extend_from_slice(&42.0f64.to_le_bytes());
        data.extend_from_slice(&0.5f64.to_le_bytes());
        data
    }

    #[test]
    fn round_trip_decode_encode() {
        let model = "/Users/me/NAM/amp.nam";
        let ir = "/Users/me/NAM/cab.wav";
        let raw = make_test_chunk(model, ir);
        let b64 = BASE64.encode(&raw);

        let chunk = decode_chunk(&b64).unwrap();
        assert_eq!(chunk.plugin_id, "NeuralAmpModeler");
        assert_eq!(chunk.version, "0.7.8");
        assert_eq!(chunk.model_path, model);
        assert_eq!(chunk.ir_path, ir);
        assert_eq!(chunk.header[0], 0xAB);
        assert_eq!(chunk.header[1], 0xCD);

        // Re-encode and verify round-trip
        let re_encoded = encode_chunk(&chunk);
        let re_decoded = decode_chunk(&re_encoded).unwrap();
        assert_eq!(re_decoded.model_path, model);
        assert_eq!(re_decoded.ir_path, ir);
        assert_eq!(re_decoded.tail, chunk.tail);
    }

    #[test]
    fn rewrite_paths_works() {
        let raw = make_test_chunk("/old/model.nam", "/old/ir.wav");
        let b64 = BASE64.encode(&raw);

        let mut chunk = decode_chunk(&b64).unwrap();
        rewrite_paths(
            &mut chunk,
            Some("/new/path/model.nam"),
            Some("/new/path/ir.wav"),
        );

        assert_eq!(chunk.model_path, "/new/path/model.nam");
        assert_eq!(chunk.ir_path, "/new/path/ir.wav");

        // Verify it survives encoding
        let b64_new = encode_chunk(&chunk);
        let chunk2 = decode_chunk(&b64_new).unwrap();
        assert_eq!(chunk2.model_path, "/new/path/model.nam");
        assert_eq!(chunk2.ir_path, "/new/path/ir.wav");
    }

    #[test]
    fn partial_rewrite() {
        let raw = make_test_chunk("/old/model.nam", "/old/ir.wav");
        let b64 = BASE64.encode(&raw);

        let mut chunk = decode_chunk(&b64).unwrap();
        rewrite_paths(&mut chunk, Some("/new/model.nam"), None);
        assert_eq!(chunk.model_path, "/new/model.nam");
        assert_eq!(chunk.ir_path, "/old/ir.wav"); // unchanged
    }
}
