use std::collections::BTreeMap;

use dynamic_template::apply::chunk::RChunkTarget;
use dynamic_template::apply::{organize, UNSORTED_FOLDER};
use dynamic_template::{
    bus_nodes, default_config, golden_template, ItemMetadata, OrganizeIntoTracks,
};
use dynamic_template_proto::{NodeKind, TemplateNode};
use monarchy::Parser;
use std::env;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        print_usage();
        return;
    }

    // The golden template is derived, not typed out, so the only way to read
    // it is to print it. `-g` dumps the full schema with its bus routing.
    if args.iter().any(|a| a == "-g" || a == "--golden") {
        print_golden();
        return;
    }

    if let Some(pos) = args.iter().position(|a| a == "--inspect") {
        let Some(path) = pos.checked_add(1).and_then(|i| args.get(i)) else {
            eprintln!("usage: dynamic-template --inspect <project.rpp>");
            std::process::exit(2);
        };
        if let Err(err) = inspect_rpp(path) {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
        return;
    }

    if let Some(pos) = args.iter().position(|a| a == "--apply-buses") {
        let rest = pos
            .checked_add(1)
            .and_then(|i| args.get(i..))
            .unwrap_or_default();
        // An explicit destination is opt-in via -o. Without it the output name
        // is *derived* from the input, so the original can never be named as
        // the destination by a slip of the shell.
        let explicit = rest
            .iter()
            .position(|a| a == "-o")
            .and_then(|i| i.checked_add(1))
            .and_then(|i| rest.get(i))
            .map(String::as_str);
        let inputs: Vec<&str> = rest
            .iter()
            .take_while(|a| !a.starts_with('-'))
            .map(String::as_str)
            .collect();

        if inputs.is_empty() {
            eprintln!("usage: dynamic-template --apply-buses <in.rpp>... [-o <out.rpp>]");
            std::process::exit(2);
        }
        if explicit.is_some() && inputs.len() > 1 {
            eprintln!("error: -o takes a single input");
            std::process::exit(2);
        }

        let mut failed: u32 = 0;
        for (i, input) in inputs.iter().enumerate() {
            let derived;
            let output = match explicit {
                Some(o) => o,
                None => match organized_path(input) {
                    Ok(path) => {
                        derived = path;
                        &derived
                    }
                    Err(err) => {
                        eprintln!("error: {input}: {err}");
                        failed = failed.saturating_add(1);
                        continue;
                    }
                },
            };
            if i > 0 {
                println!();
            }
            if let Err(err) = apply_buses_to_rpp(input, output) {
                eprintln!("error: {input}: {err}");
                failed = failed.saturating_add(1);
            }
        }
        if failed > 0 {
            eprintln!("\n{failed} of {} projects failed", inputs.len());
            std::process::exit(1);
        }
        return;
    }

    // Check for flags
    let verbose = args.iter().any(|a| a == "-v" || a == "--verbose");
    let tree_only = args.iter().any(|a| a == "-t" || a == "--tree");
    let json_output = args.iter().any(|a| a == "-j" || a == "--json");

    // Filter out flags to get inputs
    let inputs: Vec<&str> = args
        .iter()
        .filter(|a| !a.starts_with('-'))
        .map(std::string::String::as_str)
        .collect();

    if inputs.is_empty() {
        print_usage();
        return;
    }

    let config = default_config();

    if let ([only], false) = (inputs.as_slice(), tree_only) {
        // Single input - show detailed parsing info
        analyze_single(only, &config, verbose, json_output);
    } else {
        // Multiple inputs - show tree structure
        analyze_multiple(&inputs, &config, verbose, json_output);
    }
}

fn print_usage() {
    eprintln!(
        r#"
Dynamic Template CLI - Test track sorting and parsing

USAGE:
    dynamic-template [FLAGS] <input>...

FLAGS:
    -v, --verbose    Show detailed parsing information
    -t, --tree       Force tree output even for single input
    -j, --json       Output results as JSON
    -g, --golden     Print the golden session template and its bus tree

SUBCOMMANDS:
    --apply-buses <in.rpp>... [-o <out.rpp>]
        Organize each project and write the result beside the original as
        <name>.organized.RPP. The input is never written to. Pass -o to choose
        a destination yourself (single input only).

    --inspect <project.rpp>
        Read-only. Report the project's bus tree and what feeds each bus,
        parsed from the file rather than pattern-matched.


EXAMPLES:
    # Single input - shows parsing details
    dynamic-template "Kick In"

    # Multiple inputs - shows tree structure
    dynamic-template "Kick In" "Kick Out" "Snare Top" "Snare Bot"

    # Verbose mode with tree
    dynamic-template -v "Acc Guitar" "Electric Guitar Clean"

    # JSON output
    dynamic-template -j "SNR VERB" "Hi Hat"
"#
    );
}

