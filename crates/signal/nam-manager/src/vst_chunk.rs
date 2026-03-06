use crate::NamError;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

/// Decoded NAM VST3 plugin state.
///
/// The binary layout wraps the NAM plugin state inside a REAPER VST3 state envelope:
///
/// ```text
/// [0x00..0x08]  REAPER plugin hash/magic (8 bytes)
/// [0x08..0x30]  REAPER metadata fields (40 bytes)
/// [0x30..0x34]  total_outer_size (u32 LE) = data.len() - 0x3C
/// [0x34..0x3C]  flags/metadata (8 bytes)
/// [0x3C..0x40]  component_state_size (u32 LE) = size of NAM component state
/// [0x40..0x44]  flags (4 bytes)
/// [0x44..]      NAM component state: plugin_id → version → model_path → ir_path → params
///               followed by controller state tail (typically 8 bytes)
/// ```
///
/// When re-encoding after path rewrite, both size fields (0x30, 0x3C) must be
/// updated to reflect the new data lengths.
#[derive(Debug, Clone)]
pub struct NamVstChunk {
    /// Raw header bytes (preserved verbatim; size fields at 0x30 and 0x3C are patched on encode)
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
    /// Size of controller state at end of tail (computed from header during decode)
    pub controller_tail_size: usize,
}

const HEADER_SIZE: usize = 0x44;

/// Offset of the total outer size field (u32 LE): data.len() - 0x3C.
const OUTER_SIZE_OFFSET: usize = 0x30;

/// Offset of the component state size field (u32 LE).
const COMPONENT_SIZE_OFFSET: usize = 0x3C;

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

    // Read the component state size from header to compute controller tail
    let component_state_size = if data.len() >= COMPONENT_SIZE_OFFSET + 4 {
        u32::from_le_bytes([
            data[COMPONENT_SIZE_OFFSET],
            data[COMPONENT_SIZE_OFFSET + 1],
            data[COMPONENT_SIZE_OFFSET + 2],
            data[COMPONENT_SIZE_OFFSET + 3],
        ]) as usize
    } else {
        0
    };

    let header = data[..HEADER_SIZE].to_vec();
    let mut cursor = HEADER_SIZE;

    let plugin_id = read_length_prefixed_string(&data, &mut cursor)?;
    let version = read_length_prefixed_string(&data, &mut cursor)?;
    let model_path = read_length_prefixed_string(&data, &mut cursor)?;
    let ir_path = read_length_prefixed_string(&data, &mut cursor)?;

    let tail = data[cursor..].to_vec();

    // Controller tail = total data after header minus component state size
    let total_after_header = data.len() - HEADER_SIZE;
    let controller_tail_size = if component_state_size > 0 && total_after_header > component_state_size {
        total_after_header - component_state_size
    } else {
        0
    };

    Ok(NamVstChunk {
        header,
        plugin_id,
        version,
        model_path,
        ir_path,
        tail,
        controller_tail_size,
    })
}

/// Encode a NAM VST3 state chunk back to base64.
///
/// Patches both size fields in the REAPER VST3 header:
/// - Offset 0x30: total outer size = data.len() - 0x3C
/// - Offset 0x3C: component state size = (data after header) - controller_tail_size
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

    // Patch the two size fields in the header
    if data.len() > HEADER_SIZE {
        // Total outer size at 0x30 = everything from 0x3C onward
        let outer_size = (data.len() - (COMPONENT_SIZE_OFFSET)) as u32;
        data[OUTER_SIZE_OFFSET..OUTER_SIZE_OFFSET + 4]
            .copy_from_slice(&outer_size.to_le_bytes());

        // Component state size at 0x3C = data after header minus controller tail
        let total_after_header = data.len() - HEADER_SIZE;
        let component_size = (total_after_header - chunk.controller_tail_size) as u32;
        data[COMPONENT_SIZE_OFFSET..COMPONENT_SIZE_OFFSET + 4]
            .copy_from_slice(&component_size.to_le_bytes());
    }

    BASE64.encode(&data)
}

/// Create a default NAM VST3 chunk with empty paths and default parameters.
///
/// Produces a valid `NamVstChunk` that can be used as a template for
/// generating state data without a running REAPER instance. The header
/// is zero-initialized (size fields are patched on `encode_chunk`), and
/// the tail contains default NAM parameter data (8 bytes of zeros,
/// matching the controller state seen in live captures).
pub fn create_default_chunk() -> NamVstChunk {
    let header = vec![0u8; HEADER_SIZE];

    // Default controller tail: 8 bytes of zeros (matches live REAPER captures).
    // This represents the VST3 controller state that follows the component state.
    let tail = vec![0u8; 8];

    NamVstChunk {
        header,
        plugin_id: "NeuralAmpModeler".to_string(),
        version: "0.7.13".to_string(),
        model_path: String::new(),
        ir_path: String::new(),
        tail,
        controller_tail_size: 8,
    }
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

        // Patch both size fields
        let outer_size = (data.len() - COMPONENT_SIZE_OFFSET) as u32;
        data[OUTER_SIZE_OFFSET..OUTER_SIZE_OFFSET + 4]
            .copy_from_slice(&outer_size.to_le_bytes());
        let component_size = (data.len() - HEADER_SIZE) as u32; // no controller tail in test
        data[COMPONENT_SIZE_OFFSET..COMPONENT_SIZE_OFFSET + 4]
            .copy_from_slice(&component_size.to_le_bytes());

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
