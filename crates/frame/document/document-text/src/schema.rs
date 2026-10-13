//! Element and attribute vocabulary, plus "did you mean" suggestions.

use document_proto::NodeKind;

/// Element tags, in `NodeKind` order.
/// Not a node: a document's design tokens, the first child of
/// `<Document>` (see [`document_proto::Variables`]).
pub(crate) const VARIABLES_TAG: &str = "Variables";

pub(crate) const TAGS: &[&str] = &[
    "Document",
    "Page",
    "Frame",
    "Group",
    "Component",
    "ComponentSet",
    "Instance",
    "Rectangle",
    "Ellipse",
    "Line",
    "Polygon",
    "Star",
    "Vector",
    "BooleanOperation",
    "Text",
    "Utility",
    "Other",
];

pub(crate) fn tag_of(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Document => "Document",
        NodeKind::Page => "Page",
        NodeKind::Frame => "Frame",
        NodeKind::Group => "Group",
        NodeKind::Component => "Component",
        NodeKind::ComponentSet => "ComponentSet",
        NodeKind::Instance { .. } => "Instance",
        NodeKind::Rectangle => "Rectangle",
        NodeKind::Ellipse => "Ellipse",
        NodeKind::Line => "Line",
        NodeKind::Polygon => "Polygon",
        NodeKind::Star => "Star",
        NodeKind::Vector => "Vector",
        NodeKind::BooleanOperation { .. } => "BooleanOperation",
        NodeKind::Text(_) => "Text",
        NodeKind::Utility => "Utility",
        NodeKind::Other { .. } => "Other",
    }
}

/// The name a node gets when `name=` is omitted.
pub(crate) fn default_name(kind: &NodeKind) -> &str {
    match kind {
        NodeKind::Other { tag } => tag,
        k => tag_of(k),
    }
}

/// Attributes valid on every element, in print order.
pub(crate) const COMMON_ATTRS: &[&str] = &[
    "name",
    "id",
    "visible",
    "locked",
    "x",
    "y",
    "rotation",
    "transform",
    "w",
    "h",
    "opacity",
    "blend",
    "radius",
    "clip",
    "mask",
    "fill",
    "stroke",
    "stroke-weight",
    "stroke-align",
    "stroke-cap",
    "stroke-join",
    "stroke-miter",
    "stroke-dash",
    "stroke-dash-offset",
    "effects",
    "layout",
    "gap",
    "padding",
    "wrap",
    "hug",
    "justify",
    "align",
    "grow",
    "resize-x",
    "resize-y",
    "align-self",
    "absolute",
    "min-w",
    "min-h",
    "max-w",
    "max-h",
    "geometry",
    "stroke-geometry",
    "svg-snapshot",
    "control",
    "emission",
    "shader",
    "motion",
    "source",
];

/// Bound properties, as attributes (`bind:fill="$strip.ground"`): the
/// property's own attribute name behind `bind:`. Valid on every element,
/// printed last, in this order.
pub(crate) const BIND_ATTRS: &[&str] = &[
    "bind:visible",
    "bind:x",
    "bind:y",
    "bind:w",
    "bind:h",
    "bind:rotation",
    "bind:scale",
    "bind:scale-x",
    "bind:scale-y",
    "bind:opacity",
    "bind:radius",
    "bind:radius-tl",
    "bind:radius-tr",
    "bind:radius-br",
    "bind:radius-bl",
    "bind:fill",
    "bind:stroke",
    "bind:dash",
    "bind:inset-top",
    "bind:inset-bottom",
    "bind:inset-left",
    "bind:inset-right",
    "bind:size",
    "bind:geometry",
    "bind:text",
];

pub(crate) const TEXT_ATTRS: &[&str] = &[
    "text",
    "font",
    "size",
    "weight",
    "style",
    "stretch",
    "line-height",
    "letter-spacing",
    "case",
    "text-align",
    "runs",
];

/// Attributes only one element accepts: `(attr, tag)`.
pub(crate) const KIND_ATTRS: &[(&str, &str)] = &[
    ("component", "Instance"),
    ("of", "Instance"),
    ("op", "BooleanOperation"),
    ("tag", "Other"),
];

/// A component's arguments, `arg:<name>="<expr>"`: the defaults on a
/// <Component>, the values on an <Instance>.
pub(crate) const ARG_PREFIX: &str = "arg:";

/// Whether `tag` takes `arg:*` attributes.
pub(crate) fn takes_args(tag: &str) -> bool {
    matches!(tag, "Component" | "Instance")
}

/// Attributes that only mean something inside an auto-layout container.
pub(crate) const LAYOUT_ATTRS: &[&str] = &["gap", "padding", "wrap", "justify", "align", "hug"];

pub(crate) fn attrs_for(tag: &str) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = COMMON_ATTRS.to_vec();
    v.extend_from_slice(BIND_ATTRS);
    if tag == "Text" {
        v.extend_from_slice(TEXT_ATTRS);
    }
    v.extend(
        KIND_ATTRS
            .iter()
            .filter(|(_, t)| *t == tag)
            .map(|(a, _)| *a),
    );
    v
}

/// Which element owns `attr`, when it's element-specific.
pub(crate) fn owner_of(attr: &str) -> Option<&'static str> {
    if TEXT_ATTRS.contains(&attr) {
        return Some("Text");
    }
    KIND_ATTRS.iter().find(|(a, _)| *a == attr).map(|(_, t)| *t)
}

