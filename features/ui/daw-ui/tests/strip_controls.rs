//! The rest of the strip's controls, reading real track state.
//!
//! Each of these is the mute button with a different field, so the tests are
//! deliberately the same shape: the control draws its track's state, and a
//! backend event moves it without anything refetching.

use daw_proto::track::InputMonitoringMode;
use daw_proto::{Track, TrackEvent};
use daw_ui::components::mixer::ChannelStripPreview;
use daw_ui::controls::{
    IoButton, MonitorButton, PanKnob, PhaseButton, RecordArmButton, SoloButton, TrackName,
    TrackStore,
};
use dioxus::prelude::*;

thread_local! {
    static STORE: std::cell::Cell<Option<TrackStore>> = const { std::cell::Cell::new(None) };
}

fn store() -> TrackStore {
    STORE.with(|s| s.get()).expect("store handle")
}

fn settle(dom: &mut VirtualDom) {
    dom.render_immediate(&mut dioxus::core::NoOpMutations);
}

/// Mounts `body` over a store holding one track built by `make`.
fn mount(make: fn() -> Track, body: fn() -> Element) -> VirtualDom {
    #[derive(Props, Clone)]
    struct P {
        make: fn() -> Track,
        body: fn() -> Element,
    }
    impl PartialEq for P {
        fn eq(&self, other: &Self) -> bool {
            std::ptr::fn_addr_eq(self.make, other.make)
                && std::ptr::fn_addr_eq(self.body, other.body)
        }
    }

    let mut dom = VirtualDom::new_with_props(
        |p: P| {
            let mut store = use_hook(TrackStore::new);
            use_hook(|| {
                store.seed([(p.make)()]);
                provide_context(store);
                STORE.with(|s| s.set(Some(store)));
            });
            (p.body)()
        },
        P { make, body },
    );
    dom.rebuild_in_place();
    dom
}

fn base() -> Track {
    Track {
        guid: "T1".into(),
        name: "Kick".into(),
        ..Default::default()
    }
}

/// Every control here: draw it, push the backend event that changes its
/// field, and check the drawing followed — without a refetch, because
/// nothing in this test can fetch anything.
#[test]
fn each_control_follows_its_own_backend_event() {
    let cases: Vec<(&str, fn() -> Element, TrackEvent)> = vec![
        (
            "solo",
            || rsx! { SoloButton { track: "T1" } },
            TrackEvent::SoloChanged {
                guid: "T1".into(),
                soloed: true,
            },
        ),
        (
            "record arm",
            || rsx! { RecordArmButton { track: "T1" } },
            TrackEvent::ArmChanged {
                guid: "T1".into(),
                armed: true,
            },
        ),
        (
            "phase",
            || rsx! { PhaseButton { track: "T1" } },
            TrackEvent::PhaseInvertedChanged {
                guid: "T1".into(),
                inverted: true,
            },
        ),
        (
            "input monitoring",
            || rsx! { MonitorButton { track: "T1" } },
            TrackEvent::InputMonitorChanged {
                guid: "T1".into(),
                monitor: InputMonitoringMode::Normal,
            },
        ),
        (
            "name",
            || rsx! { TrackName { track: "T1" } },
            TrackEvent::Renamed {
                guid: "T1".into(),
                name: "Snare".into(),
            },
        ),
        (
            // The colour lands on the index plate, not the name plate —
            // `mcp_namebg` is a plain dark plate — so the whole strip is
            // what has to notice a colour change.
            "colour",
            || rsx! { ChannelStripPreview { track: base(), index: 0 } },
            TrackEvent::ColorChanged {
                guid: "T1".into(),
                color: Some(0xff8800),
            },
        ),
        (
            "parent send",
            || rsx! { IoButton { track: "T1" } },
            TrackEvent::ParentSendChanged {
                guid: "T1".into(),
                enabled: false,
            },
        ),
        (
            "pan",
            || rsx! { PanKnob { track: "T1" } },
            TrackEvent::PanChanged {
                guid: "T1".into(),
                pan: -0.8,
            },
        ),
    ];

    for (what, body, event) in cases {
        let mut dom = mount(base, body);
        let before = dioxus_ssr::render(&dom);
        assert!(
            before.contains("<svg") || before.contains("Kick"),
            "{what} drew nothing"
        );
        assert!(!before.contains("<img"), "{what} is blitting:\n{before}");
        assert!(
            !before.contains("currentColor"),
            "{what} left a colour to CSS"
        );

        dom.in_runtime(|| store().apply(&event));
        settle(&mut dom);
        assert_ne!(before, dioxus_ssr::render(&dom), "{what} ignored its event");
    }
}

#[test]
fn the_name_plate_shows_the_track_name() {
    let dom = mount(base, || rsx! { TrackName { track: "T1" } });
    assert!(dioxus_ssr::render(&dom).contains("Kick"));
}

