//! Integrated REAPER extension runtime helpers.
//!
//! This crate is for extension code loaded directly into REAPER. It builds the
//! DAW service client in-process through `LocalCaller`, so action registration
//! and main-thread REAPER API calls do not go through `daw-bridge`, sockets, or
//! shared memory.

use std::cell::RefCell;
use std::error::Error;
use std::sync::OnceLock;

use crossbeam_channel::{Receiver, Sender};
use daw_control::Daw;
use eyre::{Result, eyre};
use reaper_high::{MainTaskMiddleware, MainThreadTask, Reaper as HighReaper, TaskSupport};
use reaper_low::PluginContext;
use reaper_medium::ReaperSession;
use tracing::{debug, info, warn};

static GLOBAL: OnceLock<Global> = OnceLock::new();

struct Global {
    task_support: TaskSupport,
    task_sender: Sender<MainThreadTask>,
    task_receiver: Receiver<MainThreadTask>,
}

impl Global {
    fn init() {
        GLOBAL.get_or_init(|| {
            let (task_sender, task_receiver) = crossbeam_channel::unbounded();
            Global {
                task_support: TaskSupport::new(task_sender.clone()),
                task_sender,
                task_receiver,
            }
        });
    }

    fn get() -> &'static Self {
        GLOBAL
            .get()
            .expect("daw-extension-runtime global state was not initialized")
    }

    fn task_support() -> &'static TaskSupport {
        &Self::get().task_support
    }

    fn create_task_middleware(&self) -> MainTaskMiddleware {
        MainTaskMiddleware::new(self.task_sender.clone(), self.task_receiver.clone())
    }
}

/// Runtime state owned by one integrated REAPER extension.
pub struct ExtensionRuntime {
    session: RefCell<ReaperSession>,
    tokio_runtime: std::sync::Arc<tokio::runtime::Runtime>,
    task_middleware: RefCell<MainTaskMiddleware>,
}

impl std::fmt::Debug for ExtensionRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtensionRuntime").finish_non_exhaustive()
    }
}

impl ExtensionRuntime {
    /// Initialize REAPER access and create an in-process DAW runtime.
    pub fn new(context: PluginContext) -> Result<Self> {
        match HighReaper::load(context).setup() {
            Ok(_) => {
                info!("REAPER high-level API initialized");
                match HighReaper::get().wake_up() {
                    Ok(()) => info!("REAPER high-level API woke up"),
                    Err(e) => warn!("REAPER high-level API wake_up failed: {e}"),
                }
            }
            Err(_) => debug!("REAPER high-level API already initialized"),
        }

        Global::init();
        daw_reaper::set_task_support(Global::task_support());

        let tokio_runtime = std::sync::Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()?,
        );
        let task_middleware = Global::get().create_task_middleware();
        let session = ReaperSession::load(context);

        Ok(Self {
            session: RefCell::new(session),
            tokio_runtime,
            task_middleware: RefCell::new(task_middleware),
        })
    }

    /// Build a [`daw_module::ModuleContext`] backed by this runtime, for
    /// hosting [`daw_module::DawModule`] implementations.
    pub fn module_context(&self) -> daw_module::ModuleContext {
        daw_module::ModuleContext::new(self.tokio_runtime.clone())
    }

    /// Build an async DAW handle backed by the in-process REAPER dispatcher.
    pub fn build_daw(&self) -> Result<Daw> {
        self.tokio_runtime
            .block_on(daw_reaper::build_extension_daw())
            .map_err(|e| eyre!("{e}"))
    }

    /// A handle to this extension's tokio runtime.
    ///
    /// Needed to *enter* the runtime around code that spawns or sleeps
    /// without being a spawned task itself — a Dioxus panel, whose futures
    /// are polled by dioxus's own scheduler on the main thread. Both
    /// `tokio::task::spawn` and `tokio::time` abort the process when no
    /// runtime is in context, and the subscription machinery under a live
    /// panel does both.
    pub fn handle(&self) -> tokio::runtime::Handle {
        self.tokio_runtime.handle().clone()
    }

    /// Run an async task on this extension's runtime.
    pub fn spawn<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.tokio_runtime.spawn(future);
    }

    /// Drain work scheduled onto REAPER's main thread.
    pub fn process_tasks(&self) {
        self.task_middleware.borrow_mut().run();
    }

    /// Register a REAPER timer callback owned by this extension.
    pub fn add_timer(&self, timer: extern "C" fn()) -> Result<()> {
        self.session
            .borrow_mut()
            .plugin_register_add_timer(timer)
            .map_err(|e| eyre!("register extension timer: {e:?}"))
    }
}

