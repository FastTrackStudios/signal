//! Binary ProjectData chunk reader.
//!
//! The `ProjectData` binary format used inside `.logicx` bundles has:
//!
//! - A **24-byte file header** starting with the magic `0x2347` (`#G`).
//! - A sequence of **chunks**, each with a **36-byte header** followed by a
//!   variable-length data payload.
//!
//! ## Chunk header layout (36 bytes)
//!
//! | Offset | Size | Field        | Description                                    |
//! |--------|------|--------------|------------------------------------------------|
//! | 0      | 4    | `tag`        | Chunk type — 4 ASCII bytes, little-endian order |
//! | 4      | 24   | `meta`       | Flags / identifiers (partially understood)      |
//! | 28     | 8    | `data_len`   | Payload byte count, u64 little-endian           |
//!
//! The human-readable chunk type is the tag reversed, e.g. `gnoS` → `Song`.
//!
//! ## Known chunk types
//!
//! | Tag (on-disk) | Human name | Description                                 |
//! |---------------|------------|---------------------------------------------|
//! | `gnoS`        | Song       | Root song object — always first, largest    |
//! | `qeSM`        | MSeq       | MIDI sequence                               |
//! | `karT`        | Trak       | Track                                       |
//! | `qSvE`        | EvSq       | Event sequence (notes, CC, tempo, markers)  |
//! | `tSnI`        | InSt       | Instrument definition                       |
//! | `tSxT`        | TxSt       | Text / score style                          |
//! | `lytS`        | Styl       | Track style (color, icon)                   |
//! | `OCuA`        | AuCO       | Audio control output (automation)           |
//! | `nCuA`        | AuCn       | Audio channel container (channel strip)     |
//! | `ivnE`        | Envi       | Environment / mixer object                  |
//! | `OgnS`        | SngO       | Song object (aux)                           |
//! | `MroC`        | CorM       | Correlation matrix                          |
//! | `rpyH`        | Hypr       | Hyperlinks / object references              |
//! | `qSxT`        | TxSq       | Text sequence                               |
//! | `ryaL`        | Layr       | Layer definition                            |
//! | `tScS`        | ScSt       | Scene state                                 |
//! | `ediV`        | Vide       | Video reference                             |
//! | `UCuA`        | AuCU       | Audio control utility                       |
//! | `MneG`        | GenM       | General MIDI                                |
//! | `vEuA`        | AuEv       | Audio event (playback region / AUv3 state)  |
//! | `snrT`        | Trns       | Global transpose / pitch settings           |

use crate::error::{LogicError, LogicResult};
use crate::types::LogicChunk;

/// Magic bytes at offset 0–1 of the ProjectData file header.
pub const MAGIC: u16 = 0x2347;

/// Size of the file header in bytes.
pub const FILE_HEADER_LEN: usize = 24;

/// Size of each chunk header in bytes.
pub const CHUNK_HEADER_LEN: usize = 36;

/// Parse all chunks from a `ProjectData` binary blob.
///
/// Validates the file magic, then iterates sequentially through every chunk
/// until EOF, returning a flat `Vec<LogicChunk>` in file order.
pub fn parse_chunks(data: &[u8]) -> LogicResult<Vec<LogicChunk>> {
    if data.len() < FILE_HEADER_LEN {
        return Err(LogicError::TooShort { len: data.len() });
    }

    let magic = u16::from_be_bytes([data[0], data[1]]);
    if magic != MAGIC {
        return Err(LogicError::BadMagic { actual: magic });
    }

    let mut chunks = Vec::new();
    let mut offset = FILE_HEADER_LEN;

    while offset < data.len() {
        if offset + CHUNK_HEADER_LEN > data.len() {
            return Err(LogicError::TruncatedChunkHeader { offset });
        }

        let hdr = &data[offset..offset + CHUNK_HEADER_LEN];

        let tag: [u8; 4] = hdr[0..4].try_into().unwrap();
        let mut header_meta = [0u8; 24];
        header_meta.copy_from_slice(&hdr[4..28]);
        let data_len = u64::from_le_bytes(hdr[28..36].try_into().unwrap());

        let payload_start = offset + CHUNK_HEADER_LEN;
        let payload_end = payload_start + data_len as usize;

        if payload_end > data.len() {
            return Err(LogicError::TruncatedChunkData {
                chunk_type: tag_to_type_name(&tag),
                offset,
                size: data_len,
            });
        }

        let payload = data[payload_start..payload_end].to_vec();
        let type_name = tag_to_type_name(&tag);

        chunks.push(LogicChunk {
            offset,
            tag,
            type_name,
            header_meta,
            data_len,
            data: payload,
        });

        offset = payload_end;
    }

    Ok(chunks)
}

/// Reverse a 4-byte tag to its human-readable form (e.g. `gnoS` → `Song`).
fn tag_to_type_name(tag: &[u8; 4]) -> String {
    let reversed = [tag[3], tag[2], tag[1], tag[0]];
    String::from_utf8_lossy(&reversed).into_owned()
}
