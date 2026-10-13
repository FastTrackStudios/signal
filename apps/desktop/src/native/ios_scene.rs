//! The UIScene lifecycle, which iOS 27 requires of every app — and the
//! little else of UIKit the app reaches for itself ([`open_url`],
//! [`pasted`]).
//!
//! An app built with the iOS 27 SDK that does not adopt scenes is stopped at
//! launch (`_UIApplicationEvaluateRuntimeIssueForNoSceneLifecycleAdoption`
//! traps). winit — what Blitz draws into — does not adopt them yet
//! (rust-windowing/winit#4224): it makes its `UIWindow` the pre-scene way,
//! and a window with no scene is never shown.
//!
//! So the app adopts them itself: `Info.plist` names this delegate for the
//! application role (`UIApplicationSceneManifest`, written by the iOS build
//! scripts), and winit's window is given the app's scene — sized to its
//! screen and made key — as soon as both exist, whichever comes second: the
//! scene connecting (the delegate) or the window being made
//! ([`window_created`], from the shell, with winit's view). A scene-based
//! app's `UIApplication.windows` lists only windows that have a scene, so
//! the window has to be handed over rather than found. winit goes on driving
//! it as before; only whose scene it is changes.

use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::NSObjectProtocol;
use objc2::{ClassType, MainThreadMarker, MainThreadOnly, define_class};
use objc2_ui_kit::{
    UIApplication, UIResponder, UIScene, UISceneConnectionOptions, UISceneDelegate, UISceneSession,
    UIView, UIWindow, UIWindowScene, UIWindowSceneDelegate,
};

define_class!(
    // SAFETY: a plain UIResponder subclass: nothing of UIResponder's is
    // overridden, and there are no ivars to initialise.
    #[unsafe(super(UIResponder))]
    #[thread_kind = MainThreadOnly]
    #[name = "SessionSceneDelegate"]
    struct SceneDelegate;

    unsafe impl NSObjectProtocol for SceneDelegate {}

    unsafe impl UISceneDelegate for SceneDelegate {
        #[unsafe(method(scene:willConnectToSession:options:))]
        fn will_connect(
            &self,
            scene: &UIScene,
            _session: &UISceneSession,
            _options: &UISceneConnectionOptions,
        ) {
            attach(MainThreadMarker::from(scene));
        }

        #[unsafe(method(sceneDidBecomeActive:))]
        fn did_become_active(&self, scene: &UIScene) {
            attach(MainThreadMarker::from(scene));
        }
    }

    unsafe impl UIWindowSceneDelegate for SceneDelegate {}
);

thread_local! {
    /// winit's window, once the shell has handed it over.
    static WINDOW: RefCell<Option<Retained<UIWindow>>> = const { RefCell::new(None) };
}

/// Register the delegate class with the runtime, before UIKit looks it up
/// by the name `Info.plist` gives.
pub fn register() {
    let _ = SceneDelegate::class();
}

/// winit's window has been made: `ui_view` is its view (the raw window
/// handle's). Kept, and given the scene if it is connected already.
pub fn window_created(ui_view: NonNull<c_void>) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    // SAFETY: a UiKit raw window handle's `ui_view` is a live `UIView`, and
    // this runs on the main thread, where UIKit objects live.
    let view: &UIView = unsafe { ui_view.cast::<UIView>().as_ref() };
    let Some(window) = view.window() else {
        return;
    };
    WINDOW.with(|w| *w.borrow_mut() = Some(window));
    attach(mtm);
}

/// Give winit's window the app's window scene, full screen, when both exist
/// and it has none yet.
fn attach(mtm: MainThreadMarker) {
    let Some(window) = WINDOW.with(|w| w.borrow().clone()) else {
        return;
    };
    if window.windowScene().is_some() {
        return;
    }
    let scenes = UIApplication::sharedApplication(mtm).connectedScenes();
    let Some(scene) = scenes
        .iter()
        .find_map(|scene| scene.downcast::<UIWindowScene>().ok())
    else {
        return;
    };
    window.setWindowScene(Some(&scene));
    // The scene's own bounds, not its screen's: on an iPad the scene is a
    // window that need not fill the screen (iPadOS 26 windows, Split View),
    // and a window framed to the screen has its surface stretched into the
    // scene — the picture drawn taller than laid out, its foot off screen.
    // On a phone the two are the same.
    window.setFrame(objc2_ui_kit::UICoordinateSpace::bounds(
        &*scene.coordinateSpace(),
    ));
    window.makeKeyAndVisible();
    tracing::info!(
        ios.scene = "adopted",
        "ios: winit's window joined the scene"
    );
}

/// Which side the camera housing is on, for a phone on its side: `Some(true)`
/// the left, `Some(false)` the right, `None` held upright (or no scene yet).
/// UIKit names an interface orientation by where the home button would be,
/// so "landscape right" has the phone's top — the housing — on the left.
pub fn island_on_left() -> Option<bool> {
    use objc2_ui_kit::UIInterfaceOrientation;
    let mtm = MainThreadMarker::new()?;
    let scenes = UIApplication::sharedApplication(mtm).connectedScenes();
    let scene = scenes
        .iter()
        .find_map(|scene| scene.downcast::<UIWindowScene>().ok())?;
    #[allow(deprecated)]
    let orientation = scene.interfaceOrientation();
    if orientation == UIInterfaceOrientation::LandscapeRight {
        Some(true)
    } else if orientation == UIInterfaceOrientation::LandscapeLeft {
        Some(false)
    } else {
        None
    }
}

/// Open `url` in the browser.
pub fn open_url(url: &str) {
    use objc2_foundation::{NSDictionary, NSString, NSURL};
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(url) = NSURL::URLWithString(&NSString::from_str(url)) else {
        tracing::warn!("ios: not a URL to open");
        return;
    };
    // SAFETY: an empty options dictionary is a dictionary of the right
    // type; no completion handler.
    unsafe {
        UIApplication::sharedApplication(mtm).openURL_options_completionHandler(
            &url,
            &NSDictionary::new(),
            None,
        );
    }
}

/// The text on the pasteboard, if any (iOS asks its person first).
pub fn pasted() -> Option<String> {
    // SAFETY: the general pasteboard's string, read on the main thread
    // (the start screen's press handler).
    unsafe { objc2_ui_kit::UIPasteboard::generalPasteboard().string() }.map(|s| s.to_string())
}
