//! The macOS menu bar: the rig's audio state where a Mac shows app status,
//! and Settings… where a Mac keeps it.
//!
//! - **Audio readout** — a menu-bar item after the app's own menus titled
//!   like REAPER's (`48 kHz · 64 spls · 2.7 ms · DSP 12%`), refreshed every
//!   second from the embedded rig's [`RigPerf`](signal_guitar_proto::RigPerf).
//!   Its menu holds the detail: render times, dropped blocks, xruns.
//! - **Audio Settings… ⌘,** in the app menu and at the top of the Audio
//!   menu — the rig's device, outputs and headphone mix. The app's own
//!   flyout (account, engines, updates) is **Settings… ⌥⌘,**.
//!
//! Everything here runs on the main thread: dioxus-native polls the
//! VirtualDom (and so this component's future) from the AppKit event loop.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::prelude::*;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};
use objc2_foundation::NSString;

/// Set by the menu's target, drained by the component: AppKit calls the
/// action, the component opens the panel.
static SETTINGS_REQUESTED: AtomicBool = AtomicBool::new(false);
static AUDIO_SETTINGS_REQUESTED: AtomicBool = AtomicBool::new(false);

define_class!(
    // SAFETY: NSObject has no subclassing requirements; the class adds one
    // action method and no ivars.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "FtsMenuTarget"]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(openSettings:))]
        fn open_settings(&self, _sender: Option<&AnyObject>) {
            SETTINGS_REQUESTED.store(true, Ordering::Relaxed);
        }

        #[unsafe(method(openAudioSettings:))]
        fn open_audio_settings(&self, _sender: Option<&AnyObject>) {
            AUDIO_SETTINGS_REQUESTED.store(true, Ordering::Relaxed);
        }
    }
);

impl MenuTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        // SAFETY: plain NSObject init.
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

/// What the menu bar holds on to (AppKit keeps weak references to targets).
struct Installed {
    stats: Retained<NSMenuItem>,
    stats_menu: Retained<NSMenu>,
    target: Retained<MenuTarget>,
}

/// A menu item that sends `action` to `target`, with a key equivalent.
fn action_item(
    mtm: MainThreadMarker,
    title: &str,
    action: objc2::runtime::Sel,
    key: &str,
    target: &MenuTarget,
) -> Retained<NSMenuItem> {
    // SAFETY: a selector the target class defines.
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            Some(action),
            &NSString::from_str(key),
        )
    };
    // SAFETY: the target outlives the item (held in INSTALLED).
    unsafe { item.setTarget(Some(target)) };
    item
}

thread_local! {
    static INSTALLED: RefCell<Option<Installed>> = const { RefCell::new(None) };
}

fn item(mtm: MainThreadMarker, title: &str) -> Retained<NSMenuItem> {
    let item = NSMenuItem::new(mtm);
    item.setTitle(&NSString::from_str(title));
    item
}

/// Add the readout and Settings… once the app's main menu exists (winit
/// builds it as the event loop starts). `true` once installed.
fn install(mtm: MainThreadMarker) -> bool {
    if INSTALLED.with(|i| i.borrow().is_some()) {
        return true;
    }
    let app = NSApplication::sharedApplication(mtm);
    let Some(main_menu) = app.mainMenu() else {
        return false;
    };

    let target = MenuTarget::new(mtm);
    // Audio Settings… ⌘, — second in the app menu, after "About", where
    // every Mac keeps its settings; the app's own flyout beside it on ⌥⌘,.
    if let Some(app_menu) = main_menu.itemAtIndex(0).and_then(|i| i.submenu()) {
        let audio = action_item(mtm, "Audio Settings…", sel!(openAudioSettings:), ",", &target);
        let settings = action_item(mtm, "Settings…", sel!(openSettings:), ",", &target);
        settings.setKeyEquivalentModifierMask(
            objc2_app_kit::NSEventModifierFlags::Command | objc2_app_kit::NSEventModifierFlags::Option,
        );
        let at = app_menu.numberOfItems().min(1);
        app_menu.insertItem_atIndex(&audio, at);
        app_menu.insertItem_atIndex(&settings, at + 1);
        app_menu.insertItem_atIndex(&NSMenuItem::separatorItem(mtm), at + 2);
    }

    // The readout: a menu-bar title, its menu the detail lines.
    let stats_menu = NSMenu::new(mtm);
    stats_menu.setTitle(&NSString::from_str("Audio"));
    let stats = item(mtm, "Audio");
    stats.setSubmenu(Some(&stats_menu));
    main_menu.addItem(&stats);

    INSTALLED.with(|i| {
        *i.borrow_mut() = Some(Installed {
            stats,
            stats_menu,
            target,
        });
    });
    true
}

