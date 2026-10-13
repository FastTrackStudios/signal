//! Health service — connection liveness probe.

#[architect::rpc]
pub trait Health {
    /// Returns `true` if the DAW is reachable. Cheapest possible RPC
    /// round-trip — used by fts-control's health-check loop to detect
    /// disconnects faster than process polling.
    fn ping(&self) -> bool;

    /// Show a message in the DAW's console/log window.
    fn show_console_msg(&self, msg: &str);
}