/// An action to register with REAPER through the integrated action registry.
pub struct ActionDef {
    /// REAPER command name (for example, `FTS_SYNC_TOGGLE_LINK`).
    pub command_name: &'static str,
    /// Human-readable description shown in REAPER's action list.
    pub description: &'static str,
    /// Whether this action has an on/off toggle state in REAPER.
    pub toggleable: bool,
}

/// Result of registering actions with REAPER.
pub struct ActionRegistration {
    /// Receiver for action trigger events.
    /// Triggered actions, in the order REAPER fired them.
    ///
    /// A plain in-process channel, not a `vox::Rx`. This used to be one —
    /// left over from when the subscription crossed an RPC boundary — and a
    /// bare `vox::channel()` cannot work here: a vox channel streams values
    /// *within a call*, and `Tx::send` parks until the framework binds the
    /// pair as part of one. With no call to bind them the send never
    /// completed, so the receiver was silent and every action appeared to do
    /// nothing. An extension is in-process; its channel should be too.
    pub rx: tokio::sync::mpsc::UnboundedReceiver<daw_proto::ActionEvent>,
    /// Number of actions successfully registered and confirmed in the action list.
    pub registered: usize,
    /// Number of actions that failed to register or were not found in the action list.
    pub failed: usize,
}

/// Register a set of actions and subscribe to trigger events.
pub async fn register_actions(daw: &Daw, actions: &[ActionDef]) -> Result<ActionRegistration> {
    let registry = daw.action_registry();
    let mut registered = 0usize;
    let mut failed = 0usize;

    // Bridge REAPER's action callback onto the stream callers await —
    // **before** a single action is registered.
    //
    // An action becomes triggerable the moment it is registered, and
    // something may be waiting to trigger it: a startup script, a keybind,
    // a session restoring its panels. Subscribing afterwards leaves a
    // window — 18ms in the case that found this — in which REAPER fires
    // the action, the broadcast has no receivers, and the trigger is
    // dropped. The action then appears to do nothing, exactly once, at the
    // least reproducible moment. Same discipline as any other
    // subscribe-then-seed: open the ear first.
    //
    // `subscribe_actions` retired with the architect::rpc port, and what
    // replaced it here was an immediately-empty `Rx` handed back to keep
    // the struct shape — a channel nothing ever wrote to. Every consumer
    // of this API was therefore waiting forever on an event no code in the
    // tree constructed: actions registered, REAPER fired them, and the
    // handler never ran. It presents as a panel that will not open, with
    // nothing in any log, because nothing failed — the message simply had
    // no sender.
    //
    // The sender exists and always did: `register_action_main_thread`'s
    // callback broadcasts the command name. This forwards that broadcast
    // as the `ActionEvent` the stream promises. In-process only, which is
    // what an extension is.
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<daw_proto::ActionEvent>();
    let mut triggers = daw_reaper::subscribe_action_broadcasts();
    architect::platform::spawn(async move {
        loop {
            match triggers.recv().await {
                Ok(command_name) => {
                    // Unbounded, so a trigger fired before the consumer
                    // starts reading is queued rather than dropped —
                    // REAPER can run an action the moment it is registered,
                    // and a startup script does exactly that.
                    if tx
                        .send(daw_proto::ActionEvent::Triggered { command_name })
                        .is_err()
                    {
                        break;
                    }
                }
                // Lagged: a slow consumer missed triggers. Keep going —
                // dropping the subscription would silently stop every
                // action from that point on, which is worse than losing
                // the ones already missed.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!("action stream lagged, {n} trigger(s) dropped");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    for action in actions {
        let cmd_id = if action.toggleable {
            registry
                .register_toggle(action.command_name, action.description)
                .await
                .map_err(|e| eyre!("register_toggle '{}': {e}", action.command_name))?
        } else {
            registry
                .register(action.command_name, action.description)
                .await
                .map_err(|e| eyre!("register '{}': {e}", action.command_name))?
        };

        if cmd_id == 0 {
            warn!("Failed to register action: {}", action.command_name);
            failed += 1;
            continue;
        }

        let in_list = registry
            .is_in_action_list(action.command_name)
            .await
            .unwrap_or(false);

        if in_list {
            registered += 1;
        } else {
            warn!(
                "Action not in action list after registration: {} (cmd_id={cmd_id})",
                action.command_name
            );
            failed += 1;
        }
    }

    let _ = registry; // suppress unused-binding warning

    Ok(ActionRegistration {
        rx,
        registered,
        failed,
    })
}

/// Convert any extension initialization error into the boxed error type used by
/// `reaper_extension_plugin` entry points.
pub fn boxed_error(error: impl Error + 'static) -> Box<dyn Error> {
    Box::new(error)
}
