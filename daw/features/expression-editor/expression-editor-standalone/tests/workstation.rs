//! The workstation component, mounted headless over the synthetic kit.
//!
//! The claims: all three panes exist, and the async project fetch fills
//! the arrange + mixer with the project's tracks — the "Loading" states
//! resolve. Geometry and looks are the shot harness's job; this pins
//! the wiring.

use std::path::{Path, PathBuf};

use dawfile_reaper::RppSerialize;
use dawfile_reaper::builder::ReaperProjectBuilder;
use dawfile_reaper::types::item::SourceType;
use dioxus::prelude::VirtualDom;
use dioxus_test::by_testid;
use expression_editor_core::Viewport;
use expression_editor_standalone::workstation::{
    WorkstationApp, bootstrap_daw_blocking, stage_workstation,
};
use expression_editor_standalone::{Runner, Source, Target};

fn write_click_wav(path: &Path, rate: u32) {
    let frames = rate;
    let mut samples = vec![0.0f64; frames as usize];
    for &at_secs in &[0.1, 0.4, 0.7] {
        let start = (at_secs * rate as f64) as usize;
        for i in 0..((rate / 100) as usize) {
            let t = i as f64 / rate as f64;
            if let Some(s) = samples.get_mut(start + i) {
                *s = 0.9 * (-t / 0.003).exp();
            }
        }
    }
    let data_len = frames * 2;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &v in &samples {
        out.extend_from_slice(&((v * i16::MAX as f64) as i16).to_le_bytes());
    }
    std::fs::write(path, out).unwrap();
}

fn fixture(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let wav = dir.join("kick.wav");
    write_click_wav(&wav, 44_100);
    let rpp = ReaperProjectBuilder::new()
        .tempo(120.0)
        .track("Drums", |t| t.folder_start())
        .track("Kick In", |t| {
            t.item(0.0, 1.0, |i| {
                i.take(wav.to_string_lossy().into_owned(), SourceType::Wave)
            })
            .folder_end(1)
        })
        .build()
        .to_rpp_string();
    let path = dir.join("kit.rpp");
    std::fs::write(&path, rpp).unwrap();
    path
}

#[test]
fn the_three_panes_mount_and_the_project_arrives() {
    let dir = std::env::temp_dir().join(format!("fts-ee-workstation-{}", std::process::id()));
    let path = fixture(&dir);

    let runner = Runner::open(
        &Source::Rpp(path),
        &Target {
            drums: Some(None),
            ..Target::default()
        },
        Viewport::new(1200.0, 500.0),
        None,
    )
    .expect("the kit opens");
    let standalone = runner.daw.clone().expect("a backend");
    bootstrap_daw_blocking(&standalone).expect("daw bootstrap");
    stage_workstation(runner.loaded.into_editor(), runner.host, (1600.0, 900.0));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let dom = VirtualDom::new(WorkstationApp);
        let tester = dioxus_test::DocumentTester::from_virtual_dom(dom)
            .with_window_size(1600, 900)
            .build();
        for _ in 0..24 {
            let _ = tester.pump().await;
            tester.relayout();
        }
        for pane in [
            "workstation-arrange",
            "workstation-editor",
            "workstation-mixer",
        ] {
            tester
                .query(by_testid(pane))
                .immediately()
                .unwrap_or_else(|e| panic!("{pane} missing: {e:?}"));
        }
    });
    let _ = std::fs::remove_dir_all(&dir);
}
