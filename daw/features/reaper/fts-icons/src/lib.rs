//! REAPER toolbar icon generation.
//!
//! A REAPER toolbar icon is one PNG holding three cells side by side
//! (normal / hover / clicked), 30×30 per cell at 100%, with 150% and 200%
//! variants under `150/` and `200/` — same filename, no suffix. This crate
//! renders those strips from Iconify ids or generated text, installs them into
//! REAPER resource paths, and optionally points toolbar buttons at them.
//!
//! Two ways to assign an icon to a button:
//!
//! - **Offline** — [`menu::apply_assignments`] rewrites `icon_N=` lines in
//!   `reaper-menu.ini` (matched by command id, so assignments survive
//!   reordering). Requires a REAPER restart / menu-set reload.
//! - **Live** — with the `toolbar` feature, [`BuiltIcon::toolbar_icon`] yields a
//!   `daw_proto::ToolbarIcon` to hand to the `Toolbar` RPC service
//!   (`set_button_icon` / `add_button`) in a running REAPER.

pub mod color;
pub mod config;
pub mod iconify;
pub mod install;
pub mod menu;
pub mod render;
pub mod source;

use anyhow::{Context, Result};
use std::path::PathBuf;

use config::{IconEntry, Settings, StateSet};
use render::SCALES;

/// Where rendered strips go.
#[derive(Debug, Default, Clone)]
pub struct Output {
    /// Install into detected/configured REAPER resource paths.
    pub install: bool,
    /// Explicit resource paths (implies `install`).
    pub resource_paths: Vec<String>,
    /// Plain output dir, used when not installing.
    pub out_dir: Option<PathBuf>,
}

impl Output {
    fn installing(&self) -> bool {
        self.install || !self.resource_paths.is_empty()
    }
}

/// One rendered icon and where it landed.
#[derive(Debug, Clone)]
pub struct BuiltIcon {
    /// Filename stem (no extension, no scale suffix).
    pub file: String,
    /// Every PNG written for this icon (3 scales × output roots).
    pub paths: Vec<PathBuf>,
    /// Toolbar command id this icon wants to be assigned to, if any.
    pub assign: Option<String>,
}

impl BuiltIcon {
    /// The icon's REAPER file name (`<stem>.png`) — what a toolbar entry stores.
    pub fn file_name(&self) -> String {
        format!("{}.png", self.file)
    }

    /// This icon as a live-toolbar value for the `Toolbar` RPC service.
    ///
    /// Resolved by name, so REAPER looks it up in `Data/toolbar_icons` —
    /// which is exactly where [`build`] installed it.
    #[cfg(feature = "toolbar")]
    pub fn toolbar_icon(&self) -> daw_proto::ToolbarIcon {
        daw_proto::ToolbarIcon {
            kind: daw_proto::ToolbarIconKind::FileName,
            value: self.file_name(),
        }
    }
}

/// Result of a [`build`] run.
#[derive(Debug, Default, Clone)]
pub struct Report {
    pub icons: Vec<BuiltIcon>,
    /// Output roots the strips were written under.
    pub roots: Vec<PathBuf>,
    /// `(reaper-menu.ini path, buttons updated)` per resource path.
    pub assignments: Vec<(PathBuf, usize)>,
    /// Assignments requested but skipped because nothing was installed.
    pub skipped_assigns: usize,
}

/// Render every icon, install it, and apply `assign =` toolbar assignments.
pub fn build(
    defaults: &StateSet,
    icons: &[IconEntry],
    settings: &Settings,
    out: &Output,
) -> Result<Report> {
    let installing = out.installing();
    let mut resources: Vec<PathBuf> = Vec::new();
    let roots: Vec<PathBuf> = if installing {
        let explicit = if out.resource_paths.is_empty() {
            settings.resource_paths.clone().unwrap_or_default()
        } else {
            out.resource_paths.clone()
        };
        resources = install::resolve_targets(&explicit)?;
        resources
            .iter()
            .map(|r| install::toolbar_icons_dir(r))
            .collect()
    } else {
        let dir = out
            .out_dir
            .clone()
            .or_else(|| settings.out_dir.as_deref().map(install::expand))
            .unwrap_or_else(|| PathBuf::from("out"));
        vec![dir]
    };

    let mut report = Report {
        roots: roots.clone(),
        ..Default::default()
    };

    for icon in icons {
        let width = icon.width.or(settings.width).unwrap_or(render::BASE_CELL);
        let svg = source::resolve(&icon.source, width / render::BASE_CELL)
            .with_context(|| format!("icon {:?}", icon.file))?;
        let states = config::resolve(defaults, icon)?;
        let strips: Vec<_> = SCALES
            .iter()
            .map(|(scale, _)| render::render_strip(&svg, &states, *scale, width))
            .collect::<Result<_>>()?;
        let strips: [_; 3] = strips.try_into().expect("3 scales");
        let mut paths = Vec::new();
        for root in &roots {
            paths.extend(install::write_icon(root, &icon.file, &strips)?);
        }
        report.icons.push(BuiltIcon {
            file: icon.file.clone(),
            paths,
            assign: icon.assign.clone(),
        });
    }

    let assigns: Vec<(String, String)> = icons
        .iter()
        .filter_map(|i| i.assign.clone().map(|cmd| (cmd, i.file.clone())))
        .collect();
    if !assigns.is_empty() {
        if installing {
            for res in &resources {
                let n = menu::apply_assignments(res, &assigns)?;
                report.assignments.push((res.join("reaper-menu.ini"), n));
            }
        } else {
            report.skipped_assigns = assigns.len();
        }
    }

    Ok(report)
}
