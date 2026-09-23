//! `signal-desktop --menubar` — Signal in the macOS menu bar.
//!
//! A process with no window and no Dock icon (an *accessory* app) that owns
//! one status item: a waveform in the menu bar, whether or not the Signal
//! app is open. Its menu lists every Signal process on the machine — the
//! headless engine(s) and the app itself — with a way to stop each, plus
//! Open Signal Rig and Start engine.
//!
//! It exists because an engine can outlive every UI: one started by hand, by
//! a script or by another machine's session has no window to close, and the
//! app's own supervisor deliberately leaves an engine it did not start alone
//! (see `engines.rs`). The menu bar is the one place that sees all of them.
//!
//! The menu is rebuilt from a fresh process scan each time it opens
//! (`menuNeedsUpdate:`), so there is no polling and nothing to go stale.
//! Everything runs on the main thread, inside AppKit's own run loop.

use std::cell::OnceCell;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, ProtocolObject};
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSImage, NSMenu, NSMenuDelegate, NSMenuItem,
    NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::{NSObjectProtocol, NSString};

/// One Signal process, as the scan found it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SignalProcess {
    pid: i32,
    kind: Kind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// `signal-desktop --engine`: the headless engine.
    Engine,
    /// The Signal app (a window).
    App,
    /// Another menu bar — only ever this one's predecessor.
    MenuBar,
}

/// Every running `signal-desktop`, from `ps` (no process-listing crate for
/// one call a menu-open).
///
/// Found by *executable* (`comm`), then classified by its arguments: a shell,
/// `nohup` or script whose command line merely mentions the binary is not a
/// Signal process, and matching on the command text mistook exactly those
/// for a running menu bar.
fn scan() -> Vec<SignalProcess> {
    let ps = |args: &[&str]| {
        std::process::Command::new("/bin/ps")
            .args(args)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    };
    let own = std::process::id();
    let pids = signal_pids(&ps(&["-axo", "pid=,comm="]), own);
    if pids.is_empty() {
        return Vec::new();
    }
    let list = pids.iter().map(ToString::to_string).collect::<Vec<_>>().join(",");
    classify(&ps(&["-o", "pid=,args=", "-p", &list]))
}

/// The pids whose executable is `signal-desktop`, from `ps -o pid=,comm=`
/// (comm is the full executable path on macOS, spaces and all), less `own`.
fn signal_pids(ps: &str, own: u32) -> Vec<i32> {
    ps.lines()
        .filter_map(|line| {
            let (pid, comm) = line.trim_start().split_once(' ')?;
            let pid: i32 = pid.parse().ok()?;
            // `signal-menubar` is the same binary under the name the menu bar
            // runs as (see `open_app`).
            let is_signal = matches!(
                comm.trim_end().rsplit('/').next(),
                Some("signal-desktop" | "signal-menubar")
            );
            (is_signal && u32::try_from(pid).ok()? != own).then_some(pid)
        })
        .collect()
}

/// Classify `ps -o pid=,args=` lines for processes already known to be
/// `signal-desktop`, by their flags.
fn classify(ps: &str) -> Vec<SignalProcess> {
    ps.lines()
        .filter_map(|line| {
            let (pid, args) = line.trim_start().split_once(' ')?;
            let pid: i32 = pid.parse().ok()?;
            let has = |flag: &str| args.split_whitespace().any(|a| a == flag);
            let kind = if has("--engine") {
                Kind::Engine
            } else if has("--menubar") {
                Kind::MenuBar
            } else {
                Kind::App
            };
            Some(SignalProcess { pid, kind })
        })
        .collect()
}

/// SIGTERM, so an engine closes its audio device cleanly; SIGKILL if it has
/// not gone within three seconds (a wedged engine does not answer TERM's
/// graceful path any more than it answers HTTP).
fn stop(pid: i32) {
    // SAFETY: kill(2) with a pid from a fresh scan and a standard signal.
    unsafe { libc::kill(pid, libc::SIGTERM) };
    std::thread::spawn(move || {
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            // SAFETY: signal 0 probes existence without delivering anything.
            if unsafe { libc::kill(pid, 0) } != 0 {
                return;
            }
        }
        tracing::warn!(pid, "menubar: process ignored SIGTERM — killing");
        // SAFETY: as above.
        unsafe { libc::kill(pid, libc::SIGKILL) };
    });
}

