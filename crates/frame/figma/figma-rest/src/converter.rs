use crate::error::ImportError;
use figma_api::models;
use figma_model::FrameDocument;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportDiagnostics {
    pub source_schema: Option<String>,
    pub normalized_aliases: usize,
    pub unsupported_node_keys: BTreeMap<String, usize>,
}

impl ImportDiagnostics {
    pub fn top_unsupported_keys(&self, limit: usize) -> Vec<(String, usize)> {
        let mut entries: Vec<(String, usize)> = self
            .unsupported_node_keys
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        entries.into_iter().take(limit).collect()
    }
}

/// Parse Figma REST API JSON and build a flattened FrameDocument.
pub fn import_figma_file(json: &str) -> Result<FrameDocument, ImportError> {
    let mut value: serde_json::Value = serde_json::from_str(json)?;
    let mut _rewrites = 0;
    normalize_layout_compat_value(&mut value, &mut _rewrites);
    Ok(FrameDocument::from_source_value(value)?)
}

/// Convert a pre-deserialized Figma API `GetFile` into FrameDocument.
pub fn import_get_file(file: &models::GetFile) -> Result<FrameDocument, ImportError> {
    Ok(FrameDocument::from_figma_api(file.clone())?)
}

/// Parse source bytes into a FrameDocument.
///
/// JSON Figma API responses are supported. Binary `.fig` payloads currently
/// return `UnsupportedBinaryFig` from frame-proto.
pub fn import_figma_bytes(bytes: &[u8]) -> Result<FrameDocument, ImportError> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(text) {
            let mut _rewrites = 0;
            normalize_layout_compat_value(&mut value, &mut _rewrites);
            return Ok(FrameDocument::from_source_value(value)?);
        }
    }
    Ok(FrameDocument::from_source_bytes(bytes)?)
}

/// Parse source bytes and return both imported document and diagnostics
/// for normalization/dropped-field tracking.
pub fn import_figma_bytes_with_diagnostics(
    bytes: &[u8],
) -> Result<(FrameDocument, ImportDiagnostics), ImportError> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(text) {
            let mut normalized_aliases = 0;
            normalize_layout_compat_value(&mut value, &mut normalized_aliases);
            let diagnostics = ImportDiagnostics {
                source_schema: value
                    .get("schema")
                    .and_then(|s| s.as_str())
                    .map(ToString::to_string),
                normalized_aliases,
                unsupported_node_keys: collect_unsupported_node_keys(&value),
            };
            let doc = FrameDocument::from_source_value(value)?;
            return Ok((doc, diagnostics));
        }
    }
    let doc = FrameDocument::from_source_bytes(bytes)?;
    Ok((doc, ImportDiagnostics::default()))
}

fn normalize_layout_compat_value(value: &mut serde_json::Value, rewrites: &mut usize) {
    let Some(obj) = value.as_object_mut() else {
        if let Some(arr) = value.as_array_mut() {
            for entry in arr {
                normalize_layout_compat_value(entry, rewrites);
            }
        }
        return;
    };

    // Normalize alternate exporter keys into Figma REST-style keys.
    copy_key(obj, "layout_mode", "layoutMode", rewrites);
    copy_key(
        obj,
        "primary_axis_sizing_mode",
        "primaryAxisSizingMode",
        rewrites,
    );
    copy_key(
        obj,
        "counter_axis_sizing_mode",
        "counterAxisSizingMode",
        rewrites,
    );
    copy_key(
        obj,
        "primary_axis_align_items",
        "primaryAxisAlignItems",
        rewrites,
    );
    copy_key(
        obj,
        "counter_axis_align_items",
        "counterAxisAlignItems",
        rewrites,
    );
    copy_key(obj, "item_spacing", "itemSpacing", rewrites);
    copy_key(obj, "counter_axis_spacing", "counterAxisSpacing", rewrites);
    copy_key(obj, "padding_left", "paddingLeft", rewrites);
    copy_key(obj, "padding_right", "paddingRight", rewrites);
    copy_key(obj, "padding_top", "paddingTop", rewrites);
    copy_key(obj, "padding_bottom", "paddingBottom", rewrites);
    copy_key(obj, "layout_wrap", "layoutWrap", rewrites);
    copy_key(obj, "layout_positioning", "layoutPositioning", rewrites);
    copy_key(obj, "layout_align", "layoutAlign", rewrites);
    copy_key(obj, "layout_grow", "layoutGrow", rewrites);
    copy_key(obj, "min_width", "minWidth", rewrites);
    copy_key(obj, "max_width", "maxWidth", rewrites);
    copy_key(obj, "min_height", "minHeight", rewrites);
    copy_key(obj, "max_height", "maxHeight", rewrites);

    // Common geometry aliases from non-REST exporters.
    copy_key(obj, "relative_transform", "relativeTransform", rewrites);
    copy_key(
        obj,
        "absolute_bounding_box",
        "absoluteBoundingBox",
        rewrites,
    );
    copy_key(obj, "fill_geometry", "fillGeometry", rewrites);
    copy_key(obj, "stroke_geometry", "strokeGeometry", rewrites);
    copy_key(obj, "image_data_url", "imageDataUrl", rewrites);

    for (_, child) in obj.iter_mut() {
        normalize_layout_compat_value(child, rewrites);
    }
}

