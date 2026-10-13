//! Dioxus hook for driving `input::InputProcessor`.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::{
    GlobalSignal, KeyboardEvent, MouseEvent, ReadableExt, Signal, WheelEvent, WritableExt, spawn,
    use_hook, use_signal,
};
use input::{
    ActionContext, InputCommand, InputEvent, InputProcessor, KeymapConfig, ModeId, MouseAction,
};

use crate::convert::{convert_keyboard_event, convert_mouse_event, convert_wheel_event};

/// Platform sleep for the chord-sequence timeout: tokio on native, the
/// browser timer on wasm (tokio's `full` features pull mio's net stack, which
/// wasm doesn't support).
async fn sleep(duration: std::time::Duration) {
    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(duration).await;
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(duration.as_millis() as u32).await;
}

/// Global action context consumed by `use_input_processor`.
pub static ACTION_CONTEXT: GlobalSignal<ActionContext> = Signal::global(ActionContext::new);

/// When `true`, the root keyboard handler should skip action processing
/// because a text input field has focus. Increment on focus, decrement on blur
/// so nested inputs don't clobber each other.
pub static TEXT_INPUT_FOCUS_COUNT: GlobalSignal<u32> = Signal::global(|| 0);

/// Handle returned by `use_input_processor`.
#[derive(Clone)]
pub struct InputHandle {
    processor: Rc<RefCell<InputProcessor>>,
    mode: Signal<ModeId>,
    pending: Signal<Option<String>>,
    recording: Signal<bool>,
    timeout_epoch: Signal<u64>,
}

impl PartialEq for InputHandle {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.processor, &other.processor)
    }
}

impl InputHandle {
    /// Reload the processor with a new config.
    pub fn reload_config(&self, config: KeymapConfig) {
        if self.processor.borrow_mut().reload_config(config).is_ok() {
            self.sync_state();
            self.schedule_timeout_if_needed();
        }
    }

    /// Convert and process a keyboard event.
    pub fn handle_key(&self, e: &KeyboardEvent) -> Vec<InputCommand> {
        let Some(event) = convert_keyboard_event(e) else {
            return Vec::new();
        };
        self.process_event(event)
    }

    /// Convert and process a mouse event.
    pub fn handle_mouse(&self, e: &MouseEvent, action: MouseAction) -> Vec<InputCommand> {
        self.process_event(convert_mouse_event(e, action))
    }

    /// Convert and process a wheel event.
    pub fn handle_wheel(&self, e: &WheelEvent) -> Vec<InputCommand> {
        self.process_event(convert_wheel_event(e))
    }

    /// Reactive current mode.
    pub fn current_mode(&self) -> ModeId {
        (self.mode)()
    }

    /// Reactive pending sequence display.
    pub fn pending_display(&self) -> Option<String> {
        (self.pending)()
    }

    /// Borrow the underlying processor for read-only introspection (keymaps, mouse, scroll).
    pub fn processor(&self) -> std::cell::Ref<'_, InputProcessor> {
        self.processor.borrow()
    }

    /// Reactive macro recording indicator.
    pub fn is_recording(&self) -> bool {
        (self.recording)()
    }

    fn process_event(&self, event: InputEvent) -> Vec<InputCommand> {
        let ctx = ACTION_CONTEXT.read().clone();
        let commands = self.processor.borrow_mut().process(event, &ctx);
        self.sync_state();
        self.schedule_timeout_if_needed();
        commands
    }

    fn sync_state(&self) {
        let processor = self.processor.borrow();
        let mut mode = self.mode;
        mode.set(processor.current_mode().clone());
        let mut pending = self.pending;
        pending.set(processor.pending_display());
        let mut recording = self.recording;
        recording.set(processor.is_recording_macro());
    }

    fn schedule_timeout_if_needed(&self) {
        let (needs_timeout, timeout) = {
            let processor = self.processor.borrow();
            (processor.needs_timeout(), processor.timeout_duration())
        };

        let next_epoch = (self.timeout_epoch)().saturating_add(1);
        let mut timeout_epoch = self.timeout_epoch;
        timeout_epoch.set(next_epoch);

        if !needs_timeout {
            return;
        }

        let processor = self.processor.clone();
        let mode = self.mode;
        let pending = self.pending;
        let recording = self.recording;
        let timeout_epoch = self.timeout_epoch;

        spawn(async move {
            sleep(timeout).await;

            if timeout_epoch() != next_epoch {
                return;
            }

            let _commands = processor.borrow_mut().timeout_expired();

            let proc = processor.borrow();
            let mut mode = mode;
            mode.set(proc.current_mode().clone());
            let mut pending = pending;
            pending.set(proc.pending_display());
            let mut recording = recording;
            recording.set(proc.is_recording_macro());
        });
    }
}

/// Build a processor-backed hook and expose a reactive handle.
pub fn use_input_processor(config: KeymapConfig) -> InputHandle {
    let processor = use_hook(move || {
        let instance = InputProcessor::from_config(config).unwrap_or_default();
        Rc::new(RefCell::new(instance))
    });

    let mode = use_signal(|| processor.borrow().current_mode().clone());
    let pending = use_signal(|| processor.borrow().pending_display());
    let recording = use_signal(|| processor.borrow().is_recording_macro());
    let timeout_epoch = use_signal(|| 0_u64);

    InputHandle {
        processor,
        mode,
        pending,
        recording,
        timeout_epoch,
    }
}