fn analyze_single(
    input: &str,
    config: &dynamic_template::DynamicTemplateConfig,
    verbose: bool,
    json: bool,
) {
    println!("═══════════════════════════════════════════════════════════════");
    println!("INPUT: \"{input}\"");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Parse the input to get metadata
    let parser = Parser::new(config);
    let item = match parser.parse(input.to_string()) {
        Ok(item) => item,
        Err(e) => {
            eprintln!("Parse error: {e}");
            return;
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&item.metadata).unwrap_or_default()
        );
        return;
    }

    // Show parsed metadata
    println!("PARSED METADATA:");
    println!("────────────────────────────────────────────────────────────────");
    print_metadata(&item.metadata, verbose);

    // Show matched groups
    println!("\nMATCHED GROUPS:");
    println!("────────────────────────────────────────────────────────────────");
    if item.matched_groups.is_empty() {
        println!("  (none - will go to Unsorted)");
    } else {
        for (i, mg) in item.matched_groups.iter().enumerate() {
            let tree_prefix = if i.checked_add(1) == Some(item.matched_groups.len()) {
                "└─"
            } else {
                "├─"
            };
            println!("  {} {} (prefix: {:?})", tree_prefix, mg.name, mg.prefix);
        }
    }

    // Show what display name would be generated
    let display_name = item.derive_display_name();
    println!("\nDISPLAY NAME:");
    println!("────────────────────────────────────────────────────────────────");
    println!("  \"{display_name}\"");

    // Show final tracks
    println!("\nTRACKS:");
    println!("────────────────────────────────────────────────────────────────");
    match vec![input.to_string()].organize_into_tracks(config, None) {
        Ok(tracks) => {
            print_tracks(&tracks);
        }
        Err(e) => {
            eprintln!("  Error: {e}");
        }
    }
}

fn analyze_multiple(
    inputs: &[&str],
    config: &dynamic_template::DynamicTemplateConfig,
    verbose: bool,
    json: bool,
) {
    let input_strings: Vec<String> = inputs
        .iter()
        .map(std::string::ToString::to_string)
        .collect();

    println!("═══════════════════════════════════════════════════════════════");
    println!("INPUTS ({} items):", inputs.len());
    println!("═══════════════════════════════════════════════════════════════");
    for input in inputs {
        println!("  • {input}");
    }
    println!();

    if verbose {
        // Show parsing details for each input
        let parser = Parser::new(config);
        println!("PARSING DETAILS:");
        println!("────────────────────────────────────────────────────────────────");
        for input in inputs {
            let item = match parser.parse(input.to_string()) {
                Ok(item) => item,
                Err(e) => {
                    println!("\n\"{input}\": Parse error: {e}");
                    continue;
                }
            };
            println!("\n\"{input}\":");
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&item.metadata).unwrap_or_default()
                );
            } else {
                print_metadata_compact(&item.metadata);
                let groups: Vec<&str> = item
                    .matched_groups
                    .iter()
                    .map(|mg| mg.name.as_str())
                    .collect();
                println!("    matched: {groups:?}");
                println!("    display: \"{}\"", item.derive_display_name());
            }
        }
        println!();
    }

    // Convert to tracks using organize_into_tracks (default options include expansion)
    println!("TRACKS:");
    println!("────────────────────────────────────────────────────────────────");
    match input_strings.organize_into_tracks(config, None) {
        Ok(tracks) => {
            print_tracks(&tracks);
        }
        Err(e) => {
            eprintln!("Error: {e}");
        }
    }
}