thread_local! {
    /// What is showing — the menu is only rebuilt when it changes, so an
    /// open menu does not flicker under the pointer.
    static SHOWN: RefCell<(String, Vec<String>)> = const { RefCell::new((String::new(), Vec::new())) };
}

/// Put `title` in the menu bar and `lines` in its menu.
fn show(title: &str, lines: &[String]) {
    let same = SHOWN.with(|s| {
        let s = s.borrow();
        s.0 == title && s.1 == lines
    });
    if same {
        return;
    }
    SHOWN.with(|s| *s.borrow_mut() = (title.to_string(), lines.to_vec()));
    INSTALLED.with(|i| {
        let Some(inst) = i.borrow().as_ref().map(|i| (i.stats.clone(), i.stats_menu.clone(), i.target.clone())) else {
            return;
        };
        let (stats, menu, target) = inst;
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let title = NSString::from_str(title);
        // The bar shows the submenu's title for a top-level item.
        menu.setTitle(&title);
        stats.setTitle(&title);
        menu.removeAllItems();
        menu.addItem(&action_item(mtm, "Audio Settings…", sel!(openAudioSettings:), "", &target));
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        for line in lines {
            let row = item(mtm, line);
            row.setEnabled(false);
            menu.addItem(&row);
        }
    });
}

/// The rig's audio state, as the bar title and the detail lines.
fn describe(status: Option<&signal_guitar_proto::RigStatus>) -> (String, Vec<String>) {
    let Some(status) = status else {
        return ("Audio —".into(), vec!["No rig engine in this app".into()]);
    };
    if !status.running {
        return ("Audio stopped".into(), vec!["The audio device is closed".into()]);
    }
    let p = &status.perf;
    // Running with no negotiated rate: a rig with no device (design mode).
    if p.sample_rate == 0 {
        return ("Audio idle".into(), vec!["No audio device open".into()]);
    }
    let khz = p.sample_rate as f32 / 1000.0;
    let khz = if khz.fract() == 0.0 { format!("{khz:.0}") } else { format!("{khz:.1}") };
    let mut title = format!(
        "{khz} kHz · {} spls · {:.1} ms · DSP {:.0}%",
        p.block_frames,
        p.buffer_latency_ms(),
        p.mean_load * 100.0,
    );
    if p.over_budget > 0 {
        title.push_str(&format!(" · {} drops", p.over_budget));
    }
    let lines = vec![
        format!("{khz} kHz, {} samples per block", p.block_frames),
        format!("Round-trip buffer latency {:.2} ms", p.buffer_latency_ms()),
        format!(
            "DSP {:.0}% now · {:.0}% mean (render {} µs · peak {} µs · budget {} µs)",
            p.load * 100.0,
            p.mean_load * 100.0,
            p.mean_render_us,
            p.peak_render_us,
            p.budget_us()
        ),
        format!("Dropped blocks {} · xruns {}", p.over_budget, p.xruns),
    ];
    (title, lines)
}

/// Mount once in the app root. Renders nothing; drives the menu bar.
#[component]
pub fn MacMenuBar(on_settings: Callback<()>) -> Element {
    use_future(move || async move {
        loop {
            if let Some(mtm) = MainThreadMarker::new()
                && install(mtm)
            {
                if SETTINGS_REQUESTED.swap(false, Ordering::Relaxed) {
                    on_settings.call(());
                }
                if AUDIO_SETTINGS_REQUESTED.swap(false, Ordering::Relaxed) {
                    signal_guitar_ui::open_audio_settings();
                }
                let status = match crate::rig_engine::engine() {
                    Some(e) => e.rig.status().await.ok(),
                    None => None,
                };
                let (title, lines) = describe(status.as_ref());
                show(&title, &lines);
            }
            // Fast enough that ⌘, feels immediate; the readout itself only
            // needs a second.
            architect::platform::sleep(std::time::Duration::from_millis(250)).await;
        }
    });
    rsx! {}
}
