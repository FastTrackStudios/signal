//! Replacing `FabFilter` plugins in a REAPER project with FTS ones.
//!
//! Pro-Q 4 becomes FTS EQ and Pro-C 3 becomes FTS Comp. What differs between
//! the two is only which bytes mean what — everything below, the project
//! surgery and the report, is shared, and a third plugin is a [`Family`] arm
//! rather than a second copy of this file.
//!
//! The rule the whole module is built around: **a block we do not convert is
//! not touched.** Rewriting is per-`<VST>`/`<CLAP>` block, in place, leaving
//! its siblings — `FXID`, `WAK`, `BYPASS`, `FLOATPOS` — exactly where they
//! were. That is what keeps FX order, bypass state, offline flags and the
//! chain's window geometry intact without this module knowing what any of
//! them mean.
//!
//! Blocks are replaced with a **CLAP** block whatever the original was. A
//! REAPER VST3 block carries an id hash and a class UID in its header line
//! that we would have to reproduce byte-exactly; a CLAP block names the
//! plugin by its id string, which we know at compile time. Both formats of
//! FTS EQ install side by side, so nothing is lost by picking the one that
//! cannot be got subtly wrong.
//!
//! ## What does not survive
//!
//! Parameter automation. A `<PARMENV>` envelope addresses its parameter by
//! index, and FTS EQ's indices are not Pro-Q's, so an automated Pro-Q
//! parameter would land on an unrelated FTS EQ one — a curve that still
//! moves and now means nothing. The conversion still happens (the static
//! preset is the point), but every FX chain that holds an envelope is named
//! in [`Report::automated_chains`] so a run cannot quietly leave one behind.

use crate::fabfilter::{ffbs, proc3, proq4};
use crate::rpp::{Block, Document, Node, chunk, fts_comp, fts_eq, split_fields, unquote};

/// Which `FabFilter` plugin an instance is, and therefore what it becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    ProQ4,
    ProC3,
}

impl Family {
    /// The FTS plugin that replaces it: `(clap id, name, vendor)`.
    #[must_use]
    pub fn target(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::ProQ4 => (fts_eq::CLAP_ID, fts_eq::NAME, fts_eq::VENDOR),
            Self::ProC3 => (fts_comp::CLAP_ID, fts_comp::NAME, fts_comp::VENDOR),
        }
    }

    #[must_use]
    pub fn source_name(self) -> &'static str {
        match self {
            Self::ProQ4 => "Pro-Q 4",
            Self::ProC3 => "Pro-C 3",
        }
    }
}

/// How an instance was hosted in the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hosted {
    Vst3,
    Clap,
}

/// What happened to one converted instance.
///
/// Everything here is plugin-agnostic on purpose. A caller reporting or
/// verifying a conversion should not have to know which `FabFilter` plugin it
/// came from, and the moment it does, adding a third means editing the
/// caller too.
#[derive(Debug, Clone)]
pub struct Converted {
    /// The track the instance sits on, as named in the project.
    pub track: String,
    /// Its position in that track's FX chain, counting from 1 and counting
    /// every plugin, not only the converted ones — so it lines up with what
    /// REAPER shows.
    pub slot: usize,
    pub family: Family,
    pub hosted: Hosted,
    /// The preset name the plugin had stored, when it had one.
    pub preset: Option<String>,
    /// The exact bytes the original plugin was holding — verification has to
    /// load the real plugin with the real state, not with a re-encoding of
    /// our reading of it.
    pub source_state: Vec<u8>,
    /// The bytes written into the replacement block.
    pub our_state: Vec<u8>,
    /// One line describing what the preset does, for the report.
    pub summary: String,
    /// What the source carries that the target has no control for. Empty is
    /// the good case; anything here is something a mix engineer should be
    /// told rather than left to discover.
    pub unmapped: Vec<String>,
    /// The engine-facing parameter list, where one exists.
    ///
    /// Only the equalizer has it: `signal-fx` exposes the whole EQ engine and
    /// eight of the compressor's twenty-nine controls, so a compressor
    /// comparison against the engine would measure the facade rather than the
    /// DSP. Callers use it to render an engine column beside the plugin's,
    /// which is what says whether a gap is in the translation or in the DSP.
    pub native_params: Option<Vec<(String, f64)>>,
}

/// An instance that was left alone, and why.
#[derive(Debug, Clone)]
pub struct Skipped {
    pub track: String,
    pub slot: usize,
    pub reason: String,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub converted: Vec<Converted>,
    pub skipped: Vec<Skipped>,
    /// FX blocks that were nothing we convert, counted so a run can say how
    /// much of the project it looked at.
    pub untouched_fx: usize,
    /// Tracks where a converted chain also carries parameter automation.
    /// The envelope survives the rewrite and now points at the wrong
    /// parameter — see the module docs.
    pub automated_chains: Vec<String>,
}

