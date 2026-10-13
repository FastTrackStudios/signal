//! Audio file reference parsing.
//!
//! Extracts the list of WAV/AIF files referenced by the session,
//! including their filenames and sample lengths.

use crate::block::Block;
use crate::content_type::ContentType;
use crate::cursor::Cursor;
use crate::types::AudioFile;

/// Known audio file type codes (4-byte ASCII tags).
const AUDIO_TYPE_CODES: &[&[u8]] = &[b"WAVE", b"EVAW", b"AIFF", b"FFIA"];

/// Parse all audio file references from the block tree.
pub fn parse_audio_files(blocks: &[Block], cursor: &Cursor<'_>, version: u16) -> Vec<AudioFile> {
    let mut files = Vec::new();

    // Find the WAV list block (0x1004)
    let wav_list = find_block_recursive(blocks, ContentType::WavList);
    let wav_list = match wav_list {
        Some(b) => b,
        None => return files,
    };

    // Find the WAV names sub-block (0x103a)
    let wav_names = match wav_list.find_child(ContentType::WavNames) {
        Some(b) => b,
        None => return files,
    };

    // Parse filenames starting at wav_names.offset + 11.
    // Loop through the entire WavNames block (directory entries like "Audio Files"
    // are interspersed and filtered out).
    let wav_names_end = wav_names.offset + wav_names.block_size as usize;
    let mut pos = wav_names.offset + 11;
    let data = cursor.data();
    let mut index: u16 = 0;

    loop {
        if pos + 4 >= data.len() || pos >= wav_names_end {
            break;
        }

        // Read length-prefixed string (bail if length is unreasonable)
        let str_len = cursor.u32_at(pos) as usize;
        if str_len > 1024 || pos + 4 + str_len > data.len() {
            break;
        }
        let (filename, str_consumed) = cursor.length_prefixed_string(pos);
        pos += str_consumed;

        // Read 4-byte type code
        if pos + 4 > data.len() {
            break;
        }
        let type_code = &data[pos..pos + 4];
        pos += 4;

        // Skip 5 bytes after type code
        pos += 5;

        // Filter out non-audio entries
        if filename.contains(".grp")
            || filename.contains("Audio Files")
            || filename.contains("Fade Files")
        {
            continue;
        }

        // Validate type code
        let type_is_null = type_code == b"\0\0\0\0";
        let type_is_audio = AUDIO_TYPE_CODES.contains(&type_code);

        let is_valid = if version < 10 {
            type_is_audio
        } else if type_is_null {
            filename.ends_with(".wav")
                || filename.ends_with(".WAV")
                || filename.ends_with(".aif")
                || filename.ends_with(".AIF")
                || filename.ends_with(".aiff")
                || filename.ends_with(".AIFF")
        } else {
            type_is_audio
        };

        if !is_valid {
            continue;
        }

        files.push(AudioFile {
            filename,
            index,
            length: 0,        // filled in below
            source_uid: None, // filled in below
        });
        index += 1;
    }

    // Now fill in lengths from WavInfo (0x1001) blocks
    let wav_metadata_blocks: Vec<&Block> = wav_list.find_all(ContentType::WavInfo);

    for (i, info_block) in wav_metadata_blocks.iter().enumerate() {
        if i < files.len() {
            // Length is a u64 at block.offset + 8
            if info_block.offset + 16 <= data.len() {
                files[i].length = cursor.u64_at(info_block.offset + 8);
            }
        }
    }

    // File entry UIDs (raw CT 0x1003 — `AudioFileEntry`). Each entry
    // has a 6-byte UID at magic-byte +45..+50, bracketed by sentinels
    // `0x2A` at +44 and `0x80` at +51. Discovered via Frida byte-read
    // trace on the LotF session — see
    // `docs/converter-frida-discovered-offsets.md`.
    fn collect_recursive<'a>(blocks: &'a [Block], ct_raw: u16, out: &mut Vec<&'a Block>) {
        for b in blocks {
            if b.content_type_raw == ct_raw {
                out.push(b);
            }
            collect_recursive(&b.children, ct_raw, out);
        }
    }
    let mut file_entry_blocks: Vec<&Block> = Vec::new();
    collect_recursive(blocks, 0x1003, &mut file_entry_blocks);
    for (i, b) in file_entry_blocks.iter().enumerate() {
        if i >= files.len() {
            break;
        }
        let magic = b.offset.saturating_sub(7);
        let uid_at = magic + 45;
        if uid_at + 6 <= data.len()
            && data.get(magic + 44) == Some(&0x2A)
            && data.get(magic + 51) == Some(&0x80)
        {
            let mut uid = [0u8; 6];
            uid.copy_from_slice(&data[uid_at..uid_at + 6]);
            files[i].source_uid = Some(uid);
        }
    }

    files
}

/// Recursively find a block by content type.
fn find_block_recursive(blocks: &[Block], ct: ContentType) -> Option<&Block> {
    for block in blocks {
        if block.content_type == Some(ct) {
            return Some(block);
        }
        if let Some(found) = find_block_recursive(&block.children, ct) {
            return Some(found);
        }
    }
    None
}
