#[cfg(feature = "svg_export")]
#[derive(Default)]
pub(super) struct RenderCtx {
    pub group_stack: Vec<svg::node::element::Group>,
    pub clip_count: u64,
    pub gradient_count: u64,
    pub pattern_count: u64,
    pub effect_count: u64,
    pub bg_blur_count: u64,
    pub mask_count: u64,
    pub image_count: u64,
    pub has_defs: bool,
    pub has_xlink_namespace: bool,
    pub defs: svg::node::element::Definitions,
    pub deferred_gradients: Vec<Box<dyn svg::Node>>,
    pub deferred_images: Vec<svg::node::element::Image>,
    pub deferred_clip_paths: Vec<svg::node::element::ClipPath>,
    pub image_defs: std::collections::HashMap<String, String>,
}
