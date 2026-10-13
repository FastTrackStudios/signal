//! Web MIDI input backend for `midicore` — **one `MIDIAccess`**, a
//! `midimessage` listener on every selected input.
//!
//! # Access is asynchronous
//!
//! `navigator.requestMIDIAccess()` is a promise, and the first call may show
//! the user a permission prompt. So [`WebMidiInput::open`] never waits: it
//! returns at once, and connects its sources when access arrives. Call
//! [`request_access`] up front (from a user gesture, ideally) to prompt early
//! and to learn whether permission was refused — without it,
//! [`InputBackend::sources`] is empty, because nothing can be enumerated
//! before access is granted.
//!
//! # Listeners, not `onmidimessage`
//!
//! Every handler is attached with `addEventListener`, never by assigning
//! `onmidimessage` / `onstatechange`. Those are single slots on a shared
//! browser object, and assigning one would silently disconnect any other code
//! on the page listening to the same keyboard.
//!
//! # Hot-plug
//!
//! The access object fires `statechange` whenever a port appears, disappears
//! or changes state; each one triggers a reconcile against the live input map.
#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use js_sys::Array;
use midicore_proto::{
    decode_all, BackendError, Direction, InputBackend, InputConfig, MaybeSend, PortId, PortInfo,
    PortSelector, TimedEvent,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{MidiAccess, MidiMessageEvent, MidiPortDeviceState};

thread_local! {
    /// The page's MIDI access, once granted. One per page, shared by every
    /// input and by [`InputBackend::sources`].
    static ACCESS: RefCell<Option<MidiAccess>> = const { RefCell::new(None) };
}

fn access() -> Option<MidiAccess> {
    ACCESS.with(|a| a.borrow().clone())
}

/// Ask the browser for MIDI access (prompting if it has to) and keep it for
/// the page's lifetime. Cheap once granted.
///
/// # Errors
///
/// When the browser has no Web MIDI, or the user or a policy refused it —
/// worth telling the user, because the alternative is a silent "no devices".
pub async fn request_access() -> Result<(), BackendError> {
    if access().is_some() {
        return Ok(());
    }
    let window = web_sys::window().ok_or_else(|| BackendError("no window".into()))?;
    let promise = window
        .navigator()
        .request_midi_access()
        .map_err(|_| BackendError("Web MIDI is not available in this browser".into()))?;
    let granted = JsFuture::from(promise)
        .await
        .map_err(|_| BackendError("MIDI permission was refused".into()))?;
    let granted = granted
        .dyn_into::<MidiAccess>()
        .map_err(|_| BackendError("Web MIDI is not available in this browser".into()))?;
    ACCESS.with(|a| *a.borrow_mut() = Some(granted));
    Ok(())
}

/// Every connected input in the access's live map: port id → input.
fn inputs(access: &MidiAccess) -> BTreeMap<String, web_sys::MidiInput> {
    let mut out = BTreeMap::new();
    if let Ok(Some(iter)) = js_sys::try_iter(&access.inputs()) {
        for entry in iter.flatten() {
            // Map iteration yields [key, value] pairs.
            let pair: Array = entry.into();
            let Ok(input) = pair.get(1).dyn_into::<web_sys::MidiInput>() else {
                continue;
            };
            if input.state() == MidiPortDeviceState::Connected {
                out.insert(input.id(), input);
            }
        }
    }
    out
}

fn name_of(input: &web_sys::MidiInput) -> String {
    input.name().unwrap_or_else(|| "MIDI input".into())
}

fn source_info(name: String) -> PortInfo {
    PortInfo {
        id: PortId(name.clone()),
        name,
        direction: Direction::Input,
        virtual_port: false,
    }
}

type MessageListener = Closure<dyn FnMut(MidiMessageEvent)>;
/// The access we listen on for hot-plug, and the listener to remove on drop.
type Hotplug = (MidiAccess, Closure<dyn FnMut(web_sys::Event)>);

struct State {
    selectors: Vec<PortSelector>,
    sink: Rc<dyn Fn(TimedEvent)>,
    /// Inputs we listen to: port id → (input, name, listener).
    linked: BTreeMap<String, (web_sys::MidiInput, String, MessageListener)>,
    /// The `statechange` listener, once access exists.
    hotplug: Option<Hotplug>,
    closed: bool,
}

/// An open Web MIDI input. Drop to remove every listener it attached.
pub struct WebMidiInput {
    state: Rc<RefCell<State>>,
}

impl InputBackend for WebMidiInput {
    const NAME: &'static str = "webmidi";

    fn sources() -> Vec<PortInfo> {
        let Some(access) = access() else {
            return Vec::new();
        };
        let mut names: Vec<String> = inputs(&access).values().map(name_of).collect();
        names.sort();
        names.into_iter().map(source_info).collect()
    }

    fn open<F>(config: InputConfig, sink: F) -> Result<Self, BackendError>
    where
        F: Fn(TimedEvent) + MaybeSend + 'static,
    {
        let state = Rc::new(RefCell::new(State {
            selectors: config.selectors,
            sink: Rc::new(sink),
            linked: BTreeMap::new(),
            hotplug: None,
            closed: false,
        }));
        match access() {
            Some(access) => attach(&state, access),
            None => {
                let weak = Rc::downgrade(&state);
                wasm_bindgen_futures::spawn_local(async move {
                    if let Err(e) = request_access().await {
                        tracing::warn!("midicore-wasm: {e}");
                        return;
                    }
                    // The input may have been dropped while the prompt was up.
                    if let (Some(state), Some(access)) = (weak.upgrade(), access()) {
                        if !state.borrow().closed {
                            attach(&state, access);
                        }
                    }
                });
            }
        }
        Ok(Self { state })
    }

    fn select(&self, selectors: Vec<PortSelector>) {
        let changed = {
            let mut s = self.state.borrow_mut();
            let changed = s.selectors != selectors;
            s.selectors = selectors;
            changed
        };
        if changed {
            if let Some(access) = access() {
                reconcile(&self.state, &access);
            }
        }
    }

    fn connected(&self) -> Vec<PortInfo> {
        let mut names: Vec<String> = self
            .state
            .borrow()
            .linked
            .values()
            .map(|(_, name, _)| name.clone())
            .collect();
        names.sort();
        names.into_iter().map(source_info).collect()
    }
}

impl Drop for WebMidiInput {
    fn drop(&mut self) {
        let mut s = self.state.borrow_mut();
        s.closed = true;
        for (input, _, listener) in std::mem::take(&mut s.linked).into_values() {
            let _ = input.remove_event_listener_with_callback(
                "midimessage",
                listener.as_ref().unchecked_ref(),
            );
        }
        if let Some((access, listener)) = s.hotplug.take() {
            let _ = access.remove_event_listener_with_callback(
                "statechange",
                listener.as_ref().unchecked_ref(),
            );
        }
    }
}

/// Access just became available: listen for hot-plug, then connect.
fn attach(state: &Rc<RefCell<State>>, granted: MidiAccess) {
    let weak: Weak<RefCell<State>> = Rc::downgrade(state);
    let on_change = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        if let (Some(state), Some(access)) = (weak.upgrade(), access()) {
            reconcile(&state, &access);
        }
    });
    let _ =
        granted.add_event_listener_with_callback("statechange", on_change.as_ref().unchecked_ref());
    state.borrow_mut().hotplug = Some((granted.clone(), on_change));
    reconcile(state, &granted);
}

