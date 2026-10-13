//! XOR decryption for Pro Tools session files.
//!
//! Pro Tools files are XOR-encrypted starting at byte offset 0x14 (20).
//! The first 20 bytes are always cleartext and contain the encryption
//! parameters at bytes 0x12 (type) and 0x13 (seed).

use crate::error::{PtError, PtResult};

/// Byte offset where encrypted data begins.
const ENCRYPTED_START: usize = 0x14;

/// Encryption type for Pro Tools 5-9.
const XOR_TYPE_OLD: u8 = 0x01;

/// Encryption type for Pro Tools 10-12.
const XOR_TYPE_NEW: u8 = 0x05;

/// Decrypt a Pro Tools session file in-place.
///
/// Returns the XOR type byte (useful for version detection).
pub fn decrypt(data: &mut [u8]) -> PtResult<u8> {
    if data.len() < ENCRYPTED_START {
        return Err(PtError::FileTooShort(data.len()));
    }

    let xor_type = data[0x12];
    let xor_value = data[0x13];

    let xor_delta = match xor_type {
        XOR_TYPE_OLD => gen_xor_delta(xor_value, 53, false),
        XOR_TYPE_NEW => gen_xor_delta(xor_value, 11, true),
        _ => return Err(PtError::UnsupportedEncryption(xor_type)),
    };

    // Build the 256-byte XOR key table
    let mut xor_key = [0u8; 256];
    for i in 0..256u16 {
        xor_key[i as usize] = (i.wrapping_mul(xor_delta as u16) & 0xFF) as u8;
    }

    // Decrypt from offset 0x14 onward
    for (i, byte) in data.iter_mut().enumerate().skip(ENCRYPTED_START) {
        let xor_index = match xor_type {
            XOR_TYPE_OLD => i & 0xFF,
            XOR_TYPE_NEW => (i >> 12) & 0xFF,
            _ => unreachable!(),
        };
        *byte ^= xor_key[xor_index];
    }

    Ok(xor_type)
}

/// Find the multiplicative inverse to derive the XOR delta.
///
/// Solves: `i * mul ≡ xor_value (mod 256)`
/// If `negative`, negates the result (wrapping).
fn gen_xor_delta(xor_value: u8, mul: u8, negative: bool) -> u8 {
    for i in 0..=255u8 {
        if i.wrapping_mul(mul) == xor_value {
            return if negative {
                (-(i as i16) & 0xFF) as u8
            } else {
                i
            };
        }
    }
    // Fallback — should not happen with valid mul values (53 and 11 are
    // both odd, so they have multiplicative inverses mod 256)
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gen_xor_delta_old_format() {
        // For mul=53, solving i*53 ≡ xor_value (mod 256)
        // Just verify it's the inverse
        for seed in 0..=255u8 {
            let delta = gen_xor_delta(seed, 53, false);
            assert_eq!(delta.wrapping_mul(53), seed);
        }
    }

    #[test]
    fn gen_xor_delta_new_format() {
        // For mul=11, solving i*11 ≡ xor_value (mod 256), then negate
        for seed in 0..=255u8 {
            let delta = gen_xor_delta(seed, 11, true);
            let positive = (-(delta as i16) & 0xFF) as u8;
            assert_eq!(positive.wrapping_mul(11), seed);
        }
    }

    #[test]
    fn decrypt_too_short() {
        let mut data = vec![0u8; 10];
        assert!(matches!(decrypt(&mut data), Err(PtError::FileTooShort(10))));
    }
}
