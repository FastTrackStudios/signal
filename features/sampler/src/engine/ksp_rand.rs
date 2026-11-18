//! Kontakt's KSP `random()`, for scripts we reproduce draw for draw.
//!
//! Kontakt 8.7.1 (`BScriptParser::EvalExpression`) builds a draw from three
//! C `rand()` calls, `((rand() << 30) | (rand() << 15) | rand()) &
//! 0x7FFFFFFF`, and takes `v % (max − min + 1) + min`. On macOS `rand()` is
//! Park–Miller, `x = 16807·x mod (2³¹ − 1)`. Kontakt seeds it with
//! `srand(time(NULL))`; reference renders pin it with the KSP Seed CLAP at
//! transport start (features/rigs/orchestra/tests/ksp_seed/), and KONTRA
//! (`local/inspect-vars` d3f7e1b) implements the same generator from
//! `KONTRA_KSP_SEED`. With the same seed all three draw the same numbers.

/// macOS / FreeBSD `rand()`: Park–Miller "minimal standard" by Schrage's
/// method; returns the new state. A zero state is first replaced by
/// 123459876, as libc does.
pub(crate) fn libc_rand(state: &mut u32) -> i32 {
    let mut ctx = i64::from(*state);
    if ctx == 0 {
        ctx = 123_459_876;
    }
    let (hi, lo) = (ctx / 127_773, ctx % 127_773);
    let mut x = 16_807 * lo - 2_836 * hi;
    if x < 0 {
        x += 0x7FFF_FFFF;
    }
    *state = x as u32;
    x as i32
}

/// KSP `random(low, high)` (`low <= high`) on `rand()` state `state`.
pub(crate) fn ksp_random(state: &mut u32, low: i32, high: i32) -> i32 {
    let a = libc_rand(state) as u32;
    let b = libc_rand(state) as u32;
    let c = libc_rand(state) as u32;
    let v = ((b << 15) | (a << 30) | c) & 0x7FFF_FFFF;
    let span = high.wrapping_sub(low).wrapping_add(1);
    if span == 0 {
        return low;
    }
    (v as i32).wrapping_rem(span).wrapping_add(low)
}

#[cfg(test)]
mod tests {
    use super::{ksp_random, libc_rand};

    /// voyager's libc (KONTRA's vectors): srand(1) → 16807 282475249
    /// 1622650073 984943658; srand(12345) → 207482415 1790989824
    /// 2035175616 77048696.
    #[test]
    fn libc_rand_is_macos_park_miller() {
        let mut s = 1;
        let got: Vec<i32> = (0..4).map(|_| libc_rand(&mut s)).collect();
        assert_eq!(got, [16_807, 282_475_249, 1_622_650_073, 984_943_658]);
        let mut s = 12_345;
        let got: Vec<i32> = (0..4).map(|_| libc_rand(&mut s)).collect();
        assert_eq!(got, [207_482_415, 1_790_989_824, 2_035_175_616, 77_048_696]);
    }

    #[test]
    fn random_stays_in_range_and_repeats_from_a_seed() {
        let (mut a, mut b) = (777, 777);
        for _ in 0..1000 {
            let x = ksp_random(&mut a, 7000, 9000);
            assert!((7000..=9000).contains(&x));
            assert_eq!(x, ksp_random(&mut b, 7000, 9000));
        }
    }
}
