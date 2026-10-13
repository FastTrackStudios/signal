//! X-Touch hardware exerciser — plug in the surface and run:
//!
//! ```sh
//! cargo run -p daw-csi --example xtouch_test            # default: matches "x-touch"
//! cargo run -p daw-csi --example xtouch_test -- --list  # enumerate MIDI ports
//! cargo run -p daw-csi --example xtouch_test -- "X-Touch"   # explicit match
//! ```
//!
//! What it does:
//! 1. **POST sweep** — exercises every output path raw: LCD banner +
//!    color rainbow, LED rows, v-pot rings, motor-fader wave, meters.
//! 2. **Demo session** — seeds an in-process Standalone with folders
//!    (DRUMS → OH nested), colors, a VCA group, a VOX→FX BUS send,
//!    and a real CLAP plugin on Kick when one is found on disk.
//! 3. **Live driver** — runs the real `daw_csi::run` loop against it.
//!    Every gesture is logged (`daw_csi=debug`): faders write volume,
//!    GLOBAL VIEW enters folder mode, hold-SELECT spills folders in
//!    the converted-CSI set, ASSIGN-PLUGIN opens the FX zones.
//!
//! Ctrl-C to quit.

use std::time::Duration;

use daw_csi::driver::CsiConfig;
use daw_csi::mcu::{self, Button, RingMode, StripColor};
use daw_csi::midi::SurfacePort;
use daw_proto::{ProjectContext, ProjectInfo, TrackRef, Tracks};
use daw_standalone::bootstrap::build_in_process_daw;
use daw_standalone::sync::Standalone;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,daw_csi=debug".into()),
        )
        .compact()
        .init();

    let arg = std::env::args().nth(1);
    if arg.as_deref() == Some("--list") {
        println!("MIDI inputs:");
        for name in SurfacePort::list_inputs() {
            println!("  {name}");
        }
        return Ok(());
    }
    let device = arg.unwrap_or_else(|| "x-touch".into());

    // ── 1. POST sweep: raw output exercise ──────────────────────────
    {
        let mut port = SurfacePort::open(&device)?;
        println!("connected: {} — running surface POST…", port.name);
        post_sweep(&mut port).await;
        // Drop the port so the driver can reopen it.
    }

    // ── 2. Demo session ─────────────────────────────────────────────
    let standalone = demo_session();
    let bundle = standalone_clap_fx(&standalone);
    let daw = build_in_process_daw(standalone).await?;

    // Put the CLAP plugin on Kick through the service path so the
    // FX zones have something real to show.
    if let Some(bundle) = bundle {
        let project = daw.daw.current_project().await?;
        if let Ok(Some(kick)) = project.get_track_by_name("Kick In").await {
            let fx = kick.fx_chain().add(&bundle).await?;
            let params = fx.parameters().await?;
            let real = params
                .first()
                .is_some_and(|p| !p.name.starts_with("Param "));
            if real {
                println!(
                    "FX: {} on 'Kick In' ({} params) — select it, press ASSIGN-PLUGIN",
                    bundle,
                    params.len()
                );
            } else {
                fx.remove().await?;
            }
        }
    }

    println!();
    println!("driver running — things to try on the surface:");
    println!("  • faders / v-pots / mute / solo / rec / select");
    println!("  • GLOBAL VIEW = folder mode (select DRUMS to spill it)");
    println!("  • ASSIGN-PLUGIN = FX menu → select an FX → turn its params");
    println!("  • transport keys, bank/channel < >, master fader");
    println!("  • FTS_CSI_ZONES=<file> to test your own zone set");
    println!("Ctrl-C to quit.");
    println!();

    tokio::select! {
        r = daw_csi::run(daw.daw.clone(), CsiConfig { device_match: device }) => {
            r?;
        }
        _ = tokio::signal::ctrl_c() => {
            println!("\nbye");
        }
    }
    Ok(())
}

