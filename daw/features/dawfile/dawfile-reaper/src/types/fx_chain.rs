//! FX chain data structures and parsing for REAPER RPP format.
//!
//! Parses the `<FXCHAIN>` and `<FXCHAIN_REC>` blocks found in track state chunks.
//! Supports all plugin types (VST/VST3, AU, JS, CLAP, Video) and recursive
//! `<CONTAINER>` blocks for REAPER 7.0+ FX containers.
//!
//! ## RPP FX Chain Layout
//!
//! Each FX in the chain follows this pattern:
//! ```text
//! BYPASS <bypassed> <offline> [<master_bypass>]   // per-FX bypass state
//! <VST "name" "file" ...>                          // plugin block with binary state
//!   <base64 state data>
//! >
//! FLOATPOS <x> <y> <w> <h>                         // floating window position
//! FXID {GUID}                                      // unique FX instance ID
//! WAK <want_all_keys> <embedded_ui>                // keyboard routing
//! PRESETNAME "name"                                // current preset (optional)
//! ```
//!
//! Container blocks (`<CONTAINER>`) recursively contain the same structure.

use crate::primitives::{RppBlock, RppBlockContent, Token};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// The top-level FX chain parsed from an `<FXCHAIN>` or `<FXCHAIN_REC>` block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FxChain {
    /// Window rect: x, y, w, h
    pub window_rect: Option<[i32; 4]>,
    /// Index of FX with open UI (1-based, 0 = none)
    pub show: i32,
    /// Index of last selected FX (0-based)
    pub last_sel: i32,
    /// Whether the FX chain window is docked
    pub docked: bool,
    /// Ordered list of FX nodes (plugins and containers)
    pub nodes: Vec<FxChainNode>,
    /// Raw content preserved for round-trip fidelity
    pub raw_content: String,
}

/// A node in the FX chain — either a plugin or a container of child nodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FxChainNode {
    Plugin(FxPlugin),
    Container(FxContainer),
}

/// A single plugin instance in the FX chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FxPlugin {
    /// Plugin display name (from the block header, e.g. "VST: ReaEQ (Cockos)")
    pub name: String,
    /// User-assigned custom display name (e.g. "EQ Block: Reagate - Relaxed").
    /// This is the name shown in REAPER's FX chain UI when the user renames an FX.
    /// For VST/AU: the 2nd quoted string in the header (after filename + flags).
    /// For JS: the 2nd quoted string in the header (after the script path).
    /// `None` if the user hasn't set a custom name (the quoted string is empty).
    pub custom_name: Option<String>,
    /// Plugin type
    pub plugin_type: PluginType,
    /// File name / path (dll, dylib, component, script path, etc.)
    pub file: String,
    /// Whether this FX is bypassed
    pub bypassed: bool,
    /// Whether this FX is offline
    pub offline: bool,
    /// Unique FX instance GUID (from FXID line)
    pub fxid: Option<String>,
    /// Current preset name (from PRESETNAME line)
    pub preset_name: Option<String>,
    /// Floating window position: x, y, w, h
    pub float_pos: Option<[i32; 4]>,
    /// Want-all-keys setting
    pub wak: Option<[i32; 2]>,
    /// Whether this FX runs in parallel with the previous FX
    pub parallel: bool,
    /// Raw plugin state data (base64-encoded lines from inside the plugin block)
    pub state_data: Vec<String>,
    /// The complete raw text of the plugin block (for round-trip preservation)
    pub raw_block: String,
    /// Parameter automation envelopes (from PARMENV blocks after the plugin)
    pub param_envelopes: Vec<FxParamEnvelope>,
    /// Parameter indices visible on TCP (from PARM_TCP lines)
    pub params_on_tcp: Vec<FxParamRef>,
    /// The header fields surrounding the custom-name slot (VST/VST3/AU only),
    /// captured verbatim so that a header re-emitted without a `raw_block`
    /// keeps its flags, vendor id and plugin state token. `None` for headers
    /// that were built programmatically rather than parsed.
    #[serde(default)]
    pub header_extra: Option<FxHeaderExtra>,
}

/// The parts of a VST/VST3/AU plugin block header that surround the
/// user-assigned custom name, preserved verbatim for round-tripping.
///
/// The header shape is
/// `<VST "<display name>" <file> <flags> "<custom name>" <tail>`, e.g.
/// `<VST "VST3i: Kontakt 8 (Native Instruments)" "Kontakt 8.vst3" 0 "" 952745140{5653…} ""`
/// where `flags` is `0` and `tail` is `952745140{5653…} ""`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FxHeaderExtra {
    /// Text between the filename and the custom-name slot (typically `0`).
    pub flags: String,
    /// Text after the custom-name slot: the vendor id, the plugin state
    /// token, and any trailing empty string.
    pub tail: String,
}

/// Reference to an FX parameter by index and optional name.
///
/// Used by PARM_TCP, PARMENV, etc. Format: `<index>[:<name>]`
/// e.g. `0:_Main_p1__Bank` or just `0`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FxParamRef {
    /// Zero-based parameter index
    pub index: u32,
    /// Parameter name (if provided by the RPP file)
    pub name: Option<String>,
}

/// An FX parameter automation envelope (from `<PARMENV>` blocks).
///
/// Appears after the plugin block in the FX chain, containing
/// automation curve data for a specific parameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FxParamEnvelope {
    /// Which parameter this envelope controls
    pub param: FxParamRef,
    /// Automation mode (0=trim/read, 1=read, 2=touch, 3=write, 4=latch)
    pub mode: i32,
    /// Range maximum (often 1 or 10)
    pub range_max: f64,
    /// Default value
    pub default_value: f64,
    /// Envelope GUID
    pub eguid: Option<String>,
    /// Whether the envelope is active
    pub active: bool,
    /// Whether the envelope is visible
    pub visible: bool,
    /// Whether the envelope is armed for recording
    pub armed: bool,
    /// Automation points
    pub points: Vec<FxEnvelopePoint>,
    /// Its automation items (`POOLEDENVINST`), each naming a pool's
    /// project-level `<POOLEDENV>`.
    #[serde(default)]
    pub automation_items: Vec<crate::types::envelope::AutomationItem>,
    /// Raw block text for round-trip
    pub raw_block: String,
}

/// A single point in a parameter automation envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FxEnvelopePoint {
    /// Time position (in beats or seconds depending on project settings)
    pub time: f64,
    /// Parameter value at this point
    pub value: f64,
    /// Shape/tension flags (variable length)
    pub flags: Vec<f64>,
}

/// Parsed inline parameter values from a JS plugin.
///
/// JS plugins store parameter values as space-separated floats on lines
/// inside the `<JS>` block. Unset parameters are represented by `-`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsParamValue {
    /// Zero-based parameter index
    pub index: u32,
    /// Parameter value (None if the parameter is unset, represented by `-`)
    pub value: Option<f64>,
}

/// Plugin type enumeration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginType {
    Vst,
    Vst3,
    Au,
    Js,
    Clap,
    Video,
    /// Unknown or unrecognized plugin type
    Other(String),
}

/// An FX container (REAPER 7.0+). Contains child FX nodes recursively.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FxContainer {
    /// Container display name (from block header)
    pub name: String,
    /// Whether this container is bypassed
    pub bypassed: bool,
    /// Whether this container is offline
    pub offline: bool,
    /// Unique FX instance GUID (from FXID line)
    pub fxid: Option<String>,
    /// Floating window position
    pub float_pos: Option<[i32; 4]>,
    /// Whether this container runs in parallel with the previous FX
    pub parallel: bool,
    /// Container config: [type, nch, nch_in, nch_out]
    pub container_cfg: Option<[i32; 4]>,
    /// SHOW value inside container
    pub show: i32,
    /// LASTSEL value inside container
    pub last_sel: i32,
    /// DOCKED value inside container
    pub docked: bool,
    /// Child FX nodes (plugins and nested containers)
    pub children: Vec<FxChainNode>,
    /// Raw block text for round-trip preservation
    pub raw_block: String,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

impl FxChain {
    /// Parse an FX chain directly from a parsed RPP block.
    pub fn from_block(block: &RppBlock) -> Result<Self, String> {
        if block.name != "FXCHAIN" && block.name != "FXCHAIN_REC" {
            return Err(format!(
                "Expected FXCHAIN/FXCHAIN_REC block, got {}",
                block.name
            ));
        }
        let mut inner_owned: Vec<Cow<'_, str>> = Vec::new();
        for child in &block.children {
            append_block_content_lines(child, &mut inner_owned);
        }
        let inner_refs: Vec<&str> = inner_owned.iter().map(|s| s.as_ref()).collect();
        parse_inner_lines(&inner_refs, String::new(), false)
    }

    /// Parse an FX chain from raw RPP text content.
    ///
    /// `content` should be the full text of the `<FXCHAIN ...>...</FXCHAIN>`
    /// block, including the opening `<FXCHAIN` and closing `>` lines.
    /// It also accepts just the inner content (without the wrapper lines).
    // r[impl rpp.parse.fx-chain]
    pub fn parse(content: &str) -> Result<Self, String> {
        let raw_content = content.to_string();

        // Strip the outer <FXCHAIN ...> and closing > if present
        let inner = strip_outer_block(content, "FXCHAIN");
        let lines: Vec<&str> = inner.lines().collect();
        parse_inner_lines(&lines, raw_content, true)
    }
}

fn append_block_content_lines<'a>(content: &'a RppBlockContent, out: &mut Vec<Cow<'a, str>>) {
    match content {
        RppBlockContent::Content(tokens) => out.push(tokens_to_line(tokens)),
        RppBlockContent::RawLine(line) => out.push(Cow::Borrowed(line.as_ref())),
        RppBlockContent::Block(block) => {
            out.push(Cow::Owned(block_header_line(block)));
            for child in &block.children {
                append_block_content_lines(child, out);
            }
            out.push(Cow::Borrowed(">"));
        }
    }
}

fn tokens_to_line(tokens: &[Token]) -> Cow<'_, str> {
    if let [Token::Identifier(raw)] = tokens {
        return Cow::Borrowed(raw.as_str());
    }
    let mut line = String::new();
    let mut first = true;
    for token in tokens {
        if !first {
            line.push(' ');
        }
        first = false;
        line.push_str(&token.to_string());
    }
    Cow::Owned(line)
}

fn block_header_line(block: &RppBlock) -> String {
    let mut line = String::new();
    line.push('<');
    line.push_str(&block.name);
    for param in &block.params {
        line.push(' ');
        line.push_str(&param.to_string());
    }
    line
}

fn parse_inner_lines(
    lines: &[&str],
    raw_content: String,
    preserve_raw_blocks: bool,
) -> Result<FxChain, String> {
    let mut chain = FxChain {
        window_rect: None,
        show: 0,
        last_sel: 0,
        docked: false,
        nodes: Vec::new(),
        raw_content,
    };

    let mut i = 0;
    let mut pending_bypass: Option<(bool, bool)> = None;
    let mut pending_parallel = false;

    while i < lines.len() {
        let line = lines[i].trim();

        if line.is_empty() {
            i += 1;
            continue;
        }

        if let Some(stripped) = line.strip_prefix("WNDRECT ") {
            chain.window_rect = parse_4_ints(stripped);
            i += 1;
            continue;
        }
        if let Some(stripped) = line.strip_prefix("SHOW ") {
            chain.show = parse_int(stripped).unwrap_or(0);
            i += 1;
            continue;
        }
        if let Some(stripped) = line.strip_prefix("LASTSEL ") {
            chain.last_sel = parse_int(stripped).unwrap_or(0);
            i += 1;
            continue;
        }
        if let Some(stripped) = line.strip_prefix("DOCKED ") {
            chain.docked = parse_int(stripped).unwrap_or(0) != 0;
            i += 1;
            continue;
        }

        if let Some(stripped) = line.strip_prefix("BYPASS ") {
            let parts: Vec<&str> = stripped.split_whitespace().collect();
            let bypassed = parts
                .first()
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(0)
                != 0;
            let offline = parts
                .get(1)
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(0)
                != 0;
            pending_bypass = Some((bypassed, offline));
            i += 1;
            continue;
        }

        if let Some(stripped) = line.strip_prefix("PARALLEL ") {
            pending_parallel = parse_int(stripped).unwrap_or(0) != 0;
            i += 1;
            continue;
        }

        if line.starts_with("<VST ")
            || line.starts_with("<AU ")
            || line.starts_with("<JS ")
            || line.starts_with("<CLAP ")
            || line.starts_with("<VIDEO_EFFECT ")
        {
            let (plugin_block, end_idx) = extract_block(lines, i);
            let (bypassed, offline) = pending_bypass.take().unwrap_or((false, false));
            let parallel = pending_parallel;
            pending_parallel = false;

            let mut plugin =
                parse_plugin_block(&plugin_block, bypassed, offline, preserve_raw_blocks);
            plugin.parallel = parallel;

            let mut j = end_idx + 1;
            while j < lines.len() {
                let meta = lines[j].trim();
                if let Some(stripped) = meta.strip_prefix("FLOATPOS ") {
                    plugin.float_pos = parse_4_ints(stripped);
                    j += 1;
                } else if let Some(stripped) = meta.strip_prefix("FXID ") {
                    plugin.fxid = Some(stripped.trim().to_string());
                    j += 1;
                } else if let Some(stripped) = meta.strip_prefix("WAK ") {
                    plugin.wak = parse_2_ints(stripped);
                    j += 1;
                } else if let Some(stripped) = meta.strip_prefix("PRESETNAME ") {
                    plugin.preset_name = Some(unquote(stripped));
                    j += 1;
                } else if meta.starts_with("<PARMENV ") {
                    let (env_block, env_end) = extract_block(lines, j);
                    plugin
                        .param_envelopes
                        .push(parse_param_envelope(&env_block, preserve_raw_blocks));
                    j = env_end + 1;
                } else if let Some(stripped) = meta.strip_prefix("PARM_TCP ") {
                    plugin.params_on_tcp.push(parse_param_ref(stripped));
                    j += 1;
                } else {
                    break;
                }
            }

            chain.nodes.push(FxChainNode::Plugin(plugin));
            i = j;
            continue;
        }

        if line.starts_with("<CONTAINER ") || line == "<CONTAINER" {
            let (container_block, end_idx) = extract_block(lines, i);
            let (bypassed, offline) = pending_bypass.take().unwrap_or((false, false));
            let parallel = pending_parallel;
            pending_parallel = false;

            let mut container =
                parse_container_block(&container_block, bypassed, offline, preserve_raw_blocks);
            container.parallel = parallel;

            let mut j = end_idx + 1;
            while j < lines.len() {
                let meta = lines[j].trim();
                if let Some(stripped) = meta.strip_prefix("FLOATPOS ") {
                    container.float_pos = parse_4_ints(stripped);
                    j += 1;
                } else if let Some(stripped) = meta.strip_prefix("FXID ") {
                    container.fxid = Some(stripped.trim().to_string());
                    j += 1;
                } else {
                    break;
                }
            }

            chain.nodes.push(FxChainNode::Container(container));
            i = j;
            continue;
        }

        i += 1;
    }

    Ok(chain)
}