/// The Signal app to open: `SIGNAL_APP_BUNDLE` when set, else the `.app`
/// bundle this binary runs from, else this binary itself in guitar mode.
///
/// The menu bar should *not* run from inside the app's bundle: Launch
/// Services then counts it as the app already running, and opening the app
/// only activates the menu bar — no window. So it runs from its own path and
/// is told where the app is.
fn open_app() {
    let exe = std::env::current_exe().ok();
    let bundle = std::env::var_os("SIGNAL_APP_BUNDLE")
        .map(std::path::PathBuf::from)
        .filter(|p| p.exists())
        .or_else(|| {
            exe.as_ref().and_then(|p| {
                p.ancestors()
                    .find(|a| a.extension().is_some_and(|e| e == "app"))
                    .map(std::path::Path::to_path_buf)
            })
        });
    let result = match (bundle, exe) {
        (Some(app), _) => std::process::Command::new("/usr/bin/open").arg(app).spawn(),
        (None, Some(exe)) => std::process::Command::new(exe).arg("--guitar").spawn(),
        (None, None) => return,
    };
    if let Err(e) = result {
        tracing::warn!(error = %e, "menubar: could not open the Signal app");
    }
}

/// Start a headless engine, detached: it belongs to no UI, which is the
/// point — the menu bar is how it is stopped again.
fn start_engine() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    if let Err(e) = std::process::Command::new(exe)
        .arg("--engine")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        tracing::warn!(error = %e, "menubar: could not start the engine");
    }
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; the class adds
    // action methods and the menu delegate callback, and no ivars.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "FtsSignalMenuBar"]
    struct Target;

    unsafe impl NSObjectProtocol for Target {}

    unsafe impl NSMenuDelegate for Target {
        #[unsafe(method(menuNeedsUpdate:))]
        fn menu_needs_update(&self, menu: &NSMenu) {
            fill(menu, self);
        }
    }

    impl Target {
        /// Stop the process whose pid is the sender's tag.
        #[unsafe(method(stopProcess:))]
        fn stop_process(&self, sender: Option<&NSMenuItem>) {
            if let Some(item) = sender {
                if let Ok(pid) = i32::try_from(item.tag()) {
                    stop(pid);
                }
            }
        }

        #[unsafe(method(stopAll:))]
        fn stop_all(&self, _sender: Option<&AnyObject>) {
            for p in scan() {
                if p.kind != Kind::MenuBar {
                    stop(p.pid);
                }
            }
        }

        #[unsafe(method(openApp:))]
        fn open_app_action(&self, _sender: Option<&AnyObject>) {
            open_app();
        }

        #[unsafe(method(startEngine:))]
        fn start_engine_action(&self, _sender: Option<&AnyObject>) {
            start_engine();
        }

        #[unsafe(method(quitMenuBar:))]
        fn quit(&self, _sender: Option<&AnyObject>) {
            std::process::exit(0);
        }
    }
);

impl Target {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        // SAFETY: `init` on a freshly allocated NSObject subclass.
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

/// A menu item that calls `action` on `target`, or a disabled label when
/// `action` is `None`.
fn item(
    mtm: MainThreadMarker,
    title: &str,
    action: Option<objc2::runtime::Sel>,
    target: &Target,
    tag: isize,
) -> Retained<NSMenuItem> {
    // SAFETY: standard NSMenuItem construction on the main thread.
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            action,
            &NSString::from_str(""),
        )
    };
    if action.is_some() {
        // SAFETY: the target outlives the item (it lives for the process).
        unsafe { item.setTarget(Some(target)) };
        item.setTag(tag);
    } else {
        item.setEnabled(false);
    }
    item
}

