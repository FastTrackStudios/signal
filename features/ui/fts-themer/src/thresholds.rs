//! The collapse thresholds, spliced into the theme's layout file.
//!
//! #146 put REAPER's collapse heights in one Rust constant so the Dioxus
//! strip could read them. The theme states the same numbers in WALTER. That
//! is **the one seam where the panel and the theme can silently disagree**:
//! everywhere else the two are independent by design, but here they encode
//! the same question — at what height does this part of the strip go — and
//! a mismatch is a strip that collapses at one height in the app and another
//! in REAPER, with nothing to notice it.
//!
//! So the Rust constant is the source and the section-hide thresholds are
//! generated from it. Five lines, and no more. The three section heights the
//! ticket also names are *checked* rather than written, because this theme
//! states them inside expressions — see [`generated_lines`].
//!
//! # Everything else stays hand-edited
//!
//! `rtconfig.txt` is 3743 lines of hand-written WALTER with meaningful
//! comments and formatting, and it is the file a themer edits most. This
//! rewrites the *value* on five `set` lines and touches nothing else — not
//! the comments that follow them, not the indentation, not the ordering.
//! Same surgical shape as [`crate::rtconfig::add_accent_layout`].
//!
//! Full generation of the layout file is deliberately not attempted here.

use anyhow::{Context, Result, bail};

/// One generated line: the WALTER name, and the value it must carry.
///
/// The names are REAPER's, from `rtconfig.txt`. The values come from
/// `daw_theme_art::collapse::Thresholds` via [`generated_lines`], so there is one
/// place to change a number and both sides move together.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Threshold {
    pub name: &'static str,
    pub value: f32,
}

/// The five lines this module owns.
///
/// The section-hide thresholds, and only those. The ticket also names the
/// three section heights, and they are **not** generated — this theme does
/// not state them as values. `fx_sec` and `in_sec` carry their heights
/// inside expressions (`* scale 33@h`, and a three-way choice of 22/42/54),
/// and `bot_sec_h` is `+ * scale 47 * mcp_indent_s …`, a runtime term this
/// tool cannot resolve. Rewriting an expression is a different and much
/// riskier job than rewriting a literal, on the file REAPER actually loads.
///
/// So they are checked instead of written: [`section_heights_agree`] reads
/// the numbers back out of those expressions and compares them with the
/// Rust constant, which closes the same drift risk without editing a line
/// this tool does not understand.
///
/// Also out of scope for the same reason: the residual thresholds
/// `mcp_io_hide_h`, `mcp_env_hide_h` and `mcp_phase_hide_h`.
pub fn generated_lines() -> Vec<Threshold> {
    // Kept in the order the file states them, so a diff reads top to bottom.
    let t = daw_theme_art::collapse::REAPER;
    vec![
        Threshold {
            name: "hide_inputFX",
            value: t.input_fx,
        },
        Threshold {
            name: "hide_input",
            value: t.record_input,
        },
        Threshold {
            name: "hide_pan_labels",
            value: t.pan_labels,
        },
        Threshold {
            name: "hide_pan",
            value: t.pan_section,
        },
        Threshold {
            name: "hide_volume_label",
            value: t.volume_label,
        },
    ]
}

/// Do the theme's section heights still match the Rust constant?
///
/// They live inside WALTER expressions, so this looks for the number in the
/// line that defines each band rather than parsing the expression. Coarse,
/// and deliberately so: the alternative is a WALTER evaluator, and the
/// question being asked is only "has one of these numbers moved without the
/// other".
pub fn section_heights_agree(text: &str) -> Vec<String> {
    let bands = daw_theme_art::collapse::REAPER_BANDS;
    let mut wrong = Vec::new();
    for (name, number, what) in [
        ("fx_sec", bands.fx, "fx section"),
        ("in_sec", bands.input_full, "input section"),
        ("bot_sec_h", bands.bottom, "bottom section"),
    ] {
        let wanted = format_value(number);
        match first_set_states(text, name, &wanted) {
            None => wrong.push(format!("{what}: `set {name}` is not in the layout file")),
            Some((line, false)) => wrong.push(format!(
                "{what}: the theme does not state {wanted} — `{}`",
                line.trim()
            )),
            Some((_, true)) => {}
        }
    }
    wrong
}

