//! The workstation, rasterized to a PNG instead of opened.
//!
//! ```sh
//! cargo run -p expression-editor-standalone --example workstation_shot -- \
//!     song.rpp --drums --size 1920x1080 --out target/gui-shots/workstation.png
//! ```
//!
//! Same load as `--example workstation`, painted through dioxus-test's
//! CPU rasterizer — the picture and the window are the same mount.

use std::path::PathBuf;

use dioxus::prelude::VirtualDom;
use dioxus_test::DocumentTester;
use expression_editor_standalone::cli::ArgsError;
use expression_editor_standalone::workstation::{
    WorkstationApp, bootstrap_daw_blocking, stage_workstation,
};
use expression_editor_standalone::{Args, Runner};

fn main() {
    let args = match Args::from_env() {
        Ok(a) => a,
        Err(e @ (ArgsError::Help | ArgsError::List)) => {
            print!("{e}");
            return;
        }
        Err(e) => {
            eprintln!("{e}\n\n{}", expression_editor_standalone::cli::USAGE);
            std::process::exit(2);
        }
    };

    let runner = match Runner::open(&args.source, &args.target, args.viewport(), args.mode) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let Some(standalone) = runner.daw.clone() else {
        eprintln!("the workstation needs a project — open a .rpp, not a demo scene");
        std::process::exit(1);
    };
    if let Err(e) = bootstrap_daw_blocking(&standalone) {
        eprintln!("daw bootstrap failed: {e}");
        std::process::exit(1);
    }

    let out = args.out.clone().unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../target/gui-shots/expression-editor-standalone/workstation.png")
    });
    if let Some(dir) = out.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        eprintln!("create {}: {e}", dir.display());
        std::process::exit(1);
    }

    stage_workstation(
        runner.loaded.into_editor(),
        runner.host,
        (args.width as f64, args.height as f64),
    );

    // A plain current-thread runtime for the tester's pumps — main
    // itself must not be async: the daw bootstrap above builds and
    // blocks on its own runtime, which a #[tokio::main] would forbid.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("shot runtime");
    rt.block_on(async {
        let dom = VirtualDom::new(WorkstationApp);
        let tester = DocumentTester::from_virtual_dom(dom)
            .with_window_size(args.width, args.height)
            .build();
        // More pumps than the editor's shot: the project fetch is async
        // (tracks, items, peaks through the in-process link) and the
        // panes fill only once it resolves.
        for _ in 0..24 {
            let _ = tester.pump().await;
            tester.relayout();
        }
        tester.render_png(&out);
        println!("shot: {}", out.display());
    });
}