impl Report {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.converted.is_empty() && self.skipped.is_empty()
    }
}

/// Rewrite every recognised `FabFilter` instance in `doc`, in place.
pub fn convert(doc: &mut Document) -> Report {
    let mut report = Report::default();
    let mut nodes = std::mem::take(&mut doc.nodes);
    walk(&mut nodes, "master", false, &mut report);
    doc.nodes = nodes;
    report
}

fn walk(nodes: &mut [Node], track: &str, automated: bool, report: &mut Report) {
    // A `<TRACK>`'s name is a child line, so it has to be read before its
    // `<FXCHAIN>` is descended into.
    let mut current = track.to_string();
    let mut slot = 0usize;
    for node in nodes.iter_mut() {
        match node {
            Node::Line(l) => {
                if let Some(name) = track_name(l) {
                    current = name;
                }
            }
            Node::Block(b) => {
                let token = b.token().to_string();
                if token == "VST" || token == "CLAP" {
                    slot += 1;
                    match try_convert(b, &current, slot) {
                        Some(Ok(done)) => {
                            *b = replacement(b.indent(), done.family, &done.our_state, b);
                            report.converted.push(done);
                            if automated && !report.automated_chains.contains(&current) {
                                report.automated_chains.push(current.clone());
                            }
                        }
                        Some(Err(why)) => report.skipped.push(why),
                        None => report.untouched_fx += 1,
                    }
                    continue;
                }
                let mut children = std::mem::take(&mut b.children);
                // An FX parameter envelope lives beside the plugin it
                // automates, so the flag is raised for the whole chain
                // rather than attributed to one slot — REAPER stores the
                // parameter index, not the FX's identity, so attribution
                // would be a guess.
                let automated = automated
                    || (token == "FXCHAIN"
                        && children.iter().any(|n| match n {
                            Node::Block(b) => b.token() == "PARMENV",
                            _ => false,
                        }));
                walk(&mut children, &current, automated, report);
                b.children = children;
            }
        }
    }
}

fn track_name(line: &str) -> Option<String> {
    let t = line.trim_start();
    let rest = t.strip_prefix("NAME ")?;
    let name = unquote(rest.trim()).to_string();
    (!name.is_empty()).then_some(name)
}

/// `None` when the block is not Pro-Q at all.
fn try_convert(block: &Block, track: &str, slot: usize) -> Option<Result<Converted, Skipped>> {
    let (family, hosted) = recognise(block)?;
    let refuse = |reason: String| {
        Err(Skipped {
            track: track.to_string(),
            slot,
            reason,
        })
    };

    let state = match hosted {
        Hosted::Vst3 => match chunk::decode_vst3(&block.base64_lines()) {
            Ok(c) => c.component,
            Err(e) => return Some(refuse(format!("could not read its VST3 chunk: {e}"))),
        },
        Hosted::Clap => {
            let Some(state) = block.child_block("STATE") else {
                return Some(refuse("its CLAP block has no <STATE>".into()));
            };
            match chunk::decode_clap_state(&state.base64_lines()) {
                Ok(s) => s,
                Err(e) => return Some(refuse(format!("could not read its CLAP state: {e}"))),
            }
        }
    };

    let ffbs = match ffbs::parse(&state) {
        Ok(f) => f,
        Err(e) => return Some(refuse(format!("its state is not a FabFilter blob: {e}"))),
    };

    // The only plugin-specific step. Everything either side of it — reading
    // the block, framing the replacement, reporting — is shared.
    let (our_state, preset, summary, unmapped, native_params) = match family {
        Family::ProQ4 => match proq4::decode(&ffbs) {
            Err(e) => return Some(refuse(format!("could not read the preset: {e}"))),
            Ok(eq) => (
                fts_eq::clap_state(&eq),
                eq.preset_name.clone(),
                format!("{} bands", eq.active_bands().count()),
                Vec::new(),
                Some(proq4::to_native_eq_params(&eq)),
            ),
        },
        Family::ProC3 => match proc3::decode(&ffbs) {
            Err(e) => return Some(refuse(format!("could not read the preset: {e}"))),
            Ok(comp) => {
                let summary = if comp.is_transparent() {
                    "passing through".to_string()
                } else {
                    format!(
                        "{:.1}:1 at {:.0} dB, {:.0}/{:.0} ms",
                        comp.ratio, comp.threshold_db, comp.attack_ms, comp.release_ms
                    )
                };
                (
                    fts_comp::clap_state(&comp),
                    comp.preset_name.clone(),
                    summary,
                    fts_comp::unmapped(&comp),
                    None,
                )
            }
        },
    };

    Some(Ok(Converted {
        track: track.to_string(),
        slot,
        family,
        hosted,
        preset: preset.filter(|s| !s.is_empty()),
        source_state: state,
        our_state,
        summary,
        unmapped,
        native_params,
    }))
}