/// Does the *first* `set name` line state `wanted` as a bare word?
///
/// Word-wise, not by prefix: the file aligns its columns with tabs, so
/// `starts_with("set fx_sec ")` finds nothing and reports drift on a file
/// that is perfectly correct. `@h`-suffixed words count — heights are
/// written `33@h`. The first definition on purpose: later `set` lines are
/// scale variants, and scanning all of them would accept a value that
/// belongs to a different mode (narrowMode's 54 beside wide's 86).
fn first_set_states<'t>(text: &'t str, name: &str, wanted: &str) -> Option<(&'t str, bool)> {
    let line = text.lines().find(|l| is_set_of(l, name))?;
    let states = line
        .split_whitespace()
        .any(|w| w.trim_end_matches("@h") == wanted);
    Some((line, states))
}

/// Do the theme's stated offsets still match the geometry constants?
///
/// #239: the strip lays out on `daw_theme_art::geometry::mcp`, whose
/// numbers come off `rtconfig.txt`'s `[dx dy w h]` literals — the column
/// chain, the record arm's cell, the input fields. Those literals sit
/// inside expressions this tool must not rewrite, so like
/// [`section_heights_agree`] they are checked rather than written: find the
/// lines that define each name, and ask whether any of them still states
/// the group the constant came from. Coarse on purpose — the question is
/// only "has one of these numbers moved without the other".
///
/// A name may be `set` many times (scale variants, mode branches); the
/// group only has to appear on one of them, because that is the line the
/// constant was read from.
pub fn offsets_agree(text: &str) -> Vec<String> {
    use daw_theme_art::geometry::mcp as g;

    let quad = |a: f32, b: f32, c: f32, d: f32| {
        format!(
            "[{} {} {} {}]",
            format_value(a),
            format_value(b),
            format_value(c),
            format_value(d)
        )
    };

    // Each entry: the WALTER name, the literal the geometry constants
    // were read from, and what it positions. Every number in a quad is a
    // named constant — a literal that existed only inside this guard was
    // a check nobody could act on when it fired.
    let stated = [
        (
            "mcp.recarm",
            quad(0.0, 0.0, g::ARM_CELL_W, g::ARM_CELL_H),
            "the record arm's cell",
        ),
        (
            "mcp.recmon",
            quad(g::RECMON_DX, g::RECMON_FROM_ARM, g::BUTTON_W, g::BUTTON_H),
            "recmon's step off the arm",
        ),
        (
            "mcp.mute",
            quad(0.0, g::MUTE_FROM_RECMON, g::BUTTON_W, g::BUTTON_H),
            "mute's step off recmon",
        ),
        (
            "mcp.solo",
            quad(0.0, g::SOLO_FROM_MUTE, g::BUTTON_W, g::BUTTON_H),
            "solo's step off mute",
        ),
        (
            "mcp.io",
            quad(g::IO_DX, g::IO_FROM_SOLO, g::IO_W, g::IO_H),
            "io's step off solo",
        ),
        (
            "mcp.env",
            quad(g::ENV_DX, -g::ENV_FROM_FLOOR, g::ENV_W, g::ENV_FROM_FLOOR),
            "envelope's hang above the stretch floor",
        ),
        (
            "mcp.phase",
            quad(
                g::PHASE_DX,
                -g::PHASE_FROM_ENV,
                g::PHASE_W,
                g::PHASE_FROM_ENV,
            ),
            "phase's step off envelope",
        ),
        (
            "mcp.recinput",
            quad(g::RECINPUT_X, 0.0, g::RECINPUT_W, g::INPUT_FIELD_H),
            "the record-input field",
        ),
    ];

    let mut wrong = Vec::new();
    for (name, group, what) in stated {
        let mut lines = text.lines().filter(|l| is_set_of(l, name)).peekable();
        if lines.peek().is_none() {
            wrong.push(format!("{what}: `set {name}` is not in the layout file"));
            continue;
        }
        if !lines.any(|l| l.contains(&group)) {
            wrong.push(format!("{what}: no `set {name}` line states {group}"));
        }
    }

    // The strip's width is a bare value in the same kind of expression, so
    // it gets exactly the section-heights treatment — the same predicate,
    // first definition only, so narrowMode's 54 on a later line cannot
    // stand in for wide's 86.
    let wanted = format_value(g::STRIP_W);
    match first_set_states(text, "mcp_w", &wanted) {
        None => wrong.push("strip width: `set mcp_w` is not in the layout file".to_string()),
        Some((line, false)) => wrong.push(format!(
            "strip width: the theme does not state {wanted} — `{}`",
            line.trim()
        )),
        Some((_, true)) => {}
    }

    wrong
}

