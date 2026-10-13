//! Safe helpers for REAPER APIs that write into caller-provided byte buffers.

/// Call an FFI function that writes a NUL-terminated string into a fixed-size
/// buffer. Returns `None` if the callback returns `false` (indicating failure).
///
/// The callback receives `(buf_ptr: *mut i8, buf_len: i32)` and should return
/// `true` on success.
pub fn with_string_buffer<F>(size: usize, f: F) -> Option<String>
where
    F: FnOnce(*mut i8, i32) -> bool,
{
    let mut buf = vec![0u8; size];
    if !f(buf.as_mut_ptr() as *mut i8, size as i32) {
        return None;
    }
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    Some(String::from_utf8_lossy(&buf[..len]).into_owned())
}

/// Like [`with_string_buffer`] but uses the integer return value as the
/// success indicator (> 0 means success).
pub fn with_string_buffer_i32<F>(size: usize, f: F) -> Option<String>
where
    F: FnOnce(*mut i8, i32) -> i32,
{
    let mut buf = vec![0u8; size];
    let result = f(buf.as_mut_ptr() as *mut i8, size as i32);
    if result <= 0 {
        return None;
    }
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    Some(String::from_utf8_lossy(&buf[..len]).into_owned())
}

/// Like [`with_string_buffer_i32`], but grows the buffer and retries when the
/// value may not have fitted.
///
/// REAPER's `...OutNeedBig` APIs give the caller no "would not fit" signal: a
/// value longer than the buffer is written truncated and reports success like
/// any other. The only observable symptom is that the result fills the buffer
/// exactly, so that is the retry condition. A value that happens to be exactly
/// `size - 1` bytes costs one redundant call and returns the same answer.
///
/// Returns `None` at `max` rather than a truncated string: for a caller parsing
/// the result, silently-short data is worse than no data.
pub fn with_growing_string_buffer_i32<F>(initial: usize, max: usize, mut f: F) -> Option<String>
where
    F: FnMut(*mut i8, i32) -> i32,
{
    let mut size = initial.clamp(1, max);
    loop {
        let got = with_string_buffer_i32(size, &mut f)?;
        // A result shorter than the buffer cannot have been truncated.
        if got.len() + 1 < size {
            return Some(got);
        }
        if size >= max {
            return None;
        }
        size = (size * 2).min(max);
    }
}

/// Read a NUL-terminated string from a raw byte buffer (e.g. stack-allocated).
pub fn string_from_buffer(buf: &[u8]) -> String {
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..len]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// Stand-in for a REAPER `...OutNeedBig` call: writes as much of `value` as
    /// fits, NUL-terminates, and reports success either way — which is exactly
    /// the behaviour that makes truncation invisible.
    fn filler<'a>(value: &'a str, calls: &'a Cell<usize>) -> impl FnMut(*mut i8, i32) -> i32 + 'a {
        move |buf, len| {
            calls.set(calls.get() + 1);
            let cap = (len as usize).saturating_sub(1);
            let n = value.len().min(cap);
            unsafe {
                std::ptr::copy_nonoverlapping(value.as_ptr(), buf as *mut u8, n);
                *(buf as *mut u8).add(n) = 0;
            }
            1
        }
    }

    #[test]
    fn value_shorter_than_buffer_returns_in_one_call() {
        let calls = Cell::new(0);
        let got = with_growing_string_buffer_i32(64, 4096, filler("hello", &calls));
        assert_eq!(got.as_deref(), Some("hello"));
        assert_eq!(calls.get(), 1);
    }

    /// The regression this helper exists for: the old fixed-4096 read returned a
    /// truncated string and no error.
    #[test]
    fn value_longer_than_initial_buffer_is_not_truncated() {
        let value = "x".repeat(10_000);
        let calls = Cell::new(0);
        let got = with_growing_string_buffer_i32(4096, 1 << 20, filler(&value, &calls));
        assert_eq!(got.as_deref(), Some(value.as_str()));
        assert!(calls.get() > 1, "should have had to grow");
    }

    #[test]
    fn value_exactly_filling_the_buffer_still_comes_back_whole() {
        // len == size - 1 is indistinguishable from truncation, so it costs a
        // retry — but the answer must be right.
        let value = "y".repeat(63);
        let calls = Cell::new(0);
        let got = with_growing_string_buffer_i32(64, 4096, filler(&value, &calls));
        assert_eq!(got.as_deref(), Some(value.as_str()));
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn gives_up_with_none_rather_than_a_short_string() {
        let value = "z".repeat(10_000);
        let calls = Cell::new(0);
        assert_eq!(
            with_growing_string_buffer_i32(64, 256, filler(&value, &calls)),
            None
        );
    }

    #[test]
    fn failure_from_the_callee_is_still_none() {
        assert_eq!(with_growing_string_buffer_i32(64, 4096, |_, _| 0), None);
    }
}