fn print_metadata(metadata: &ItemMetadata, verbose: bool) {
    // Always show these
    if let Some(ref group) = metadata.group {
        println!("  group:       {group:?}");
    }
    if let Some(ref multi_mic) = metadata.multi_mic {
        println!("  multi_mic:   {multi_mic:?}");
    }
    if let Some(ref effect) = metadata.effect {
        println!("  effect:      {effect:?}");
    }
    if let Some(ref increment) = metadata.increment {
        println!("  increment:   {increment:?}");
    }
    if let Some(ref track_type) = metadata.track_type {
        println!("  track_type:  {track_type:?}");
    }
    if let Some(ref variant) = metadata.variant {
        println!("  variant:     {variant:?}");
    }
    if let Some(ref layers) = metadata.layers {
        println!("  layers:      {layers:?}");
    }
    if let Some(ref channel) = metadata.channel {
        println!("  channel:     {channel:?}");
    }
    if let Some(ref performer) = metadata.performer {
        println!("  performer:   {performer:?}");
    }
    if let Some(ref arrangement) = metadata.arrangement {
        println!("  arrangement: {arrangement:?}");
    }
    if let Some(ref section) = metadata.section {
        println!("  section:     {section:?}");
    }
    if let Some(ref tagged_collection) = metadata.tagged_collection {
        println!("  tagged_coll: {tagged_collection:?}");
    }

    // Only show in verbose mode
    if verbose {
        if let Some(ref rec_tag) = metadata.rec_tag {
            println!("  rec_tag:     {rec_tag:?}");
        }
        if let Some(ref playlist) = metadata.playlist {
            println!("  playlist:    {playlist:?}");
        }
        if let Some(ref file_ext) = metadata.file_extension {
            println!("  file_ext:    {file_ext:?}");
        }
        if let Some(ref original) = metadata.original_name {
            println!("  original:    {original:?}");
        }
    }

    // Always show unparsed words if present
    if let Some(ref unparsed) = metadata.unparsed_words {
        if !unparsed.is_empty() {
            println!("  unparsed:    {unparsed:?}");
        }
    }

    // Show if nothing was parsed
    if metadata.group.is_none()
        && metadata.multi_mic.is_none()
        && metadata.effect.is_none()
        && metadata.increment.is_none()
        && metadata.track_type.is_none()
        && metadata.layers.is_none()
        && metadata.performer.is_none()
        && metadata.arrangement.is_none()
    {
        println!("  (no metadata parsed)");
    }
}

fn print_metadata_compact(metadata: &ItemMetadata) {
    let mut parts = Vec::new();

    if let Some(ref group) = metadata.group {
        parts.push(format!("group={group:?}"));
    }
    if let Some(ref multi_mic) = metadata.multi_mic {
        parts.push(format!("mic={multi_mic:?}"));
    }
    if let Some(ref effect) = metadata.effect {
        parts.push(format!("fx={effect:?}"));
    }
    if let Some(ref increment) = metadata.increment {
        parts.push(format!("inc={increment}"));
    }
    if let Some(ref track_type) = metadata.track_type {
        parts.push(format!("type={track_type}"));
    }
    if let Some(ref unparsed) = metadata.unparsed_words {
        if !unparsed.is_empty() {
            parts.push(format!("unparsed={unparsed:?}"));
        }
    }

    if parts.is_empty() {
        println!("    metadata: (none)");
    } else {
        println!("    metadata: {}", parts.join(", "));
    }
}

fn print_tracks(hierarchy: &daw_proto::TrackHierarchy) {
    use daw_proto::FolderDepthChange;

    let mut depth = 0;

    for track in &hierarchy.tracks {
        let indent_str = "  ".repeat(depth);
        let folder_marker = if track.is_folder { "📁 " } else { "   " };
        let items_info = if track.items.is_empty() {
            String::new()
        } else {
            format!(" [{} item(s)]", track.items.len())
        };

        println!(
            "{}{}{}{}",
            indent_str, folder_marker, track.name, items_info
        );

        // Show items if any
        if !track.items.is_empty() {
            for item in &track.items {
                println!("{indent_str}      └─ \"{item}\"");
            }
        }

        // Handle folder depth changes
        match track.folder_depth_change {
            FolderDepthChange::FolderStart => {
                depth = depth.saturating_add(1);
            }
            FolderDepthChange::ClosesLevels(n) => {
                // n is negative, so we add it (subtracting the absolute value)
                let signed = i32::try_from(depth)
                    .unwrap_or(i32::MAX)
                    .saturating_add(i32::from(n))
                    .max(0);
                depth = usize::try_from(signed).unwrap_or(0);
            }
            FolderDepthChange::Normal => {}
        }
    }
}