/// Rebuild `menu` from a fresh scan.
fn fill(menu: &NSMenu, target: &Target) {
    let mtm = MainThreadMarker::from(target);
    menu.removeAllItems();
    let procs = scan();
    let engines: Vec<_> = procs.iter().filter(|p| p.kind == Kind::Engine).collect();
    let apps: Vec<_> = procs.iter().filter(|p| p.kind == Kind::App).collect();

    let add = |i: Retained<NSMenuItem>| menu.addItem(&i);
    let sep = || menu.addItem(&NSMenuItem::separatorItem(mtm));

    if engines.is_empty() {
        add(item(mtm, "Engine: not running", None, target, 0));
    }
    for e in &engines {
        add(item(mtm, &format!("Engine running · pid {}", e.pid), None, target, 0));
        add(item(
            mtm,
            "    Stop engine",
            Some(sel!(stopProcess:)),
            target,
            e.pid as isize,
        ));
    }
    for a in &apps {
        add(item(mtm, &format!("Signal app open · pid {}", a.pid), None, target, 0));
        add(item(
            mtm,
            "    Quit Signal app",
            Some(sel!(stopProcess:)),
            target,
            a.pid as isize,
        ));
    }
    sep();
    add(item(mtm, "Open Signal Rig", Some(sel!(openApp:)), target, 0));
    if engines.is_empty() {
        add(item(mtm, "Start engine", Some(sel!(startEngine:)), target, 0));
    }
    if !engines.is_empty() || !apps.is_empty() {
        add(item(mtm, "Stop everything", Some(sel!(stopAll:)), target, 0));
    }
    sep();
    add(item(mtm, "Quit Signal menu bar", Some(sel!(quitMenuBar:)), target, 0));
}

thread_local! {
    /// Kept alive for the process: AppKit holds the status item and menu
    /// weakly enough that they must be owned somewhere.
    static KEEP: OnceCell<(Retained<NSStatusItem>, Retained<NSMenu>, Retained<Target>)> =
        const { OnceCell::new() };
}

/// Entry point for `signal-desktop --menubar`: never returns.
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    // One menu bar per login: a second launch (the app's launcher starts one
    // every time) leaves the first in place.
    if scan().iter().any(|p| p.kind == Kind::MenuBar) {
        tracing::info!("menubar: already running");
        return;
    }

    let mtm = MainThreadMarker::new().expect("--menubar runs on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    // No Dock icon, no app menu: a status item and nothing else.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let target = Target::new(mtm);
    let menu = NSMenu::new(mtm);
    menu.setDelegate(Some(ProtocolObject::from_ref(&*target)));
    menu.setAutoenablesItems(false);

    let status = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
    if let Some(button) = status.button(mtm) {
        // SAFETY: a system symbol name and a plain description string.
        let image = unsafe {
            NSImage::imageWithSystemSymbolName_accessibilityDescription(
                &NSString::from_str("waveform"),
                Some(&NSString::from_str("Signal")),
            )
        };
        match image {
            Some(image) => {
                image.setTemplate(true);
                button.setImage(Some(&image));
            }
            None => button.setTitle(&NSString::from_str("Signal")),
        }
        button.setToolTip(Some(&NSString::from_str("Signal")));
    }
    status.setMenu(Some(&menu));
    KEEP.with(|k| {
        let _ = k.set((status, menu, target));
    });

    tracing::info!("menubar: up");
    app.run();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_signal_by_executable_not_by_command_text() {
        let comm = "\
  101 /Volumes/dev-drive/signal/target/release/signal-desktop
  102 /Volumes/dev-drive/bin/Signal Rig.app/Contents/MacOS/signal-desktop
  103 /Volumes/dev-drive/bin/Signal Rig.app/Contents/MacOS/signal-desktop
  104 /bin/zsh
  105 /usr/bin/nohup
  106 /Volumes/dev-drive/bin/signal-menubar
";
        // 104/105 mention signal-desktop in their arguments; only the
        // executable counts. 103 is this process; 106 is another menu bar.
        assert_eq!(signal_pids(comm, 103), vec![101, 102, 106]);
    }

    #[test]
    fn classifies_by_flag() {
        let args = "\
  101 /Volumes/dev-drive/signal/target/release/signal-desktop --engine
  102 /Volumes/dev-drive/bin/Signal Rig.app/Contents/MacOS/signal-desktop --guitar
  103 /Volumes/dev-drive/bin/Signal Rig.app/Contents/MacOS/signal-desktop --menubar
";
        assert_eq!(
            classify(args),
            vec![
                SignalProcess { pid: 101, kind: Kind::Engine },
                SignalProcess { pid: 102, kind: Kind::App },
                SignalProcess { pid: 103, kind: Kind::MenuBar },
            ]
        );
    }
}
