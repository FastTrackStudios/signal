//! `fts-themer` — edit the FastTrackStudio REAPER theme from the shell.
//!
//! The same operations the web GUI drives, minus the preview.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use fts_themer::{ThemeDir, add_accent, color::Rgb, groups};

/// Default theme location, relative to the repo root.
const DEFAULT_THEME: &str = "features/reaper/fts-theme";

#[derive(Parser)]
#[command(
    name = "fts-themer",
    about = "Edit a REAPER theme's colors and artwork"
)]
struct Cli {
    /// Theme directory (holding <name>.ReaperTheme) or the .ReaperTheme itself.
    #[arg(long, short, global = true, default_value = DEFAULT_THEME)]
    theme: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show what the theme is and where its parts live.
    Info,
    /// List color keys and their values, grouped by the area they paint.
    Colors {
        /// Only keys whose name contains this substring.
        #[arg(long, short)]
        filter: Option<String>,
        /// Only this group (e.g. `mcp`, `arrange`).
        #[arg(long, short)]
        group: Option<String>,
    },
    /// Set one or more colors: `col_cursor=#00b0f9`.
    Set {
        /// `key=#rrggbb` pairs.
        #[arg(required = true)]
        assignments: Vec<String>,
    },
    /// List the accent (fader color) variants this theme offers.
    Accents,
    /// Generate a new accent variant, artwork and layouts.
    AddAccent {
        /// Folder/layout name, e.g. `crimson`.
        name: String,
        /// Target color, `#rrggbb`.
        color: String,
        /// Existing accent to recolor from.
        #[arg(long, default_value = "blue")]
        from: String,
        /// Take only hue and saturation from the color, keeping the source
        /// artwork's lightness — so the new accent sits tonally with the set.
        #[arg(long)]
        keep_tone: bool,
    },
    /// Paint this REAPER theme from the canonical FTS theme
    ///
    /// Only the keys the FTS palette determines are written; everything
    /// else in the .ReaperTheme is left exactly as it was.
    Apply {
        /// Canonical theme (.styx). Omit for the built-in FTS default.
        from: Option<PathBuf>,
        /// Show what would change without writing.
        #[arg(long)]
        dry_run: bool,
        /// Override one REAPER key exactly: `col_arrangebg=#424242`.
        ///
        /// The theme derives ~200 keys from twenty authored colours, which
        /// is what keeps the parts nobody thought about from drifting
        /// grey. This reaches the rest: anything set here is applied last
        /// and wins, so all ~420 of REAPER's keys are addressable without
        /// hand-authoring 420 colours. Repeatable.
        #[arg(long = "set", value_name = "KEY=#RRGGBB")]
        set: Vec<String>,
        /// Also write libSwell.colortheme into this REAPER resource dir —
        /// the menu bar, dialogs, buttons and lists, which the .ReaperTheme
        /// palette cannot reach.
        #[arg(long)]
        swell: Option<PathBuf>,
    },
    /// Restyle the theme's ARTWORK onto the palette
    ///
    /// The palette can't reach PNGs — toolbar backgrounds, mixer strips,
    /// button faces. This pushes every neutral pixel through a luminance
    /// ramp built from the theme, so chrome moves onto the theme's surfaces
    /// while bevels and gradients survive. LEDs, fader caps and WALTER
    /// marker pixels are left alone.
    Restyle {
        /// Canonical theme (.styx). Omit for the built-in FTS default.
        from: Option<PathBuf>,
        /// Report what would change without writing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Render the component-drawn artwork into the theme
    ///
    /// Each image is drawn by a Dioxus component and rasterised from the
    /// vector at 100/150/200 % — the same components the web GUI renders
    /// live. Replaces the inherited art rather than recolouring it.
    Generate {
        /// Report what would be written without writing.
        #[arg(long)]
        dry_run: bool,
        /// Also rewrite every image a vector control does *not* draw.
        ///
        /// Off by default, and it should stay off while the theme carries
        /// the palette it inherited. The traced path reproduces the
        /// inherited art through the theme's luminance ramp, which is
        /// right when the palette differs from the source and destructive
        /// when it does not: mapping the art onto its own colours still
        /// rounds through the ramp's stops. One run of it lifted
        /// `tcp_mainbg` from #333333 to #3d3d3d and washed out the toolbar
        /// icons, across 2547 images, with nothing in the output to say
        /// so — which is exactly why it is no longer the default.
        ///
        /// Recovering from it is `rsync -a --existing .source-art/ ./`
        /// inside the theme directory, then a plain `generate`.
        #[arg(long)]
        traced: bool,
    },
    /// Retint the colour literals inside rtconfig.txt
    ///
    /// WALTER scripts carry hardcoded RGB — the mixer strip body is
    /// `[0 0 0 0 61 61 61]` in a `set mcp_bg_color` line. The palette,
    /// the artwork and SWELL can all be perfectly dark while this keeps
    /// the theme's original greys.
    Walter {
        /// Report what would change without writing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Screenshot a real REAPER wearing this theme, on a private X display.
    Shot {
        /// Output PNG.
        #[arg(long, short, default_value = "target/theme-shots/theme.png")]
        out: PathBuf,
        /// Shoot against an existing resource dir (e.g. ~/fts-dev) instead of
        /// a throwaway profile. Its reaper.ini is restored afterwards.
        #[arg(long)]
        profile: Option<PathBuf>,
        /// Xvfb screen spec.
        #[arg(long, default_value = "1920x1200x24")]
        geometry: String,
        /// X display to run on.
        #[arg(long, default_value = ":97")]
        display: String,
        /// Seconds to let REAPER settle before capturing.
        #[arg(long, default_value_t = 14)]
        settle: u64,
        /// Extension library to install before launch (repeatable).
        ///
        /// Photographing a *panel* rather than the theme's own chrome means
        /// the extension that owns the panel has to be loaded.
        #[arg(long = "plugin")]
        plugins: Vec<PathBuf>,
        /// Action to run at startup, by named-command id (repeatable).
        ///
        /// e.g. `--action fts-mixer` to open the mixer panel. Written as a
        /// `__startup.lua` REAPER runs itself.
        #[arg(long = "action")]
        actions: Vec<String>,
        /// Window title to capture. Defaults to REAPER's own window; pass a
        /// panel's title to photograph a floating panel instead.
        #[arg(long)]
        window: Option<String>,
        /// Capture the whole screen rather than one window.
        #[arg(long)]
        full: bool,
    },
    /// Write the collapse thresholds into the theme's layout file.
    ///
    /// The Dioxus strip and the theme encode the same collapse heights;
    /// this makes the Rust constant the source of both. Idempotent, and it
    /// touches nothing but the value on each `set` line.
    Thresholds {
        /// Report what would change without writing.
        #[arg(long)]
        check: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut theme = ThemeDir::open(&cli.theme)
        .with_context(|| format!("open theme at {}", cli.theme.display()))?;

    match cli.command {
        Command::Info => {
            println!("name       {}", theme.name);
            println!("ini        {}", theme.ini_path().display());
            println!("images     {}", theme.images_dir().display());
            println!("rtconfig   {}", theme.rtconfig_path().display());
            println!("colors     {}", theme.ini().len());
            println!("accents    {}", theme.accents()?.join(", "));
        }

        Command::Colors { filter, group } => {
            let keys = theme.ini().keys();
            let wanted = group.map(|g| g.to_ascii_lowercase());
            for (grp, members) in groups::group_all(keys) {
                if let Some(w) = &wanted
                    && !grp.label().to_ascii_lowercase().contains(w)
                    && !format!("{grp:?}").to_ascii_lowercase().contains(w)
                {
                    continue;
                }
                let shown: Vec<&str> = members
                    .into_iter()
                    .filter(|k| filter.as_ref().is_none_or(|f| k.contains(f.as_str())))
                    .collect();
                if shown.is_empty() {
                    continue;
                }
                println!("\n{}", grp.label());
                for key in shown {
                    let raw = theme.ini().int(key).unwrap_or(0);
                    if groups::is_color(key) {
                        println!("  {key:<34} {}", Rgb::from_colorref(raw).to_hex());
                    } else {
                        println!("  {key:<34} {raw}");
                    }
                }
            }
        }

        Command::Set { assignments } => {
            for pair in &assignments {
                let (key, value) = pair
                    .split_once('=')
                    .with_context(|| format!("expected key=#rrggbb, got {pair:?}"))?;
                let color = Rgb::parse_hex(value).with_context(|| format!("color for {key}"))?;
                let before = theme.ini().color(key);
                theme.ini_mut().set_color(key, color);
                match before {
                    Some(b) => println!("{key}: {} -> {}", b.to_hex(), color.to_hex()),
                    None => println!("{key}: (new) -> {}", color.to_hex()),
                }
            }
            theme.save_ini()?;
            println!("\nWrote {}", theme.ini_path().display());
        }

        Command::Accents => {
            for accent in theme.accents()? {
                println!("{accent}");
            }
        }

        Command::AddAccent {
            name,
            color,
            from,
            keep_tone,
        } => {
            let color = Rgb::parse_hex(&color)?;
            let report = add_accent(&theme, &name, color, &from, keep_tone)?;
            for path in &report.images {
                println!("wrote {}", path.display());
            }
            match report.layouts {
                0 => println!(
                    "layouts already registered in {}",
                    report.rtconfig.display()
                ),
                n => println!("added {n} layout blocks to {}", report.rtconfig.display()),
            }
        }

        Command::Apply {
            from,
            dry_run,
            set,
            swell,
        } => {
            let mut source = fts_themer::apply::load_theme(from.as_deref())?;
            for pair in &set {
                let (key, value) = pair
                    .split_once('=')
                    .with_context(|| format!("expected KEY=#rrggbb, got {pair:?}"))?;
                let rgb = Rgb::parse_hex(value).with_context(|| format!("colour for {key}"))?;
                source
                    .overrides
                    .insert(key.to_string(), daw_theme::Color::rgb(rgb.r, rgb.g, rgb.b));
            }

            let report =
                fts_themer::apply::apply_theme_to(&cli.theme, &source, dry_run, swell.as_deref())?;
            for (key, before, after) in &report.changed {
                println!("{key:<26} {} -> {}", before.to_hex(), after.to_hex());
            }
            // Reported, not written: see `ApplyReport`. Both are silent
            // failures in REAPER, so they have to be loud here.
            for key in &report.unknown {
                eprintln!("SKIPPED {key}: this theme has no such key");
            }
            for key in &report.not_a_colour {
                eprintln!("SKIPPED {key}: not a colour (blend mode or flag)");
            }
            println!(
                "\n{} changed, {} already correct{}",
                report.changed.len(),
                report.unchanged,
                if dry_run {
                    " (dry run — nothing written)"
                } else {
                    ""
                }
            );
        }

        Command::Restyle { from, dry_run } => {
            let source = fts_themer::apply::load_theme(from.as_deref())?;
            let ramp = daw_theme::Ramp::for_chrome(&source);
            let report = fts_themer::restyle::restyle(&theme, &ramp, dry_run)?;
            for (path, err) in &report.failed {
                eprintln!("FAILED {}: {err}", path.display());
            }
            println!(
                "{} images restyled, {} already correct{}",
                report.changed.len(),
                report.unchanged,
                if dry_run {
                    " (dry run — nothing written)"
                } else {
                    ""
                }
            );
            if !report.failed.is_empty() {
                println!("{} failed", report.failed.len());
            }
        }

        Command::Generate { dry_run, traced } => {
            let report = fts_themer::generate::generate(&theme, dry_run, !traced)?;
            for (name, err) in &report.failed {
                eprintln!("FAILED {name}: {err}");
            }
            for path in &report.written {
                println!("wrote {}", path.display());
            }
            println!(
                "\n{} images generated{}",
                report.written.len(),
                if dry_run {
                    " (dry run — nothing written)"
                } else {
                    ""
                }
            );
            // Worth separating: only the vector ones are drawn from real
            // component geometry and get sharper at 150/200%. The rest
            // replay traced rects, which is pixel-exact but still a
            // picture of a bitmap — and a control quietly falling back to
            // its trace looks identical in the output.
            println!(
                "  {} from vector components, {} from traced art",
                report.vectorised.len(),
                report.written.len() - report.vectorised.len(),
            );
        }

        Command::Walter { dry_run } => {
            let source = fts_themer::apply::load_theme(None)?;
            let ramp = daw_theme::Ramp::for_chrome(&source);
            let changes = fts_themer::walter_colors::retint_file_from(
                &theme.rtconfig_path(),
                &theme.images_dir().join(fts_themer::restyle::SOURCE_DIR),
                &ramp,
                dry_run,
            )?;
            for c in changes.iter().take(20) {
                println!("{:>5}  {}", c.line, c.after);
            }
            println!(
                "\n{} lines retinted{}",
                changes.len(),
                if dry_run {
                    " (dry run — nothing written)"
                } else {
                    ""
                }
            );
        }

        Command::Thresholds { check } => {
            use fts_themer::thresholds;

            let path = cli.theme.join(format!("{}/rtconfig.txt", theme.name));
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("read {}", path.display()))?;

            let wanted = thresholds::generated_lines();
            let (patched, changed) = thresholds::splice(&text, &wanted)?;
            let mut drifted = thresholds::section_heights_agree(&text);
            drifted.extend(thresholds::offsets_agree(&text));

            for line in &drifted {
                eprintln!("  DRIFT: {line}");
            }
            if check {
                for t in &wanted {
                    println!("  {} = {}", t.name, t.value);
                }
                if changed > 0 {
                    anyhow::bail!("{changed} threshold(s) in the layout file are out of date");
                }
                if !drifted.is_empty() {
                    anyhow::bail!(
                        "{} stated value(s) disagree with the Rust constants",
                        drifted.len()
                    );
                }
                println!("  thresholds are current");
            } else if changed == 0 {
                println!("  thresholds already current — nothing written");
            } else {
                std::fs::write(&path, patched)
                    .with_context(|| format!("write {}", path.display()))?;
                println!("  wrote {changed} threshold(s) to {}", path.display());
            }
        }
        Command::Shot {
            out,
            profile,
            geometry,
            display,
            settle,
            plugins,
            actions,
            window,
            full,
        } => {
            use fts_themer::shot::{self, Profile, ShotOptions};

            let mut opts = ShotOptions::new(&cli.theme, &out);
            opts.geometry = geometry;
            opts.display = display;
            opts.settle = std::time::Duration::from_secs(settle);
            opts.plugins = plugins;
            opts.startup_actions = actions;
            opts.window = if full {
                fts_themer::shot::Capture::Screen
            } else if let Some(title) = window {
                fts_themer::shot::Capture::Window(title)
            } else {
                opts.window
            };
            if let Some(dir) = profile {
                opts.profile = Profile::Existing(dir);
            }
            println!("  theme:   {}", theme.name);
            println!("  tracks:  {}", opts.tracks.len());
            let path = shot::capture(&opts)?;
            println!("\nWrote {}", path.display());
        }
    }

    Ok(())
}
