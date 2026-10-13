//! `impl Health for Reaper` — sync trait + REAPER's console-msg helper.
//!
//! Mounting goes through `daw_proto::health::serve(Reaper)`. The
//! architect::rpc bridge hops calls onto REAPER's main thread via
//! `HasDispatcher`. Bodies assume main-thread execution.

use daw_proto::Health;

impl Health for crate::Reaper {
    fn ping(&self) -> bool {
        true
    }

    fn show_console_msg(&self, msg: &str) {
        let reaper = reaper_high::Reaper::get();
        reaper.show_console_msg(msg);
    }
}
