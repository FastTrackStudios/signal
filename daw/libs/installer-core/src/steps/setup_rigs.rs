//! Set up wrapper .app bundles for each selected rig profile.
//!
//! Each profile gets its own .app that launches REAPER via reaper-launcher
//! with a specific configuration (role, rig type, theme).

use std::path::Path;

use eyre::Context;
use tracing::info;

use crate::profiles::ALL_PROFILES;
use crate::progress::{EventSender, InstallEvent, InstallStep};

/// Create wrapper .app bundles for each selected profile.
///
/// - `reaper_dir`: The installed REAPER directory (contains REAPER.app)
/// - `launcher_bin`: Optional path to a pre-built reaper-launcher binary.
///   If None, the step is skipped.
/// - `selected_profiles`: Profile IDs to install (e.g. ["guitar", "drums"])
pub async fn setup_rigs(
    install_root: &Path,
    reaper_dir: &Path,
    launcher_bin: Option<&Path>,
    selected_profiles: &[String],
    tx: &EventSender,
) -> eyre::Result<()> {
    if selected_profiles.is_empty() {
        let _ = tx
            .send(InstallEvent::StepProgress {
                step: InstallStep::SetupRigs,
                fraction: 1.0,
                message: "No profiles selected, skipping".into(),
            })
            .await;
        return Ok(());
    }

    let launcher = match launcher_bin {
        Some(p) if p.exists() => p,
        _ => {
            let _ = tx
                .send(InstallEvent::StepProgress {
                    step: InstallStep::SetupRigs,
                    fraction: 1.0,
                    message: "Launcher binary not available, skipping rig setup".into(),
                })
                .await;
            return Ok(());
        }
    };

    let reaper_exe = reaper_dir.join("REAPER.app/Contents/MacOS/REAPER");
    let resources_dir = reaper_dir.to_string_lossy().to_string();
    let ini_path = reaper_dir.join("reaper.ini").to_string_lossy().to_string();

    let version = format!(
        "1.0.{}",
        std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );

    let total = selected_profiles.len();
    for (i, profile_id) in selected_profiles.iter().enumerate() {
        let profile = ALL_PROFILES.iter().find(|p| p.id == profile_id.as_str());

        let profile = match profile {
            Some(p) => p,
            None => continue,
        };

        let app_name = profile.display_name;
        let bundle_dir = install_root.join(format!("{app_name}.app"));
        let contents_dir = bundle_dir.join("Contents");
        let macos_dir = contents_dir.join("MacOS");
        let resources_app_dir = contents_dir.join("Resources");

        let _ = tx
            .send(InstallEvent::StepProgress {
                step: InstallStep::SetupRigs,
                fraction: i as f32 / total as f32,
                message: format!("Creating {app_name}.app..."),
            })
            .await;

        // Clean existing
        if bundle_dir.exists() {
            tokio::fs::remove_dir_all(&bundle_dir).await?;
        }

        tokio::fs::create_dir_all(&macos_dir).await?;
        tokio::fs::create_dir_all(&resources_app_dir).await?;

        // Copy reaper-launcher as the bundle executable
        tokio::fs::copy(launcher, macos_dir.join("REAPER"))
            .await
            .wrap_err_with(|| format!("Failed to copy launcher into {app_name}.app"))?;

        // Make executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                macos_dir.join("REAPER"),
                std::fs::Permissions::from_mode(0o755),
            )?;
        }

        // Write launch.json
        let launch_config = serde_json::json!({
            "role": "signal",
            "rig_type": profile.id,
            "reaper_executable": reaper_exe.to_string_lossy(),
            "resources_dir": resources_dir,
            "ini_path": ini_path,
            "ini_overrides": {},
            "restore_ini_after_launch": true,
            "reaper_args": ["-newinst", "-nosplash", "-ignoreerrors"]
        });
        let launch_json = serde_json::to_string_pretty(&launch_config)?;
        tokio::fs::write(contents_dir.join("launch.json"), &launch_json).await?;

        // Write Info.plist
        let bundle_id_suffix = profile.id.replace('-', "");
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>{app_name}</string>
    <key>CFBundleDisplayName</key>
    <string>{app_name}</string>
    <key>CFBundleIdentifier</key>
    <string>com.fasttrackstudio.{bundle_id_suffix}</string>
    <key>CFBundleExecutable</key>
    <string>REAPER</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleVersion</key>
    <string>{version}</string>
    <key>CFBundleShortVersionString</key>
    <string>{version}</string>
    <key>LSUIElement</key>
    <false/>
    <key>CFBundleIconFile</key>
    <string>main-mac</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>"#
        );
        tokio::fs::write(contents_dir.join("Info.plist"), &plist).await?;

        // Generate tinted + badged icon from the REAPER base icon
        let base_icon = reaper_dir.join("REAPER.app/Contents/Resources/main-mac.icns");
        let dest_icon = resources_app_dir.join("main-mac.icns");
        if base_icon.exists() {
            match crate::icon_gen::generate_rig_icns(&base_icon, &dest_icon, profile.id) {
                Ok(()) => info!("Generated icon for {app_name}"),
                Err(e) => {
                    tracing::warn!("Icon generation failed for {app_name}: {e:#}, using base icon");
                    tokio::fs::copy(&base_icon, &dest_icon).await?;
                }
            }
        }

        // Ad-hoc codesign so macOS doesn't block it
        let _ = tokio::process::Command::new("codesign")
            .args(["--force", "--sign", "-"])
            .arg(&bundle_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await;

        // Register with LaunchServices
        let _ = tokio::process::Command::new(
            "/System/Library/Frameworks/CoreServices.framework/\
             Frameworks/LaunchServices.framework/Support/lsregister",
        )
        .args(["-f"])
        .arg(&bundle_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;

        info!("Created {app_name}.app");
    }

    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::SetupRigs,
            fraction: 1.0,
            message: format!("Created {} rig apps", total),
        })
        .await;

    Ok(())
}