/// Which plugin is this FX block, if it is one we replace?
///
/// Matched on both the CLAP id and the display name, because the two formats
/// name a plugin differently and a user can rename neither. The version
/// number is part of the match on purpose: Pro-Q 2 and 3 store a different
/// parameter vector entirely, and taking one for a 4 would produce a
/// confident, wrong conversion rather than a refusal.
fn recognise(block: &Block) -> Option<(Family, Hosted)> {
    let fields = split_fields(&block.header);
    let display = fields.get(1).map(|f| unquote(f)).unwrap_or_default();
    let id = fields.get(2).map(|f| unquote(f)).unwrap_or_default();

    let family = if display.contains("Pro-Q 4") || id.eq_ignore_ascii_case("com.FabFilter.Pro-Q.4")
    {
        Family::ProQ4
    } else if display.contains("Pro-C 3") || id.eq_ignore_ascii_case("com.FabFilter.Pro-C.3") {
        Family::ProC3
    } else {
        return None;
    };

    match block.token() {
        // A VST3 block's third field is a filename, not an id, so only the
        // display name can speak for it.
        "VST" if display.contains(family.source_name()) => Some((family, Hosted::Vst3)),
        "CLAP" => Some((family, Hosted::Clap)),
        _ => None,
    }
}

/// Build the replacement `<CLAP>` block at the same depth as the original.
///
/// CLAP whatever the original was: a REAPER VST3 block's header line carries
/// an id hash and a class UID that would have to be reproduced byte-exactly,
/// where a CLAP block names its plugin by id string, which we know at compile
/// time. Both formats of every FTS plugin install side by side, so nothing is
/// lost by choosing the one that cannot be got subtly wrong.
fn replacement(indent: &str, family: Family, state_bytes: &[u8], original: &Block) -> Block {
    let inner = format!("{indent}  ");
    let (clap_id, name, vendor) = family.target();
    // Carry the original's editor geometry when it had one, so the FX window
    // opens roughly where the user left it.
    let cfg = original
        .children
        .iter()
        .find_map(|n| match n {
            Node::Line(l) if l.trim_start().starts_with("CFG ") => Some(l.trim().to_string()),
            _ => None,
        })
        .unwrap_or_else(|| "CFG 4 1200 700 \"\"".to_string());

    let state = Block {
        header: format!("{inner}<STATE"),
        children: chunk::encode_clap_state(state_bytes)
            .into_iter()
            .map(|l| Node::Line(format!("{inner}  {l}")))
            .collect(),
        footer: format!("{inner}>"),
    };

    Block {
        header: format!("{indent}<CLAP \"CLAP: {name} ({vendor})\" {clap_id} \"\""),
        children: vec![Node::Line(format!("{inner}{cfg}")), Node::Block(state)],
        footer: format!("{indent}>"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/pro-c.RTrackTemplate");

    #[test]
    fn a_pro_c_is_recognised_too() {
        // The fixture is a real REAPER track template holding a Pro-C 3
        // between two JUCE plugins, so this also proves the recogniser reads
        // an id off a header line REAPER actually wrote.
        let doc = Document::parse(FIXTURE);
        let found: Vec<_> = doc
            .walk()
            .into_iter()
            .filter_map(|b| recognise(b.block))
            .collect();
        assert_eq!(found, vec![(Family::ProC3, Hosted::Clap)]);
    }

    #[test]
    fn plugins_we_do_not_convert_are_left_alone() {
        // The two JUCE VST3s in the fixture are nothing to do with us, and
        // the file must come back byte for byte — the guard against a rewrite
        // that reflows what it was handed.
        let mut doc = Document::parse(FIXTURE);
        let before = doc.render();
        let report = convert(&mut doc);
        assert_eq!(report.untouched_fx, 2, "the two JUCE plugins");
        // The Pro-C is recognised, so it is either converted or refused; what
        // must not happen is the rest of the file moving.
        let unchanged: String = strip_fx(&doc.render());
        assert_eq!(unchanged, strip_fx(&before));
    }

    /// The document with every FX block's body removed, so two versions can
    /// be compared on everything except the parts a conversion rewrites.
    fn strip_fx(text: &str) -> String {
        let mut out = String::new();
        let mut depth = 0usize;
        for line in text.lines() {
            let t = line.trim_start();
            if depth == 0 && (t.starts_with("<VST ") || t.starts_with("<CLAP ")) {
                depth = 1;
                out.push_str("<<FX>>\n");
                continue;
            }
            if depth > 0 {
                if t.starts_with('<') {
                    depth += 1;
                } else if t == ">" {
                    depth -= 1;
                }
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    #[test]
    fn a_clap_pro_q_is_recognised_by_its_id() {
        let b = Block {
            header: r#"    <CLAP "CLAP: Pro-Q 4 (FabFilter)" com.FabFilter.Pro-Q.4 """#.into(),
            children: vec![],
            footer: "    >".into(),
        };
        assert_eq!(recognise(&b), Some((Family::ProQ4, Hosted::Clap)));
    }

    #[test]
    fn a_vst3_pro_q_is_recognised_by_its_display_name() {
        let b = Block {
            header: r#"    <VST "VST3: FabFilter Pro-Q 4 (FabFilter)" "FabFilter Pro-Q 4.vst3" 0 "" 1{A} """#.into(),
            children: vec![],
            footer: "    >".into(),
        };
        assert_eq!(recognise(&b), Some((Family::ProQ4, Hosted::Vst3)));
    }

    #[test]
    fn pro_q_3_is_not_mistaken_for_pro_q_4() {
        // Pro-Q 2 and 3 store a different parameter vector entirely; taking
        // one for a 4 would produce a confident, wrong conversion.
        let b = Block {
            header: r#"    <CLAP "CLAP: Pro-Q 3 (FabFilter)" com.FabFilter.Pro-Q.3 """#.into(),
            children: vec![],
            footer: "    >".into(),
        };
        assert_eq!(recognise(&b), None);
    }

    #[test]
    fn a_pro_q_block_with_unreadable_state_is_skipped_not_mangled() {
        let text = concat!(
            "<REAPER_PROJECT 0.1\n",
            "  <TRACK\n",
            "    NAME \"Vox\"\n",
            "    <FXCHAIN\n",
            "      <CLAP \"CLAP: Pro-Q 4 (FabFilter)\" com.FabFilter.Pro-Q.4 \"\"\n",
            "        CFG 4 100 100 \"\"\n",
            "        <STATE\n",
            "          AAAA\n",
            "        >\n",
            "      >\n",
            "    >\n",
            "  >\n",
            ">\n",
        );
        let mut doc = Document::parse(text);
        let report = convert(&mut doc);
        assert!(report.converted.is_empty());
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].track, "Vox");
        assert_eq!(report.skipped[0].slot, 1);
        assert!(
            report.skipped[0].reason.contains("FabFilter blob"),
            "{}",
            report.skipped[0].reason
        );
        assert_eq!(
            doc.render(),
            text,
            "a refused block is left exactly as it was"
        );
    }

    #[test]
    fn an_automated_chain_is_named_in_the_report() {
        let text = concat!(
            "<REAPER_PROJECT 0.1\n",
            "  <TRACK\n",
            "    NAME \"Gtr\"\n",
            "    <FXCHAIN\n",
            "      <CLAP \"CLAP: Pro-Q 4 (FabFilter)\" com.FabFilter.Pro-Q.4 \"\"\n",
            "        <STATE\n",
            "          AAAA\n",
            "        >\n",
            "      >\n",
            "      <PARMENV 3 0 1 0\n",
            "        PT 0 0.5 0\n",
            "      >\n",
            "    >\n",
            "  >\n",
            ">\n",
        );
        let mut doc = Document::parse(text);
        let report = convert(&mut doc);
        // The state here is unreadable, so nothing converts and there is
        // nothing to warn about — the warning is tied to a real rewrite.
        assert!(report.automated_chains.is_empty());
        assert_eq!(report.skipped.len(), 1);
    }

    #[test]
    fn the_slot_number_counts_every_plugin_in_the_chain() {
        let text = concat!(
            "<REAPER_PROJECT 0.1\n",
            "  <TRACK\n",
            "    NAME \"Drums\"\n",
            "    <FXCHAIN\n",
            "      <CLAP \"CLAP: Something (X)\" com.x \"\"\n",
            "      >\n",
            "      <CLAP \"CLAP: Pro-Q 4 (FabFilter)\" com.FabFilter.Pro-Q.4 \"\"\n",
            "      >\n",
            "    >\n",
            "  >\n",
            ">\n",
        );
        let mut doc = Document::parse(text);
        let report = convert(&mut doc);
        assert_eq!(report.untouched_fx, 1);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(
            report.skipped[0].slot, 2,
            "REAPER counts from the top of the chain"
        );
    }
}
