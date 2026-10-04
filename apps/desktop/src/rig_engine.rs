//! `rig_engine.rs` — the signal engine embedded IN-PROCESS.
//!
//! On iOS the app cannot spawn `fasttrackstudio --engine` as a child
//! process (the `engines.rs` supervisor path), so the rig runs inside the
//! app: `GuitarRigBackend::new()` → its `LayerRouter` served over an
//! in-memory link (`architect::LocalServer`) → the SAME three typed
//! clients (`RigClient`, `RigStreamClient`, `AudioSettingsClient`) every
//! remote uses. Byte-for-byte the `session_engine.rs` pattern applied to
//! the rig — the core cannot tell it isn't a network remote.
//!
//! Desktop builds can use this too (a future "embedded engine" toggle);
//! today it is wired up by the iOS shell (`mobile_view.rs`).

use std::sync::{Arc, OnceLock};

use architect::rig::RigBackend as _;
#[cfg(feature = "signal")]
use signal_account_proto::account::AccountAuthClient;
use signal_guitar::GuitarRigBackend;
use signal_guitar::proto::audio::AudioSettingsClient;
use signal_guitar::proto::rig::{Rig as _, RigClient, RigStreamClient};
#[cfg(feature = "signal-keys-rig")]
use signal_keys_proto::keys::{KeysRigClient, KeysRigStreamClient};
#[cfg(feature = "signal")]
use signal_tone3000_proto::tone3000::{Tone3000Client, Tone3000StreamClient};

/// The embedded rig: the backend + the established in-process clients.
#[derive(Clone)]
pub struct RigEngine {
    pub rig: RigClient,
    pub stream: RigStreamClient,
    pub settings: AudioSettingsClient,
    /// The TONE3000 catalog. `Option` for symmetry with the network path,
    /// where an older engine may not serve it; in-process it is always here.
    #[cfg(feature = "signal")]
    pub tones: Option<Tone3000Client>,
    #[cfg(feature = "signal")]
    pub tones_stream: Option<Tone3000StreamClient>,
    /// The `FastTrackStudio` account (sign in once, TONE3000 — and whatever
    /// else it gathers — works without a second per-machine authorization).
    /// `Option` for the same reason `tones` is: the RPC exists once this
    /// engine is up, but establishing a client is still one more thing that
    /// can fail, and a UI with no account button is the right degradation.
    #[cfg(feature = "signal")]
    pub account: Option<AccountAuthClient>,
    /// The in-process keys rig (sampler engine) — dormant until the keys
    /// view starts it.
    #[cfg(feature = "signal-keys-rig")]
    pub keys: KeysRigClient,
    #[cfg(feature = "signal-keys-rig")]
    pub keys_stream: KeysRigStreamClient,
    /// Keeps the `LocalServer` acceptor tasks alive for the app's lifetime.
    _scope: Arc<architect::Scope>,
}

static ENGINE: OnceLock<RigEngine> = OnceLock::new();

/// The embedded rig engine, if [`bootstrap_blocking`] succeeded.
pub fn engine() -> Option<&'static RigEngine> {
    ENGINE.get()
}