/// Exercise every raw output path once — visual confirmation that
/// encode + transport work before the driver takes over.
async fn post_sweep(port: &mut SurfacePort) {
    let sleep = |ms| tokio::time::sleep(Duration::from_millis(ms));

    // LCD banner + color rainbow.
    let banner = [
        "FASTTRK", " STUDIO", "  CSI  ", "X-TOUCH", "SURFACE", "  POST ", "  OK   ", "  \\o/  ",
    ];
    for (i, text) in banner.iter().enumerate() {
        port.send(&mcu::encode_lcd(i as u8, 0, text));
        port.send(&mcu::encode_lcd(i as u8, 1, "·······"));
    }
    let rainbow = [
        StripColor::Red,
        StripColor::Yellow,
        StripColor::Green,
        StripColor::Cyan,
        StripColor::Blue,
        StripColor::Magenta,
        StripColor::White,
        StripColor::Red,
    ];
    port.send(&mcu::encode_strip_colors(rainbow));
    sleep(150).await;

    // LED rows: rec → solo → mute → select, on then off.
    for row in [0u8, 1, 2, 3] {
        for strip in 0..8u8 {
            let b = match row {
                0 => Button::Rec(strip),
                1 => Button::Solo(strip),
                2 => Button::Mute(strip),
                _ => Button::Select(strip),
            };
            port.send(&mcu::encode_button_led(b, true));
        }
        sleep(90).await;
    }
    sleep(150).await;
    for row in [0u8, 1, 2, 3] {
        for strip in 0..8u8 {
            let b = match row {
                0 => Button::Rec(strip),
                1 => Button::Solo(strip),
                2 => Button::Mute(strip),
                _ => Button::Select(strip),
            };
            port.send(&mcu::encode_button_led(b, false));
        }
    }

    // V-pot ring chase.
    for posn in 1..=11u8 {
        for strip in 0..8u8 {
            port.send(&mcu::encode_vpot_ring(
                strip,
                RingMode::SingleDot,
                posn,
                false,
            ));
        }
        sleep(35).await;
    }

    // Motor fader wave: staggered rise, then drop.
    for step in 0..=10u16 {
        for strip in 0..9u8 {
            let phase = (step as i32 - strip as i32).clamp(0, 10) as u16;
            port.send(&mcu::encode_fader(strip, phase * 1638));
        }
        sleep(60).await;
    }
    sleep(200).await;
    for strip in 0..9u8 {
        port.send(&mcu::encode_fader(strip, 0));
    }

    // Meters: one full ramp (they decay on their own).
    for level in 0..=12u8 {
        for strip in 0..8u8 {
            port.send(&mcu::encode_meter(strip, level));
        }
        sleep(40).await;
    }
}

