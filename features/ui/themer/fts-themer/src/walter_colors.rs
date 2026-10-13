//! Theming the colour literals inside `rtconfig.txt`.
//!
//! The fourth place REAPER takes colour from, and the least obvious:
//! WALTER scripts carry **hardcoded RGB literals**.
//!
//! ```text
//! set mcp_bg_color  theme_version>1 [0 0 0 0 61 61 61] [0 0 0 0 51 51 51]
//! ```
//!
//! `61 61 61` is the mixer strip body. It is not a palette key, not an
//! image, and not SWELL — so a theme can have all three of those perfectly
//! dark and still render its mixer in the original's grey, which is exactly
//! what happened here.
//!
//! # Only `*color*` assignments
//!
//! WALTER's bracket syntax is overloaded: `[x y w h ls ts rs bs]` is a
//! rectangle and `[r g b]` is a colour, with no syntactic difference. So
//! rewriting every bracket would silently relayout the entire theme.
//!
//! Only lines assigning a variable whose **name contains `color`** are
//! touched, and within those only the trailing 3-tuples — which covers the
//! `[r g b]`, `[r g b a]` and `[0 0 0 0 r g b]` forms REAPER uses without
//! having to parse WALTER properly.

use anyhow::{Context, Result};
use std::path::Path;

use daw_theme::Ramp;

/// A literal that was rewritten.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub line: usize,
    pub before: String,
    pub after: String,
}

/// Rewrite the colour literals in `text` through `ramp`.
pub fn retint(text: &str, ramp: &Ramp) -> (String, Vec<Change>) {
    let mut changes = Vec::new();
    let out: Vec<String> = text
        .lines()
        .enumerate()
        .map(|(i, line)| {
            if !is_color_assignment(line) {
                return line.to_string();
            }
            let rewritten = rewrite_line(line, ramp);
            if rewritten != line {
                changes.push(Change {
                    line: i + 1,
                    before: line.trim().to_string(),
                    after: rewritten.trim().to_string(),
                });
            }
            rewritten
        })
        .collect();

    let mut joined = out.join("\n");
    if text.ends_with('\n') {
        joined.push('\n');
    }
    (joined, changes)
}

/// Does this line assign to a variable whose name says it is a colour?
fn is_color_assignment(line: &str) -> bool {
    let t = line.trim_start();
    let Some(rest) = t.strip_prefix("set ") else {
        return false;
    };
    let Some(name) = rest.split_whitespace().next() else {
        return false;
    };
    // `set mcp.volume.color …` and `set gl_pan_color …` both qualify;
    // `set tcp.volume [x y w h]` does not.
    name.contains("color") || name.contains("colour")
}