/// Build the rig backend, serve it over a memory link, establish the
/// clients, and open audio. Call once before the UI launches; failure is
/// non-fatal (the rig view shows its offline notice).
///
/// Owns a dedicated leaked multi-thread runtime (the same shape as the
/// session engine): the `LocalServer` acceptors and vox pumps live on it for
/// the process lifetime, independent of the GUI's runtime.
pub fn bootstrap_blocking() -> eyre::Result<()> {
    if ENGINE.get().is_some() {
        return Ok(());
    }

    let runtime = Box::leak(Box::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_stack_size(16 * 1024 * 1024)
            .enable_all()
            .build()?,
    ));

    let engine = runtime.block_on(async {
        // The backend spawns its own OS threads (meter pump, drive
        // calibration) and opens audio off-thread on start().
        let backend = GuitarRigBackend::new();
        let router = backend.router();

        // The keys rig mounts on the same in-process router — one more
        // backend + merge_router, exactly the engine_main.rs wiring. It
        // stays dormant (no audio) until the keys view starts it.
        #[cfg(feature = "signal-keys-rig")]
        let keys_backend = signal_keys::KeysRigBackend::new();
        #[cfg(feature = "signal-keys-rig")]
        let router = router.merge_router(keys_backend.router());

        // TONE3000 and the FastTrackStudio account come with the full
        // `signal` feature: their sign-in lands on a localhost listener,
        // which the phone cannot offer (iOS needs ASWebAuthenticationSession
        // for that), so the iPhone build leaves them out for now.
        #[cfg(feature = "signal")]
        let (router, account, tone3000) = {
            // TONE3000 on the same in-process router: with no separate
            // engine process to hold the session, the embedded one holds it.
            let config_dir = signal_sampler::rig_prefs::signal_config_dir();
            // The FastTrackStudio account — same reasoning as engine_main.rs:
            // linking TONE3000 to it once means every machine signed in to the
            // account can download without its own authorization.
            let account = std::sync::Arc::new(signal_account::Account::new(
                signal_account::AccountConfig::from_env(&config_dir),
            ));
            let tone3000 = signal_tone3000::Tone3000Backend::new(signal_tone3000::Config::from_env(
                &config_dir,
                signal_nam::nam_root_from_env(&config_dir.join("nam")),
            ))
            .with_account(account.clone());
            let router = router.merge_router(tone3000.router());
            let account_rpc = signal_account::AccountBackend::new(account.clone());
            let router = router.merge_router(account_rpc.router());
            (router, account, tone3000)
        };

        let scope = architect::Scope::new();
        let server = architect::LocalServer::serve(router, scope.clone());
        let rig: RigClient = server
            .establish()
            .await
            .map_err(|e| eyre::eyre!("rig client: {e:?}"))?;
        let stream: RigStreamClient = server
            .establish()
            .await
            .map_err(|e| eyre::eyre!("rig stream client: {e:?}"))?;
        let settings: AudioSettingsClient = server
            .establish()
            .await
            .map_err(|e| eyre::eyre!("audio settings client: {e:?}"))?;
        #[cfg(feature = "signal-keys-rig")]
        let keys: KeysRigClient = server
            .establish()
            .await
            .map_err(|e| eyre::eyre!("keys client: {e:?}"))?;
        #[cfg(feature = "signal-keys-rig")]
        let keys_stream: KeysRigStreamClient = server
            .establish()
            .await
            .map_err(|e| eyre::eyre!("keys stream client: {e:?}"))?;

        // Open the audio device + load the profile (returns immediately;
        // the open happens on the backend's own thread). On iOS we only
        // open when a real interface is present — otherwise there's no
        // input to route (the built-in mic is never used), and the hotplug
        // watcher opens the rig when one is connected. The perf model /
        // stacks are built in `new()`, so the UI is populated either way.
        #[cfg(not(target_os = "ios"))]
        backend.start();
        // Design mode (the simulator) opens no device: `start` only lays the
        // profile out, which is what the pages draw from.
        #[cfg(target_os = "ios")]
        if crate::ios_audio::has_external_input() || signal_guitar::library::rig_is_design() {
            backend.start();
        } else {
            // No interface yet: the whole surface, with no audio — the top
            // bar says so, and the hotplug watcher opens the rig when one is
            // plugged in.
            backend.show_without_audio();
        }

        #[cfg(feature = "signal")]
        let tones: Option<Tone3000Client> = server.establish().await.ok();
        #[cfg(feature = "signal")]
        let tones_stream: Option<Tone3000StreamClient> = server.establish().await.ok();
        #[cfg(feature = "signal")]
        let account_client: Option<AccountAuthClient> = server.establish().await.ok();

        // The OAuth redirect (TONE3000's and the FastTrackStudio account's)
        // still needs a real listener at the registered `localhost:4040`
        // URL — embedded mode otherwise has no HTTP surface at all, by
        // design (see `RigEngine`'s module doc). Best-effort and scoped to
        // exactly these two routes: a `signal-desktop --engine` already
        // running on that port keeps the callback (its own `Account`/
        // `Tone3000Backend` read and write the SAME session files, from the
        // same config dir, so either one completing a sign-in is enough),
        // and this one simply does not also try to bind it.
        #[cfg(feature = "signal")]
        {
            let addr = crate::engine_tone3000::callback_listen_addr(&account);
            let app = crate::engine_tone3000::standalone_callback_router(
                account.clone(),
                tone3000.clone(),
            );
            tokio::spawn(async move {
                match tokio::net::TcpListener::bind(&addr).await {
                    Ok(listener) => {
                        tracing::info!(addr, "embedded: sign-in callback listener up");
                        if let Err(e) = axum::serve(listener, app).await {
                            tracing::warn!(addr, %e, "embedded: sign-in callback listener died");
                        }
                    }
                    Err(e) => tracing::info!(
                        addr,
                        %e,
                        "embedded: sign-in callback port already taken — a                          signal-desktop --engine on it will serve sign-in                          callbacks instead"
                    ),
                }
            });
        }

        Ok::<_, eyre::Report>(RigEngine {
            rig,
            stream,
            #[cfg(feature = "signal")]
            tones,
            #[cfg(feature = "signal")]
            tones_stream,
            #[cfg(feature = "signal")]
            account: account_client,
            settings,
            #[cfg(feature = "signal-keys-rig")]
            keys,
            #[cfg(feature = "signal-keys-rig")]
            keys_stream,
            _scope: scope,
        })
    })?;

    let _ = ENGINE.set(engine);

    // iOS: watch for audio-interface hotplug and the record permission.
    // Polling is fine for device hotplug.
    //
    // - An interface plugged in: a plain start, which opens its input (and
    //   takes over from the DI player's output-only rig, if that was up).
    // - Unplugged: stop — never reopened on the built-in mic, which plays
    //   straight back out of the speaker — and if the DI player was on, it
    //   carries on output only.
    // - The record permission just granted: restart, so the input that was
    //   silent until now is heard.
    #[cfg(target_os = "ios")]
    {
        crate::ios_audio::request_record_permission();
        let rig = ENGINE.get().unwrap().rig.clone();
        let handle = runtime.handle().clone();
        std::thread::spawn(move || {
            let mut had = crate::ios_audio::has_external_input();
            let mut granted = crate::ios_audio::record_permission_granted();
            // What the session says, at the start and whenever the route or
            // the access changes: the one event that answers "why is the
            // guitar not heard" (see `daw_audio_io::session_report`).
            let report = |why: &str| {
                if let Some(session) = signal_guitar::audio_session_report() {
                    tracing::info!(audio.session = %session, audio.why = why, "audio session");
                }
            };
            report("start");
            let mut tick: u32 = 0;
            loop {
                std::thread::sleep(std::time::Duration::from_millis(1000));
                tick = tick.wrapping_add(1);
                // Every five seconds, how the audio is: running, the levels
                // in and out, the rate and buffer — a guitar that is not
                // reaching the rig reads as an input level at the floor.
                if tick % 5 == 0 {
                    let rig = rig.clone();
                    handle.spawn(async move {
                        if let Ok(s) = rig.status().await {
                            let db = |p: f32| if p > 0.0 { (20.0 * p.log10()).max(-90.0) } else { -90.0 };
                            tracing::info!(
                                audio.running = s.running,
                                audio.in_db = db(s.input_peak),
                                audio.in_l_db = db(s.input_peak_l),
                                audio.in_r_db = db(s.input_peak_r),
                                audio.out_db = db(s.output_peak),
                                audio.rate = s.perf.sample_rate,
                                audio.frames = s.perf.block_frames,
                                audio.di = s.di_playing,
                                audio.error = %s.audio_error,
                                "audio heartbeat"
                            );
                        }
                    });
                }
                let now = crate::ios_audio::has_external_input();
                let perm = crate::ios_audio::record_permission_granted();
                if now != had || perm != granted {
                    report(if now != had { "route" } else { "access" });
                }
                if now != had {
                    had = now;
                    tracing::info!(external = now, "audio route changed");
                    crate::ios_audio::configure();
                    let rig = rig.clone();
                    handle.spawn(async move {
                        if now {
                            let _ = rig.start().await;
                        } else {
                            let di = rig.status().await.ok().filter(|s| s.di_playing).map(|s| s.di_clip);
                            let _ = rig.stop().await;
                            if let Some(clip) = di {
                                let _ = rig.play_di(clip, true).await;
                            }
                        }
                    });
                } else if perm && !granted && now {
                    tracing::info!("record permission granted — restarting the rig");
                    let rig = rig.clone();
                    handle.spawn(async move {
                        let _ = rig.restart().await;
                    });
                }
                granted = perm;
            }
        });
    }

    Ok(())
}
