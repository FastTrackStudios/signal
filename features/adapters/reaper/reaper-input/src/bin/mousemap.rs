//! `mousemap` — translate a REAPER mouse map into fts-extensions styx.
//!
//! Reads a `.ReaperMouseMap` export (or a `reaper-mouse.ini`) and emits a
//! `mouse-profile.styx` settings block with **decoded behavior names**, using
//! reaper-input's own `mouse_modifiers::behaviors` tables — the same data the
//! plugin uses at runtime, so names always match.
//!
//! Usage:
//!   mousemap <map.ReaperMouseMap> [options]
//!
//! Options:
//!   --all              Include Win/Super combos (mm_8-15). Default: mm_0-7 only.
//!   --table            Print a human-readable table instead of styx.
//!   --settings-only    Emit just the `settings (...)` block (no name/description header).
//!   -o, --out `<FILE>`   Write to `<FILE>` instead of stdout.
//!   -h, --help         Show this help.
//!
//! REAPER modifier index → mods: bit0=Shift bit1=Ctrl/Cmd bit2=Alt/Opt bit3=Win/Super.

use reaper_input::input::mouse_modifiers::behaviors::get_mouse_modifier_name;
use reaper_input::input::mouse_modifiers::types::{MouseButtonInput, MouseModifierContext};

fn mods_str(n: u32) -> String {
    let mut p = Vec::new();
    if n & 1 != 0 {
        p.push("S");
    }
    if n & 2 != 0 {
        p.push("C");
    }
    if n & 4 != 0 {
        p.push("A");
    }
    if n & 8 != 0 {
        p.push("W");
    }
    if p.is_empty() {
        String::new()
    } else {
        format!("<{}->", p.join("-"))
    }
}

/// Derive the mouse interaction from the `MM_CTX_*` suffix.
fn button_for(ctx: &str) -> MouseButtonInput {
    if ctx.ends_with("_MMOUSE_CLK") {
        MouseButtonInput::MiddleClick
    } else if ctx.ends_with("_MMOUSE") {
        MouseButtonInput::MiddleDrag
    } else if ctx.ends_with("_RMOUSE") {
        MouseButtonInput::RightDrag
    } else if ctx.ends_with("_DBLCLK") {
        MouseButtonInput::DoubleClick
    } else if ctx.ends_with("_CLK") {
        MouseButtonInput::Click
    } else {
        MouseButtonInput::LeftDrag
    }
}

/// Decode `(context, behavior_id)` → display name, or `None` if unmapped.
fn decode_name(ctx: &str, behavior_id: u32) -> Option<String> {
    let name = MouseModifierContext::from_reaper_string(ctx)
        .map(|c| get_mouse_modifier_name(&c, button_for(ctx), behavior_id))?;
    let is_raw = name.is_empty()
        || name == format!("{behavior_id} m")
        || name == format!("{behavior_id}")
        || name.starts_with("Unknown")
        || name.eq_ignore_ascii_case("unknown");
    if is_raw { None } else { Some(name) }
}

struct Entry {
    n: u32,
    behavior: String, // raw value, e.g. "2 m"
    id: u32,
}

fn parse_map(src: &str) -> Vec<(String, Vec<Entry>)> {
    let mut out: Vec<(String, Vec<Entry>)> = Vec::new();
    for line in src.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix('[') {
            let ctx = rest.trim_end_matches(']').to_string();
            if ctx.starts_with("MM_CTX_") {
                out.push((ctx, Vec::new()));
            }
            continue;
        }
        if let Some((k, v)) = line.split_once('=')
            && let Some(ns) = k.strip_prefix("mm_")
            && let Ok(n) = ns.parse::<u32>()
            && let Some((_, entries)) = out.last_mut()
        {
            let behavior = v.trim().to_string();
            let id = behavior
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            entries.push(Entry { n, behavior, id });
        }
    }
    out
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut path: Option<String> = None;
    let mut include_win = false;
    let mut table = false;
    let mut settings_only = false;
    let mut out_file: Option<String> = None;

    while let Some(a) = args.next() {
        match a.as_str() {
            "--all" => include_win = true,
            "--table" => table = true,
            "--settings-only" => settings_only = true,
            "-o" | "--out" => out_file = args.next(),
            "-h" | "--help" => {
                eprintln!(
                    "mousemap <map.ReaperMouseMap> [--all] [--table] [--settings-only] [-o FILE]"
                );
                return;
            }
            other if path.is_none() => path = Some(other.to_string()),
            other => {
                eprintln!("mousemap: unexpected argument: {other}");
                std::process::exit(2);
            }
        }
    }

    let Some(path) = path else {
        eprintln!("mousemap: missing input map path. See --help.");
        std::process::exit(2);
    };
    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("mousemap: cannot read {path}: {e}");
            std::process::exit(1);
        }
    };

    let map = parse_map(&src);
    let mut body = String::new();
    let (mut n_ctx, mut n_set, mut n_named) = (0u32, 0u32, 0u32);

    for (ctx, entries) in &map {
        let mut rows: Vec<&Entry> = entries.iter().filter(|e| include_win || e.n < 8).collect();
        if rows.is_empty() {
            continue;
        }
        rows.sort_by_key(|e| e.n);
        n_ctx += 1;
        if table {
            body.push_str(&format!("{ctx}\n"));
            for e in rows {
                n_set += 1;
                let name = decode_name(ctx, e.id);
                if name.is_some() {
                    n_named += 1;
                }
                let m = mods_str(e.n);
                let m = if m.is_empty() { "(none)" } else { &m };
                body.push_str(&format!(
                    "    {m:10} {:8} {}\n",
                    e.behavior,
                    name.as_deref().unwrap_or("?")
                ));
            }
        } else {
            body.push_str(&format!("\n    // {ctx}\n"));
            for e in rows {
                n_set += 1;
                let desc = match decode_name(ctx, e.id) {
                    Some(name) => {
                        n_named += 1;
                        format!(", desc \"{}\"", name.replace('"', "'"))
                    }
                    None => String::new(),
                };
                body.push_str(&format!(
                    "    {{ctx {ctx}, mods \"{}\", behavior \"{}\"{desc}}}\n",
                    mods_str(e.n),
                    e.behavior
                ));
            }
        }
    }

    let output = if table {
        body
    } else if settings_only {
        format!("settings (\n{body})\n")
    } else {
        format!(
            "name        \"FastTrackStudio\"\n\
             description \"FastTrackStudio mouse modifiers (generated by `mousemap` from a REAPER export)\"\n\
             \n\
             // mods: <S->=Shift  <C->=Ctrl/Cmd  <A->=Alt/Opt  <W->=Win/Super.\n\
             \n\
             settings (\n{body})\n"
        )
    };

    match out_file {
        Some(f) => {
            if let Err(e) = std::fs::write(&f, &output) {
                eprintln!("mousemap: cannot write {f}: {e}");
                std::process::exit(1);
            }
            eprintln!("mousemap: wrote {f} — {n_ctx} contexts, {n_set} settings, {n_named} named");
        }
        None => {
            print!("{output}");
            eprintln!(
                "mousemap: {n_ctx} contexts, {n_set} settings, {n_named} named{}",
                if include_win {
                    ""
                } else {
                    " (mm_0-7; pass --all for Win/Super combos)"
                }
            );
        }
    }
}