/// Build the demo project: 8 top-level folders — one per strip in
/// folder mode (CSI's root shows exactly the folder parents) — each
/// holding a realistic member list. Plus a VCA group, a send, and a
/// reverb bus that lives outside the folders (Track mode only, like
/// CSI treats loose tracks).
fn demo_session() -> Standalone {
    let s = Standalone::new();
    s.seed_project(ProjectInfo {
        guid: "xtouch-demo".into(),
        name: "X-Touch Demo".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Project("xtouch-demo".into());

    // (folder name, folder color, members). Colors picked so the
    // X-Touch scribble quantizer (7-color backlight) lands on the
    // family color: drums→red, bass→yellow, electric→blue,
    // acoustic→cyan (closest to light blue), keys→green,
    // synths/bgvs→magenta (no purple backlight), vocals→white
    // (pale pink reads as pink on white).
    let folders: &[(&str, u32, &[&str])] = &[
        (
            "Drums",
            0xE0_3030, // red
            &[
                "Kick In", "Kick Out", "Snare Tp", "Snare Bt", "HiHat", "Rack Tom", "Flr Tom",
                "OH L", "OH R", "Room",
            ],
        ),
        ("Bass", 0xE6_C832, &["Bass DI", "Bass Amp"]), // yellow
        (
            "Electric",
            0x28_40C8, // dark blue
            &["EG1 L", "EG1 R", "EG2 L", "EG2 R"],
        ),
        ("Acoustic", 0x50_C8E6, &["Acous 1", "Acous 2"]), // light blue → cyan
        (
            "Keys",
            0x3C_C850, // green
            &["Piano L", "Piano R", "Rhodes", "Organ"],
        ),
        (
            "Synths",
            0x90_40E0, // purple → magenta backlight
            &["Pad 1", "Pad 2", "Lead Syn", "Arp"],
        ),
        ("BGVs", 0xE0_40E0, &["BGV 1", "BGV 2", "BGV 3"]), // magenta
        ("Vocals", 0xFF_E8F0, &["Lead Vox", "Vox Dbl"]),   // pink → white backlight
    ];

    let mut by_name: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (folder, color, members) in folders {
        let fg = Tracks::add(&s, ctx.clone(), folder, None).expect("add folder");
        let _ = Tracks::set_color(&s, ctx.clone(), TrackRef::Guid(fg.clone()), *color);
        by_name.insert(folder.to_string(), fg.clone());
        for (i, member) in members.iter().enumerate() {
            let mg = Tracks::add(&s, ctx.clone(), member, None).expect("add member");
            let _ = Tracks::set_color(&s, ctx.clone(), TrackRef::Guid(mg.clone()), *color);
            by_name.insert(member.to_string(), mg.clone());
            // Last member closes the folder back to root.
            if i == members.len() - 1 {
                let _ = Tracks::set_folder_depth(&s, ctx.clone(), TrackRef::Guid(mg), -1);
            }
        }
        let _ = Tracks::set_folder_depth(&s, ctx.clone(), TrackRef::Guid(fg), 1);
    }

    // A loose reverb bus + the band VCA — reachable in Track mode;
    // folder mode shows only the 8 parents (CSI semantics).
    let verb = Tracks::add(&s, ctx.clone(), "Verb Bus", None).expect("add verb");
    let _ = Tracks::set_color(&s, ctx.clone(), TrackRef::Guid(verb.clone()), 0x60_60A0);
    let vca = Tracks::add(&s, ctx.clone(), "BAND VCA", None).expect("add vca");
    let _ = Tracks::set_color(&s, ctx.clone(), TrackRef::Guid(vca), 0xF0F0F0);

    // VCA: BAND VCA leads group 1; the band folders' rhythm section
    // follows (engine honors this at playback; the VCA zone spills it).
    s.write_project("xtouch-demo", |p| {
        for t in p.tracks.iter_mut() {
            match t.name.as_str() {
                "BAND VCA" => t.grouping.vca_lead = 1,
                "Bass DI" | "Bass Amp" | "EG1 L" | "EG1 R" | "EG2 L" | "EG2 R" => {
                    t.grouping.vca_follow = 1
                }
                _ => {}
            }
        }
    });

    // A send: Lead Vox → Verb Bus (sends zone shows it when Lead Vox
    // is selected).
    if let Some(lead) = by_name.get("Lead Vox") {
        let _ = daw_proto::routing::Routing::add_send(
            &s,
            ctx.clone(),
            TrackRef::Guid(lead.clone()),
            TrackRef::Guid(verb),
        );
    }

    // Select Kick In so the sends/FX zones have a target out of the box.
    if let Some(kick) = by_name.get("Kick In") {
        let _ = Tracks::set_selected(&s, ctx, TrackRef::Guid(kick.clone()), true);
    }
    s
}

/// First CLAP bundle on disk that we can try (same probe order as
/// the FX-zone tests).
fn standalone_clap_fx(_s: &Standalone) -> Option<String> {
    if let Some(p) = std::env::var_os("DAW_TEST_CLAP_BUNDLE") {
        return Some(p.to_string_lossy().into_owned());
    }
    let home = std::env::var("HOME").unwrap_or_default();
    [
        format!("{home}/.clap/delay-plugin.clap"),
        format!("{home}/.clap/chorus-plugin.clap"),
        format!("{home}/.clap/gate-plugin.clap"),
        "/usr/lib/clap/lsp-plugins-clap.clap".to_string(),
    ]
    .into_iter()
    .find(|candidate| std::path::Path::new(candidate).exists())
}
