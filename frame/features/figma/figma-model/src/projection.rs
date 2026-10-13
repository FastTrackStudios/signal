use crate::document::FrameDocument;
use crate::id::NodeId;
use crate::layout::{
    AutoLayout, AutoLayoutAlignSelf, AutoLayoutCounterAlign, AutoLayoutMode, AutoLayoutPositioning,
    AutoLayoutPrimaryAlign, AutoLayoutSizingMode, AutoLayoutWrap,
};
use crate::node::FrameNode;
use figma_api::models;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderNodeClass {
    Container,
    Text,
    Shape,
    Vector,
    Utility,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderNodeProjection {
    pub id: NodeId,
    pub figma_id: String,
    pub figma_type: String,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub class: RenderNodeClass,
    pub opacity: Option<f64>,
    pub blend_mode: Option<models::BlendMode>,
    pub size: Option<models::Vector>,
    pub relative_transform: Option<Vec<Vec<f64>>>,
    pub fills: Vec<models::Paint>,
    pub strokes: Vec<models::Paint>,
    pub effects: Vec<models::Effect>,
    pub fill_geometry: Vec<models::Path>,
    pub stroke_geometry: Vec<models::Path>,
    pub text: Option<RenderTextProjection>,
    pub stroke_style: Option<StrokeStyleProjection>,
    pub compositing: CompositingProjection,
    pub feature_flags: RenderFeatureFlags,
    pub component: Option<ComponentProjection>,
    pub asset_refs: Option<AssetRefsProjection>,
    pub image_data_url: Option<String>,
    pub layout_sizing_horizontal: Option<String>,
    pub layout_sizing_vertical: Option<String>,
    pub corner_smoothing: Option<f64>,
    pub explicit_variable_modes: Option<serde_json::Value>,
    pub bound_variables: Option<serde_json::Value>,
    pub plugin_data: Option<serde_json::Value>,
    pub shared_plugin_data: Option<serde_json::Value>,
    pub auto_layout: Option<AutoLayout>,
    pub children: Vec<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RenderFeatureFlags {
    pub has_mask: bool,
    pub has_alpha_mask: bool,
    pub has_luminance_mask: bool,
    pub has_clip: bool,
    pub has_non_normal_blend: bool,
    pub has_drop_shadow: bool,
    pub has_inner_shadow: bool,
    pub has_layer_blur: bool,
    pub has_background_blur: bool,
    pub has_image_fill: bool,
    pub has_gradient_fill: bool,
    pub has_svg_export: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderTextProjection {
    pub characters: String,
    pub style: Option<models::TypeStyle>,
    pub line_height_px: Option<f64>,
    pub letter_spacing: Option<f64>,
    pub text_case: Option<String>,
    pub text_decoration: Option<String>,
    pub text_auto_resize: Option<String>,
    pub text_align_horizontal: Option<String>,
    pub text_align_vertical: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrokeStyleProjection {
    pub weight: Option<f64>,
    pub align: Option<models::has_geometry_trait::StrokeAlign>,
    pub join: Option<models::has_geometry_trait::StrokeJoin>,
    pub cap: Option<models::has_geometry_trait::StrokeCap>,
    pub miter_limit: Option<f64>,
    pub dash_pattern: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CompositingProjection {
    pub opacity: Option<f64>,
    pub blend_mode: Option<models::BlendMode>,
    pub clips_content: Option<bool>,
    pub is_mask: Option<bool>,
    pub mask_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentProjection {
    pub component_id: Option<String>,
    pub component_set_id: Option<String>,
    pub variant_properties: std::collections::HashMap<String, String>,
    pub component_properties: Option<serde_json::Value>,
    pub component_property_references: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetRefsProjection {
    pub svg: Option<String>,
    pub png: Option<String>,
    pub image_fill_assets: std::collections::HashMap<String, String>,
}

impl FrameDocument {
    pub fn project_node(&self, node_id: NodeId) -> Option<RenderNodeProjection> {
        let node = self.get_node(node_id)?;
        Some(project_node(node))
    }

    pub fn project_subtree(&self, root: NodeId) -> Vec<RenderNodeProjection> {
        self.walk_subtree(root)
            .into_iter()
            .filter_map(|id| self.project_node(id))
            .collect()
    }

    pub fn project_pages(&self) -> Vec<RenderNodeProjection> {
        self.pages
            .iter()
            .copied()
            .filter_map(|id| self.project_node(id))
            .collect()
    }
}

pub fn project_node(node: &FrameNode) -> RenderNodeProjection {
    let raw = &node.raw;

    RenderNodeProjection {
        id: node.id,
        figma_id: node.figma_id.clone(),
        figma_type: node.figma_type.clone(),
        name: node.name.clone(),
        visible: node.visible,
        locked: node.locked,
        class: classify_node_type(&node.figma_type),
        opacity: raw.get("opacity").and_then(|v| v.as_f64()),
        blend_mode: get_typed(raw, "blendMode"),
        size: get_typed(raw, "size"),
        relative_transform: get_typed(raw, "relativeTransform"),
        fills: get_typed_array(raw, "fills"),
        strokes: get_typed_array(raw, "strokes"),
        effects: get_typed_array(raw, "effects"),
        fill_geometry: get_typed_array(raw, "fillGeometry"),
        stroke_geometry: get_typed_array(raw, "strokeGeometry"),
        text: get_text_projection(raw),
        stroke_style: get_stroke_style_projection(raw),
        compositing: get_compositing_projection(raw),
        feature_flags: get_render_feature_flags(raw),
        component: get_component_projection(raw),
        asset_refs: get_asset_refs_projection(raw),
        image_data_url: raw
            .get("imageDataUrl")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        layout_sizing_horizontal: get_string(raw, "layoutSizingHorizontal"),
        layout_sizing_vertical: get_string(raw, "layoutSizingVertical"),
        corner_smoothing: raw.get("cornerSmoothing").and_then(|v| v.as_f64()),
        explicit_variable_modes: raw.get("explicitVariableModes").cloned(),
        bound_variables: raw.get("boundVariables").cloned(),
        plugin_data: raw.get("pluginData").cloned(),
        shared_plugin_data: raw.get("sharedPluginData").cloned(),
        auto_layout: get_auto_layout_projection(raw),
        children: node.children.clone(),
    }
}

fn get_render_feature_flags(raw: &serde_json::Value) -> RenderFeatureFlags {
    let has_mask = raw.get("isMask").and_then(|v| v.as_bool()).unwrap_or(false);
    let mask_type = raw.get("maskType").and_then(|v| v.as_str()).unwrap_or("");
    let has_alpha_mask = has_mask && mask_type.eq_ignore_ascii_case("ALPHA");
    let has_luminance_mask = has_mask && mask_type.eq_ignore_ascii_case("LUMINANCE");
    let has_clip = raw
        .get("clipsContent")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let has_non_normal_blend = raw
        .get("blendMode")
        .and_then(|v| v.as_str())
        .map(is_non_normal_blend_mode)
        .unwrap_or(false);

    let mut has_drop_shadow = false;
    let mut has_inner_shadow = false;
    let mut has_layer_blur = false;
    let mut has_background_blur = false;
    if let Some(effects) = raw.get("effects").and_then(|v| v.as_array()) {
        for effect in effects {
            if !effect
                .get("visible")
                .and_then(|v| v.as_bool())
                .unwrap_or(true)
            {
                continue;
            }
            match effect.get("type").and_then(|v| v.as_str()) {
                Some("DROP_SHADOW") => has_drop_shadow = true,
                Some("INNER_SHADOW") => has_inner_shadow = true,
                Some("LAYER_BLUR") => has_layer_blur = true,
                Some("BACKGROUND_BLUR") => has_background_blur = true,
                _ => {}
            }
        }
    }

    let mut has_image_fill = false;
    let mut has_gradient_fill = false;
    if let Some(fills) = raw.get("fills").and_then(|v| v.as_array()) {
        for fill in fills {
            if !fill
                .get("visible")
                .and_then(|v| v.as_bool())
                .unwrap_or(true)
            {
                continue;
            }
            match fill.get("type").and_then(|v| v.as_str()) {
                Some("IMAGE") => has_image_fill = true,
                Some("GRADIENT_LINEAR")
                | Some("GRADIENT_RADIAL")
                | Some("GRADIENT_ANGULAR")
                | Some("GRADIENT_DIAMOND") => has_gradient_fill = true,
                _ => {}
            }
        }
    }

    let has_svg_export = raw
        .get("exports")
        .and_then(|v| v.get("svgBase64"))
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false);

    RenderFeatureFlags {
        has_mask,
        has_alpha_mask,
        has_luminance_mask,
        has_clip,
        has_non_normal_blend,
        has_drop_shadow,
        has_inner_shadow,
        has_layer_blur,
        has_background_blur,
        has_image_fill,
        has_gradient_fill,
        has_svg_export,
    }
}

fn is_non_normal_blend_mode(mode: &str) -> bool {
    !matches!(mode, "NORMAL" | "PASS_THROUGH")
}

pub fn classify_node_type(figma_type: &str) -> RenderNodeClass {
    match figma_type {
        "DOCUMENT" | "CANVAS" | "FRAME" | "GROUP" | "COMPONENT" | "COMPONENT_SET" | "INSTANCE"
        | "SECTION" | "TRANSFORM_GROUP" | "TABLE" | "TABLE_CELL" => RenderNodeClass::Container,
        "TEXT" | "TEXT_PATH" | "SHAPE_WITH_TEXT" | "STICKY" => RenderNodeClass::Text,
        "RECTANGLE" | "ELLIPSE" | "LINE" | "REGULAR_POLYGON" | "STAR" | "BOOLEAN_OPERATION"
        | "CONNECTOR" | "WASHI_TAPE" => RenderNodeClass::Shape,
        "VECTOR" => RenderNodeClass::Vector,
        "SLICE" | "EMBED" | "LINK_UNFURL" | "WIDGET" => RenderNodeClass::Utility,
        _ => RenderNodeClass::Unknown,
    }
}

fn get_text_projection(raw: &serde_json::Value) -> Option<RenderTextProjection> {
    let characters = raw.get("characters")?.as_str()?.to_string();
    let style = get_typed(raw, "style");
    let line_height_px = raw
        .get("style")
        .and_then(|v| v.get("lineHeightPx"))
        .and_then(|v| v.as_f64());
    let letter_spacing = raw
        .get("style")
        .and_then(|v| v.get("letterSpacing"))
        .and_then(|v| v.as_f64());
    Some(RenderTextProjection {
        characters,
        style,
        line_height_px,
        letter_spacing,
        text_case: get_string(raw, "textCase"),
        text_decoration: get_string(raw, "textDecoration"),
        text_auto_resize: get_string(raw, "textAutoResize"),
        text_align_horizontal: get_string(raw, "textAlignHorizontal"),
        text_align_vertical: get_string(raw, "textAlignVertical"),
    })
}

fn get_stroke_style_projection(raw: &serde_json::Value) -> Option<StrokeStyleProjection> {
    let dash_pattern = raw
        .get("dashPattern")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect::<Vec<_>>())
        .unwrap_or_default();
    let out = StrokeStyleProjection {
        weight: get_f64(raw, "strokeWeight"),
        align: get_typed(raw, "strokeAlign"),
        join: get_typed(raw, "strokeJoin"),
        cap: get_typed(raw, "strokeCap"),
        miter_limit: get_f64(raw, "strokeMiterLimit"),
        dash_pattern,
    };
    if out.weight.is_none()
        && out.align.is_none()
        && out.join.is_none()
        && out.cap.is_none()
        && out.miter_limit.is_none()
        && out.dash_pattern.is_empty()
    {
        None
    } else {
        Some(out)
    }
}

fn get_compositing_projection(raw: &serde_json::Value) -> CompositingProjection {
    CompositingProjection {
        opacity: get_f64(raw, "opacity"),
        blend_mode: get_typed(raw, "blendMode"),
        clips_content: raw.get("clipsContent").and_then(|v| v.as_bool()),
        is_mask: raw.get("isMask").and_then(|v| v.as_bool()),
        mask_type: get_string(raw, "maskType"),
    }
}

fn get_component_projection(raw: &serde_json::Value) -> Option<ComponentProjection> {
    let variant_properties = raw
        .get("variantProperties")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect::<std::collections::HashMap<_, _>>()
        })
        .unwrap_or_default();
    let out = ComponentProjection {
        component_id: get_string(raw, "componentId"),
        component_set_id: get_string(raw, "componentSetId"),
        variant_properties,
        component_properties: raw.get("componentProperties").cloned(),
        component_property_references: raw
            .get("componentPropertyReferences")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect::<std::collections::HashMap<_, _>>()
            })
            .unwrap_or_default(),
    };
    if out.component_id.is_none()
        && out.component_set_id.is_none()
        && out.variant_properties.is_empty()
        && out.component_properties.is_none()
        && out.component_property_references.is_empty()
    {
        None
    } else {
        Some(out)
    }
}

fn get_asset_refs_projection(raw: &serde_json::Value) -> Option<AssetRefsProjection> {
    let svg = raw
        .get("assetRefs")
        .and_then(|v| v.get("svg"))
        .and_then(|v| v.as_str())
        .map(ToString::to_string);
    let png = raw
        .get("assetRefs")
        .and_then(|v| v.get("png"))
        .and_then(|v| v.as_str())
        .map(ToString::to_string);
    let image_fill_assets = raw
        .get("imageFillAssets")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| {
                    // Support both v1 flat string and v2 nested object with "base64" field
                    let s = v
                        .as_str()
                        .or_else(|| v.get("base64").and_then(|b| b.as_str()))?;
                    Some((k.clone(), s.to_string()))
                })
                .collect::<std::collections::HashMap<_, _>>()
        })
        .unwrap_or_default();
    if svg.is_none() && png.is_none() && image_fill_assets.is_empty() {
        None
    } else {
        Some(AssetRefsProjection {
            svg,
            png,
            image_fill_assets,
        })
    }
}