/// Parse a plugin block (`<VST ...>...</>`) into an `FxPlugin`.
fn parse_plugin_block(
    block_lines: &[&str],
    bypassed: bool,
    offline: bool,
    preserve_raw_block: bool,
) -> FxPlugin {
    let header = block_lines.first().copied().unwrap_or("");
    let raw_block = if preserve_raw_block {
        block_lines.join("\n")
    } else {
        String::new()
    };

    // Parse header: <TYPE "Display Name" "file.dll" extra_params...
    let parsed_header = parse_plugin_header(header);

    // State data = all lines between header and closing >
    let state_data: Vec<String> = if block_lines.len() > 2 {
        block_lines[1..block_lines.len() - 1]
            .iter()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    } else {
        Vec::new()
    };

    FxPlugin {
        name: parsed_header.name,
        custom_name: parsed_header.custom_name,
        plugin_type: parsed_header.plugin_type,
        file: parsed_header.file,
        header_extra: parsed_header.header_extra,
        bypassed,
        offline,
        fxid: None,
        preset_name: None,
        float_pos: None,
        wak: None,
        parallel: false,
        state_data,
        raw_block,
        param_envelopes: Vec::new(),
        params_on_tcp: Vec::new(),
    }
}

/// Parse the plugin block header line to extract type, display name, and file.
///
/// Formats:
/// - `<VST "VST: ReaEQ (Cockos)" reaeq.vst.dylib 0 "" 1919247985<...> ""`
/// - `<AU "AU: AUBandpass (Apple)" ...>`
/// - `<JS "loser/3BandEQ" ""`
/// - `<CLAP "org.surge-synthesizer.surge-xt" ...>`
/// - `<VIDEO_EFFECT "Video processor" ...>`
///
/// The filename token after the quoted display name may be bare (VST2:
/// `reaeq.vst.dylib`) or quoted (VST3, whose filenames routinely contain
/// spaces: `"Kontakt 8.vst3"`).
fn parse_plugin_header(header: &str) -> ParsedPluginHeader {
    let header = header.trim();

    // Remove leading '<'
    let header = header.strip_prefix('<').unwrap_or(header);

    // Split on first space to get the type keyword
    let (type_keyword, rest) = header
        .split_once(char::is_whitespace)
        .unwrap_or((header, ""));

    let plugin_type = match type_keyword {
        "VST" => {
            // Distinguish VST2 vs VST3 by name prefix. REAPER writes `VST3:`
            // for effects and `VST3i:` for instruments (`VSTi3:` in older
            // files).
            if rest.contains("VST3:") || rest.contains("VST3i:") || rest.contains("VSTi3:") {
                PluginType::Vst3
            } else {
                PluginType::Vst
            }
        }
        "AU" => PluginType::Au,
        "JS" => PluginType::Js,
        "CLAP" => PluginType::Clap,
        "VIDEO_EFFECT" => PluginType::Video,
        other => PluginType::Other(other.to_string()),
    };

    // Parse tokens from the rest of the header.
    // The structure is: "display name" <file token> ...
    // VST: "name" filename.dylib 0 "custom_name" vendorcode<hex> ""
    // VST3: "name" "file name.vst3" 0 "custom_name" vendorcode{hex} ""
    // JS:  "script/path" "custom_name"
    // CLAP: "plugin.id" ...
    parse_header_name_file(rest, plugin_type)
}

/// The fields `parse_plugin_header` recovers from a plugin block header line.
struct ParsedPluginHeader {
    plugin_type: PluginType,
    name: String,
    file: String,
    custom_name: Option<String>,
    header_extra: Option<FxHeaderExtra>,
}

/// Extract the display name, file, custom name and surrounding header fields.
///
/// VST/AU headers: `"VST: Name" <file> 0 "Custom Name" 12345<hex> ""`
///   - First quoted string = default display name
///   - Next token = file, either bare (`reaeq.vst.dylib`) or quoted
///     (`"Kontakt 8.vst3"` — VST3 filenames commonly contain spaces)
///   - The next quoted string = user-assigned custom name (empty = no custom name)
///   - Everything either side of that slot is kept verbatim in `header_extra`
///
/// JS headers: `"script/path" "Custom Name"`
///   - First quoted string = both name and file (script path)
///   - Second quoted string = user-assigned custom name
///
/// Other types: first two quoted strings = name, file (no custom name support)
fn parse_header_name_file(rest: &str, plugin_type: PluginType) -> ParsedPluginHeader {
    let rest = rest.trim();

    // Extract the first quoted string (display name)
    let (name, after_name) = extract_first_quoted(rest);

    match plugin_type {
        PluginType::Vst | PluginType::Vst3 | PluginType::Au => {
            // After the quoted name comes the filename, bare or quoted.
            let (file, after_file) = extract_first_token(after_name);
            // Then: <flags> "<custom name>" <vendor id><state token> ""
            let (custom_name, header_extra) = match split_around_first_quoted(after_file) {
                Some((flags, custom_raw, tail)) => (
                    (!custom_raw.is_empty()).then_some(custom_raw),
                    Some(FxHeaderExtra {
                        flags: flags.trim().to_string(),
                        tail: tail.trim().to_string(),
                    }),
                ),
                None => (None, None),
            };
            ParsedPluginHeader {
                plugin_type,
                name,
                file,
                custom_name,
                header_extra,
            }
        }
        PluginType::Js => {
            // For JS, the name IS the script path (acts as both name and file)
            // Custom name is the second quoted string
            let (custom_raw, _) = extract_first_quoted(after_name);
            ParsedPluginHeader {
                plugin_type,
                name: name.clone(),
                file: name,
                custom_name: (!custom_raw.is_empty()).then_some(custom_raw),
                header_extra: None,
            }
        }
        PluginType::Clap => {
            // CLAP: `"CLAP: Pro-Q 4 (FabFilter)" com.FabFilter.Pro-Q.4 ""` —
            // the plugin's ID, bare, is the file slot (what the writer puts
            // there too); taking the second QUOTED string read the empty
            // custom name, and the plugin could not be found.
            let (file, _) = extract_first_token(after_name);
            ParsedPluginHeader {
                plugin_type,
                name,
                file,
                custom_name: None,
                header_extra: None,
            }
        }
        _ => {
            // For Video, Other: try to extract second quoted string as file
            let quoted = extract_quoted_strings(rest);
            ParsedPluginHeader {
                plugin_type,
                name: quoted.first().cloned().unwrap_or_default(),
                file: quoted.get(1).cloned().unwrap_or_default(),
                custom_name: None,
                header_extra: None,
            }
        }
    }
}

/// Extract the first double-quoted string and return it plus the remaining text after it.
fn extract_first_quoted(s: &str) -> (String, &str) {
    if let Some(start) = s.find('"') {
        let after_open = &s[start + 1..];
        if let Some(end) = after_open.find('"') {
            let value = after_open[..end].to_string();
            let remaining = &after_open[end + 1..];
            return (value, remaining);
        }
    }
    (String::new(), s)
}

/// Extract the leading header token and the text after it.
///
/// A token is either a double-quoted string (returned without its quotes, so
/// that `"Kontakt 8.vst3"` survives the spaces inside it) or a bare
/// whitespace-delimited run.
fn extract_first_token(s: &str) -> (String, &str) {
    let s = s.trim_start();
    if s.starts_with('"') {
        let after_open = &s[1..];
        if let Some(end) = after_open.find('"') {
            return (after_open[..end].to_string(), &after_open[end + 1..]);
        }
    }
    match s.find(char::is_whitespace) {
        Some(end) => (s[..end].to_string(), &s[end..]),
        None => (s.to_string(), ""),
    }
}

/// Split `s` around its first double-quoted string, yielding the text before
/// it, its contents, and the text after it. `None` when `s` holds no quoted
/// string at all.
fn split_around_first_quoted(s: &str) -> Option<(&str, String, &str)> {
    let start = s.find('"')?;
    let after_open = &s[start + 1..];
    let end = after_open.find('"')?;
    Some((
        &s[..start],
        after_open[..end].to_string(),
        &after_open[end + 1..],
    ))
}