/// The track colour paints the index plate, and the name plate stays dark
/// whatever the colour is.
///
/// REAPER's `mcp_namebg` is a plain plate and only `mcp_idxbg` under it
/// carries the tint. Painting both doubled the coloured block at the bottom
/// of every strip.
#[test]
fn the_colour_lands_on_the_index_plate_and_not_the_name_plate() {
    fn coloured() -> Track {
        Track {
            color: Some(0xff8800),
            ..base()
        }
    }
    let name = dioxus_ssr::render(&mount(coloured, || rsx! { TrackName { track: "T1" } }));
    let strip = dioxus_ssr::render(&mount(
        coloured,
        || rsx! { ChannelStripPreview { track: coloured() } },
    ));

    assert!(
        !name.contains("#ff8800"),
        "the name plate took the track colour:\n{name}"
    );
    assert!(
        name.contains(daw_theme::defaults::STRIP_BODY),
        "the name plate is not `mcp_namebg`'s token:\n{name}"
    );
    // The *tinted* colour, not the raw one: REAPER paints a panel at 70%
    // of the track's colour, measured in one screenshot holding both
    // renders of the same project. #ff8800 shaded 30% toward black.
    let tinted = daw_theme_art::dress::panel_tint(daw_theme::Color::rgb(0xff, 0x88, 0x00));
    assert!(
        strip.contains(&tinted.to_hex()),
        "the strip did not paint the tinted colour ({}):\n{strip}",
        tinted.to_hex()
    );
    assert!(
        !strip.contains("#ff8800"),
        "the strip painted the raw track colour"
    );
}

/// The two reads that did not exist before this ticket. They are on the
/// track itself rather than behind getters of their own, so building a
/// strip stays exactly one bulk read — which is the entire point of that
/// call.
#[test]
fn the_bulk_read_carries_record_input_and_parent_send() {
    use daw_proto::track::RecordInput;

    // Both are on `Track`, so a strip that has the track has them.
    let t = Track {
        record_input: RecordInput::Audio { channel: 3 },
        parent_send: false,
        ..base()
    };
    assert_eq!(t.record_input, RecordInput::Audio { channel: 3 });
    assert!(!t.parent_send);

    // A default track sends to its parent: "cut off from the master" is
    // not a state anything should fall into by omission.
    assert!(Track::default().parent_send);
    assert_eq!(Track::default().record_input, RecordInput::None);
}

/// A track cut off from its parent draws the disabled badge, and one that
/// is not does not.
#[test]
fn the_io_indicator_reflects_the_parent_send() {
    fn cut_off() -> Track {
        Track {
            parent_send: false,
            ..base()
        }
    }
    let sending = dioxus_ssr::render(&mount(base, || rsx! { IoButton { track: "T1" } }));
    let cut = dioxus_ssr::render(&mount(cut_off, || rsx! { IoButton { track: "T1" } }));
    assert_ne!(sending, cut, "the IO button ignores parent send");
}

/// Pan is a drag, so like the fader it renders the UI's in-flight value and
/// ignores the engine's echo until the gesture is over.
#[test]
fn a_pan_drag_leads_the_engine_and_ignores_its_echo() {
    let mut dom = mount(base, || rsx! { PanKnob { track: "T1" } });
    let centred = dioxus_ssr::render(&dom);

    dom.in_runtime(|| {
        let mut store = store();
        let mut drafts = store.drafts();
        drafts.set_pan("T1", 0.75);
        // The engine, still catching up, reports where it had got to.
        store.apply(&TrackEvent::PanChanged {
            guid: "T1".into(),
            pan: 0.0,
        });
        assert_eq!(store.pan("T1"), 0.75, "the echo fought the finger");

        drafts.release_pan("T1");
        drafts.take_dirty();
        drafts.retire();
        store.apply(&TrackEvent::PanChanged {
            guid: "T1".into(),
            pan: -0.5,
        });
        assert_eq!(store.pan("T1"), -0.5, "an idle knob ignored the engine");
    });
    settle(&mut dom);
    assert_ne!(centred, dioxus_ssr::render(&dom), "the knob never moved");
}

/// Volume and pan are held independently: dragging one must not suppress
/// the other's events.
#[test]
fn holding_one_value_does_not_suppress_the_other() {
    let dom = mount(base, || rsx! { PanKnob { track: "T1" } });
    dom.in_runtime(|| {
        let mut store = store();
        store.drafts().set_pan("T1", 0.5);
        store.apply(&TrackEvent::VolumeChanged {
            guid: "T1".into(),
            volume: 0.9,
        });
        assert_eq!(store.volume("T1"), 0.9, "a pan drag suppressed volume");
        assert_eq!(store.pan("T1"), 0.5);
    });
}