/// Listen to what the selectors want; stop listening to the rest.
fn reconcile(state: &Rc<RefCell<State>>, access: &MidiAccess) {
    let mut s = state.borrow_mut();
    if s.closed {
        return;
    }
    let wanted: BTreeMap<String, (web_sys::MidiInput, String)> = inputs(access)
        .into_iter()
        .map(|(id, input)| {
            let name = name_of(&input);
            (id, (input, name))
        })
        .filter(|(_, (_, name))| s.selectors.iter().any(|sel| sel.matches(name)))
        .collect();

    s.linked.retain(|id, (input, name, listener)| {
        if wanted.contains_key(id) {
            return true;
        }
        let _ = input
            .remove_event_listener_with_callback("midimessage", listener.as_ref().unchecked_ref());
        tracing::info!(midi.port = %name, "midicore-wasm: disconnected");
        false
    });

    for (id, (input, name)) in wanted {
        if s.linked.contains_key(&id) {
            continue;
        }
        let sink = s.sink.clone();
        let listener = MessageListener::new(move |ev: MidiMessageEvent| {
            let Ok(data) = ev.data() else { return };
            // DOMHighResTimeStamp is milliseconds with a sub-ms fraction.
            let timestamp_us = (ev.time_stamp() * 1000.0) as u64;
            decode_all(&data, |event| {
                sink(TimedEvent {
                    timestamp_us,
                    event,
                });
            });
        });
        if input
            .add_event_listener_with_callback("midimessage", listener.as_ref().unchecked_ref())
            .is_ok()
        {
            tracing::info!(midi.port = %name, "midicore-wasm: connected");
            s.linked.insert(id, (input, name, listener));
        }
    }
}