/// Print the derived golden session template: the folder schema with each
/// node's bus attachment, followed by the bus tree itself.
fn print_golden() {
    let template = golden_template();

    println!("═══════════════════════════════════════════════════════════════");
    println!("{} (v{})", template.name, template.version);
    println!("═══════════════════════════════════════════════════════════════\n");

    println!("STRUCTURE (→ marks a bus attachment point):");
    println!("────────────────────────────────────────────────────────────────");
    for node in &template.root {
        print_golden_node(node, 0);
    }

    println!("\nBUS TRACKS (nested folder tracks; a bus feeds its parent folder):");
    println!("────────────────────────────────────────────────────────────────");
    for node in bus_nodes(&template.buses) {
        print_golden_node(&node, 0);
    }

    println!("\nBUS SOURCES:");
    println!("────────────────────────────────────────────────────────────────");
    for b in &template.buses {
        if b.sources.is_empty() {
            continue;
        }
        let joined: Vec<String> = b.sources.iter().map(|s| s.join("/")).collect();
        println!("  {} ← {}", b.name, joined.join(", "));
    }
}

fn print_golden_node(node: &TemplateNode, depth: usize) {
    let indent = "  ".repeat(depth);
    let marker = match node.kind {
        NodeKind::Folder => "📁",
        NodeKind::Track => "  ",
        NodeKind::Dimension => "◆ ",
        NodeKind::Collection => "▣ ",
    };
    let routing = match (&node.routing.bus, node.routing.parent_send) {
        (Some(bus), _) => format!("  → {bus}"),
        (None, false) => "  → (no send)".to_string(),
        (None, true) => String::new(),
    };
    let vocab = if node.vocabulary.is_empty() {
        String::new()
    } else {
        format!("  [{}]", node.vocabulary.join(", "))
    };
    println!("{indent}{marker} {}{vocab}{routing}", node.name);
    for child in &node.children {
        print_golden_node(child, depth.saturating_add(1));
    }
}

/// Build the bus tree a real `.RPP` needs and write the result out.
///
/// The buses are derived from the project's own track names: each is
/// classified, its canonical group path resolved to a bus, and only the buses
/// those paths reach are created. Re-running over an already-organized project
/// is a no-op.
fn apply_buses_to_rpp(input: &str, output: &str) -> Result<(), Box<dyn std::error::Error>> {
    if input == output {
        return Err("refusing to write over the input project".into());
    }

    // The raw chunk tree, not `read_project`'s typed model. A typed model can
    // only write back the fields it knows, so organizing through one silently
    // dropped the master track, every RENDER_*/RECORD_CFG setting, per-item
    // CHANMODE/YPOS and all the <EXT> blocks — REAPER opened the result with
    // "11160 elements in the project were not understood". Editing the tree
    // leaves every untouched line byte-identical. See `apply::chunk`.
    let source = std::fs::read_to_string(input)?;
    let mut project = dawfile_reaper::read_rpp_chunk(&source)?;
    // The shared pipeline (`dynamic_template::apply::organize`) — the same
    // pass the Session window and the REAPER action run live.
    let organized = {
        let mut target = RChunkTarget::new(&mut project);
        match organize(&mut target) {
            Ok(organized) => organized,
            Err(never) => match never {},
        }
    };

    print_bus_summary(
        input,
        organized.existing,
        organized.unclassified.len(),
        &organized.buses,
        &organized.justified,
    );
    print_folder_repair(organized.broken_depths, &organized.folder_fixes);
    println!("  {} tracks coloured by classification", organized.painted);
    println!();
    print_bus_application(&organized.applied);
    print_routing_report(&organized.routing);

    if !organized.unsorted.is_empty() {
        match &organized.gathered {
            Some(g) => {
                println!("  {} moved into {UNSORTED_FOLDER}", g.moved.len());
                if !g.skipped.is_empty() {
                    // Say which of the two reasons applied, so "left in place"
                    // is a fact about the project rather than a shrug.
                    let report = RChunkTarget::new(&mut project);
                    let carries = g
                        .skipped
                        .iter()
                        .filter(|i| report.carries_folder_structure(**i))
                        .count();
                    println!(
                        "  {} left in place: {} nested inside a folder, {carries} carry one",
                        g.skipped.len(),
                        g.skipped.len().saturating_sub(carries)
                    );
                    println!("    nested, would need moving out of their folder:");
                    for i in g
                        .skipped
                        .iter()
                        .filter(|i| !report.carries_folder_structure(**i))
                    {
                        if let Some(name) = report.name_of(*i) {
                            println!("      {name}");
                        }
                    }
                }
            }
            None => println!(
                "  {} unsorted, none movable — all are inside a folder or carry one",
                organized.unsorted.len()
            ),
        }
    }

    // Write back with the line endings the input used. REAPER reads either,
    // but a session saved on macOS is CRLF throughout, and silently rewriting
    // all 21k lines to LF makes the organize pass impossible to review as a
    // diff — the one thing the chunk-tree backend exists to preserve.
    let text = dawfile_reaper::stringify_rpp_node(&dawfile_reaper::RNodeTree::Chunk(project));
    let mut text = if text.ends_with('\n') {
        text
    } else {
        text + "\n"
    };
    if source.contains("\r\n") {
        text = text.replace('\n', "\r\n");
    }
    std::fs::write(output, text)?;
    println!("  → {output}");
    Ok(())
}

