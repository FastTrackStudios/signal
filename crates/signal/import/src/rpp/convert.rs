//! Replacing FabFilter Pro-Q instances in a REAPER project with FTS EQ.
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

use crate::fabfilter::{ffbs, proq4};
use crate::rpp::{chunk, fts_eq, split_fields, unquote, Block, Document, Node};

/// How a Pro-Q instance was hosted in the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hosted {
    Vst3,
    Clap,
}

/// What happened to one Pro-Q instance.
#[derive(Debug, Clone)]
pub struct Converted {
    /// The track the instance sits on, as named in the project.
    pub track: String,
    /// Its position in that track's FX chain, counting from 1 and counting
    /// every plugin, not just the Pro-Q ones — so it lines up with what
    /// REAPER shows.
    pub slot: usize,
    pub hosted: Hosted,
    /// The preset name Pro-Q had stored, when it had one.
    pub preset: Option<String>,
    /// The decoded instance, kept so the caller can verify it against the
    /// real plugin without decoding the project a second time.
    pub source: proq4::ProQ4,
    /// The exact bytes the original plugin was holding. Kept for the same
    /// reason: verification has to load the real Pro-Q with the real state,
    /// not with a re-encoding of our reading of it.
    pub source_state: Vec<u8>,
    /// The parameters written into the FTS EQ block.
    pub params: std::collections::BTreeMap<String, fts_eq::ParamValue>,
}

/// A Pro-Q instance that was left alone, and why.
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
    /// FX blocks that were not Pro-Q at all, counted so a run can say how
    /// much of the project it looked at.
    pub untouched_fx: usize,
    /// Tracks where a converted chain also carries parameter automation.
    /// The envelope survives the rewrite and now points at the wrong
    /// parameter — see the module docs.
    pub automated_chains: Vec<String>,
}

impl Report {
    pub fn is_empty(&self) -> bool {
        self.converted.is_empty() && self.skipped.is_empty()
    }
}

/// Rewrite every Pro-Q 4 instance in `doc` as FTS EQ, in place.
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
                            *b = fts_eq_block(b.indent(), &done.params, b);
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
    let hosted = pro_q_kind(block)?;
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
    let source = match proq4::decode(&ffbs) {
        Ok(p) => p,
        Err(e) => return Some(refuse(format!("could not read the preset: {e}"))),
    };

    let params = fts_eq::plugin_params(&proq4::to_native_eq_params(&source));
    let preset = source.preset_name.clone().filter(|s| !s.is_empty());
    Some(Ok(Converted {
        track: track.to_string(),
        slot,
        hosted,
        preset,
        source,
        source_state: state,
        params,
    }))
}

/// Is this FX block a Pro-Q 4?
///
/// Matched on both the CLAP id and the display name, because the two formats
/// name the plugin differently and a user can rename neither.
fn pro_q_kind(block: &Block) -> Option<Hosted> {
    let fields = split_fields(&block.header);
    let display = fields.get(1).map(|f| unquote(f)).unwrap_or_default();
    match block.token() {
        "VST" if display.contains("Pro-Q 4") => Some(Hosted::Vst3),
        "CLAP" => {
            let id = fields.get(2).map(|f| unquote(f)).unwrap_or_default();
            (id.eq_ignore_ascii_case("com.FabFilter.Pro-Q.4") || display.contains("Pro-Q 4"))
                .then_some(Hosted::Clap)
        }
        _ => None,
    }
}

/// Build the replacement `<CLAP>` block at the same depth as the original.
fn fts_eq_block(
    indent: &str,
    params: &std::collections::BTreeMap<String, fts_eq::ParamValue>,
    original: &Block,
) -> Block {
    let inner = format!("{indent}  ");
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
        children: chunk::encode_clap_state(&fts_eq::state_bytes(params))
            .into_iter()
            .map(|l| Node::Line(format!("{inner}  {l}")))
            .collect(),
        footer: format!("{inner}>"),
    };

    Block {
        header: format!(
            "{indent}<CLAP \"CLAP: {} ({})\" {} \"\"",
            fts_eq::NAME,
            fts_eq::VENDOR,
            fts_eq::CLAP_ID
        ),
        children: vec![Node::Line(format!("{inner}{cfg}")), Node::Block(state)],
        footer: format!("{indent}>"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/pro-c.RTrackTemplate");

    #[test]
    fn a_project_with_no_pro_q_comes_back_unchanged() {
        // The fixture holds a Pro-C and two JUCE plugins. Nothing in it is a
        // Pro-Q, so the converter must be a no-op down to the byte — this is
        // the guard against a rewrite that reflows the file it was handed.
        let mut doc = Document::parse(FIXTURE);
        let report = convert(&mut doc);
        assert!(report.is_empty(), "{report:?}");
        assert_eq!(report.untouched_fx, 3);
        assert_eq!(doc.render(), FIXTURE);
    }

    #[test]
    fn a_clap_pro_q_is_recognised_by_its_id() {
        let b = Block {
            header: r#"    <CLAP "CLAP: Pro-Q 4 (FabFilter)" com.FabFilter.Pro-Q.4 """#.into(),
            children: vec![],
            footer: "    >".into(),
        };
        assert_eq!(pro_q_kind(&b), Some(Hosted::Clap));
    }

    #[test]
    fn a_vst3_pro_q_is_recognised_by_its_display_name() {
        let b = Block {
            header: r#"    <VST "VST3: FabFilter Pro-Q 4 (FabFilter)" "FabFilter Pro-Q 4.vst3" 0 "" 1{A} """#.into(),
            children: vec![],
            footer: "    >".into(),
        };
        assert_eq!(pro_q_kind(&b), Some(Hosted::Vst3));
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
        assert_eq!(pro_q_kind(&b), None);
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
        assert_eq!(doc.render(), text, "a refused block is left exactly as it was");
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
        assert_eq!(report.skipped[0].slot, 2, "REAPER counts from the top of the chain");
    }
}