/// Rewrite the generated values in `text`, leaving everything else alone.
///
/// Returns the patched text and how many lines actually changed — zero on a
/// second run, which is the idempotence the caller checks.
///
/// A name the file does not define is an error rather than an insertion: a
/// threshold this tool invented would be a `set` REAPER never reads, and
/// silently doing nothing useful is worse than refusing.
pub fn splice(text: &str, thresholds: &[Threshold]) -> Result<(String, usize)> {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut changed = 0;

    for threshold in thresholds {
        let idx = find_set(&lines, threshold.name)
            .with_context(|| format!("no `set {}` in the layout file", threshold.name))?;
        let patched = rewrite_value(&lines[idx], threshold.name, threshold.value)?;
        if patched != lines[idx] {
            lines[idx] = patched;
            changed += 1;
        }
    }

    // `lines()` drops the trailing newline; put it back if it was there.
    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    Ok((out, changed))
}

/// The line defining `name`, if the file has exactly one.
///
/// WALTER lets a name be `set` more than once — later definitions win, and
/// the file uses that for scale variants. A threshold defined twice is
/// therefore ambiguous, and picking one would rewrite a value REAPER might
/// not be reading.
fn find_set(lines: &[String], name: &str) -> Result<usize> {
    let matches: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| is_set_of(l, name))
        .map(|(i, _)| i)
        .collect();
    match matches.as_slice() {
        [one] => Ok(*one),
        [] => bail!("not defined"),
        many => bail!("defined {} times, at lines {:?}", many.len(), many),
    }
}

fn is_set_of(line: &str, name: &str) -> bool {
    let mut words = line.split_whitespace();
    words.next() == Some("set") && words.next() == Some(name)
}

/// Replace the value on one `set` line, keeping its indentation, its spacing
/// and any trailing comment exactly as they were.
fn rewrite_value(line: &str, name: &str, value: f32) -> Result<String> {
    let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
    let body = &line[indent.len()..];

    // Split off a trailing comment so the value swap cannot eat it.
    let (code, comment) = match body.find(';') {
        Some(at) => (&body[..at], Some(&body[at..])),
        None => (body, None),
    };

    // `set` `name` then the value, preserving the run of whitespace the
    // file uses to align its columns — the alignment is the formatting a
    // themer maintains, and a generator that collapses it rewrites the
    // whole file's look one line at a time.
    let after_name = code
        .find(name)
        .map(|at| at + name.len())
        .context("line does not contain its own name")?;
    let gap: String = code[after_name..]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect();
    // Reversed back: `take_while` over `rev()` yields the trailing run
    // backwards, so a `" \t"` before a comment came out as `"\t "` — the
    // line then differed from itself and every no-op splice reported a
    // change.
    let trailing: String = code
        .chars()
        .rev()
        .take_while(|c| c.is_whitespace())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    let value = format_value(value);
    let mut out = format!("{indent}set {name}{gap}{value}{trailing}");
    if let Some(comment) = comment {
        out.push_str(comment);
    }
    Ok(out)
}

