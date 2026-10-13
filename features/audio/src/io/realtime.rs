//! Realtime scheduling for the audio threads.
//!
//! An audio callback has a deadline every block (2.7 ms at 128 frames,
//! 48 kHz). Scheduled like any other thread (`SCHED_OTHER`), it is
//! pre-empted whenever the machine is busy — a build, a browser — and a
//! few milliseconds off the CPU mid-block is a click or a dropout however
//! fast the render is. DAWs and PipeWire's own data loop run `SCHED_FIFO`;
//! the stream threads cpal makes for us (`pw_out`, `pw_in`) did not.

/// The `SCHED_FIFO` priority the audio threads ask for: above every
/// ordinary thread, below PipeWire's data loop and a PTP daemon (80+),
/// which must never wait on us.
pub const AUDIO_PRIORITY: i32 = 60;

/// Make the calling thread realtime (`SCHED_FIFO` at [`AUDIO_PRIORITY`],
/// reset on fork). Call it from the audio callback's own thread — once,
/// on its first block. `true` when it took; refused (no `RLIMIT_RTPRIO`,
/// no `audio` group), it says so once and the thread stays as it was.
#[must_use]
pub fn promote_current_thread() -> bool {
    #[cfg(target_os = "linux")]
    {
        let param = libc::sched_param {
            sched_priority: AUDIO_PRIORITY,
        };
        // SAFETY: pid 0 is the calling thread; `param` outlives the call,
        // which only reads it.
        let result = unsafe {
            libc::sched_setscheduler(
                0,
                libc::SCHED_FIFO | libc::SCHED_RESET_ON_FORK,
                &raw const param,
            )
        };
        if result == 0 {
            tracing::info!(
                priority = AUDIO_PRIORITY,
                "daw-audio-io: audio thread is realtime"
            );
            true
        } else {
            let error = std::io::Error::last_os_error();
            tracing::warn!(%error, "daw-audio-io: audio thread could not go realtime; it may be pre-empted (clicks under load)");
            false
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    /// A thread that asks goes realtime where the user may (the studio
    /// machine's `audio` group); where it may not, it says so and carries
    /// on — never a panic.
    #[test]
    fn a_thread_goes_realtime_or_says_why_not() {
        let took = std::thread::spawn(|| {
            let took = super::promote_current_thread();
            let policy = unsafe { libc::sched_getscheduler(0) };
            (took, policy)
        })
        .join()
        .expect("the thread");
        match took {
            (true, policy) => assert_eq!(policy & !libc::SCHED_RESET_ON_FORK, libc::SCHED_FIFO),
            (false, policy) => assert_eq!(policy, libc::SCHED_OTHER),
        }
    }
}