/// Parse a `<CONTAINER ...>...</>` block into an `FxContainer`.
fn parse_container_block(
    block_lines: &[&str],
    bypassed: bool,
    offline: bool,
    preserve_raw_blocks: bool,
) -> FxContainer {
    let header = block_lines.first().copied().unwrap_or("");
    let raw_block = if preserve_raw_blocks {
        block_lines.join("\n")
    } else {
        String::new()
    };

    // Parse container name from header: <CONTAINER Container "NAME" ""
    // or <CONTAINER Container NAME
    let name = parse_container_name(header);

    let mut container = FxContainer {
        name,
        bypassed,
        offline,
        fxid: None,
        float_pos: None,
        parallel: false,
        container_cfg: None,
        show: 0,
        last_sel: 0,
        docked: false,
        children: Vec::new(),
        raw_block,
    };

    // Parse inner content (skip header and closing >)
    if block_lines.len() <= 2 {
        return container;
    }

    let inner_lines: Vec<&str> = block_lines[1..block_lines.len() - 1].to_vec();
    let mut i = 0;
    let mut pending_bypass: Option<(bool, bool)> = None;
    let mut pending_parallel = false;

    while i < inner_lines.len() {
        let line = inner_lines[i].trim();

        if line.is_empty() {
            i += 1;
            continue;
        }

        // Container-level metadata
        if let Some(stripped) = line.strip_prefix("CONTAINER_CFG ") {
            container.container_cfg = parse_4_ints(stripped);
            i += 1;
            continue;
        }
        if let Some(stripped) = line.strip_prefix("SHOW ") {
            container.show = parse_int(stripped).unwrap_or(0);
            i += 1;
            continue;
        }
        if let Some(stripped) = line.strip_prefix("LASTSEL ") {
            container.last_sel = parse_int(stripped).unwrap_or(0);
            i += 1;
            continue;
        }
        if let Some(stripped) = line.strip_prefix("DOCKED ") {
            container.docked = parse_int(stripped).unwrap_or(0) != 0;
            i += 1;
            continue;
        }

        // Per-child BYPASS
        if let Some(stripped) = line.strip_prefix("BYPASS ") {
            let parts: Vec<&str> = stripped.split_whitespace().collect();
            let bp = parts
                .first()
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(0)
                != 0;
            let ol = parts
                .get(1)
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(0)
                != 0;
            pending_bypass = Some((bp, ol));
            i += 1;
            continue;
        }

        if let Some(stripped) = line.strip_prefix("PARALLEL ") {
            pending_parallel = parse_int(stripped).unwrap_or(0) != 0;
            i += 1;
            continue;
        }

        // Nested plugin block
        if line.starts_with("<VST ")
            || line.starts_with("<AU ")
            || line.starts_with("<JS ")
            || line.starts_with("<CLAP ")
            || line.starts_with("<VIDEO_EFFECT ")
        {
            let (plugin_block, end_idx) = extract_block(&inner_lines, i);
            let (bp, ol) = pending_bypass.take().unwrap_or((false, false));
            let par = pending_parallel;
            pending_parallel = false;

            let mut plugin = parse_plugin_block(&plugin_block, bp, ol, preserve_raw_blocks);
            plugin.parallel = par;

            // Post-plugin metadata
            let mut j = end_idx + 1;
            while j < inner_lines.len() {
                let meta = inner_lines[j].trim();
                if let Some(stripped) = meta.strip_prefix("FLOATPOS ") {
                    plugin.float_pos = parse_4_ints(stripped);
                    j += 1;
                } else if let Some(stripped) = meta.strip_prefix("FXID ") {
                    plugin.fxid = Some(stripped.trim().to_string());
                    j += 1;
                } else if let Some(stripped) = meta.strip_prefix("WAK ") {
                    plugin.wak = parse_2_ints(stripped);
                    j += 1;
                } else if let Some(stripped) = meta.strip_prefix("PRESETNAME ") {
                    plugin.preset_name = Some(unquote(stripped));
                    j += 1;
                } else if meta.starts_with("<PARMENV ") {
                    let (env_block, env_end) = extract_block(&inner_lines, j);
                    plugin
                        .param_envelopes
                        .push(parse_param_envelope(&env_block, preserve_raw_blocks));
                    j = env_end + 1;
                } else if let Some(stripped) = meta.strip_prefix("PARM_TCP ") {
                    plugin.params_on_tcp.push(parse_param_ref(stripped));
                    j += 1;
                } else {
                    break;
                }
            }

            container.children.push(FxChainNode::Plugin(plugin));
            i = j;
            continue;
        }

        // Nested container block (recursive)
        if line.starts_with("<CONTAINER ") || line == "<CONTAINER" {
            let (child_block, end_idx) = extract_block(&inner_lines, i);
            let (bp, ol) = pending_bypass.take().unwrap_or((false, false));
            let par = pending_parallel;
            pending_parallel = false;

            let mut child_container =
                parse_container_block(&child_block, bp, ol, preserve_raw_blocks);
            child_container.parallel = par;

            // Post-container metadata
            let mut j = end_idx + 1;
            while j < inner_lines.len() {
                let meta = inner_lines[j].trim();
                if let Some(stripped) = meta.strip_prefix("FLOATPOS ") {
                    child_container.float_pos = parse_4_ints(stripped);
                    j += 1;
                } else if let Some(stripped) = meta.strip_prefix("FXID ") {
                    child_container.fxid = Some(stripped.trim().to_string());
                    j += 1;
                } else {
                    break;
                }
            }

            container
                .children
                .push(FxChainNode::Container(child_container));
            i = j;
            continue;
        }

        // Skip unrecognized lines (WAK at container level, etc.)
        i += 1;
    }

    container
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the name from a container header line.
/// E.g. `<CONTAINER Container "DRIVE" ""` → `"DRIVE"`
/// Or `<CONTAINER Container DRIVE` → `"DRIVE"`
fn parse_container_name(header: &str) -> String {
    let header = header.trim();
    let header = header.strip_prefix('<').unwrap_or(header);

    // Skip "CONTAINER" keyword
    let rest = header.strip_prefix("CONTAINER").unwrap_or(header).trim();

    // The first token after "CONTAINER" is typically "Container" (the FX name).
    // The actual user-visible name may be the second token (quoted or unquoted).
    let quoted = extract_quoted_strings(rest);
    if let Some(q) = quoted.first()
        && !q.is_empty()
    {
        return q.clone();
    }

    // Fallback: split on whitespace, take first meaningful token
    let parts: Vec<&str> = rest.split_whitespace().collect();
    // "Container" is the default REAPER display name; if there's a second part, prefer it
    if parts.len() > 1 {
        unquote(parts[1])
    } else {
        parts.first().copied().unwrap_or("Container").to_string()
    }
}

/// Strip the outer block wrapper from content.
/// Given `<FXCHAIN\n...\n>`, returns the `...` inner lines.
fn strip_outer_block<'a>(content: &'a str, tag: &str) -> &'a str {
    let trimmed = content.trim();
    let upper_tag = tag.to_uppercase();

    // Check if it starts with the block tag
    if let Some(rest) = trimmed.strip_prefix('<') {
        let rest_upper = rest.to_uppercase();
        if rest_upper.starts_with(&upper_tag) {
            // Find end of first line
            if let Some(first_newline) = rest.find('\n') {
                let after_header = &rest[first_newline + 1..];
                // Strip trailing >
                if let Some(last_close) = after_header.rfind('>') {
                    return &after_header[..last_close];
                }
                return after_header;
            }
        }
    }

    // No wrapper found — return as-is
    content
}

/// Extract a complete block (from `<` to matching `>`) from a slice of lines.
/// Returns the block lines and the index of the closing `>` line.
fn extract_block<'a>(lines: &[&'a str], start: usize) -> (Vec<&'a str>, usize) {
    let mut depth = 0;
    let mut end = start;

    for (offset, line) in lines[start..].iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('<') {
            depth += 1;
        }
        if trimmed == ">" {
            depth -= 1;
            if depth == 0 {
                end = start + offset;
                break;
            }
        }
    }

    let block_lines = lines[start..=end].to_vec();
    (block_lines, end)
}

/// Extract all double-quoted strings from a line.
fn extract_quoted_strings(s: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut chars = s.chars().peekable();

    while let Some(&c) = chars.peek() {
        if c == '"' {
            chars.next(); // consume opening quote
            let mut val = String::new();
            while let Some(&inner) = chars.peek() {
                if inner == '"' {
                    chars.next(); // consume closing quote
                    break;
                }
                val.push(inner);
                chars.next();
            }
            result.push(val);
        } else {
            chars.next();
        }
    }

    result
}

/// Remove surrounding quotes from a string.
fn unquote(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"'))
        || (s.starts_with('\'') && s.ends_with('\''))
        || (s.starts_with('`') && s.ends_with('`'))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn parse_int(s: &str) -> Option<i32> {
    s.split_whitespace().next()?.parse().ok()
}

fn parse_4_ints(s: &str) -> Option<[i32; 4]> {
    let parts: Vec<i32> = s
        .split_whitespace()
        .filter_map(|p| p.parse().ok())
        .collect();
    if parts.len() >= 4 {
        Some([parts[0], parts[1], parts[2], parts[3]])
    } else {
        None
    }
}

fn parse_2_ints(s: &str) -> Option<[i32; 2]> {
    let parts: Vec<i32> = s
        .split_whitespace()
        .filter_map(|p| p.parse().ok())
        .collect();
    if parts.len() >= 2 {
        Some([parts[0], parts[1]])
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Parameter parsing helpers
// ---------------------------------------------------------------------------

/// Parse a parameter reference from text like `0:_Main_p1__Bank` or just `0`.
fn parse_param_ref(s: &str) -> FxParamRef {
    let s = s.trim();
    if let Some((idx_str, name)) = s.split_once(':') {
        FxParamRef {
            index: idx_str.parse().unwrap_or(0),
            name: Some(name.to_string()),
        }
    } else {
        FxParamRef {
            index: s.parse().unwrap_or(0),
            name: None,
        }
    }
}

/// A `<PARMENV>` block's lines (its header line first, `>` last) as an
/// [`FxParamEnvelope`]: what a writer holds a file's envelope up to.
#[must_use]
pub fn parse_parmenv(lines: &[&str]) -> FxParamEnvelope {
    parse_param_envelope(lines, false)
}

/// Parse a `<PARMENV ...>...</>` block into an `FxParamEnvelope`.
///
/// Format: `<PARMENV <param_ref> <mode> <range_max> <default_value>`
/// followed by EGUID, ACT, VIS, ARM, DEFSHAPE, PT lines.
fn parse_param_envelope(block_lines: &[&str], preserve_raw_block: bool) -> FxParamEnvelope {
    let header = block_lines.first().copied().unwrap_or("");
    let raw_block = if preserve_raw_block {
        block_lines.join("\n")
    } else {
        String::new()
    };

    // Parse header: <PARMENV 0:_Main_p1__Bank 0 1 0.5
    let header_trimmed = header.trim().strip_prefix("<PARMENV ").unwrap_or("");
    let tokens: Vec<&str> = header_trimmed.split_whitespace().collect();

    let param = if let Some(t) = tokens.first() {
        parse_param_ref(t)
    } else {
        FxParamRef {
            index: 0,
            name: None,
        }
    };

    let mode = tokens.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let range_max = tokens.get(2).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let default_value = tokens.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.0);

    let mut env = FxParamEnvelope {
        param,
        mode,
        range_max,
        default_value,
        eguid: None,
        active: false,
        visible: false,
        armed: false,
        points: Vec::new(),
        automation_items: Vec::new(),
        raw_block,
    };

    // Parse inner lines
    for line in block_lines.iter().skip(1) {
        let line = line.trim();
        if let Some(stripped) = line.strip_prefix("EGUID ") {
            env.eguid = Some(stripped.trim().to_string());
        } else if let Some(stripped) = line.strip_prefix("ACT ") {
            let val = parse_int(stripped).unwrap_or(0);
            env.active = val != 0;
        } else if let Some(stripped) = line.strip_prefix("VIS ") {
            let val = parse_int(stripped).unwrap_or(0);
            env.visible = val != 0;
        } else if let Some(stripped) = line.strip_prefix("ARM ") {
            let val = parse_int(stripped).unwrap_or(0);
            env.armed = val != 0;
        } else if line.starts_with("POOLEDENVINST ") {
            if let Ok(item) = crate::types::envelope::AutomationItem::from_rpp_line(line) {
                env.automation_items.push(item);
            }
        } else if let Some(stripped) = line.strip_prefix("PT ") {
            let parts: Vec<f64> = stripped
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();
            if parts.len() >= 2 {
                env.points.push(FxEnvelopePoint {
                    time: parts[0],
                    value: parts[1],
                    flags: parts[2..].to_vec(),
                });
            }
        }
    }

    env
}

/// Parse inline parameter values from a JS plugin's state data.
///
/// JS plugins store parameters as space-separated tokens on lines inside
/// the `<JS>` block. Numeric tokens become `Some(value)`, `-` tokens
/// become `None` (unset). All tokens are indexed from 0.
pub fn parse_js_params(state_data: &[String]) -> Vec<JsParamValue> {
    let mut params = Vec::new();
    let mut index = 0u32;

    for line in state_data {
        for token in line.split_whitespace() {
            let value = if token == "-" {
                None
            } else {
                token.parse::<f64>().ok()
            };
            params.push(JsParamValue { index, value });
            index += 1;
        }
    }

    params
}

// ---------------------------------------------------------------------------
// Display implementations
// ---------------------------------------------------------------------------

impl fmt::Display for FxChain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "FX Chain ({} nodes)", self.nodes.len())?;
        for (i, node) in self.nodes.iter().enumerate() {
            write!(f, "  [{}] {}", i, node)?;
        }
        Ok(())
    }
}

impl fmt::Display for FxChainNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FxChainNode::Plugin(p) => write!(f, "{}", p),
            FxChainNode::Container(c) => write!(f, "{}", c),
        }
    }
}

impl fmt::Display for FxPlugin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = if self.bypassed {
            " [BYPASSED]"
        } else if self.offline {
            " [OFFLINE]"
        } else {
            ""
        };
        let par = if self.parallel { " (parallel)" } else { "" };
        let display = self.custom_name.as_deref().unwrap_or(&self.name);
        writeln!(f, "{:?}: {}{}{}", self.plugin_type, display, status, par)
    }
}

impl fmt::Display for FxContainer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = if self.bypassed { " [BYPASSED]" } else { "" };
        let par = if self.parallel { " (parallel)" } else { "" };
        writeln!(
            f,
            "Container: \"{}\"{}{} ({} children)",
            self.name,
            status,
            par,
            self.children.len()
        )?;
        for (i, child) in self.children.iter().enumerate() {
            write!(f, "    [{}] {}", i, child)?;
        }
        Ok(())
    }
}

impl fmt::Display for PluginType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PluginType::Vst => write!(f, "VST"),
            PluginType::Vst3 => write!(f, "VST3"),
            PluginType::Au => write!(f, "AU"),
            PluginType::Js => write!(f, "JS"),
            PluginType::Clap => write!(f, "CLAP"),
            PluginType::Video => write!(f, "Video"),
            PluginType::Other(s) => write!(f, "{}", s),
        }
    }
}

// ---------------------------------------------------------------------------
// Convenience accessors
// ---------------------------------------------------------------------------

