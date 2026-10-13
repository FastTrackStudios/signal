//! midir behind the [`InputBackend`] contract — the fallback for platforms
//! with no native backend (Windows today).
//!
//! midir cannot re-select sources on an open connection: its API is one OS
//! connection per source. So this keeps one connection set per distinct
//! selector (collapsing to a single omni set when any selector is `All`) and
//! reopens only when the selectors or the enumerated ports actually changed.
//! There is no registry to push hot-plug, so the owning thread re-reads the
//! port list on a timer — the churn that makes this costly under JACK does
//! not exist on WinMM or CoreMIDI.

use std::sync::mpsc;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use midicore_proto::{
    BackendError, Direction, InputBackend, InputConfig, MaybeSend, PortId, PortInfo, PortSelector,
    TimedEvent,
};

use crate::{input_ports, MidiInput};

/// How often the owning thread re-reads the port list.
pub const RESCAN: Duration = Duration::from_secs(1);

enum Cmd {
    Select(Vec<PortSelector>),
    Quit,
}

/// midir connections re-selected as one input. Drop to close them all.
pub struct SelectableInput {
    tx: mpsc::Sender<Cmd>,
    connected: Arc<RwLock<Vec<String>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

type Sink = Arc<Mutex<dyn Fn(TimedEvent) + Send>>;

/// Omni subsumes every name filter; otherwise one set per distinct selector,
/// because midir's `NameContains` opens only the first match.
fn normalize(selectors: Vec<PortSelector>) -> Vec<PortSelector> {
    if selectors.iter().any(|s| matches!(s, PortSelector::All)) {
        return vec![PortSelector::All];
    }
    let mut unique = Vec::new();
    for s in selectors {
        if !unique.contains(&s) {
            unique.push(s);
        }
    }
    unique
}

fn reopen(selectors: &[PortSelector], sink: &Sink) -> Vec<MidiInput> {
    selectors
        .iter()
        .filter_map(|selector| {
            let sink = Arc::clone(sink);
            let forward = move |ev| {
                (sink
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner))(ev)
            };
            match MidiInput::open(selector.clone(), forward) {
                Ok(conn) => Some(conn),
                // No device yet is normal; the next rescan retries.
                Err(e) => {
                    tracing::debug!(midi.selector = ?selector, "midicore-midir: open skipped: {e}");
                    None
                }
            }
        })
        .collect()
}

impl InputBackend for SelectableInput {
    const NAME: &'static str = "midir";

    fn sources() -> Vec<PortInfo> {
        crate::input_devices()
    }

    fn open<F>(config: InputConfig, sink: F) -> Result<Self, BackendError>
    where
        F: Fn(TimedEvent) + MaybeSend + 'static,
    {
        let sink: Sink = Arc::new(Mutex::new(sink));
        let (tx, rx) = mpsc::channel();
        let connected = Arc::new(RwLock::new(Vec::new()));
        let thread = {
            let connected = connected.clone();
            let mut selectors = normalize(config.selectors);
            std::thread::Builder::new()
                .name("midicore-midir".into())
                .spawn(move || {
                    let mut seen: Option<Vec<String>> = None;
                    let mut conns: Vec<MidiInput> = Vec::new();
                    loop {
                        let now = input_ports();
                        // A transient empty scan is not an unplug.
                        let unplugged_all = now.is_empty() && !conns.is_empty();
                        if !unplugged_all && seen.as_ref() != Some(&now) {
                            conns.clear(); // close before reopening
                            conns = reopen(&selectors, &sink);
                            let mut names: Vec<String> = conns
                                .iter()
                                .flat_map(|c| c.opened.iter().cloned())
                                .collect();
                            names.sort();
                            names.dedup();
                            if let Ok(mut c) = connected.write() {
                                *c = names;
                            }
                            seen = Some(now);
                        }
                        match rx.recv_timeout(RESCAN) {
                            Ok(Cmd::Select(s)) => {
                                let s = normalize(s);
                                if s != selectors {
                                    selectors = s;
                                    seen = None;
                                }
                            }
                            Ok(Cmd::Quit) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                        }
                    }
                })
                .map_err(|e| BackendError(format!("spawn midir thread: {e}")))?
        };
        Ok(Self {
            tx,
            connected,
            thread: Some(thread),
        })
    }

    fn select(&self, selectors: Vec<PortSelector>) {
        let _ = self.tx.send(Cmd::Select(selectors));
    }

    fn connected(&self) -> Vec<PortInfo> {
        self.connected
            .read()
            .map(|c| c.clone())
            .unwrap_or_else(|e| e.into_inner().clone())
            .into_iter()
            .map(|name| PortInfo {
                id: PortId(name.clone()),
                name,
                direction: Direction::Input,
                virtual_port: false,
            })
            .collect()
    }
}

impl Drop for SelectableInput {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::normalize;
    use midicore_proto::PortSelector;

    #[test]
    fn omni_collapses_every_other_selector() {
        let s = normalize(vec![
            PortSelector::NameContains("s88".into()),
            PortSelector::All,
        ]);
        assert_eq!(s, vec![PortSelector::All]);
    }

    #[test]
    fn duplicate_selectors_open_once() {
        let s = normalize(vec![
            PortSelector::NameContains("s88".into()),
            PortSelector::NameContains("mio".into()),
            PortSelector::NameContains("s88".into()),
        ]);
        assert_eq!(s.len(), 2);
    }
}
