//! reaper-launcher entry point.
//!
//! Modes:
//! - Default: read launch.json, patch reaper.ini, exec REAPER
//! - `install-icons --id <id> --rig-type <type>`: generate and install XDG icons

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && args[1] == "install-icons" {
        #[cfg(feature = "icon-gen")]
        {
            install_icons(&args[2..]);
        }
        #[cfg(not(feature = "icon-gen"))]
        {
            eprintln!("reaper-launcher was built without the icon-gen feature");
            std::process::exit(1);
        }
    } else {
        reaper_launcher::launch();
    }
}

#[cfg(feature = "icon-gen")]
fn install_icons(args: &[String]) {
    let mut id = None;
    let mut rig_type = None;
    let mut no_tint = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--id" => {
                id = args.get(i + 1).map(|s| s.as_str());
                i += 2;
            }
            "--rig-type" => {
                rig_type = args.get(i + 1).map(|s| s.as_str());
                i += 2;
            }
            "--no-tint" => {
                no_tint = true;
                i += 1;
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                std::process::exit(1);
            }
        }
    }

    let id = id.unwrap_or_else(|| {
        eprintln!("Usage: reaper-launcher install-icons --id <id> --rig-type <type>");
        std::process::exit(1);
    });
    let rig_type = rig_type.unwrap_or_else(|| {
        eprintln!("Usage: reaper-launcher install-icons --id <id> --rig-type <type>");
        std::process::exit(1);
    });

    let (color, badge) = reaper_launcher::icon_gen::rig_appearance(rig_type).unwrap_or_else(|| {
        eprintln!("Unknown rig type: {rig_type}");
        std::process::exit(1);
    });

    let config = reaper_launcher::icon_gen::IconConfig {
        badge_text: badge.to_string(),
        color,
        sizes: vec![48, 128, 256],
        no_tint,
    };

    match reaper_launcher::icon_gen::generate_and_install_icons(id, &config) {
        Ok(()) => eprintln!("Icons installed for {id}"),
        Err(e) => {
            eprintln!("Failed to install icons: {e}");
            std::process::exit(1);
        }
    }
}