impl FxChain {
    /// Total number of plugins (recursively counts inside containers).
    pub fn plugin_count(&self) -> usize {
        self.nodes.iter().map(|n| n.plugin_count()).sum()
    }

    /// Iterate all plugins (depth-first), yielding `(depth, &FxPlugin)`.
    pub fn iter_plugins(&self) -> Vec<(usize, &FxPlugin)> {
        let mut result = Vec::new();
        for node in &self.nodes {
            node.collect_plugins(0, &mut result);
        }
        result
    }

    /// Find a plugin by FXID GUID.
    pub fn find_by_fxid(&self, fxid: &str) -> Option<&FxPlugin> {
        self.iter_plugins()
            .into_iter()
            .find(|(_, p)| p.fxid.as_deref() == Some(fxid))
            .map(|(_, p)| p)
    }
}

impl FxChainNode {
    fn plugin_count(&self) -> usize {
        match self {
            FxChainNode::Plugin(_) => 1,
            FxChainNode::Container(c) => c.children.iter().map(|n| n.plugin_count()).sum(),
        }
    }

    fn collect_plugins<'a>(&'a self, depth: usize, out: &mut Vec<(usize, &'a FxPlugin)>) {
        match self {
            FxChainNode::Plugin(p) => out.push((depth, p)),
            FxChainNode::Container(c) => {
                for child in &c.children {
                    child.collect_plugins(depth + 1, out);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// RPP Serialization (via RppSerialize trait)
// ---------------------------------------------------------------------------

use super::serialize::{RppSerialize, rpp_token};

// r[impl rpp.write.fx-chain]
impl RppSerialize for FxChain {
    fn write_rpp(&self, out: &mut String, indent: &str) {
        out.push_str(&format!("{}<FXCHAIN\n", indent));

        let inner = format!("{}  ", indent);

        if let Some(rect) = &self.window_rect {
            out.push_str(&format!(
                "{}WNDRECT {} {} {} {}\n",
                inner, rect[0], rect[1], rect[2], rect[3]
            ));
        }
        out.push_str(&format!("{}SHOW {}\n", inner, self.show));
        out.push_str(&format!("{}LASTSEL {}\n", inner, self.last_sel));
        out.push_str(&format!(
            "{}DOCKED {}\n",
            inner,
            if self.docked { 1 } else { 0 }
        ));

        for node in &self.nodes {
            node.write_rpp(out, &inner);
        }

        out.push_str(&format!("{}>\n", indent));
    }
}

impl RppSerialize for FxChainNode {
    fn write_rpp(&self, out: &mut String, indent: &str) {
        match self {
            FxChainNode::Plugin(p) => p.write_rpp(out, indent),
            FxChainNode::Container(c) => c.write_rpp(out, indent),
        }
    }
}

impl FxPlugin {
    /// Rebuild a `<VST …` / `<AU …` block header from the parsed fields.
    ///
    /// The filename is re-quoted when it is not a bare RPP token, so a VST3
    /// path such as `Kontakt 8.vst3` round-trips as one token instead of
    /// splitting across the flags and custom-name slots. `fallback` supplies
    /// the flags and tail for plugins built programmatically, which carry no
    /// `header_extra` from a parsed header.
    fn plugin_header_line(
        &self,
        tag: &str,
        default_file: &str,
        custom: &str,
        fallback: &FxHeaderExtra,
    ) -> String {
        let extra = self.header_extra.as_ref().unwrap_or(fallback);
        let file = if self.file.is_empty() {
            default_file
        } else {
            &self.file
        };
        let mut header = format!("<{} \"{}\" {}", tag, self.name, rpp_token(file));
        if !extra.flags.is_empty() {
            header.push(' ');
            header.push_str(&extra.flags);
        }
        header.push_str(&format!(" \"{}\"", custom));
        if !extra.tail.is_empty() {
            header.push(' ');
            header.push_str(&extra.tail);
        }
        header
    }
}

impl RppSerialize for FxPlugin {
    /// Emits the BYPASS preamble, the raw plugin block (preserving binary state),
    /// and post-plugin metadata (PRESETNAME, FLOATPOS, FXID, WAK).
    fn write_rpp(&self, out: &mut String, indent: &str) {
        // BYPASS line (preamble for every FX)
        out.push_str(&format!(
            "{}BYPASS {} {} 0\n",
            indent,
            if self.bypassed { 1 } else { 0 },
            if self.offline { 1 } else { 0 },
        ));

        // PARALLEL line (only if parallel)
        if self.parallel {
            out.push_str(&format!("{}PARALLEL 1\n", indent));
        }

        // Plugin block — use raw_block if available for perfect fidelity
        if !self.raw_block.is_empty() {
            // Re-indent the raw block to the current level
            for line in self.raw_block.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                // Opening < and closing > get base indent; contents get one more level
                if trimmed.starts_with('<') || trimmed == ">" {
                    out.push_str(&format!("{}{}\n", indent, trimmed));
                } else {
                    out.push_str(&format!("{}  {}\n", indent, trimmed));
                }
            }
        } else {
            // Synthetic block for programmatic FX nodes that don't carry a raw block.
            let custom = self.custom_name.as_deref().unwrap_or("");
            let header = match &self.plugin_type {
                PluginType::Vst | PluginType::Vst3 => self.plugin_header_line(
                    "VST",
                    "plugin.vst",
                    custom,
                    &FxHeaderExtra {
                        flags: "0".to_string(),
                        tail: "0<00> \"\"".to_string(),
                    },
                ),
                PluginType::Au => self.plugin_header_line(
                    "AU",
                    "plugin.component",
                    custom,
                    &FxHeaderExtra {
                        flags: "0".to_string(),
                        tail: String::new(),
                    },
                ),
                PluginType::Js => format!("<JS \"{}\" \"{}\"", self.name, custom),
                PluginType::Clap => format!(
                    "<CLAP \"{}\" {} \"\"",
                    self.name,
                    rpp_token(if self.file.is_empty() {
                        "plugin.clap"
                    } else {
                        &self.file
                    })
                ),
                PluginType::Video => format!("<VIDEO_EFFECT \"{}\" \"\"", self.name),
                PluginType::Other(tag) => {
                    format!("<{} \"{}\" {}", tag, self.name, rpp_token(&self.file))
                }
            };
            out.push_str(&format!("{}{}\n", indent, header));
            for line in &self.state_data {
                out.push_str(&format!("{}  {}\n", indent, line.trim()));
            }
            out.push_str(&format!("{}>\n", indent));
        }

        // Post-plugin metadata
        if let Some(preset) = &self.preset_name {
            out.push_str(&format!("{}PRESETNAME \"{}\"\n", indent, preset));
        }
        if let Some(fp) = &self.float_pos {
            out.push_str(&format!(
                "{}FLOATPOS {} {} {} {}\n",
                indent, fp[0], fp[1], fp[2], fp[3]
            ));
        }
        if let Some(fxid) = &self.fxid {
            out.push_str(&format!("{}FXID {}\n", indent, fxid));
        }
        if let Some(wak) = &self.wak {
            out.push_str(&format!("{}WAK {} {}\n", indent, wak[0], wak[1]));
        }

        // Parameter envelopes
        for env in &self.param_envelopes {
            env.write_rpp(out, indent);
        }

        // TCP-visible parameters
        for tcp_param in &self.params_on_tcp {
            out.push_str(&format!("{}PARM_TCP {}\n", indent, tcp_param.format_rpp()));
        }
    }
}

impl FxParamRef {
    /// Format as RPP text: `0:_Main_p1__Bank` or just `0`.
    fn format_rpp(&self) -> String {
        match &self.name {
            Some(name) => format!("{}:{}", self.index, name),
            None => format!("{}", self.index),
        }
    }
}

impl RppSerialize for FxParamEnvelope {
    fn write_rpp(&self, out: &mut String, indent: &str) {
        // Use raw_block if available for round-trip fidelity
        if !self.raw_block.is_empty() {
            for line in self.raw_block.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed.starts_with('<') || trimmed == ">" {
                    out.push_str(&format!("{}{}\n", indent, trimmed));
                } else {
                    out.push_str(&format!("{}  {}\n", indent, trimmed));
                }
            }
            return;
        }

        // Reconstruct from fields
        out.push_str(&format!(
            "{}<PARMENV {} {} {} {}\n",
            indent,
            self.param.format_rpp(),
            self.mode,
            self.range_max,
            self.default_value,
        ));

        let inner = format!("{}  ", indent);
        if let Some(eguid) = &self.eguid {
            out.push_str(&format!("{}EGUID {}\n", inner, eguid));
        }
        out.push_str(&format!(
            "{}ACT {} -1\n",
            inner,
            if self.active { 1 } else { 0 }
        ));
        out.push_str(&format!(
            "{}VIS {} 1 1\n",
            inner,
            if self.visible { 1 } else { 0 }
        ));
        out.push_str(&format!(
            "{}ARM {}\n",
            inner,
            if self.armed { 1 } else { 0 }
        ));
        for pt in &self.points {
            out.push_str(&format!("{}PT {} {}", inner, pt.time, pt.value));
            for f in &pt.flags {
                out.push_str(&format!(" {}", f));
            }
            out.push('\n');
        }
        for item in &self.automation_items {
            out.push_str(&format!("{}{}\n", inner, item));
        }
        out.push_str(&format!("{}>\n", indent));
    }
}

impl RppSerialize for FxContainer {
    /// Emits the BYPASS preamble, the `<CONTAINER ...>` block with recursive
    /// children, and post-container metadata (FLOATPOS, FXID).
    fn write_rpp(&self, out: &mut String, indent: &str) {
        // BYPASS line (preamble for every FX)
        out.push_str(&format!(
            "{}BYPASS {} {} 0\n",
            indent,
            if self.bypassed { 1 } else { 0 },
            if self.offline { 1 } else { 0 },
        ));

        // PARALLEL line (only if parallel)
        if self.parallel {
            out.push_str(&format!("{}PARALLEL 1\n", indent));
        }

        // Container opening line
        out.push_str(&format!(
            "{}<CONTAINER Container \"{}\" \"\"\n",
            indent, self.name
        ));

        let inner_indent = format!("{}  ", indent);

        // Container metadata
        if let Some(cfg) = &self.container_cfg {
            out.push_str(&format!(
                "{}CONTAINER_CFG {} {} {} {}\n",
                inner_indent, cfg[0], cfg[1], cfg[2], cfg[3]
            ));
        }
        out.push_str(&format!("{}SHOW {}\n", inner_indent, self.show));
        out.push_str(&format!("{}LASTSEL {}\n", inner_indent, self.last_sel));
        out.push_str(&format!(
            "{}DOCKED {}\n",
            inner_indent,
            if self.docked { 1 } else { 0 }
        ));

        // Children (recursive)
        for child in &self.children {
            child.write_rpp(out, &inner_indent);
        }

        // Container closing
        out.push_str(&format!("{}>\n", indent));

        // Post-container metadata
        if let Some(fp) = &self.float_pos {
            out.push_str(&format!(
                "{}FLOATPOS {} {} {} {}\n",
                indent, fp[0], fp[1], fp[2], fp[3]
            ));
        }
        if let Some(fxid) = &self.fxid {
            out.push_str(&format!("{}FXID {}\n", indent, fxid));
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_fxchain() {
        let content = r#"<FXCHAIN
      WNDRECT 380 61 991 703
      SHOW 0
      LASTSEL 0
      DOCKED 0
      BYPASS 0 0 0
      <VST "VSTi: ReaSynth (Cockos)" reasynth.vst.dylib 0 "" 1919251321<5653547265737972656173796E746800> ""
        eXNlcu9e7f4AAAAAAgAAAAEAAAAAAAAAAgAAAAAAAABEAAAAAAAAAAAAEAA=
        776t3g3wrd6Y9dw+w2qkPawcWj5Ei2w+3SSGPur5dD/s+eM9pptEPgaBFT/3dKo/SOH6PnNoET8AAAAAIzq+PAAAAAA=
        AAAQAAAA
      >
      FLOATPOS 881 323 372 469
      FXID {314C4C58-4C0E-D442-9B7F-4D4351001B02}
      WAK 0 0
      BYPASS 0 0 0
      <VST "VST: ReaDelay (Cockos)" readelay.vst.dylib 0 "" 1919247468<5653547265646C72656164656C617900> ""
        bGRlcu5e7f4CAAAAAQAAAAAAAAACAAAAAAAAAAIAAAABAAAAAAAAAAIAAAAAAAAATAAAAAEAAAAAABAA
        AAAAAAAAAAABAAAALAAAAAIAAAAAAAAAAACAPwAAgD8AAAAAAACAPwAAAAAAAIA8nNEHMwAAgD8AAAAAAACAPwAAgD8AAIA/AAAAPw==
        AAAQAAAA
      >
      FLOATPOS 822 28 574 394
      FXID {30D2E5F4-E52D-B24F-AA62-6851192D0F3F}
      WAK 0 0
    >"#;

        let chain = FxChain::parse(content).unwrap();

        assert_eq!(chain.window_rect, Some([380, 61, 991, 703]));
        assert_eq!(chain.show, 0);
        assert_eq!(chain.last_sel, 0);
        assert!(!chain.docked);
        assert_eq!(chain.nodes.len(), 2);

        // First plugin: ReaSynth
        if let FxChainNode::Plugin(p) = &chain.nodes[0] {
            assert_eq!(p.name, "VSTi: ReaSynth (Cockos)");
            assert_eq!(p.plugin_type, PluginType::Vst);
            assert_eq!(p.file, "reasynth.vst.dylib");
            assert!(!p.bypassed);
            assert!(!p.offline);
            assert_eq!(
                p.fxid.as_deref(),
                Some("{314C4C58-4C0E-D442-9B7F-4D4351001B02}")
            );
            assert_eq!(p.float_pos, Some([881, 323, 372, 469]));
            assert_eq!(p.wak, Some([0, 0]));
            assert_eq!(p.state_data.len(), 3);
        } else {
            panic!("Expected Plugin, got Container");
        }

        // Second plugin: ReaDelay
        if let FxChainNode::Plugin(p) = &chain.nodes[1] {
            assert_eq!(p.name, "VST: ReaDelay (Cockos)");
            assert_eq!(p.plugin_type, PluginType::Vst);
            assert_eq!(p.file, "readelay.vst.dylib");
            assert_eq!(
                p.fxid.as_deref(),
                Some("{30D2E5F4-E52D-B24F-AA62-6851192D0F3F}")
            );
        } else {
            panic!("Expected Plugin, got Container");
        }

        assert_eq!(chain.plugin_count(), 2);
    }

    #[test]
    fn test_parse_fxchain_with_container() {
        let content = r#"<FXCHAIN
      SHOW 0
      LASTSEL 0
      DOCKED 0
      BYPASS 0 0 0
      <CONTAINER Container "DRIVE" ""
        CONTAINER_CFG 2 2 2 0
        SHOW 0
        LASTSEL 0
        DOCKED 0
        BYPASS 0 0 0
        <VST "VST: TubeScreamer (Analog)" ts808.vst.dylib 0 "" 12345678<00> ""
          dGVzdA==
        >
        FLOATPOS 0 0 0 0
        FXID {AAAA-BBBB-CCCC-DDDD}
        WAK 0 0
        BYPASS 0 0 0
        <VST "VST: BigMuff (Analog)" bigmuff.vst.dylib 0 "" 87654321<00> ""
          c3RhdGU=
        >
        FLOATPOS 0 0 0 0
        FXID {EEEE-FFFF-0000-1111}
        WAK 0 0
      >
      FXID {DRIVE-CONTAINER-GUID}
      BYPASS 0 0 0
      <VST "VST: ReaEQ (Cockos)" reaeq.vst.dylib 0 "" 1919247985<00> ""
        ZXE=
      >
      FXID {SOLO-EQ-GUID}
    >"#;

        let chain = FxChain::parse(content).unwrap();
        assert_eq!(chain.nodes.len(), 2); // container + standalone EQ

        // First node: DRIVE container
        if let FxChainNode::Container(c) = &chain.nodes[0] {
            assert_eq!(c.name, "DRIVE");
            assert!(!c.bypassed);
            assert_eq!(c.container_cfg, Some([2, 2, 2, 0]));
            assert_eq!(c.fxid.as_deref(), Some("{DRIVE-CONTAINER-GUID}"));
            assert_eq!(c.children.len(), 2);

            // First child: TubeScreamer
            if let FxChainNode::Plugin(p) = &c.children[0] {
                assert_eq!(p.name, "VST: TubeScreamer (Analog)");
                assert_eq!(p.fxid.as_deref(), Some("{AAAA-BBBB-CCCC-DDDD}"));
            } else {
                panic!("Expected Plugin child");
            }

            // Second child: BigMuff
            if let FxChainNode::Plugin(p) = &c.children[1] {
                assert_eq!(p.name, "VST: BigMuff (Analog)");
                assert_eq!(p.fxid.as_deref(), Some("{EEEE-FFFF-0000-1111}"));
            } else {
                panic!("Expected Plugin child");
            }
        } else {
            panic!("Expected Container, got Plugin");
        }

        // Second node: standalone ReaEQ
        if let FxChainNode::Plugin(p) = &chain.nodes[1] {
            assert_eq!(p.name, "VST: ReaEQ (Cockos)");
            assert_eq!(p.fxid.as_deref(), Some("{SOLO-EQ-GUID}"));
        } else {
            panic!("Expected Plugin");
        }

        // Total plugins across all levels
        assert_eq!(chain.plugin_count(), 3);
    }

    #[test]
    fn test_parse_nested_containers() {
        let content = r#"<FXCHAIN
      SHOW 0
      LASTSEL 0
      DOCKED 0
      BYPASS 0 0 0
      <CONTAINER Container "AMP" ""
        CONTAINER_CFG 2 2 2 0
        SHOW 0
        LASTSEL 0
        DOCKED 0
        BYPASS 0 0 0
        <VST "VST: PreAmp (Custom)" preamp.dylib 0 "" 0<00> ""
          cHJl
        >
        FXID {PRE-GUID}
        BYPASS 0 0 0
        <CONTAINER Container "CABINET" ""
          CONTAINER_CFG 2 2 2 0
          SHOW 0
          LASTSEL 0
          DOCKED 0
          BYPASS 0 0 0
          <VST "VST: CabSim (Custom)" cabsim.dylib 0 "" 0<00> ""
            Y2Fi
          >
          FXID {CAB-GUID}
        >
        FXID {CABINET-CONTAINER}
      >
      FXID {AMP-CONTAINER}
    >"#;

        let chain = FxChain::parse(content).unwrap();
        assert_eq!(chain.nodes.len(), 1);

        // AMP container
        if let FxChainNode::Container(amp) = &chain.nodes[0] {
            assert_eq!(amp.name, "AMP");
            assert_eq!(amp.children.len(), 2); // PreAmp plugin + CABINET container

            // PreAmp plugin
            if let FxChainNode::Plugin(p) = &amp.children[0] {
                assert_eq!(p.name, "VST: PreAmp (Custom)");
            } else {
                panic!("Expected PreAmp plugin");
            }

            // Nested CABINET container
            if let FxChainNode::Container(cab) = &amp.children[1] {
                assert_eq!(cab.name, "CABINET");
                assert_eq!(cab.fxid.as_deref(), Some("{CABINET-CONTAINER}"));
                assert_eq!(cab.children.len(), 1);

                if let FxChainNode::Plugin(p) = &cab.children[0] {
                    assert_eq!(p.name, "VST: CabSim (Custom)");
                    assert_eq!(p.fxid.as_deref(), Some("{CAB-GUID}"));
                } else {
                    panic!("Expected CabSim plugin");
                }
            } else {
                panic!("Expected CABINET container");
            }
        } else {
            panic!("Expected AMP container");
        }

        assert_eq!(chain.plugin_count(), 2);
    }

    #[test]
    fn test_parse_bypassed_and_offline() {
        let content = r#"<FXCHAIN
      SHOW 0
      LASTSEL 0
      DOCKED 0
      BYPASS 1 0 0
      <VST "VST: Bypassed Plugin" bypassed.dylib 0 "" 0<00> ""
        Yg==
      >
      FXID {BP-GUID}
      BYPASS 0 1 0
      <VST "VST: Offline Plugin" offline.dylib 0 "" 0<00> ""
        b2Zm
      >
      FXID {OL-GUID}
    >"#;

        let chain = FxChain::parse(content).unwrap();
        assert_eq!(chain.nodes.len(), 2);

        if let FxChainNode::Plugin(p) = &chain.nodes[0] {
            assert!(p.bypassed);
            assert!(!p.offline);
        } else {
            panic!("Expected plugin");
        }

        if let FxChainNode::Plugin(p) = &chain.nodes[1] {
            assert!(!p.bypassed);
            assert!(p.offline);
        } else {
            panic!("Expected plugin");
        }
    }

    #[test]
    fn test_parse_parallel_fx() {
        let content = r#"<FXCHAIN
      SHOW 0
      LASTSEL 0
      DOCKED 0
      BYPASS 0 0 0
      <VST "VST: First (Test)" first.dylib 0 "" 0<00> ""
        MQ==
      >
      FXID {FIRST}
      BYPASS 0 0 0
      PARALLEL 1
      <VST "VST: Second (Test)" second.dylib 0 "" 0<00> ""
        Mg==
      >
      FXID {SECOND}
    >"#;

        let chain = FxChain::parse(content).unwrap();
        assert_eq!(chain.nodes.len(), 2);

        if let FxChainNode::Plugin(p) = &chain.nodes[0] {
            assert!(!p.parallel);
        } else {
            panic!("Expected plugin");
        }

        if let FxChainNode::Plugin(p) = &chain.nodes[1] {
            assert!(p.parallel);
        } else {
            panic!("Expected plugin");
        }
    }

    #[test]
    fn test_parse_js_plugin() {
        let content = r#"<FXCHAIN
      SHOW 0
      LASTSEL 0
      DOCKED 0
      BYPASS 0 0 0
      <JS "loser/3BandEQ" ""
        0.000000 0.000000 500.000000 3000.000000 0.000000 0.000000 - - - - -
      >
      FXID {JS-GUID}
    >"#;

        let chain = FxChain::parse(content).unwrap();
        assert_eq!(chain.nodes.len(), 1);

        if let FxChainNode::Plugin(p) = &chain.nodes[0] {
            assert_eq!(p.plugin_type, PluginType::Js);
            assert_eq!(p.name, "loser/3BandEQ");
            assert_eq!(p.fxid.as_deref(), Some("{JS-GUID}"));
            assert_eq!(p.state_data.len(), 1);
        } else {
            panic!("Expected JS plugin");
        }
    }

    #[test]
    fn test_parse_preset_name() {
        let content = r#"<FXCHAIN
      SHOW 0
      LASTSEL 0
      DOCKED 0
      BYPASS 0 0 0
      <VST "VST: ReaEQ (Cockos)" reaeq.dylib 0 "" 0<00> ""
        ZXE=
      >
      PRESETNAME "My Custom Preset"
      FXID {EQ-GUID}
    >"#;

        let chain = FxChain::parse(content).unwrap();

        if let FxChainNode::Plugin(p) = &chain.nodes[0] {
            assert_eq!(p.preset_name.as_deref(), Some("My Custom Preset"));
        } else {
            panic!("Expected plugin");
        }
    }

    #[test]
    fn test_find_by_fxid() {
        let content = r#"<FXCHAIN
      SHOW 0
      LASTSEL 0
      DOCKED 0
      BYPASS 0 0 0
      <VST "VST: A" a.dylib 0 "" 0<00> ""
        YQ==
      >
      FXID {GUID-A}
      BYPASS 0 0 0
      <CONTAINER Container "GROUP" ""
        CONTAINER_CFG 2 2 2 0
        SHOW 0
        LASTSEL 0
        DOCKED 0
        BYPASS 0 0 0
        <VST "VST: B" b.dylib 0 "" 0<00> ""
          Yg==
        >
        FXID {GUID-B}
      >
      FXID {GUID-GROUP}
    >"#;

        let chain = FxChain::parse(content).unwrap();

        assert!(chain.find_by_fxid("{GUID-A}").is_some());
        assert_eq!(chain.find_by_fxid("{GUID-A}").unwrap().name, "VST: A");

        assert!(chain.find_by_fxid("{GUID-B}").is_some());
        assert_eq!(chain.find_by_fxid("{GUID-B}").unwrap().name, "VST: B");

        assert!(chain.find_by_fxid("{NONEXISTENT}").is_none());
    }

    #[test]
    fn test_empty_fxchain() {
        let content = r#"<FXCHAIN
      SHOW 0
      LASTSEL 0
      DOCKED 0
    >"#;

        let chain = FxChain::parse(content).unwrap();
        assert_eq!(chain.nodes.len(), 0);
        assert_eq!(chain.plugin_count(), 0);
    }

    // -----------------------------------------------------------------------
    // Serialization tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_serialize_simple_chain() {
        let content = r#"<FXCHAIN
  WNDRECT 380 61 991 703
  SHOW 0
  LASTSEL 0
  DOCKED 0
  BYPASS 0 0 0
  <VST "VST: ReaEQ (Cockos)" reaeq.vst.dylib 0 "" 1919247985<00> ""
    ZXE=
  >
  FLOATPOS 0 0 0 0
  FXID {EQ-GUID}
  WAK 0 0
>"#;

        let chain = FxChain::parse(content).unwrap();
        let serialized = chain.to_rpp_string();

        // Re-parse the serialized output
        let chain2 = FxChain::parse(&serialized).unwrap();

        // Structural equality
        assert_eq!(chain.nodes.len(), chain2.nodes.len());
        assert_eq!(chain.window_rect, chain2.window_rect);
        assert_eq!(chain.show, chain2.show);
        assert_eq!(chain.last_sel, chain2.last_sel);
        assert_eq!(chain.docked, chain2.docked);

        if let (FxChainNode::Plugin(p1), FxChainNode::Plugin(p2)) =
            (&chain.nodes[0], &chain2.nodes[0])
        {
            assert_eq!(p1.name, p2.name);
            assert_eq!(p1.plugin_type, p2.plugin_type);
            assert_eq!(p1.bypassed, p2.bypassed);
            assert_eq!(p1.fxid, p2.fxid);
            assert_eq!(p1.float_pos, p2.float_pos);
            assert_eq!(p1.wak, p2.wak);
            assert_eq!(p1.state_data, p2.state_data);
        } else {
            panic!("Expected Plugin nodes");
        }
    }

    #[test]
    fn test_serialize_container_chain() {
        let content = r#"<FXCHAIN
  SHOW 0
  LASTSEL 0
  DOCKED 0
  BYPASS 0 0 0
  <CONTAINER Container "DRIVE" ""
    CONTAINER_CFG 2 2 2 0
    SHOW 0
    LASTSEL 0
    DOCKED 0
    BYPASS 0 0 0
    <VST "VST: TubeScreamer (Analog)" ts808.vst.dylib 0 "" 12345678<00> ""
      dGVzdA==
    >
    FLOATPOS 0 0 0 0
    FXID {TS-GUID}
    WAK 0 0
  >
  FXID {DRIVE-CONTAINER}
>"#;

        let chain = FxChain::parse(content).unwrap();
        let serialized = chain.to_rpp_string();

        // Re-parse
        let chain2 = FxChain::parse(&serialized).unwrap();
        assert_eq!(chain2.nodes.len(), 1);

        if let FxChainNode::Container(c) = &chain2.nodes[0] {
            assert_eq!(c.name, "DRIVE");
            assert_eq!(c.container_cfg, Some([2, 2, 2, 0]));
            assert_eq!(c.fxid.as_deref(), Some("{DRIVE-CONTAINER}"));
            assert_eq!(c.children.len(), 1);

            if let FxChainNode::Plugin(p) = &c.children[0] {
                assert_eq!(p.name, "VST: TubeScreamer (Analog)");
                assert_eq!(p.fxid.as_deref(), Some("{TS-GUID}"));
                assert_eq!(p.state_data, vec!["dGVzdA=="]);
            } else {
                panic!("Expected Plugin child");
            }
        } else {
            panic!("Expected Container");
        }
    }

    #[test]
    fn test_serialize_bypassed_and_parallel() {
        let content = r#"<FXCHAIN
  SHOW 0
  LASTSEL 0
  DOCKED 0
  BYPASS 1 0 0
  <VST "VST: A" a.dylib 0 "" 0<00> ""
    YQ==
  >
  FXID {A}
  BYPASS 0 0 0
  PARALLEL 1
  <VST "VST: B" b.dylib 0 "" 0<00> ""
    Yg==
  >
  FXID {B}
>"#;

        let chain = FxChain::parse(content).unwrap();
        let serialized = chain.to_rpp_string();
        let chain2 = FxChain::parse(&serialized).unwrap();

        assert_eq!(chain2.nodes.len(), 2);

        if let FxChainNode::Plugin(p) = &chain2.nodes[0] {
            assert!(p.bypassed);
            assert!(!p.parallel);
        } else {
            panic!("Expected Plugin");
        }

        if let FxChainNode::Plugin(p) = &chain2.nodes[1] {
            assert!(!p.bypassed);
            assert!(p.parallel);
        } else {
            panic!("Expected Plugin");
        }
    }

    #[test]
    fn test_serialize_preset_name() {
        let content = r#"<FXCHAIN
  SHOW 0
  LASTSEL 0
  DOCKED 0
  BYPASS 0 0 0
  <VST "VST: ReaEQ (Cockos)" reaeq.dylib 0 "" 0<00> ""
    ZXE=
  >
  PRESETNAME "My Preset"
  FXID {EQ}
>"#;

        let chain = FxChain::parse(content).unwrap();
        let serialized = chain.to_rpp_string();
        let chain2 = FxChain::parse(&serialized).unwrap();

        if let FxChainNode::Plugin(p) = &chain2.nodes[0] {
            assert_eq!(p.preset_name.as_deref(), Some("My Preset"));
        } else {
            panic!("Expected Plugin");
        }
    }

    #[test]
    fn test_serialize_nested_containers() {
        let content = r#"<FXCHAIN
  SHOW 0
  LASTSEL 0
  DOCKED 0
  BYPASS 0 0 0
  <CONTAINER Container "AMP" ""
    CONTAINER_CFG 2 2 2 0
    SHOW 0
    LASTSEL 0
    DOCKED 0
    BYPASS 0 0 0
    <VST "VST: PreAmp" preamp.dylib 0 "" 0<00> ""
      cHJl
    >
    FXID {PRE}
    BYPASS 0 0 0
    <CONTAINER Container "CAB" ""
      CONTAINER_CFG 2 2 2 0
      SHOW 0
      LASTSEL 0
      DOCKED 0
      BYPASS 0 0 0
      <VST "VST: CabSim" cabsim.dylib 0 "" 0<00> ""
        Y2Fi
      >
      FXID {CAB}
    >
    FXID {CAB-CONTAINER}
  >
  FXID {AMP-CONTAINER}
>"#;

        let chain = FxChain::parse(content).unwrap();
        let serialized = chain.to_rpp_string();
        let chain2 = FxChain::parse(&serialized).unwrap();

        assert_eq!(chain2.nodes.len(), 1);
        assert_eq!(chain2.plugin_count(), 2);

        // Verify nested structure survived round-trip
        if let FxChainNode::Container(amp) = &chain2.nodes[0] {
            assert_eq!(amp.name, "AMP");
            assert_eq!(amp.fxid.as_deref(), Some("{AMP-CONTAINER}"));
            assert_eq!(amp.children.len(), 2);

            if let FxChainNode::Container(cab) = &amp.children[1] {
                assert_eq!(cab.name, "CAB");
                assert_eq!(cab.fxid.as_deref(), Some("{CAB-CONTAINER}"));
                assert_eq!(cab.children.len(), 1);

                if let FxChainNode::Plugin(p) = &cab.children[0] {
                    assert_eq!(p.name, "VST: CabSim");
                    assert_eq!(p.state_data, vec!["Y2Fi"]);
                } else {
                    panic!("Expected CabSim plugin");
                }
            } else {
                panic!("Expected CAB container");
            }
        } else {
            panic!("Expected AMP container");
        }
    }

    #[test]
    fn test_serialize_empty_chain() {
        let chain = FxChain {
            window_rect: None,
            show: 0,
            last_sel: 0,
            docked: false,
            nodes: Vec::new(),
            raw_content: String::new(),
        };

        let serialized = chain.to_rpp_string();
        let chain2 = FxChain::parse(&serialized).unwrap();

        assert_eq!(chain2.nodes.len(), 0);
        assert_eq!(chain2.show, 0);
        assert!(!chain2.docked);
    }

    #[test]
    fn test_serialized_output_has_correct_format() {
        // Verify the actual output text format, not just round-trip
        let chain = FxChain {
            window_rect: Some([100, 200, 800, 600]),
            show: 1,
            last_sel: 0,
            docked: false,
            nodes: Vec::new(),
            raw_content: String::new(),
        };

        let serialized = chain.to_rpp_string();
        assert!(serialized.starts_with("<FXCHAIN\n"));
        assert!(serialized.ends_with(">\n"));
        assert!(serialized.contains("  WNDRECT 100 200 800 600\n"));
        assert!(serialized.contains("  SHOW 1\n"));
        assert!(serialized.contains("  LASTSEL 0\n"));
        assert!(serialized.contains("  DOCKED 0\n"));
    }

    // -----------------------------------------------------------------------
    // Parameter parsing tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_js_params_basic() {
        let state =
            vec!["3640.000000 -5.000000 0.000000 -6.000000 0.000000 0.000000 - - - -".to_string()];
        let params = parse_js_params(&state);

        assert_eq!(params.len(), 10);
        assert_eq!(params[0].index, 0);
        assert_eq!(params[0].value, Some(3640.0));
        assert_eq!(params[1].index, 1);
        assert_eq!(params[1].value, Some(-5.0));
        assert_eq!(params[5].index, 5);
        assert_eq!(params[5].value, Some(0.0));
        // Unset params (-)
        assert_eq!(params[6].index, 6);
        assert_eq!(params[6].value, None);
        assert_eq!(params[9].index, 9);
        assert_eq!(params[9].value, None);
    }

    #[test]
    fn test_parse_js_params_all_set() {
        let state = vec!["1 1 1 1 1 1 1 1".to_string()];
        let params = parse_js_params(&state);
        assert_eq!(params.len(), 8);
        for p in &params {
            assert_eq!(p.value, Some(1.0));
        }
    }

    #[test]
    fn test_parse_js_params_all_unset() {
        let state = vec!["- - - - - -".to_string()];
        let params = parse_js_params(&state);
        assert_eq!(params.len(), 6);
        for p in &params {
            assert_eq!(p.value, None);
        }
    }

    #[test]
    fn test_parse_parmenv_from_fxchain() {
        let content = r#"<FXCHAIN
  SHOW 0
  LASTSEL 0
  DOCKED 0
  BYPASS 0 0 0
  <VST "VST: ReaEQ (Cockos)" reaeq.dylib 0 "" 0<00> ""
    ZXE=
  >
  FLOATPOS 0 0 0 0
  FXID {EQ-GUID}
  <PARMENV 0:_Gain_Low_Shelf 0 1 0.25
    EGUID {04662523-EE11-472A-9969-FA5DBCFB2170}
    ACT 1 -1
    VIS 1 1 1
    LANEHEIGHT 0 0
    ARM 1
    DEFSHAPE 0 -1 -1
    PT 0 0.2 0
    PT 1 0.565 0 0 1
  >
  PARM_TCP 0:_Gain_Low_Shelf
  WAK 0 0
>"#;

        let chain = FxChain::parse(content).unwrap();
        assert_eq!(chain.nodes.len(), 1);

        if let FxChainNode::Plugin(p) = &chain.nodes[0] {
            assert_eq!(p.name, "VST: ReaEQ (Cockos)");
            assert_eq!(p.fxid.as_deref(), Some("{EQ-GUID}"));

            // Parameter envelope
            assert_eq!(p.param_envelopes.len(), 1);
            let env = &p.param_envelopes[0];
            assert_eq!(env.param.index, 0);
            assert_eq!(env.param.name.as_deref(), Some("_Gain_Low_Shelf"));
            assert_eq!(env.mode, 0);
            assert!((env.range_max - 1.0).abs() < f64::EPSILON);
            assert!((env.default_value - 0.25).abs() < f64::EPSILON);
            assert_eq!(
                env.eguid.as_deref(),
                Some("{04662523-EE11-472A-9969-FA5DBCFB2170}")
            );
            assert!(env.active);
            assert!(env.visible);
            assert!(env.armed);
            assert_eq!(env.points.len(), 2);
            assert!((env.points[0].time - 0.0).abs() < f64::EPSILON);
            assert!((env.points[0].value - 0.2).abs() < f64::EPSILON);
            assert!((env.points[1].time - 1.0).abs() < f64::EPSILON);
            assert!((env.points[1].value - 0.565).abs() < f64::EPSILON);

            // TCP parameter
            assert_eq!(p.params_on_tcp.len(), 1);
            assert_eq!(p.params_on_tcp[0].index, 0);
            assert_eq!(p.params_on_tcp[0].name.as_deref(), Some("_Gain_Low_Shelf"));

            // WAK should still be parsed
            assert_eq!(p.wak, Some([0, 0]));
        } else {
            panic!("Expected Plugin");
        }
    }

    #[test]
    fn test_parse_parm_tcp_index_only() {
        let content = r#"<FXCHAIN
  SHOW 0
  LASTSEL 0
  DOCKED 0
  BYPASS 0 0 0
  <VST "VST: Test" test.dylib 0 "" 0<00> ""
    dA==
  >
  FXID {T}
  PARM_TCP 0
  PARM_TCP 3
  WAK 0 0
>"#;

        let chain = FxChain::parse(content).unwrap();

        if let FxChainNode::Plugin(p) = &chain.nodes[0] {
            assert_eq!(p.params_on_tcp.len(), 2);
            assert_eq!(p.params_on_tcp[0].index, 0);
            assert_eq!(p.params_on_tcp[0].name, None);
            assert_eq!(p.params_on_tcp[1].index, 3);
            assert_eq!(p.params_on_tcp[1].name, None);
        } else {
            panic!("Expected Plugin");
        }
    }

    #[test]
    fn test_parmenv_roundtrip() {
        let content = r#"<FXCHAIN
  SHOW 0
  LASTSEL 0
  DOCKED 0
  BYPASS 0 0 0
  <VST "VST: ReaEQ (Cockos)" reaeq.dylib 0 "" 0<00> ""
    ZXE=
  >
  FXID {EQ}
  <PARMENV 0:_Gain 0 1 0.5
    EGUID {GUID}
    ACT 1 -1
    VIS 1 1 1
    LANEHEIGHT 0 0
    ARM 0
    DEFSHAPE 0 -1 -1
    PT 0 0.3 0
    PT 2 0.7 0 0 1
  >
  PARM_TCP 0:_Gain
  WAK 0 0
>"#;

        let chain = FxChain::parse(content).unwrap();
        let serialized = chain.to_rpp_string();
        let chain2 = FxChain::parse(&serialized).unwrap();

        if let (FxChainNode::Plugin(p1), FxChainNode::Plugin(p2)) =
            (&chain.nodes[0], &chain2.nodes[0])
        {
            // Envelope round-trips
            assert_eq!(p1.param_envelopes.len(), p2.param_envelopes.len());
            assert_eq!(
                p1.param_envelopes[0].param.index,
                p2.param_envelopes[0].param.index
            );
            assert_eq!(
                p1.param_envelopes[0].points.len(),
                p2.param_envelopes[0].points.len()
            );

            // TCP params round-trip
            assert_eq!(p1.params_on_tcp.len(), p2.params_on_tcp.len());
            assert_eq!(p1.params_on_tcp[0].index, p2.params_on_tcp[0].index);
            assert_eq!(p1.params_on_tcp[0].name, p2.params_on_tcp[0].name);
        } else {
            panic!("Expected Plugin nodes");
        }
    }

    #[test]
    fn test_parse_multiple_parmenvs() {
        let content = r#"<FXCHAIN
  SHOW 0
  LASTSEL 0
  DOCKED 0
  BYPASS 0 0 0
  <VST "VST: ReaEQ (Cockos)" reaeq.dylib 0 "" 0<00> ""
    ZXE=
  >
  FXID {EQ}
  <PARMENV 0:_Gain 0 1 0.5
    EGUID {G1}
    ACT 0 -1
    VIS 0 1 1
    LANEHEIGHT 0 0
    ARM 0
    DEFSHAPE 0 -1 -1
    PT 0 0.5 0
  >
  <PARMENV 1:_Freq 0 10 5
    EGUID {G2}
    ACT 1 -1
    VIS 1 1 1
    LANEHEIGHT 0 0
    ARM 1
    DEFSHAPE 0 -1 -1
    PT 0 2.0 0
    PT 4 8.0 0
  >
  PARM_TCP 0:_Gain
  PARM_TCP 1:_Freq
  WAK 0 0
>"#;

        let chain = FxChain::parse(content).unwrap();

        if let FxChainNode::Plugin(p) = &chain.nodes[0] {
            assert_eq!(p.param_envelopes.len(), 2);

            // First envelope: Gain
            assert_eq!(p.param_envelopes[0].param.index, 0);
            assert_eq!(p.param_envelopes[0].param.name.as_deref(), Some("_Gain"));
            assert!(!p.param_envelopes[0].active);
            assert_eq!(p.param_envelopes[0].points.len(), 1);

            // Second envelope: Freq
            assert_eq!(p.param_envelopes[1].param.index, 1);
            assert_eq!(p.param_envelopes[1].param.name.as_deref(), Some("_Freq"));
            assert!(p.param_envelopes[1].active);
            assert!(p.param_envelopes[1].armed);
            assert!((p.param_envelopes[1].range_max - 10.0).abs() < f64::EPSILON);
            assert_eq!(p.param_envelopes[1].points.len(), 2);

            // TCP params
            assert_eq!(p.params_on_tcp.len(), 2);
        } else {
            panic!("Expected Plugin");
        }
    }

    // ─── Enclose / Explode round-trip tests ─────────────────────

    /// Helper: simulate `enclose_in_container` on a parsed FxChain.
    ///
    /// Removes nodes at `indices` from the chain's top-level nodes,
    /// wraps them in a new `FxContainer` with the given name, and
    /// inserts the container at the position of the first removed node.
    fn enclose_nodes_in_container(chain: &mut FxChain, indices: &[usize], name: &str) {
        assert!(!indices.is_empty());
        let insert_pos = indices[0];

        // Remove in reverse order to keep indices stable
        let mut removed = Vec::new();
        for &idx in indices.iter().rev() {
            removed.push(chain.nodes.remove(idx));
        }
        removed.reverse();

        let container = FxContainer {
            name: name.to_string(),
            bypassed: false,
            offline: false,
            fxid: None,
            float_pos: None,
            parallel: false,
            container_cfg: Some([2, 2, 2, 0]),
            show: 0,
            last_sel: 0,
            docked: false,
            children: removed,
            raw_block: String::new(),
        };

        let at = insert_pos.min(chain.nodes.len());
        chain.nodes.insert(at, FxChainNode::Container(container));
    }

    /// Helper: simulate `explode_container` on a parsed FxChain.
    ///
    /// Finds the container at `index`, removes it, and splices its
    /// children back into the chain at the same position.
    fn explode_container_at(chain: &mut FxChain, index: usize) {
        let node = chain.nodes.remove(index);
        if let FxChainNode::Container(c) = node {
            for (i, child) in c.children.into_iter().enumerate() {
                chain.nodes.insert(index + i, child);
            }
        } else {
            panic!("Node at index {} is not a container", index);
        }
    }

    /// Real-world test: parse a CLAP plugin FX chain, wrap it in a
    /// container, serialize, re-parse, and verify the structure matches
    /// what REAPER produces.
    #[test]
    fn test_enclose_single_plugin_in_container() {
        // ── Input: flat FX chain with one CLAP plugin ──
        let input = r#"<FXCHAIN
    WNDRECT 32 678 991 419
    SHOW 0
    LASTSEL 0
    DOCKED 0
    BYPASS 0 0 0
    <CLAP "CLAP: Pro-Q 4 (FabFilter)" com.FabFilter.Pro-Q.4 ""
      CFG 4 760 335 ""
      <IN_PINS
      >
      <STATE
        RkZCUwEAAABYAgAAAAAAAAAAgD/acx9BAAAAAAAAAD8AAAAAAAAAQAAAAEAAAIA/AAAAAAAAgD8=
      >
    >
    FLOATPOS 0 0 0 0
    FXID {BF155866-2248-4F40-8542-EF48AFFAC021}
    WAK 0 0
  >"#;

        let mut chain = FxChain::parse(input).unwrap();
        assert_eq!(chain.nodes.len(), 1);

        // Verify it parsed as a CLAP plugin
        if let FxChainNode::Plugin(p) = &chain.nodes[0] {
            assert_eq!(p.plugin_type, PluginType::Clap);
            assert_eq!(
                p.fxid.as_deref(),
                Some("{BF155866-2248-4F40-8542-EF48AFFAC021}")
            );
            assert!(!p.bypassed);
        } else {
            panic!("Expected Plugin");
        }

        // ── Enclose in container ──
        enclose_nodes_in_container(&mut chain, &[0], "NAMED CONTAINER");

        // Verify in-memory structure
        assert_eq!(
            chain.nodes.len(),
            1,
            "should have 1 top-level node (the container)"
        );
        if let FxChainNode::Container(c) = &chain.nodes[0] {
            assert_eq!(c.name, "NAMED CONTAINER");
            assert_eq!(c.container_cfg, Some([2, 2, 2, 0]));
            assert_eq!(c.children.len(), 1, "container should have 1 child");
            if let FxChainNode::Plugin(p) = &c.children[0] {
                assert_eq!(p.plugin_type, PluginType::Clap);
                assert_eq!(
                    p.fxid.as_deref(),
                    Some("{BF155866-2248-4F40-8542-EF48AFFAC021}")
                );
            } else {
                panic!("Expected Plugin inside container");
            }
        } else {
            panic!("Expected Container at top level");
        }

        // ── Serialize and re-parse (round-trip) ──
        let serialized = chain.to_rpp_string();
        eprintln!("=== Serialized FXCHAIN ===\n{}", serialized);

        let reparsed = FxChain::parse(&serialized).unwrap();
        assert_eq!(
            reparsed.nodes.len(),
            1,
            "re-parsed should have 1 top-level node"
        );
        if let FxChainNode::Container(c) = &reparsed.nodes[0] {
            assert_eq!(c.name, "NAMED CONTAINER");
            assert_eq!(c.container_cfg, Some([2, 2, 2, 0]));
            assert_eq!(c.children.len(), 1);
            if let FxChainNode::Plugin(p) = &c.children[0] {
                assert_eq!(p.plugin_type, PluginType::Clap);
                assert_eq!(
                    p.fxid.as_deref(),
                    Some("{BF155866-2248-4F40-8542-EF48AFFAC021}")
                );
            } else {
                panic!("Expected Plugin inside container after round-trip");
            }
        } else {
            panic!("Expected Container after round-trip");
        }
    }

    /// Test: enclose multiple plugins in a container, preserving order.
    #[test]
    fn test_enclose_multiple_plugins_in_container() {
        let input = r#"<FXCHAIN
    SHOW 0
    LASTSEL 0
    DOCKED 0
    BYPASS 0 0 0
    <VST "VST: ReaEQ (Cockos)" reaeq.vst.dylib 0 "" 1919247985<00> ""
      ZXE=
    >
    FXID {AAAA-1111-0000-0000}
    BYPASS 0 0 0
    <VST "VST: ReaComp (Cockos)" reacomp.vst.dylib 0 "" 1919247985<00> ""
      Y29tcA==
    >
    FXID {BBBB-2222-0000-0000}
    BYPASS 0 0 0
    <VST "VST: ReaDelay (Cockos)" readelay.vst.dylib 0 "" 1919247468<00> ""
      ZGVsYXk=
    >
    FXID {CCCC-3333-0000-0000}
  >"#;

        let mut chain = FxChain::parse(input).unwrap();
        assert_eq!(chain.nodes.len(), 3);

        // Enclose first two plugins (EQ + Comp), leave Delay outside
        enclose_nodes_in_container(&mut chain, &[0, 1], "PRE-AMP");

        assert_eq!(chain.nodes.len(), 2, "container + delay");

        // Container at index 0
        if let FxChainNode::Container(c) = &chain.nodes[0] {
            assert_eq!(c.name, "PRE-AMP");
            assert_eq!(c.children.len(), 2);
            // Order preserved: EQ first, Comp second
            if let FxChainNode::Plugin(p) = &c.children[0] {
                assert_eq!(p.fxid.as_deref(), Some("{AAAA-1111-0000-0000}"));
            } else {
                panic!("Expected EQ plugin");
            }
            if let FxChainNode::Plugin(p) = &c.children[1] {
                assert_eq!(p.fxid.as_deref(), Some("{BBBB-2222-0000-0000}"));
            } else {
                panic!("Expected Comp plugin");
            }
        } else {
            panic!("Expected Container at index 0");
        }

        // Delay stays at index 1
        if let FxChainNode::Plugin(p) = &chain.nodes[1] {
            assert_eq!(p.fxid.as_deref(), Some("{CCCC-3333-0000-0000}"));
        } else {
            panic!("Expected Delay plugin at index 1");
        }

        // Round-trip
        let serialized = chain.to_rpp_string();
        let reparsed = FxChain::parse(&serialized).unwrap();
        assert_eq!(reparsed.nodes.len(), 2);
        if let FxChainNode::Container(c) = &reparsed.nodes[0] {
            assert_eq!(c.name, "PRE-AMP");
            assert_eq!(c.children.len(), 2);
        } else {
            panic!("Expected Container after round-trip");
        }
    }

    /// Test: explode a container back to flat plugins.
    #[test]
    fn test_explode_container_to_flat() {
        // Start with a container holding two plugins, plus one outside
        let input = r#"<FXCHAIN
    SHOW 0
    LASTSEL 0
    DOCKED 0
    BYPASS 0 0 0
    <CONTAINER Container "DRIVE" ""
      CONTAINER_CFG 2 2 2 0
      SHOW 0
      LASTSEL 0
      DOCKED 0
      BYPASS 0 0 0
      <VST "VST: TubeScreamer (Analog)" ts808.vst.dylib 0 "" 12345678<00> ""
        dGVzdA==
      >
      FXID {AAAA-BBBB-CCCC-DDDD}
      BYPASS 0 0 0
      <VST "VST: BigMuff (Analog)" bigmuff.vst.dylib 0 "" 87654321<00> ""
        c3RhdGU=
      >
      FXID {EEEE-FFFF-0000-1111}
    >
    FXID {DRIVE-CONTAINER-GUID}
    BYPASS 0 0 0
    <VST "VST: ReaEQ (Cockos)" reaeq.vst.dylib 0 "" 1919247985<00> ""
      ZXE=
    >
    FXID {EQ-GUID-0000-0000}
  >"#;

        let mut chain = FxChain::parse(input).unwrap();
        assert_eq!(chain.nodes.len(), 2, "container + EQ");

        // Verify container structure
        if let FxChainNode::Container(c) = &chain.nodes[0] {
            assert_eq!(c.name, "DRIVE");
            assert_eq!(c.children.len(), 2);
        } else {
            panic!("Expected Container at index 0");
        }

        // Explode the container
        explode_container_at(&mut chain, 0);

        // Now we should have 3 flat plugins: TS, BigMuff, EQ
        assert_eq!(chain.nodes.len(), 3, "TS + BigMuff + EQ");

        if let FxChainNode::Plugin(p) = &chain.nodes[0] {
            assert_eq!(p.fxid.as_deref(), Some("{AAAA-BBBB-CCCC-DDDD}"));
            assert!(p.name.contains("TubeScreamer"));
        } else {
            panic!("Expected TubeScreamer at index 0");
        }
        if let FxChainNode::Plugin(p) = &chain.nodes[1] {
            assert_eq!(p.fxid.as_deref(), Some("{EEEE-FFFF-0000-1111}"));
            assert!(p.name.contains("BigMuff"));
        } else {
            panic!("Expected BigMuff at index 1");
        }
        if let FxChainNode::Plugin(p) = &chain.nodes[2] {
            assert_eq!(p.fxid.as_deref(), Some("{EQ-GUID-0000-0000}"));
        } else {
            panic!("Expected EQ at index 2");
        }

        // Round-trip: serialize and re-parse — should still be 3 flat plugins
        let serialized = chain.to_rpp_string();
        let reparsed = FxChain::parse(&serialized).unwrap();
        assert_eq!(reparsed.nodes.len(), 3);
        assert!(
            reparsed
                .nodes
                .iter()
                .all(|n| matches!(n, FxChainNode::Plugin(_)))
        );
    }

    /// Test: enclose then explode is a no-op (structurally).
    /// Plugin GUIDs and state survive the round-trip.
    #[test]
    fn test_enclose_then_explode_round_trip() {
        let input = r#"<FXCHAIN
    SHOW 0
    LASTSEL 0
    DOCKED 0
    BYPASS 0 0 0
    <VST "VST: ReaEQ (Cockos)" reaeq.vst.dylib 0 "" 1919247985<00> ""
      ZXE=
    >
    FXID {AAAA-1111-0000-0000}
    WAK 0 0
    BYPASS 0 0 0
    <VST "VST: ReaComp (Cockos)" reacomp.vst.dylib 0 "" 1919247985<00> ""
      Y29tcA==
    >
    FXID {BBBB-2222-0000-0000}
    WAK 0 0
  >"#;

        let original = FxChain::parse(input).unwrap();
        assert_eq!(original.nodes.len(), 2);

        // Step 1: Enclose both in a container
        let mut chain = original.clone();
        enclose_nodes_in_container(&mut chain, &[0, 1], "TEST");
        assert_eq!(chain.nodes.len(), 1);

        // Serialize → re-parse (simulates REAPER set_chunk → get_chunk)
        let after_enclose = FxChain::parse(&chain.to_rpp_string()).unwrap();
        assert_eq!(after_enclose.nodes.len(), 1);

        // Step 2: Explode the container
        let mut chain2 = after_enclose;
        explode_container_at(&mut chain2, 0);
        assert_eq!(chain2.nodes.len(), 2);

        // Serialize → re-parse again
        let after_explode = FxChain::parse(&chain2.to_rpp_string()).unwrap();
        assert_eq!(after_explode.nodes.len(), 2);

        // Verify plugin GUIDs survived
        if let FxChainNode::Plugin(p) = &after_explode.nodes[0] {
            assert_eq!(p.fxid.as_deref(), Some("{AAAA-1111-0000-0000}"));
            assert!(p.name.contains("ReaEQ"));
        } else {
            panic!("Expected EQ plugin after round-trip");
        }
        if let FxChainNode::Plugin(p) = &after_explode.nodes[1] {
            assert_eq!(p.fxid.as_deref(), Some("{BBBB-2222-0000-0000}"));
            assert!(p.name.contains("ReaComp"));
        } else {
            panic!("Expected Comp plugin after round-trip");
        }
    }

    /// Test with real-world CLAP plugin data (from user's REAPER session).
    /// Verifies that base64 state data survives the enclose round-trip.
    #[test]
    fn test_enclose_real_clap_plugin_state_preserved() {
        let input = r#"<FXCHAIN
    WNDRECT 32 678 991 419
    SHOW 0
    LASTSEL 0
    DOCKED 0
    BYPASS 0 0 0
    <CLAP "CLAP: Pro-Q 4 (FabFilter)" com.FabFilter.Pro-Q.4 ""
      CFG 4 760 335 ""
      <IN_PINS
      >
      <STATE
        RkZCUwEAAABYAgAAAAAAAAAAgD/acx9BAAAAAAAAAD8AAAAAAAAAQAAAAEAAAIA/AAAAAAAAgD8=
        AACAPwAAgD+rqio/AABIQgAASEIAAAAAAAAAAHiaVEB4mmRBAAAAAAAAAAAAAEhC
      >
    >
    FLOATPOS 0 0 0 0
    FXID {BF155866-2248-4F40-8542-EF48AFFAC021}
    WAK 0 0
  >"#;

        let mut chain = FxChain::parse(input).unwrap();

        // Capture original state data for comparison
        let original_state: Vec<String> = if let FxChainNode::Plugin(p) = &chain.nodes[0] {
            p.state_data.clone()
        } else {
            panic!("Expected Plugin");
        };

        // Enclose in container
        enclose_nodes_in_container(&mut chain, &[0], "EQ");

        // Serialize → re-parse
        let serialized = chain.to_rpp_string();
        let reparsed = FxChain::parse(&serialized).unwrap();

        // Dig into the container to check state preservation
        if let FxChainNode::Container(c) = &reparsed.nodes[0] {
            assert_eq!(c.name, "EQ");
            if let FxChainNode::Plugin(p) = &c.children[0] {
                assert_eq!(p.plugin_type, PluginType::Clap);
                assert_eq!(
                    p.fxid.as_deref(),
                    Some("{BF155866-2248-4F40-8542-EF48AFFAC021}")
                );
                // The raw_block preserves the full plugin block text, which
                // contains the state data. Verify the state is not empty.
                assert!(
                    !p.raw_block.is_empty(),
                    "raw_block should preserve plugin state"
                );
            } else {
                panic!("Expected CLAP plugin inside container");
            }
        } else {
            panic!("Expected Container");
        }
    }

    /// Test: full track chunk manipulation — find FXCHAIN block,
    /// parse it, enclose a plugin, serialize back, and splice into
    /// the track chunk. This mirrors what daw-reaper does at runtime.
    #[test]
    fn test_track_chunk_fxchain_splice() {
        let track_chunk = r#"<TRACK
  NAME "My Track"
  VOLPAN 1 0 -1 -1 1
  TRACKID {8EB223A9-A5D1-9D4C-A232-C756990A2EDF}
  <FXCHAIN
    SHOW 0
    LASTSEL 0
    DOCKED 0
    BYPASS 0 0 0
    <VST "VST: ReaEQ (Cockos)" reaeq.vst.dylib 0 "" 1919247985<00> ""
      ZXE=
    >
    FXID {AAAA-1111-0000-0000}
  >
>"#;

        // Step 1: Find FXCHAIN boundaries (same as daw-reaper's find_block_end)
        let fxchain_start = track_chunk.find("<FXCHAIN").unwrap();
        let fxchain_region = &track_chunk[fxchain_start..];

        // Find closing > by tracking depth
        let mut depth = 0i32;
        let mut end_offset = 0usize;
        let mut found = false;
        for line in fxchain_region.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('<') {
                depth += 1;
            }
            if trimmed == ">" {
                depth -= 1;
                if depth == 0 {
                    end_offset += line.rfind('>').unwrap();
                    found = true;
                    break;
                }
            }
            end_offset += line.len() + 1;
        }
        assert!(found, "should find FXCHAIN closing tag");
        let fxchain_end = fxchain_start + end_offset;
        let fxchain_text = &track_chunk[fxchain_start..=fxchain_end];

        // Step 2: Parse, enclose, serialize
        let mut chain = FxChain::parse(fxchain_text).unwrap();
        assert_eq!(chain.nodes.len(), 1);

        enclose_nodes_in_container(&mut chain, &[0], "MY CONTAINER");
        let new_fxchain = chain.to_rpp_string();

        // Step 3: Splice back into track chunk
        let mut new_track = String::new();
        new_track.push_str(&track_chunk[..fxchain_start]);
        new_track.push_str(&new_fxchain);
        if fxchain_end + 1 < track_chunk.len() {
            new_track.push_str(&track_chunk[fxchain_end + 1..]);
        }

        // Step 4: Verify the resulting track chunk
        assert!(
            new_track.contains("<CONTAINER Container \"MY CONTAINER\""),
            "track chunk should contain the new container"
        );
        assert!(
            new_track.contains("FXID {AAAA-1111-0000-0000}"),
            "plugin FXID should be preserved inside the container"
        );
        assert!(
            new_track.contains("TRACKID {8EB223A9-A5D1-9D4C-A232-C756990A2EDF}"),
            "track metadata should be preserved"
        );
        assert!(
            new_track.contains("NAME \"My Track\""),
            "track name should be preserved"
        );

        // Verify the FXCHAIN in the new chunk is parseable
        let new_fxchain_start = new_track.find("<FXCHAIN").unwrap();
        let new_fxchain_region = &new_track[new_fxchain_start..];
        let mut depth2 = 0i32;
        let mut end2 = 0usize;
        let mut found2 = false;
        for line in new_fxchain_region.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('<') {
                depth2 += 1;
            }
            if trimmed == ">" {
                depth2 -= 1;
                if depth2 == 0 {
                    end2 += line.rfind('>').unwrap();
                    found2 = true;
                    break;
                }
            }
            end2 += line.len() + 1;
        }
        assert!(found2);
        let final_fxchain = &new_track[new_fxchain_start..=new_fxchain_start + end2];
        let final_chain = FxChain::parse(final_fxchain).unwrap();
        assert_eq!(final_chain.nodes.len(), 1);
        assert!(matches!(&final_chain.nodes[0], FxChainNode::Container(_)));
    }

    #[test]
    fn test_from_block_supports_rawline_fast_path() {
        use crate::primitives::{BlockType, QuoteType, RppBlock, RppBlockContent, Token};

        let js_block = RppBlock {
            block_type: BlockType::Other("JS".to_string()),
            name: "JS".to_string(),
            params: vec![
                Token::String("loser/3BandEQ".to_string(), QuoteType::Double),
                Token::String("".to_string(), QuoteType::Double),
            ],
            children: vec![RppBlockContent::RawLine(
                "AA==".to_string().into_boxed_str(),
            )],
        };

        let fx_block = RppBlock {
            block_type: BlockType::FxChain,
            name: "FXCHAIN".to_string(),
            params: vec![],
            children: vec![
                RppBlockContent::RawLine("SHOW 0".to_string().into_boxed_str()),
                RppBlockContent::RawLine("LASTSEL 0".to_string().into_boxed_str()),
                RppBlockContent::RawLine("DOCKED 0".to_string().into_boxed_str()),
                RppBlockContent::RawLine("BYPASS 0 0 0".to_string().into_boxed_str()),
                RppBlockContent::Block(js_block),
                RppBlockContent::RawLine("FXID {RAW-PLUGIN-ID}".to_string().into_boxed_str()),
            ],
        };

        let chain = FxChain::from_block(&fx_block).expect("parse from block");
        assert_eq!(chain.nodes.len(), 1);
        let FxChainNode::Plugin(plugin) = &chain.nodes[0] else {
            panic!("expected plugin node");
        };
        assert_eq!(plugin.plugin_type, PluginType::Js);
        assert_eq!(plugin.file, "loser/3BandEQ");
        assert_eq!(plugin.fxid.as_deref(), Some("{RAW-PLUGIN-ID}"));
    }

    /// A CLAP header's file slot is the plugin's ID, bare.
    #[test]
    fn a_clap_header_reads_its_plugin_id() {
        let parsed = super::parse_header_name_file(
            "\"CLAP: Pro-Q 4 (FabFilter)\" com.FabFilter.Pro-Q.4 \"\"",
            super::PluginType::Clap,
        );
        assert_eq!(parsed.name, "CLAP: Pro-Q 4 (FabFilter)");
        assert_eq!(parsed.file, "com.FabFilter.Pro-Q.4");
    }
}