pub fn get_auto_layout_projection(raw: &serde_json::Value) -> Option<AutoLayout> {
    let mode = get_string(raw, "layoutMode")
        .and_then(|v| parse_layout_mode(v.as_str()))
        .filter(|m| *m != AutoLayoutMode::None);
    let primary_axis_sizing_mode =
        get_string(raw, "primaryAxisSizingMode").and_then(|v| parse_sizing_mode(v.as_str()));
    let counter_axis_sizing_mode =
        get_string(raw, "counterAxisSizingMode").and_then(|v| parse_sizing_mode(v.as_str()));
    let primary_axis_align_items =
        get_string(raw, "primaryAxisAlignItems").and_then(|v| parse_primary_align(v.as_str()));
    let counter_axis_align_items =
        get_string(raw, "counterAxisAlignItems").and_then(|v| parse_counter_align(v.as_str()));
    let item_spacing = get_f64(raw, "itemSpacing");
    let counter_axis_spacing = get_f64(raw, "counterAxisSpacing");
    let padding_left = get_f64(raw, "paddingLeft");
    let padding_right = get_f64(raw, "paddingRight");
    let padding_top = get_f64(raw, "paddingTop");
    let padding_bottom = get_f64(raw, "paddingBottom");
    let wrap = get_string(raw, "layoutWrap").and_then(|v| parse_layout_wrap(v.as_str()));
    let positioning =
        get_string(raw, "layoutPositioning").and_then(|v| parse_layout_positioning(v.as_str()));
    let align_self = get_string(raw, "layoutAlign").and_then(|v| parse_align_self(v.as_str()));
    let grow = get_f64(raw, "layoutGrow");
    let min_width = get_f64(raw, "minWidth");
    let max_width = get_f64(raw, "maxWidth");
    let min_height = get_f64(raw, "minHeight");
    let max_height = get_f64(raw, "maxHeight");

    let out = AutoLayout {
        mode,
        primary_axis_sizing_mode,
        counter_axis_sizing_mode,
        primary_axis_align_items,
        counter_axis_align_items,
        item_spacing,
        counter_axis_spacing,
        padding_left,
        padding_right,
        padding_top,
        padding_bottom,
        wrap,
        positioning,
        align_self,
        grow,
        min_width,
        max_width,
        min_height,
        max_height,
    };

    if out == AutoLayout::default() {
        None
    } else {
        Some(out)
    }
}

