use std::path::PathBuf;
use std::process::Command;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        Some("install") => install(),
        Some("uninstall") => uninstall(),
        Some("status") => fts_devtools::status(),
        _ => {
            eprintln!("usage: cargo xtask <command>");
            eprintln!();
            eprintln!("commands:");
            eprintln!("  install     Build and symlink signal-extension into REAPER");
            eprintln!("  uninstall   Remove signal-extension symlink from REAPER");
            eprintln!("  status      Show installed extensions and plugins");
            std::process::exit(1);
        }
    }
}

fn install() {
    // Build the extension
    let status = Command::new("cargo")
        .args(["build", "-p", "signal-extension"])
        .status()
        .expect("failed to run cargo build");

    if !status.success() {
        eprintln!("cargo build failed");
        std::process::exit(1);
    }

    // Find the built binary
    let binary = target_dir().join("signal-extension");
    fts_devtools::install_extension(&binary, "signal")
        .expect("failed to install signal extension");
}

fn uninstall() {
    fts_devtools::uninstall_extension("signal");
}

fn target_dir() -> PathBuf {
    // Walk up from xtask dir to workspace root, then into target/debug
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().unwrap().join("target").join("debug")
}
