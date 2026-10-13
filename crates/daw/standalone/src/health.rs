//! `impl Health for Standalone`.

use daw_proto::Health;

use crate::sync::Standalone;

impl Health for Standalone {
    fn ping(&self) -> bool {
        true
    }

    fn show_console_msg(&self, _msg: &str) {}
}