fn print_bus_summary(
    input: &str,
    existing: usize,
    unclassified: usize,
    buses: &[dynamic_template_proto::TemplateBus],
    justified: &BTreeMap<&'static str, Vec<String>>,
) {
    println!("{input}");
    println!("  {existing} tracks; {unclassified} classified to no bus");
    println!();
    println!("  BUS               TRACKS  BECAUSE");
    for bus in buses {
        let tracks = justified.get(bus.name.as_str());
        let count = tracks.map_or(0, Vec::len);
        // A bus with no tracks of its own is a parent pulled in to carry its
        // children — MIX BUS, INST BUS, GUITAR BUS.
        let because = match tracks {
            Some(t) if !t.is_empty() => {
                let shown = t.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
                if count > 3 {
                    format!("{shown}, +{} more", count.saturating_sub(3))
                } else {
                    shown
                }
            }
            _ => "(sums the buses beneath it)".to_string(),
        };
        println!("  {:<18} {count:>5}  {because}", bus.name);
    }
    println!();
}

fn print_folder_repair(broken: usize, fixes: &[dynamic_template::apply::FolderFix<usize>]) {
    if fixes.is_empty() {
        return;
    }
    println!(
        "  ISBUS repair: {} tracks rewritten ({broken} had sat at a negative depth)",
        fixes.len()
    );
    for fix in fixes.iter().take(5) {
        println!("    {:<28} {:>3} → {:>3}", fix.name, fix.from, fix.to);
    }
    if fixes.len() > 5 {
        println!("    … and {} more", fixes.len().saturating_sub(5));
    }
    println!();
}

fn print_bus_application(applied: &dynamic_template::apply::AppliedBuses<usize>) {
    println!(
        "  {} created: {}",
        applied.created.len(),
        if applied.created.is_empty() {
            "—".to_string()
        } else {
            applied.created.join(", ")
        }
    );
    println!(
        "  {} already present: {}",
        applied.reused.len(),
        if applied.reused.is_empty() {
            "—".to_string()
        } else {
            applied.reused.join(", ")
        }
    );
    if !applied.nested {
        println!("  (appended flat and wired by send — the project already had buses)");
    }
}

fn print_routing_report(routing: &dynamic_template::apply::RoutingReport) {
    println!();
    println!(
        "  {} tracks routed to their bus; {} reach it through a parent folder",
        routing.routed.len(),
        routing.covered.len()
    );
    if !routing.already_routed.is_empty() {
        println!(
            "  {} already fed a bus and were left as routed",
            routing.already_routed.len()
        );
    }
    if !routing.control_only.is_empty() {
        println!(
            "  {} control-only, deliberately unrouted: {}",
            routing.control_only.len(),
            routing.control_only.join(", ")
        );
    }
    if !routing.unrouted.is_empty() {
        println!(
            "  {} route nowhere: {}",
            routing.unrouted.len(),
            routing.unrouted.join(", ")
        );
    }
}

