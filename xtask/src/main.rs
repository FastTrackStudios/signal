use std::path::{Path, PathBuf};
use std::process::Command;

use reaper_test::runner::{self, TestPackage, TestRunner};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        Some("install") => install()?,
        Some("uninstall") => uninstall()?,
        Some("status") => fts_devtools::status(),
        // Delegate bundle (and any other nih_plug_xtask commands) to the bundler
        Some("bundle") => nih_plug_xtask::main()?,
        Some("reaper-test") => {
            let filter = args.get(1).cloned();
            let keep_open = args.iter().any(|a| a == "--keep-open");
            reaper_test(filter, keep_open)?;
        }
        _ => {
            eprintln!("usage: cargo xtask <command>");
            eprintln!();
            eprintln!("commands:");
            eprintln!("  install       Build and install signal-extension + fts-signal-controller into REAPER");
            eprintln!("  uninstall     Remove signal-extension and fts-signal-controller from REAPER");
            eprintln!("  bundle        Bundle CLAP plugins (delegates to nih_plug_xtask)");
            eprintln!("  status        Show installed extensions and plugins");
            eprintln!("  reaper-test   Run REAPER integration tests (signal-extension only)");
            std::process::exit(1);
        }
    }
    Ok(())
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn target_dir() -> PathBuf {
    workspace_root().join("target").join("debug")
}

fn install() -> Result<(), Box<dyn std::error::Error>> {
    let root = workspace_root();

    // ── 1. Build and install signal-extension ────────────────────────
    println!("── Building signal-extension ──");
    let status = Command::new("cargo")
        .args(["build", "-p", "signal-extension"])
        .current_dir(&root)
        .status()?;
    if !status.success() {
        return Err("cargo build -p signal-extension failed".into());
    }

    let binary = target_dir().join("signal-extension");
    fts_devtools::install_extension(&binary, "signal")?;
    println!("  Installed signal-extension");

    // ── 2. Bundle and install fts-signal-controller CLAP plugin ──────
    println!("\n── Bundling fts-signal-controller ──");
    let status = Command::new("cargo")
        .args([
            "run", "--package", "xtask", "--",
            "bundle", "fts-signal-controller",
        ])
        .current_dir(&root)
        .status()?;
    if !status.success() {
        return Err("Failed to bundle fts-signal-controller".into());
    }

    // Symlink the .clap into REAPER's UserPlugins/FX/ for each REAPER install
    let clap_file = "FTS Signal Controller.clap";
    let bundled = root.join("target/bundled").join(clap_file);

    if !bundled.exists() {
        return Err(format!("{clap_file} not found at {}", bundled.display()).into());
    }

    for reaper_dir in fts_devtools::reaper_dirs() {
        let fx_dir = reaper_dir.join("UserPlugins/FX");
        std::fs::create_dir_all(&fx_dir)?;

        let dest = fx_dir.join(clap_file);
        if dest.exists() || dest.is_symlink() {
            let _ = std::fs::remove_file(&dest).or_else(|_| std::fs::remove_dir_all(&dest));
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&bundled, &dest)?;
        #[cfg(not(unix))]
        std::fs::copy(&bundled, &dest)?;
        println!("  Installed {clap_file} -> {}", dest.display());
    }

    println!("\n✓ All components installed");
    Ok(())
}

fn uninstall() -> Result<(), Box<dyn std::error::Error>> {
    // Remove signal-extension
    fts_devtools::uninstall_extension("signal");

    // Remove fts-signal-controller CLAP from all REAPER installs
    let clap_file = "FTS Signal Controller.clap";
    for reaper_dir in fts_devtools::reaper_dirs() {
        let dest = reaper_dir.join("UserPlugins/FX").join(clap_file);
        if dest.exists() || dest.is_symlink() {
            let _ = std::fs::remove_file(&dest).or_else(|_| std::fs::remove_dir_all(&dest));
            println!("Removed {}", dest.display());
        }
    }

    Ok(())
}

fn reaper_test(
    filter: Option<String>,
    keep_open: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let ci = std::env::var("CI").is_ok();
    let timeout_secs: u64 = std::env::var("REAPER_TEST_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    let resources_dir = runner::fts_reaper_resources();

    let runner = TestRunner {
        resources_dir: resources_dir.clone(),
        extension_log: PathBuf::from("/tmp/daw-bridge.log"),
        timeout_secs,
        keep_open,
        ci,
        // Only load signal-extension — skip session-extension, sync-extension, etc.
        extension_whitelist: vec!["signal-extension".into()],
    };

    // ── Step 1: Build signal-extension ─────────────────────────────────
    runner::section(ci, "reaper-test: build signal-extension");
    println!("Building signal-extension...");
    let status = Command::new("cargo")
        .args(["build", "-p", "signal-extension"])
        .current_dir(workspace_root)
        .status()?;
    if !status.success() {
        return Err("Failed to build signal-extension".into());
    }
    runner::end_section(ci);

    // ── Step 2: Install into fts-extensions/ ───────────────────────────
    runner::section(ci, "reaper-test: install signal-extension");
    let user_plugins_dir = resources_dir.join("UserPlugins");
    let fts_ext_dir = user_plugins_dir.join("fts-extensions");
    std::fs::create_dir_all(&fts_ext_dir)?;

    let ext_src = workspace_root.join("target/debug/signal-extension");
    if ext_src.exists() {
        let ext_dst = fts_ext_dir.join("signal-extension");
        std::fs::copy(&ext_src, &ext_dst)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&ext_dst, std::fs::Permissions::from_mode(0o755))?;
        }
        println!("  Installed signal-extension -> {}", ext_dst.display());
    } else {
        return Err(format!(
            "signal-extension binary not found at {}",
            ext_src.display()
        )
        .into());
    }
    runner::end_section(ci);

    // ── Step 3: Build test binaries ────────────────────────────────────
    runner::section(ci, "reaper-test: build test binaries");
    println!("Building test binaries...");
    let status = Command::new("cargo")
        .args(["test", "-p", "signal", "--features", "daw", "--no-run"])
        .current_dir(workspace_root)
        .status()?;
    if !status.success() {
        return Err("Failed to build signal test binaries".into());
    }
    runner::end_section(ci);

    // ── Step 4: Clean, pre-warm, patch INI ─────────────────────────────
    runner.clean_stale_sockets();
    runner.prewarm_reaper();
    runner.patch_ini();

    // ── Step 5: Spawn REAPER ───────────────────────────────────────────
    let mut reaper = runner.spawn_reaper()?;
    reaper.wait_for_socket(&runner)?;

    // ── Step 6: Run tests ──────────────────────────────────────────────
    let packages = vec![TestPackage {
        package: "signal".into(),
        features: vec!["daw".into()],
        test_threads: 1,
        default_skips: vec![],
    }];

    let tests_passed = runner.run_tests(&mut reaper, &packages, filter.as_deref())?;

    // ── Step 7: Cleanup and report ─────────────────────────────────────
    if !tests_passed {
        reaper.report_failure(&runner);
        reaper.stop(&runner);
        return Err("Some tests failed".into());
    }

    reaper.stop(&runner);
    println!("\nAll signal tests passed!");
    Ok(())
}
