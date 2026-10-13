//! Supply source chunks for newly built items and takes. Existing chunks stay intact.
use super::*;
use crate::document::SourceRef;

pub(super) fn write_missing_sources(
    root: &mut RChunk,
    document: &DawDocument,
    store: &ObjectStore,
    report: &mut ExportReport,
) -> DawResult<()> {
    for child in &mut root.children {
        let RNodeTree::Chunk(track) = child else {
            continue;
        };
        if track.name().as_deref() != Some("TRACK") {
            continue;
        }
        let Some(node) = track_guid(track).and_then(|guid| document.track_by_guid(&guid)) else {
            continue;
        };
        for child in &mut track.children {
            let RNodeTree::Chunk(item) = child else {
                continue;
            };
            if item.name().as_deref() != Some("ITEM") {
                continue;
            }
            let Some(node) = child_node(item, "IGUID")
                .and_then(|line| param(line, 1))
                .and_then(|guid| node.items.iter().find(|item| item.id.as_str() == guid))
            else {
                continue;
            };
            for (start, end, guid) in take_runs(item).into_iter().rev() {
                if item.children[start..end].iter().any(|child| matches!(child, RNodeTree::Chunk(chunk) if chunk.name().as_deref() == Some("SOURCE"))) { continue; }
                let Some(take) = guid
                    .as_ref()
                    .and_then(|guid| node.takes.iter().find(|take| take.id.as_str() == guid))
                else {
                    continue;
                };
                let source = match &take.source {
                    SourceRef::Empty => continue,
                    SourceRef::File { path, kind } => {
                        let mut chunk = RChunk::new(vec![RToken::new("SOURCE"), RToken::new(kind)]);
                        chunk.children.push(node_line(&["FILE", path]));
                        chunk
                    }
                    SourceRef::Object { object, .. } => {
                        let bytes = store.get(object)?;
                        let text = std::str::from_utf8(bytes)
                            .map_err(|error| DawError::Rpp(error.to_string()))?;
                        dawfile_reaper::rpp_tree::read_rpp_chunk(text)
                            .map_err(|error| DawError::Rpp(error.to_string()))?
                    }
                };
                item.children.insert(end, RNodeTree::Chunk(source));
                // Named by the ITEM as well as the take. A take id is
                // minted when the take is, so a take created by an
                // edit has an id that appears nowhere the caller has
                // ever seen — and a report line naming only that is
                // unattributable by construction. Anyone auditing the
                // churn of an edit knows which ITEMS they touched.
                report
                    .changes
                    .push(format!("item {}: take {}: source added", node.id, take.id));
            }
        }
    }
    Ok(())
}