fn copy_key(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    from: &str,
    to: &str,
    rewrites: &mut usize,
) {
    if obj.contains_key(to) {
        return;
    }
    if let Some(value) = obj.get(from).cloned() {
        obj.insert(to.to_string(), value);
        *rewrites += 1;
    }
}

fn collect_unsupported_node_keys(value: &serde_json::Value) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    collect_unsupported_node_keys_recursive(value, &mut out);
    out
}

fn collect_unsupported_node_keys_recursive(
    value: &serde_json::Value,
    out: &mut BTreeMap<String, usize>,
) {
    if let Some(arr) = value.as_array() {
        for entry in arr {
            collect_unsupported_node_keys_recursive(entry, out);
        }
        return;
    }
    let Some(obj) = value.as_object() else {
        return;
    };

    let is_node = obj.contains_key("id")
        && obj.contains_key("type")
        && (obj.contains_key("children")
            || obj.get("type").and_then(|v| v.as_str()) == Some("DOCUMENT")
            || obj.get("type").and_then(|v| v.as_str()) == Some("CANVAS"));
    if is_node {
        for key in obj.keys() {
            if !is_supported_projection_key(key) {
                *out.entry(key.clone()).or_insert(0) += 1;
            }
        }
    }

    for (_, child) in obj {
        collect_unsupported_node_keys_recursive(child, out);
    }
}