/// Read-only report of what a project's routing actually looks like.
///
/// Parsed with the real RPP parser, never by scanning the text: a track's
/// `NAME` and the `NAME` of an item inside it are the same token, so any
/// regex that pairs names with track records drifts and silently mislabels
/// every track after the first item.
fn inspect_rpp(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let project = dawfile_reaper::io::read_project(path)?;
    let names: Vec<&str> = project.tracks.iter().map(|t| t.name.as_str()).collect();

    println!("{path}");
    println!("  {} tracks", names.len());

    let mut depth = 0i32;
    let mut negative: u32 = 0;
    for t in &project.tracks {
        depth = depth.saturating_add(t.folder.as_ref().map_or(0, |f| f.indentation));
        if depth < 0 {
            negative = negative.saturating_add(1);
        }
    }
    println!("  folder depth: sum {depth}, {negative} tracks below zero");
    println!();

    // Who feeds whom. REAPER records a send on the destination, so a track's
    // receives name its sources.
    println!("  DESTINATION          FED BY");
    for (i, track) in project.tracks.iter().enumerate() {
        if track.receives.is_empty() {
            continue;
        }
        let sources: Vec<&str> = track
            .receives
            .iter()
            .filter_map(|r| {
                usize::try_from(r.source_track_index)
                    .ok()
                    .and_then(|i| names.get(i))
                    .copied()
            })
            .collect();
        let shown = sources
            .iter()
            .take(4)
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
        let extra = if sources.len() > 4 {
            format!(", +{} more", sources.len().saturating_sub(4))
        } else {
            String::new()
        };
        let name = names.get(i).copied().unwrap_or_default();
        println!("  {name:<20} [{}] {shown}{extra}", sources.len());
    }

    // A track feeding two destinations arrives twice wherever they converge.
    let mut fan_out: std::collections::HashMap<usize, Vec<&str>> =
        std::collections::HashMap::default();
    for (i, track) in project.tracks.iter().enumerate() {
        for r in &track.receives {
            fan_out
                .entry(usize::try_from(r.source_track_index).unwrap_or(usize::MAX))
                .or_default()
                .push(names.get(i).copied().unwrap_or_default());
        }
    }
    // Two sends only *double* a track if both destinations converge. Two paths
    // into the mix subtree do; a mix send alongside a headphone cue or a
    // parallel verb does not — and flagging those buries the real faults under
    // routing the engineer put there on purpose.
    let reaches_mix = |name: &str| -> bool {
        let mut cursor = dynamic_template::buses::spec(name).map(|s| s.name);
        while let Some(n) = cursor {
            if n == dynamic_template::buses::names::MIX {
                return true;
            }
            cursor = dynamic_template::buses::spec(n).and_then(|s| s.parent);
        }
        false
    };

    let mut doubled = Vec::new();
    let mut parallel = Vec::new();
    for (src, dests) in &fan_out {
        if dests.len() < 2 {
            continue;
        }
        if dests.iter().filter(|d| reaches_mix(d)).count() > 1 {
            doubled.push((*src, dests));
        } else {
            parallel.push((*src, dests));
        }
    }

    println!();
    if doubled.is_empty() {
        println!("  no track reaches the mix by more than one path");
    } else {
        println!(
            "  WARNING: {} tracks reach the mix by more than one path (doubled):",
            doubled.len()
        );
        for (src, dests) in doubled.iter().take(10) {
            println!("    {} -> {:?}", names.get(*src).unwrap_or(&"?"), dests);
        }
    }
    if !parallel.is_empty() {
        println!(
            "  {} tracks have a parallel send beside their mix path (cue, verb - fine)",
            parallel.len()
        );
    }
    Ok(())
}

/// Where an organized copy of `input` belongs: beside it, as
/// `<name>.organized.RPP`.
///
/// Deriving the name rather than taking one means the original can never be
/// named as the destination — the failure that costs someone a session.
///
/// Re-running on an already-organized file is refused rather than producing
/// `x.organized.organized.RPP` or overwriting it in place. The pipeline
/// converges from the original anyway, so the original is always the right
/// input.
fn organized_path(input: &str) -> Result<String, Box<dyn std::error::Error>> {
    let path = std::path::Path::new(input);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("no file name")?;

    if stem.ends_with(".organized") {
        return Err("already an organized copy — run this on the original project".into());
    }

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("RPP");
    let name = format!("{stem}.organized.{ext}");
    Ok(path.with_file_name(name).to_string_lossy().into_owned())
}
