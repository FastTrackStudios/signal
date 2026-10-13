//! Surgical edits to `rtconfig.txt`.
//!
//! `rtconfig.txt` is hand-written WALTER with meaningful comments and
//! formatting, and it is the file a themer edits most. Nothing here reformats
//! it: edits find the relevant block and splice lines in beside it, matching
//! the surrounding indentation, so a generated accent reads like the nine that
//! shipped with the theme.

use anyhow::{Result, bail};

/// The three DPI tiers, as `(layout name prefix, image folder prefix)`.
const TIERS: [(&str, &str); 3] = [("", ""), ("150%_", "150/"), ("200%_", "200/")];

/// Layout-name prefix for the mixer fader-accent variants.
const ACCENT_LAYOUT: &str = "A_Fader_";

/// Capitalise for a layout name: `crimson` → `Crimson`.
fn layout_case(name: &str) -> String {
    let mut c = name.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Image folders registered as fader-accent variants, at 100% (no DPI prefix).
///
/// This is the authoritative list. Going by the filesystem instead — "any
/// subfolder holding the accent image" — wrongly catches layout variants like
/// `strip/`, which overrides the fader cap among ten other images and is not
/// an accent at all.
pub fn accent_folders(text: &str) -> Vec<String> {
    let marker = format!("Layout \"{ACCENT_LAYOUT}");
    let mut found: Vec<String> = text
        .lines()
        .map(str::trim_start)
        .filter(|l| l.starts_with(&marker))
        .filter_map(|l| {
            // Layout "A_Fader_Blue" "blue"  →  the second quoted field.
            let mut quoted = l.split('"').skip(1).step_by(2);
            let _name = quoted.next()?;
            let folder = quoted.next()?.trim();
            (!folder.is_empty()).then(|| folder.to_string())
        })
        .collect();
    found.sort();
    found.dedup();
    found
}

/// Does an accent layout for `name` already exist?
pub fn has_accent_layout(text: &str, name: &str) -> bool {
    let want = format!("{ACCENT_LAYOUT}{}\"", layout_case(name));
    text.lines()
        .any(|l| l.trim_start().starts_with("Layout \"") && l.contains(&want))
}

/// Register an accent variant by splicing a `Layout` block into each DPI tier.
///
/// Mirrors the shipped blocks exactly:
///
/// ```text
/// Layout "A_Fader_Purple" "purple"
///     set mcp.label .
/// EndLayout
/// ```
///
/// Returns the number of tiers patched. Idempotent: a name that's already
/// registered is left alone and reported as 0.
pub fn add_accent_layout(text: &str, name: &str, folder: &str) -> Result<(String, usize)> {
    if has_accent_layout(text, name) {
        return Ok((text.to_string(), 0));
    }

    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut patched = 0;

    // Work back to front so earlier insert points keep their indices.
    for (layout_prefix, folder_prefix) in TIERS.into_iter().rev() {
        let marker = format!("Layout \"{layout_prefix}{ACCENT_LAYOUT}");
        // The last existing accent block in this tier is our template.
        let Some(last) = lines
            .iter()
            .rposition(|l| l.trim_start().starts_with(&marker))
        else {
            continue;
        };
        let Some(end) = lines[last..]
            .iter()
            .position(|l| l.trim_start().starts_with("EndLayout"))
            .map(|off| last + off)
        else {
            bail!("accent layout at line {} has no EndLayout", last + 1);
        };

        // Reuse the template's own indentation, tabs and all.
        let outer = indent_of(&lines[last]);
        let inner = indent_of(lines.get(last + 1).map_or("", String::as_str));
        let inner = if inner.len() > outer.len() {
            inner
        } else {
            format!("{outer}\t")
        };

        let block = vec![
            String::new(),
            format!(
                "{outer}Layout \"{layout_prefix}{ACCENT_LAYOUT}{}\" \"{folder_prefix}{folder}\"",
                layout_case(name)
            ),
            format!("{inner}set mcp.label ."),
            format!("{outer}EndLayout"),
        ];
        lines.splice(end + 1..end + 1, block);
        patched += 1;
    }

    if patched == 0 {
        bail!(
            "no existing \"{ACCENT_LAYOUT}*\" layouts to extend — is this a Reapertips-derived theme?"
        );
    }

    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    Ok((out, patched))
}

/// Leading whitespace of a line, verbatim (tabs stay tabs).
fn indent_of(line: &str) -> String {
    line.chars().take_while(|c| c.is_whitespace()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RT: &str = "\
Layout \"A\"
\tLayout \"A_Fader_Blue\" \"blue\"
\t\tset mcp.label .
\tEndLayout
EndLayout

Layout \"150%_A\" \"150\"
\tLayout \"150%_A_Fader_Blue\" \"150/blue\"
\t\tset mcp.label .
\tEndLayout
EndLayout

Layout \"200%_A\" \"200\"
\tLayout \"200%_A_Fader_Blue\" \"200/blue\"
\t\tset mcp.label .
\tEndLayout
EndLayout
";

    #[test]
    fn patches_all_three_dpi_tiers() {
        let (out, n) = add_accent_layout(RT, "crimson", "crimson").unwrap();
        assert_eq!(n, 3);
        assert!(out.contains("Layout \"A_Fader_Crimson\" \"crimson\""));
        assert!(out.contains("Layout \"150%_A_Fader_Crimson\" \"150/crimson\""));
        assert!(out.contains("Layout \"200%_A_Fader_Crimson\" \"200/crimson\""));
    }

    #[test]
    fn new_block_stays_inside_its_parent_layout() {
        let (out, _) = add_accent_layout(RT, "crimson", "crimson").unwrap();
        let lines: Vec<&str> = out.lines().collect();
        let new = lines
            .iter()
            .position(|l| l.contains("\"A_Fader_Crimson\""))
            .unwrap();
        // The next unindented EndLayout closes Layout "A" — ours must precede it.
        let parent_end = lines
            .iter()
            .skip(new)
            .position(|l| *l == "EndLayout")
            .unwrap();
        assert!(parent_end > 0, "block escaped its parent:\n{out}");
        // And it must come after the template block it was modelled on.
        let template = lines
            .iter()
            .position(|l| l.contains("\"A_Fader_Blue\""))
            .unwrap();
        assert!(template < new);
    }

    #[test]
    fn indentation_matches_the_surrounding_blocks() {
        let (out, _) = add_accent_layout(RT, "crimson", "crimson").unwrap();
        let line = out
            .lines()
            .find(|l| l.contains("\"A_Fader_Crimson\""))
            .unwrap();
        assert!(line.starts_with('\t'), "lost the tab indent: {line:?}");
        assert!(!line.starts_with("\t\t"), "over-indented: {line:?}");
    }

    #[test]
    fn is_idempotent() {
        let (once, n1) = add_accent_layout(RT, "crimson", "crimson").unwrap();
        let (twice, n2) = add_accent_layout(&once, "crimson", "crimson").unwrap();
        assert_eq!((n1, n2), (3, 0));
        assert_eq!(once, twice);
    }

    #[test]
    fn name_is_capitalised_for_the_layout_but_not_the_folder() {
        let (out, _) = add_accent_layout(RT, "hot pink", "hot pink").unwrap();
        assert!(out.contains("Layout \"A_Fader_Hot pink\" \"hot pink\""));
    }

    #[test]
    fn untouched_lines_survive_verbatim() {
        let (out, _) = add_accent_layout(RT, "crimson", "crimson").unwrap();
        for line in RT.lines() {
            assert!(
                out.lines().any(|l| l == line),
                "lost original line {line:?}"
            );
        }
    }

    #[test]
    fn accent_folders_reads_the_100_percent_tier_only() {
        // The 150%/200% tiers name the same folder with a DPI prefix; the
        // caller wants "blue", not "blue, 150/blue, 200/blue".
        assert_eq!(accent_folders(RT), vec!["blue"]);
    }

    #[test]
    fn accent_folders_ignores_non_accent_layouts() {
        let rt = "Layout \"A_Strip\" \"strip\"\nEndLayout\n";
        assert!(accent_folders(rt).is_empty());
    }

    #[test]
    fn errors_when_the_theme_has_no_accent_layouts() {
        assert!(add_accent_layout("version 6.0\n", "x", "x").is_err());
    }
}
