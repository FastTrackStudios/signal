//! Rule `motion`: every node's motions read things that exist.
//!
//! - error: an expression that doesn't parse, a param or feed the catalog
//!   doesn't have, a feed read the wrong way (`level()` of a scalar),
//!   `motion(name)` naming no motion, a morph with fewer than two shapes,
//!   a colour that isn't one, keyframes out of order;
//! - warning: a shape provider that isn't built in (fine only if the host
//!   registers it), a geometric motion on a `Group` (a group's transform
//!   doesn't move its children — animate a frame).

use std::collections::HashSet;

use document_proto::motion::{FeedRead, Source, Target, expr};
use document_proto::{NodeKind, Scene};
use surface_proto::{Catalog, EntryId, FeedKind, ParamAddr};

use super::{Issue, Severity, nearest};

const RULE: &str = "motion";

pub(crate) fn check(scene: &Scene, catalog: &Catalog, out: &mut Vec<Issue>) {
    let names: HashSet<&str> = scene
        .nodes
        .values()
        .flat_map(|n| n.motion.iter().filter_map(|m| m.name.as_deref()))
        .collect();
    for id in scene.walk(scene.root) {
        let Some(node) = scene.node(id) else { continue };
        for (i, m) in node.motion.iter().enumerate() {
            let mut push = |severity, message: String, fix: String| {
                out.push(Issue {
                    severity,
                    rule: RULE.into(),
                    node: node.id,
                    node_name: node.name.clone(),
                    message: format!("motion[{i}]: {message}"),
                    fix,
                });
            };
            for p in m.problems() {
                push(
                    Severity::Error,
                    p,
                    "correct it (see the motion reference: expressions, targets, dynamics)".into(),
                );
            }
            for (at, src) in m.expressions() {
                let Ok(e) = expr::parse(src) else { continue };
                for s in e.sources() {
                    if let Some((message, fix)) = source_issue(s, catalog) {
                        push(Severity::Error, format!("{at} `{src}`: {message}"), fix);
                    }
                }
                for name in e.motions() {
                    if !names.contains(name) {
                        push(
                            Severity::Error,
                            format!("{at} `{src}` reads motion `{name}`, which no node has"),
                            format!(
                                "name a motion `{name}` (\"name\":\"{name}\"), or read an existing one{}",
                                nearest(name, names.iter().copied())
                            ),
                        );
                    }
                }
            }
            if let Target::Shape { provider, .. } = &m.target
                && !motion_runtime::shapes::BUILTIN.contains(&provider.as_str())
            {
                push(
                    Severity::Warning,
                    format!(
                        "shape provider `{provider}` isn't built in; it draws nothing unless the host registers it"
                    ),
                    format!(
                        "use a builtin ({}){}",
                        motion_runtime::shapes::BUILTIN.join(", "),
                        nearest(provider, motion_runtime::shapes::BUILTIN.iter().copied())
                    ),
                );
            }
            if geometric(&m.target) && matches!(node.kind, NodeKind::Group) {
                push(
                    Severity::Warning,
                    "moves a Group, whose transform doesn't carry its children".into(),
                    "put the parts in a Frame and animate that".into(),
                );
            }
        }
    }
}

fn geometric(t: &Target) -> bool {
    matches!(
        t,
        Target::X
            | Target::Y
            | Target::Rotate
            | Target::Scale
            | Target::StretchX
            | Target::StretchY
            | Target::SkewX
            | Target::SkewY
    )
}

fn source_issue(s: &Source, catalog: &Catalog) -> Option<(String, String)> {
    match s {
        Source::Param { addr, .. } => {
            catalog
                .param(&ParamAddr::new(addr.clone()))
                .is_none()
                .then(|| {
                    (
                        format!("no param `{addr}` in the catalog"),
                        format!(
                            "use an existing address{}",
                            nearest(addr, catalog.params.iter().map(|p| p.addr.0.as_str()))
                        ),
                    )
                })
        }
        Source::Feed { id, read } => {
            let Some(info) = catalog.stream(&EntryId::new(id.clone())) else {
                return Some((
                    format!("no feed `{id}` in the catalog"),
                    format!(
                        "use an existing feed{}",
                        nearest(id, catalog.streams.iter().map(|s| s.id.0.as_str()))
                    ),
                ));
            };
            let ok = match read {
                FeedRead::Raw(i) => (*i as usize) < info.kind.frame_len().max(1),
                FeedRead::Level(ch) => {
                    matches!(info.kind, FeedKind::Level { channels } if ch.is_none_or(|c| c < channels.max(1)))
                }
                FeedRead::Gr => matches!(info.kind, FeedKind::GainReduction),
                FeedRead::Scalar => matches!(info.kind, FeedKind::Scalar { .. }),
                FeedRead::Band(..) => matches!(info.kind, FeedKind::Spectrum { .. }),
            };
            (!ok).then(|| {
                let how = match read {
                    FeedRead::Raw(i) => format!("feed({id}, {i})"),
                    FeedRead::Level(_) => format!("level({id})"),
                    FeedRead::Gr => format!("gr({id})"),
                    FeedRead::Scalar => format!("scalar({id})"),
                    FeedRead::Band(..) => format!("band({id}, …)"),
                };
                (
                    format!("`{how}` doesn't fit feed `{id}` ({:?})", info.kind),
                    "read it by its kind: level() for levels, gr() for gain reduction, scalar() for scalars, band() for spectra, feed(id, i) for a raw value".into(),
                )
            })
        }
    }
}
