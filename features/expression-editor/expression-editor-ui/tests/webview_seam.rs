//! Nothing may hand a WebView an `Any` attribute.
//!
//! `CustomWidgetAttr` is how Blitz is given a painted scene. A WebView
//! does not merely ignore it — dioxus panics on the first mutation
//! carrying one, "Any attributes are not supported by the current
//! renderer", and takes the window with it. So it is not enough for the
//! surfaces that use it to render correctly; every one of them has to be
//! compiled out under `webview`.
//!
//! This is a source check rather than a render test on purpose. The
//! panic happens at the DOM mutation, far from the component that caused
//! it, and it only fires for whichever surface is on screen — the
//! painted cursor was missed exactly this way, because it is mounted in
//! every view and so looked like every surface being broken at once.
//! Reading the source finds an ungated use whether or not a test happens
//! to mount it.

use std::path::Path;

#[test]
fn every_custom_widget_is_compiled_out_under_webview() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut ungated = Vec::new();

    for file in walk(&src) {
        // The seam itself defines the widgets; it is native-only as a
        // whole module.
        if file.file_name().is_some_and(|n| n == "roll_widget.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&file).expect("read source");
        for (i, line) in text.lines().enumerate() {
            if !line.contains("CustomWidgetAttr::new") {
                continue;
            }
            // The construction must sit under a `cfg` that excludes
            // webview. Look back a few lines for it: the attribute is on
            // the `let`, which may be preceded by comments.
            let before: Vec<&str> = text.lines().take(i).collect();
            let guarded = before
                .iter()
                .rev()
                .take(4)
                .any(|prior| prior.contains(r#"cfg(not(feature = "webview"))"#));
            if !guarded {
                ungated.push(format!("{}:{}", file.display(), i + 1));
            }
        }
    }

    assert!(
        ungated.is_empty(),
        "these hand a WebView an Any attribute and will panic it:\n  {}",
        ungated.join("\n  ")
    );
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out
}
