//! Open and closed subpaths: an authored stroke strokes open subpaths as
//! lines and never fills them.

/// `path`'s closed subpaths and its open ones, as two paths.
pub(crate) fn split_open_closed(path: &kurbo::BezPath) -> (kurbo::BezPath, kurbo::BezPath) {
    use kurbo::PathEl;
    let (mut closed, mut open) = (kurbo::BezPath::new(), kurbo::BezPath::new());
    let mut sub: Vec<PathEl> = Vec::new();
    let mut flush = |sub: &mut Vec<PathEl>| {
        // A lone move-to draws nothing either way.
        if sub.len() > 1 {
            let target = if matches!(sub.last(), Some(PathEl::ClosePath)) {
                &mut closed
            } else {
                &mut open
            };
            sub.iter().for_each(|el| target.push(*el));
        }
        sub.clear();
    };
    for el in path.elements() {
        if matches!(el, PathEl::MoveTo(_)) {
            flush(&mut sub);
        }
        sub.push(*el);
        if matches!(el, PathEl::ClosePath) {
            flush(&mut sub);
        }
    }
    flush(&mut sub);
    (closed, open)
}

/// The closed subpaths of SVG path data `d` (`None` when it doesn't parse).
pub(crate) fn closed_subpaths_svg(d: &str) -> Option<String> {
    let path = kurbo::BezPath::from_svg(d).ok()?;
    Some(split_open_closed(&path).0.to_svg())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_open_from_closed() {
        let path =
            kurbo::BezPath::from_svg("M0 0 L10 0 L10 10 Z M20 0 L30 0 M40 40 A5 5 0 0 1 50 50")
                .unwrap();
        let (closed, open) = split_open_closed(&path);
        assert_eq!(closed.elements().len(), 4);
        assert!(matches!(
            closed.elements().last(),
            Some(kurbo::PathEl::ClosePath)
        ));
        assert_eq!(
            open.elements()
                .iter()
                .filter(|e| matches!(e, kurbo::PathEl::MoveTo(_)))
                .count(),
            2
        );
        assert_eq!(closed_subpaths_svg("M0 0 L5 5").as_deref(), Some(""));
    }
}
