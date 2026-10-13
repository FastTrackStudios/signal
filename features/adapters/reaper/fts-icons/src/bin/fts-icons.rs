use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use fts_icons::config::{self, ConfigFile, IconEntry, Settings, StateSet, StyleOverride};
use fts_icons::{Output, Report, build, iconify, install};

#[derive(Parser)]
#[command(
    name = "fts-icons",
    version,
    about = "REAPER toolbar icon generator (Iconify-backed)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Search the Iconify collection
    Search {
        /// Search terms
        query: Vec<String>,
        #[arg(long, default_value_t = 48)]
        limit: u32,
    },
    /// Generate a single icon from an Iconify id
    Make(MakeArgs),
    /// Generate every icon in a TOML config
    Build {
        /// Config file (see `fts-icons init` for an example)
        config: PathBuf,
        #[command(flatten)]
        out: OutArgs,
    },
    /// Write an example icons.toml
    Init {
        #[arg(default_value = "icons.toml")]
        path: PathBuf,
    },
    /// Show detected REAPER resource paths
    Paths,
}

#[derive(Args)]
struct OutArgs {
    /// Install into REAPER resource path(s) (Data/toolbar_icons + 150/ + 200/)
    #[arg(long)]
    install: bool,
    /// Explicit REAPER resource path (repeatable; implies --install)
    #[arg(long = "resource-path")]
    resource_paths: Vec<String>,
    /// Output dir for non-install builds
    #[arg(short, long)]
    out: Option<PathBuf>,
}

impl From<&OutArgs> for Output {
    fn from(a: &OutArgs) -> Self {
        Output {
            install: a.install,
            resource_paths: a.resource_paths.clone(),
            out_dir: a.out.clone(),
        }
    }
}

#[derive(Args)]
struct MakeArgs {
    /// Iconify id, e.g. mdi:eye-outline
    id: String,
    /// Output filename (no extension); default derived from the id
    #[arg(long)]
    file: Option<String>,

    /// Normal-state icon color (hover/clicked inherit unless overridden)
    #[arg(long)]
    icon: Option<String>,
    #[arg(long)]
    bg: Option<String>,
    #[arg(long)]
    border: Option<String>,

    #[arg(long)]
    hover_icon: Option<String>,
    #[arg(long)]
    hover_bg: Option<String>,
    #[arg(long)]
    hover_border: Option<String>,

    #[arg(long)]
    clicked_icon: Option<String>,
    #[arg(long)]
    clicked_bg: Option<String>,
    #[arg(long)]
    clicked_border: Option<String>,

    /// Border stroke width (px at 100%)
    #[arg(long)]
    border_width: Option<f32>,
    /// Icon size inside the 30px cell (px at 100%)
    #[arg(long)]
    icon_size: Option<f32>,
    /// Background plate size (px at 100%)
    #[arg(long)]
    bg_size: Option<f32>,
    /// Plate/border corner radius (px at 100%)
    #[arg(long)]
    radius: Option<f32>,
    /// Cell width in px at 100% (default 30; 60 = double-wide)
    #[arg(long)]
    width: Option<f32>,

    #[command(flatten)]
    out: OutArgs,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Search { query, limit } => {
            let q = query.join(" ");
            if q.is_empty() {
                bail!("give a search term");
            }
            let icons = iconify::search(&q, limit)?;
            if icons.is_empty() {
                println!("no matches for {q:?}");
            }
            for id in icons {
                println!("{id}");
            }
        }
        Cmd::Make(args) => make(args)?,
        Cmd::Build { config, out } => {
            let cfg: ConfigFile = config::load(&config)?;
            if cfg.icons.is_empty() {
                bail!("config has no [[icon]] entries");
            }
            report(build(
                &cfg.defaults,
                &cfg.icons,
                &cfg.settings,
                &(&out).into(),
            )?);
        }
        Cmd::Init { path } => {
            if path.exists() {
                bail!("{} already exists", path.display());
            }
            std::fs::write(&path, config::EXAMPLE)?;
            println!("wrote {}", path.display());
        }
        Cmd::Paths => {
            let paths = install::detect_resource_paths();
            if paths.is_empty() {
                println!("no REAPER resource paths detected");
            }
            for p in paths {
                println!("{}", p.display());
            }
        }
    }
    Ok(())
}

fn make(args: MakeArgs) -> Result<()> {
    let file = args
        .file
        .clone()
        .unwrap_or_else(|| args.id.replace([':', '-', '.'], "_").to_lowercase());
    let sized = StyleOverride {
        border_width: args.border_width,
        icon_size: args.icon_size,
        bg_size: args.bg_size,
        corner_radius: args.radius,
        ..Default::default()
    };
    let entry = IconEntry {
        file,
        source: args.id.clone(),
        assign: None,
        width: args.width,
        all: (!sized.is_empty()).then_some(sized),
        normal: state_override(&args.icon, &args.bg, &args.border),
        hover: state_override(&args.hover_icon, &args.hover_bg, &args.hover_border),
        clicked: state_override(&args.clicked_icon, &args.clicked_bg, &args.clicked_border),
    };
    report(build(
        &StateSet::default(),
        &[entry],
        &Settings::default(),
        &(&args.out).into(),
    )?);
    Ok(())
}

fn state_override(
    icon: &Option<String>,
    bg: &Option<String>,
    border: &Option<String>,
) -> Option<StyleOverride> {
    let ov = StyleOverride {
        icon: icon.clone(),
        bg: bg.clone(),
        border: border.clone(),
        ..Default::default()
    };
    (!ov.is_empty()).then_some(ov)
}

fn report(r: Report) {
    for icon in &r.icons {
        for p in &icon.paths {
            println!("{}", p.display());
        }
    }
    for (ini, n) in &r.assignments {
        println!("{}: {n} toolbar button(s) updated", ini.display());
    }
    if !r.assignments.is_empty() {
        println!("restart REAPER (or reload menu sets) to pick up toolbar changes");
    }
    if r.skipped_assigns > 0 {
        println!(
            "note: {} assign(s) skipped — only applied with --install",
            r.skipped_assigns
        );
    }
    println!(
        "done: {} icon(s) → {} target(s)",
        r.icons.len(),
        r.roots.len()
    );
}