fn get_string(raw: &serde_json::Value, key: &str) -> Option<String> {
    raw.get(key)?.as_str().map(ToString::to_string)
}

fn get_f64(raw: &serde_json::Value, key: &str) -> Option<f64> {
    raw.get(key).and_then(|v| v.as_f64())
}

fn parse_layout_mode(v: &str) -> Option<AutoLayoutMode> {
    match v {
        "NONE" => Some(AutoLayoutMode::None),
        "HORIZONTAL" => Some(AutoLayoutMode::Horizontal),
        "VERTICAL" => Some(AutoLayoutMode::Vertical),
        _ => None,
    }
}

fn parse_sizing_mode(v: &str) -> Option<AutoLayoutSizingMode> {
    match v {
        "FIXED" => Some(AutoLayoutSizingMode::Fixed),
        "AUTO" => Some(AutoLayoutSizingMode::Hug),
        _ => None,
    }
}

fn parse_primary_align(v: &str) -> Option<AutoLayoutPrimaryAlign> {
    match v {
        "MIN" => Some(AutoLayoutPrimaryAlign::Min),
        "CENTER" => Some(AutoLayoutPrimaryAlign::Center),
        "MAX" => Some(AutoLayoutPrimaryAlign::Max),
        "SPACE_BETWEEN" => Some(AutoLayoutPrimaryAlign::SpaceBetween),
        _ => None,
    }
}