/// WALTER numbers are plain; an integer threshold should not gain a `.0`.
fn format_value(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// Read the generated values back out of a layout file.
///
/// The other half of the guarantee: a test parses these and compares them
/// with the constant, so the two sides cannot drift even if someone edits
/// the file by hand.
pub fn parse(text: &str, names: &[&str]) -> Result<Vec<(String, f32)>> {
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    names
        .iter()
        .map(|name| {
            let idx =
                find_set(&lines, name).with_context(|| format!("reading `set {name}` back"))?;
            let body = lines[idx].split(';').next().unwrap_or_default();
            let value: f32 = body
                .split_whitespace()
                .nth(2)
                .context("no value on the line")?
                .parse()
                .with_context(|| format!("`{name}` is not a plain number"))?;
            Ok((name.to_string(), value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixture in the file's own shape: tab-and-space alignment, a
    /// trailing space before the comment, and an unrelated line to prove
    /// the splice leaves its neighbours alone.
    ///
    /// Built by joining explicit lines rather than as one continued string
    /// literal — a `\`-continuation eats the leading whitespace of the next
    /// line, which is exactly the thing these tests are about.
    fn sample() -> String {
        [
            "    set hide_inputFX\t\t\t    400 \t; height below which input effects is hidden.",
            "    set hide_input                  350 \t; height below which input dropdown is hidden.",
            "",
            "; unrelated",
            "set fx_sec_h 33",
            "",
        ]
        .join("\n")
    }

    fn sample_thresholds() -> Vec<Threshold> {
        vec![
            Threshold {
                name: "hide_inputFX",
                value: 400.0,
            },
            Threshold {
                name: "hide_input",
                value: 350.0,
            },
            Threshold {
                name: "fx_sec_h",
                value: 33.0,
            },
        ]
    }

    /// Already-correct values are left byte-identical — the first run of a
    /// generator over a hand-written file must not reformat it.
    #[test]
    fn a_file_already_at_the_right_values_is_untouched() {
        let (out, changed) = splice(&sample(), &sample_thresholds()).unwrap();
        assert_eq!(changed, 0);
        assert_eq!(out, sample(), "a no-op splice rewrote the file");
    }

    /// Running it twice produces no diff the second time.
    #[test]
    fn the_splice_is_idempotent() {
        let mut changed_values = sample_thresholds();
        changed_values[0].value = 420.0;

        let (once, first) = splice(&sample(), &changed_values).unwrap();
        assert_eq!(first, 1);
        let (twice, second) = splice(&once, &changed_values).unwrap();
        assert_eq!(second, 0, "the second run changed something");
        assert_eq!(once, twice);
    }

    /// Comments, indentation and column alignment survive a rewrite.
    #[test]
    fn everything_but_the_value_survives() {
        let mut values = sample_thresholds();
        values[0].value = 420.0;
        let (out, _) = splice(&sample(), &values).unwrap();
        let line = out.lines().next().unwrap();

        assert!(
            line.starts_with("    set hide_inputFX\t\t\t    "),
            "indent or gap lost: {line:?}"
        );
        assert!(line.contains("420"), "value not written: {line:?}");
        assert!(
            line.ends_with("; height below which input effects is hidden."),
            "comment lost: {line:?}"
        );
        // And the untouched lines really are untouched.
        assert!(out.contains("; unrelated"));
        assert_eq!(out.lines().count(), sample().lines().count());
    }

    #[test]
    fn the_values_read_back_as_written() {
        let mut values = sample_thresholds();
        values[1].value = 300.0;
        let (out, _) = splice(&sample(), &values).unwrap();
        let read = parse(&out, &["hide_inputFX", "hide_input", "fx_sec_h"]).unwrap();
        assert_eq!(read[1], ("hide_input".to_string(), 300.0));
    }

    /// A name the file does not define is refused rather than invented.
    #[test]
    fn an_unknown_threshold_is_an_error_not_an_insertion() {
        let bogus = [Threshold {
            name: "hide_nothing",
            value: 1.0,
        }];
        assert!(splice(&sample(), &bogus).is_err());
    }

    /// The offsets guard is not vacuous: a line whose group has moved is
    /// reported, by name. (The shipped file passing is the other half,
    /// tested in `thresholds_match_the_theme`.)
    #[test]
    fn a_moved_offset_is_reported() {
        // recmon's dy nudged from 20 to 21 — one number, one row of drift.
        let text = "set mcp.recmon + + [0 padding] [mcp.recarm mcp.recarm] * scale [7 21 21 20]\n";
        let wrong = offsets_agree(text);
        assert!(
            wrong.iter().any(|w| w.contains("mcp.recmon")),
            "the moved recmon step went unreported: {wrong:#?}"
        );
    }
}