const ATTR_SYNONYMS: &[(&str, &str)] = &[
    ("colour", "fill"),
    ("color", "fill"),
    ("background", "fill"),
    ("background-color", "fill"),
    ("bg", "fill"),
    ("fill-color", "fill"),
    ("fills", "fill"),
    ("width", "w"),
    ("height", "h"),
    ("left", "x"),
    ("top", "y"),
    ("border-radius", "radius"),
    ("rounded", "radius"),
    ("corner-radius", "radius"),
    ("cornerradius", "radius"),
    ("stroke-width", "stroke-weight"),
    ("strokewidth", "stroke-weight"),
    ("strokeweight", "stroke-weight"),
    ("border-width", "stroke-weight"),
    ("border", "stroke"),
    ("border-color", "stroke"),
    ("stroke-color", "stroke"),
    ("strokes", "stroke"),
    ("rotate", "rotation"),
    ("angle", "rotation"),
    ("direction", "layout"),
    ("flex", "layout"),
    ("flex-direction", "layout"),
    ("auto-layout", "layout"),
    ("spacing", "gap"),
    ("item-spacing", "gap"),
    ("font-size", "size"),
    ("fontsize", "size"),
    ("font-weight", "weight"),
    ("fontweight", "weight"),
    ("font-family", "font"),
    ("fontfamily", "font"),
    ("font-style", "style"),
    ("fontstyle", "style"),
    ("font-stretch", "stretch"),
    ("font-width", "stretch"),
    ("width-percent", "stretch"),
    ("content", "text"),
    ("characters", "text"),
    ("label", "text"),
    ("value", "text"),
    ("overflow", "clip"),
    ("clips-content", "clip"),
    ("clip-content", "clip"),
    ("hidden", "visible"),
    ("alpha", "opacity"),
    ("blend-mode", "blend"),
    ("blendmode", "blend"),
    ("mix-blend-mode", "blend"),
    ("justify-content", "justify"),
    ("align-items", "align"),
    ("items", "align"),
    ("flex-grow", "grow"),
    ("textalign", "text-align"),
    ("lineheight", "line-height"),
    ("letterspacing", "letter-spacing"),
    ("text-case", "case"),
    ("text-transform", "case"),
    ("key", "id"),
    ("shadow", "effects"),
    ("shadows", "effects"),
    ("effect", "effects"),
    ("path", "geometry"),
    ("paths", "geometry"),
    ("d", "geometry"),
    ("min-width", "min-w"),
    ("min-height", "min-h"),
    ("max-width", "max-w"),
    ("max-height", "max-h"),
    ("position", "absolute"),
    ("dash", "stroke-dash"),
    ("stroke-dasharray", "stroke-dash"),
];

const TAG_SYNONYMS: &[(&str, &str)] = &[
    ("rect", "Rectangle"),
    ("circle", "Ellipse"),
    ("oval", "Ellipse"),
    ("div", "Frame"),
    ("view", "Frame"),
    ("box", "Frame"),
    ("container", "Frame"),
    ("stack", "Frame"),
    ("row", "Frame"),
    ("column", "Frame"),
    ("col", "Frame"),
    ("card", "Frame"),
    ("autolayout", "Frame"),
    ("path", "Vector"),
    ("svg", "Vector"),
    ("icon", "Vector"),
    ("span", "Text"),
    ("label", "Text"),
    ("p", "Text"),
    ("h1", "Text"),
    ("image", "Rectangle"),
    ("img", "Rectangle"),
    ("boolean", "BooleanOperation"),
    ("union", "BooleanOperation"),
    ("doc", "Document"),
    ("canvas", "Page"),
    ("symbol", "Component"),
    ("componentinstance", "Instance"),
];

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            cur.push(sub.min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

fn closest<'a>(word: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let w = word.to_ascii_lowercase();
    candidates
        .iter()
        .map(|c| (levenshtein(&w, &c.to_ascii_lowercase()), *c))
        .filter(|(d, c)| *d <= 2.max(c.len() / 3) && *d < c.len().max(w.len()))
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}

pub(crate) fn suggest_attr(word: &str, valid: &[&'static str]) -> Option<&'static str> {
    let w = word.to_ascii_lowercase().replace('_', "-");
    if valid.contains(&w.as_str()) {
        return valid.iter().find(|v| **v == w).copied();
    }
    let squashed = w.replace('-', "");
    let synonym = |s: &str| ATTR_SYNONYMS.iter().find(|(k, _)| *k == s).map(|(_, a)| *a);
    // Exact synonym, else the closest spelling among attributes and synonyms
    // (so `widht` finds `width` → `w`).
    let mut candidates: Vec<&'static str> = valid.to_vec();
    candidates.extend(ATTR_SYNONYMS.iter().map(|(k, _)| *k));
    synonym(&w)
        .or_else(|| synonym(&squashed))
        .or_else(|| closest(&w, &candidates).map(|c| synonym(c).unwrap_or(c)))
        .filter(|a| valid.contains(a))
}

pub(crate) fn suggest_tag(word: &str) -> Option<&'static str> {
    let w = word.to_ascii_lowercase();
    TAGS.iter()
        .find(|t| t.to_ascii_lowercase() == w)
        .copied()
        .or_else(|| TAG_SYNONYMS.iter().find(|(s, _)| *s == w).map(|(_, t)| *t))
        .or_else(|| closest(word, TAGS))
}