fn parse_counter_align(v: &str) -> Option<AutoLayoutCounterAlign> {
    match v {
        "MIN" => Some(AutoLayoutCounterAlign::Min),
        "CENTER" => Some(AutoLayoutCounterAlign::Center),
        "MAX" => Some(AutoLayoutCounterAlign::Max),
        "BASELINE" => Some(AutoLayoutCounterAlign::Baseline),
        _ => None,
    }
}

fn parse_layout_positioning(v: &str) -> Option<AutoLayoutPositioning> {
    match v {
        "AUTO" => Some(AutoLayoutPositioning::Auto),
        "ABSOLUTE" => Some(AutoLayoutPositioning::Absolute),
        _ => None,
    }
}

fn parse_layout_wrap(v: &str) -> Option<AutoLayoutWrap> {
    match v {
        "NO_WRAP" => Some(AutoLayoutWrap::NoWrap),
        "WRAP" => Some(AutoLayoutWrap::Wrap),
        _ => None,
    }
}

fn parse_align_self(v: &str) -> Option<AutoLayoutAlignSelf> {
    match v {
        "INHERIT" => Some(AutoLayoutAlignSelf::Inherit),
        "STRETCH" => Some(AutoLayoutAlignSelf::Stretch),
        "MIN" => Some(AutoLayoutAlignSelf::Min),
        "CENTER" => Some(AutoLayoutAlignSelf::Center),
        "MAX" => Some(AutoLayoutAlignSelf::Max),
        _ => Some(AutoLayoutAlignSelf::Auto),
    }
}

