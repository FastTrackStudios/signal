//! Browse a preset library from the terminal — the same model the plugin
//! editors drive, with no UI in the way.
//!
//! ```text
//! cargo run -p preset-browser --example browse -- <library-dir> [query]
//! ```
//!
//! Useful for checking a library loaded correctly, and for working on the
//! browsing behaviour without opening a plugin.

use preset_browser::{load_directory, PresetBrowser, SortMode};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(dir) = args.next() else {
        eprintln!("usage: browse <library-dir> [query]");
        std::process::exit(2);
    };
    let query = args.collect::<Vec<_>>().join(" ");

    let report = match load_directory(&dir) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    if !report.skipped.is_empty() {
        println!("skipped {} unreadable file(s):", report.skipped.len());
        for (path, why) in report.skipped.iter().take(3) {
            println!("  {}: {why}", path.display());
        }
    }

    let mut browser = PresetBrowser::new(report.presets);
    println!("{} presets, {} categories", browser.all().len(), browser.categories().len());

    let verified = browser
        .all()
        .iter()
        .filter(|p| p.tags.iter().any(|t| t == "verified"))
        .count();
    println!("{verified} verified against their reference");

    if !query.is_empty() {
        browser.set_query(&query);
        println!("\nsearch {query:?} -> {} matches", browser.visible_count());
    }

    browser.set_sort_mode(SortMode::Category);
    println!("\nfirst 12 in category order:");
    for &i in browser.visible().iter().take(12) {
        let p = &browser.all()[i];
        let quality = p
            .match_error.map_or_else(|| "—".into(), |e| format!("{e:.3}"));
        println!(
            "  {:<34} {:<18} err {:<6} {} params",
            p.name.chars().take(34).collect::<String>(),
            p.category.as_deref().unwrap_or("—").chars().take(18).collect::<String>(),
            quality,
            p.parameters.len()
        );
    }

    // Stepping is what a next/previous button does.
    browser.select_next();
    if let Some(p) = browser.selected() {
        println!("\nselected: {} ({} parameters)", p.name, p.parameters.len());
        for (name, value) in p.parameters.iter().take(6) {
            println!("    {name:<16} {value:.4}");
        }
    }
}