/// Rewrite every bracketed literal on one line.
fn rewrite_line(line: &str, ramp: &Ramp) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;

    while let Some(open) = rest.find('[') {
        let Some(close_rel) = rest[open..].find(']') else {
            break;
        };
        let close = open + close_rel;
        out.push_str(&rest[..=open]);
        let inner = &rest[open + 1..close];
        out.push_str(&rewrite_literal(inner, ramp));
        out.push(']');
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

/// Rewrite the numbers inside one `[...]`.
///
/// Only *all-numeric* literals are touched: WALTER also puts variable names
/// in brackets (`[trackcolor_r trackcolor_g trackcolor_b]`), and those are
/// already dynamic — rewriting them would replace a live track colour with
/// a constant.
fn rewrite_literal(inner: &str, ramp: &Ramp) -> String {
    let parts: Vec<&str> = inner.split_whitespace().collect();
    let nums: Option<Vec<u32>> = parts.iter().map(|p| p.parse::<u32>().ok()).collect();
    let Some(nums) = nums else {
        return inner.to_string();
    };
    if nums.iter().any(|n| *n > 255) {
        // Out of colour range — a coordinate or a flag word.
        return inner.to_string();
    }

    // The RGB triple is the last three values: `[r g b]`, `[r g b a]` has
    // alpha last so is handled below, and `[0 0 0 0 r g b]` prefixes it.
    let (rgb_at, keeps_alpha) = match nums.len() {
        3 => (0, false),
        4 => (0, true),
        7 => (4, false),
        _ => return inner.to_string(),
    };

    let c = daw_theme::Color::rgb(
        nums[rgb_at] as u8,
        nums[rgb_at + 1] as u8,
        nums[rgb_at + 2] as u8,
    );
    let m = ramp.apply(c);

    let mut out: Vec<String> = nums.iter().map(|n| n.to_string()).collect();
    out[rgb_at] = m.r.to_string();
    out[rgb_at + 1] = m.g.to_string();
    out[rgb_at + 2] = m.b.to_string();
    let _ = keeps_alpha; // alpha is at index 3 and deliberately untouched
    out.join(" ")
}

/// Where the pristine rtconfig is kept, beside the pristine artwork.
///
/// A luminance ramp is not idempotent — it compounds on its own output, so
/// a second run darkens again and there is no way back to try a different
/// palette. Exactly the problem `restyle` has with images, and it takes
/// exactly the same answer: always retint from an untouched copy.
pub const SOURCE_NAME: &str = "rtconfig.txt";

/// Retint the rtconfig at `path`, always from the pristine copy in
/// `source_dir`, which is created from `path` on first use.
pub fn retint_file_from(
    path: &Path,
    source_dir: &Path,
    ramp: &Ramp,
    dry_run: bool,
) -> Result<Vec<Change>> {
    let pristine = source_dir.join(SOURCE_NAME);
    if !pristine.is_file() {
        if dry_run {
            anyhow::bail!(
                "no pristine rtconfig yet — run without --dry-run once to create {}",
                pristine.display()
            );
        }
        std::fs::create_dir_all(source_dir)?;
        std::fs::copy(path, &pristine).with_context(|| format!("snapshot {}", path.display()))?;
    }

    let text = std::fs::read_to_string(&pristine)
        .with_context(|| format!("read {}", pristine.display()))?;
    let (out, changes) = retint(&text, ramp);

    // Compare against what is live, not against the pristine source, or
    // every run reports the whole file as changed.
    let current = std::fs::read_to_string(path).unwrap_or_default();
    if out == current {
        return Ok(Vec::new());
    }
    if !dry_run {
        std::fs::write(path, out).with_context(|| format!("write {}", path.display()))?;
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_theme::Theme;

    fn ramp() -> Ramp {
        Ramp::for_chrome(&Theme::default())
    }

    /// A ramp that definitely moves greys.
    ///
    /// The default theme currently *is* the source theme's palette, so its
    /// ramp is near enough the identity on the source's own greys — which
    /// is the point of holding those values, and which silently turned the
    /// tests below into assertions that nothing happens. They need a
    /// palette that differs from the input to test retinting at all.
    fn contrasting_ramp() -> Ramp {
        let mut theme = Theme::default();
        let c = &mut theme.chrome;
        c.surface = daw_theme::Color::hex("#0d0d11").unwrap();
        c.surface_raised = daw_theme::Color::hex("#15151c").unwrap();
        c.surface_sunken = daw_theme::Color::hex("#0a0a0e").unwrap();
        c.border = daw_theme::Color::hex("#2b2b38").unwrap();
        Ramp::for_chrome(&theme)
    }

    #[test]
    fn retints_the_mixer_background() {
        // The literal that started this: the mixer strip body, grey in an
        // otherwise fully dark theme.
        let src = "    set mcp_bg_color  theme_version>1 [0 0 0 0 61 61 61] [0 0 0 0 51 51 51]\n";
        let (out, changes) = retint(src, &contrasting_ramp());
        assert_eq!(changes.len(), 1);
        assert!(!out.contains("61 61 61"), "{out}");
        assert!(!out.contains("51 51 51"), "{out}");
        // The leading blend/alpha prefix is preserved.
        assert!(out.contains("[0 0 0 0 "), "{out}");
    }

    #[test]
    fn leaves_layout_rectangles_alone() {
        // `[x y w h]` is syntactically identical to a colour. Rewriting one
        // would move the element instead of recolouring it — and the theme
        // would look broken in a way that has nothing to do with colour.
        let src = "    set tcp.volume [0 0 100 20]\n    set mcp.meter [4 8 12 60 0 0 1 1]\n";
        let (out, changes) = retint(src, &ramp());
        assert!(changes.is_empty(), "{changes:?}");
        assert_eq!(out, src);
    }

    #[test]
    fn leaves_variable_literals_alone() {
        // `[trackcolor_r …]` is a live track colour; replacing it with a
        // constant would make every track the same shade.
        let src = "    set track_color [trackcolor_r trackcolor_g trackcolor_b]\n";
        let (out, changes) = retint(src, &ramp());
        assert!(changes.is_empty());
        assert_eq!(out, src);
    }

    #[test]
    fn handles_several_literals_on_one_line() {
        let src = "    set c_color width>1 [255 255 255] [10 10 10]\n";
        let (out, _) = retint(src, &ramp());
        assert_eq!(out.matches('[').count(), 2, "{out}");
        assert!(!out.contains("[255 255 255]"), "{out}");
    }

    #[test]
    fn preserves_alpha_in_a_four_tuple() {
        let src = "    set gl_pan_color [0 0 0 120]\n";
        let (out, _) = retint(src, &ramp());
        assert!(out.trim().ends_with("120]"), "alpha moved or lost: {out}");
    }

    #[test]
    fn ignores_non_colour_assignments() {
        let src = "    set tinted_text_sat 0.5\n    set hideValues 0\n";
        let (out, changes) = retint(src, &ramp());
        assert!(changes.is_empty());
        assert_eq!(out, src);
    }

    #[test]
    fn out_of_range_values_are_not_colours() {
        // A `*color*` variable can still carry a scalar or a large number.
        let src = "    set foo_color [1000 2 3]\n";
        let (out, changes) = retint(src, &ramp());
        assert!(changes.is_empty(), "{changes:?}");
        assert_eq!(out, src);
    }

    #[test]
    fn retinting_compounds_which_is_why_a_pristine_copy_exists() {
        // A luminance ramp is not a fixed point on its own output: run it
        // twice and the theme darkens twice, with no way back to try a
        // different palette. `retint_file_from` therefore always reads an
        // untouched copy — the same answer `restyle` needs for images.
        let src = "    set mcp_bg_color [0 0 0 0 61 61 61]\n";
        let (once, _) = retint(src, &contrasting_ramp());
        let (twice, _) = retint(&once, &contrasting_ramp());
        assert_ne!(once, twice, "if this ever holds, the snapshot is redundant");
    }

    #[test]
    fn retinting_from_a_pristine_copy_is_stable() {
        let dir = std::env::temp_dir().join(format!("fts-walter-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let live = dir.join("rtconfig.txt");
        let source_dir = dir.join(".source-art");
        std::fs::write(&live, "    set mcp_bg_color [0 0 0 0 61 61 61]\n").unwrap();

        let first = retint_file_from(&live, &source_dir, &ramp(), false).unwrap();
        assert!(!first.is_empty());
        let after_one = std::fs::read_to_string(&live).unwrap();

        let second = retint_file_from(&live, &source_dir, &ramp(), false).unwrap();
        assert!(second.is_empty(), "second run changed {second:?}");
        assert_eq!(std::fs::read_to_string(&live).unwrap(), after_one);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn indentation_and_comments_survive() {
        let src = "\tset mcp_bg_color [61 61 61]   ; the strip body\n";
        let (out, _) = retint(src, &ramp());
        assert!(out.starts_with('\t'), "{out}");
        assert!(out.contains("; the strip body"), "{out}");
    }
}