fn get_typed<T>(raw: &serde_json::Value, key: &str) -> Option<T>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let value = raw.get(key)?;
    serde_json::from_value(value.clone()).ok()
}

fn get_typed_array<T>(raw: &serde_json::Value, key: &str) -> Vec<T>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let Some(value) = raw.get(key) else {
        return Vec::new();
    };
    let Some(array) = value.as_array() else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|entry| serde_json::from_value(entry.clone()).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::FrameNode;

    #[test]
    fn projects_typed_stroke_and_blend_fields() {
        let node = FrameNode::new_synthetic(serde_json::json!({
            "id": "10:10",
            "name": "Shape",
            "type": "RECTANGLE",
            "visible": true,
            "locked": false,
            "blendMode": "MULTIPLY",
            "strokeWeight": 2.5,
            "strokeAlign": "OUTSIDE",
            "strokeJoin": "ROUND",
            "strokeCap": "SQUARE",
            "strokeMiterLimit": 12.0,
            "dashPattern": [4, 2],
            "isMask": true,
            "maskType": "ALPHA",
            "clipsContent": true,
            "effects": [
                { "type": "DROP_SHADOW", "visible": true },
                { "type": "INNER_SHADOW", "visible": true },
                { "type": "LAYER_BLUR", "visible": true },
                { "type": "BACKGROUND_BLUR", "visible": true }
            ],
            "fills": [
                { "type": "IMAGE", "visible": true },
                { "type": "GRADIENT_LINEAR", "visible": true }
            ],
            "exports": { "svgBase64": "Zm9v" },
            "assetRefs": { "svg": "svg:abc" },
            "imageFillAssets": { "img1": "Zm9v" },
            "componentId": "comp:1",
            "componentSetId": "set:1",
            "variantProperties": { "Size": "Large" },
            "componentPropertyReferences": { "label": "text" },
            "layoutSizingHorizontal": "HUG",
            "layoutSizingVertical": "FILL",
            "cornerSmoothing": 0.42,
            "explicitVariableModes": { "col": "mode1" },
            "boundVariables": { "fills": [{ "id": "var1" }] },
            "pluginData": { "fts": "ok" },
            "sharedPluginData": { "org": { "k": "v" } }
        }));

        let p = project_node(&node);
        assert_eq!(p.blend_mode, Some(models::BlendMode::Multiply));
        assert!(p.feature_flags.has_non_normal_blend);
        assert!(p.feature_flags.has_image_fill);
        assert!(p.feature_flags.has_gradient_fill);
        assert!(p.feature_flags.has_drop_shadow);
        assert!(p.feature_flags.has_inner_shadow);
        assert!(p.feature_flags.has_layer_blur);
        assert!(p.feature_flags.has_background_blur);
        assert!(p.feature_flags.has_mask);
        assert!(p.feature_flags.has_alpha_mask);
        assert!(!p.feature_flags.has_luminance_mask);
        assert!(p.feature_flags.has_clip);
        assert!(p.feature_flags.has_svg_export);
        let stroke = p.stroke_style.expect("stroke style projected");
        assert_eq!(stroke.weight, Some(2.5));
        assert_eq!(
            stroke.align,
            Some(models::has_geometry_trait::StrokeAlign::Outside)
        );
        assert_eq!(
            stroke.join,
            Some(models::has_geometry_trait::StrokeJoin::Round)
        );
        assert_eq!(
            stroke.cap,
            Some(models::has_geometry_trait::StrokeCap::Square)
        );
        assert_eq!(stroke.miter_limit, Some(12.0));
        assert_eq!(stroke.dash_pattern, vec![4.0, 2.0]);

        let comp = p.component.expect("component projection");
        assert_eq!(comp.component_id.as_deref(), Some("comp:1"));
        assert_eq!(comp.component_set_id.as_deref(), Some("set:1"));
        assert_eq!(
            comp.variant_properties.get("Size").map(String::as_str),
            Some("Large")
        );
        assert_eq!(
            comp.component_property_references
                .get("label")
                .map(String::as_str),
            Some("text")
        );

        let assets = p.asset_refs.expect("asset refs projection");
        assert_eq!(assets.svg.as_deref(), Some("svg:abc"));
        assert_eq!(
            assets.image_fill_assets.get("img1").map(String::as_str),
            Some("Zm9v")
        );
        assert_eq!(p.corner_smoothing, Some(0.42));
        assert_eq!(p.layout_sizing_horizontal.as_deref(), Some("HUG"));
        assert_eq!(p.layout_sizing_vertical.as_deref(), Some("FILL"));
        assert!(p.explicit_variable_modes.is_some());
        assert!(p.bound_variables.is_some());
        assert!(p.plugin_data.is_some());
        assert!(p.shared_plugin_data.is_some());
    }

    #[test]
    fn pass_through_blend_is_not_treated_as_non_normal() {
        assert!(!is_non_normal_blend_mode("PASS_THROUGH"));
        assert!(!is_non_normal_blend_mode("NORMAL"));
        assert!(is_non_normal_blend_mode("MULTIPLY"));
    }
}