fn is_supported_projection_key(key: &str) -> bool {
    matches!(
        key,
        "id" | "name"
            | "type"
            | "visible"
            | "locked"
            | "opacity"
            | "blendMode"
            | "size"
            | "relativeTransform"
            | "fills"
            | "strokes"
            | "effects"
            | "fillGeometry"
            | "strokeGeometry"
            | "characters"
            | "style"
            | "textCase"
            | "textDecoration"
            | "textAutoResize"
            | "textAlignHorizontal"
            | "textAlignVertical"
            | "imageDataUrl"
            | "layoutMode"
            | "primaryAxisSizingMode"
            | "counterAxisSizingMode"
            | "primaryAxisAlignItems"
            | "counterAxisAlignItems"
            | "itemSpacing"
            | "counterAxisSpacing"
            | "paddingLeft"
            | "paddingRight"
            | "paddingTop"
            | "paddingBottom"
            | "layoutWrap"
            | "layoutPositioning"
            | "layoutAlign"
            | "layoutGrow"
            | "layoutSizingHorizontal"
            | "layoutSizingVertical"
            | "minWidth"
            | "maxWidth"
            | "minHeight"
            | "maxHeight"
            | "strokeWeight"
            | "strokeAlign"
            | "strokeJoin"
            | "strokeCap"
            | "strokeMiterLimit"
            | "dashPattern"
            | "clipsContent"
            | "isMask"
            | "maskType"
            | "componentId"
            | "componentSetId"
            | "variantProperties"
            | "componentProperties"
            | "componentPropertyReferences"
            | "assetRefs"
            | "imageFillAssetRefs"
            | "imageFillAssets"
            | "explicitVariableModes"
            | "boundVariables"
            | "pluginData"
            | "sharedPluginData"
            | "children"
            | "exports"
            | "absoluteBoundingBox"
            | "rotation"
            | "constraints"
            | "cornerRadius"
            | "cornerSmoothing"
            | "topLeftRadius"
            | "topRightRadius"
            | "bottomLeftRadius"
            | "bottomRightRadius"
            | "rectangleCornerRadii"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_minimal_get_file_json() {
        let json = r#"{
            "name": "Test File",
            "role": "viewer",
            "lastModified": "2024-01-01T00:00:00Z",
            "editorType": "figma",
            "version": "1",
            "document": {
                "id": "0:0",
                "name": "Document",
                "type": "DOCUMENT",
                "scrollBehavior": "SCROLLS",
                "children": [
                    {
                        "id": "0:1",
                        "name": "Page 1",
                        "type": "CANVAS",
                        "scrollBehavior": "SCROLLS",
                        "children": [],
                        "backgroundColor": { "r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0 },
                        "prototypeStartNodeID": null,
                        "flowStartingPoints": [],
                        "prototypeDevice": { "type": "NONE", "rotation": "NONE" }
                    }
                ]
            },
            "components": {},
            "componentSets": {},
            "schemaVersion": 0,
            "styles": {}
        }"#;

        let doc = import_figma_file(json).expect("import should succeed");
        assert_eq!(doc.name, "Test File");
        assert_eq!(doc.pages.len(), 1);
        assert!(doc.node_count() >= 2);
    }

    #[test]
    fn imports_from_json_bytes() {
        let json = r#"{
            "name": "Bytes Test",
            "role": "viewer",
            "lastModified": "2024-01-01T00:00:00Z",
            "editorType": "figma",
            "version": "1",
            "document": {
                "id": "0:0",
                "name": "Document",
                "type": "DOCUMENT",
                "scrollBehavior": "SCROLLS",
                "children": [
                    {
                        "id": "0:1",
                        "name": "Page 1",
                        "type": "CANVAS",
                        "scrollBehavior": "SCROLLS",
                        "children": [],
                        "backgroundColor": { "r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0 },
                        "prototypeStartNodeID": null,
                        "flowStartingPoints": [],
                        "prototypeDevice": { "type": "NONE", "rotation": "NONE" }
                    }
                ]
            },
            "components": {},
            "componentSets": {},
            "schemaVersion": 0,
            "styles": {}
        }"#;

        let doc = import_figma_bytes(json.as_bytes()).expect("import from bytes should succeed");
        assert_eq!(doc.name, "Bytes Test");
        assert_eq!(doc.pages.len(), 1);
    }

    #[test]
    fn rejects_non_json_binary_fig_bytes_for_now() {
        let err = import_figma_bytes(&[0x89, b'F', b'I', b'G']).expect_err("should fail");
        let message = err.to_string();
        assert!(message.contains("not a supported .fig ZIP container"));
    }

    #[test]
    fn imports_fts_export_v2_with_asset_refs() {
        let json = r#"{
            "schema": "fts.figma.export/v2",
            "generatedAt": "2026-02-15T00:00:00.000Z",
            "source": {
                "editorType": "figma",
                "fileKey": "abc",
                "page": { "id": "0:1", "name": "Canvas" }
            },
            "assets": [
                {
                    "id": "svg:deadbeef",
                    "kind": "svg",
                    "mime": "image/svg+xml",
                    "encoding": "base64",
                    "data": "PHN2Zy8+",
                    "byteLength": 6
                }
            ],
            "nodes": [
                {
                    "id": "10:1",
                    "name": "Main",
                    "type": "FRAME",
                    "visible": true,
                    "assetRefs": { "svg": "svg:deadbeef" },
                    "children": []
                }
            ]
        }"#;

        let doc = import_figma_bytes(json.as_bytes()).expect("import from v2 should succeed");
        let main = doc
            .get_by_figma_id("10:1")
            .expect("main frame should exist in figma index");
        let svg_base64 = main
            .raw
            .get("exports")
            .and_then(|v| v.get("svgBase64"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        assert_eq!(svg_base64, "PHN2Zy8+");
    }

    #[test]
    fn diagnostics_reports_alias_rewrites_and_unknown_keys() {
        let json = r#"{
            "schema": "fts.figma.export/v2",
            "nodes": [{
                "id": "1:1",
                "name": "Node",
                "type": "FRAME",
                "layout_mode": "HORIZONTAL",
                "custom_we_dont_support_yet": true,
                "children": []
            }],
            "source": { "editorType": "figma", "page": { "id": "0:1", "name": "Canvas" } },
            "selection": { "ids": [], "names": [], "totalRoots": 0, "totalNodes": 0 },
            "options": { "includeSvg": true, "includePng": false, "maxDepth": 4 },
            "generatedAt": "2026-02-15T00:00:00.000Z"
        }"#;

        let (_doc, diagnostics) =
            import_figma_bytes_with_diagnostics(json.as_bytes()).expect("import should succeed");
        assert_eq!(
            diagnostics.source_schema.as_deref(),
            Some("fts.figma.export/v2")
        );
        assert!(diagnostics.normalized_aliases >= 1);
        assert_eq!(
            diagnostics
                .unsupported_node_keys
                .get("custom_we_dont_support_yet")
                .copied(),
            Some(1)
        );
        assert_eq!(
            diagnostics.top_unsupported_keys(1),
            vec![("custom_we_dont_support_yet".to_string(), 1)]
        );
    }
}
