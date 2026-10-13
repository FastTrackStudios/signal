//! The rules of hooks, checked over the app's own source.
//!
//! A Dioxus hook (`use_*`, and `try_use_context` — which is one) must be
//! called on every render, in the same order. One behind `&&` or `||` is
//! called on some renders and not others, and the render after the change
//! panics ("unable to retrieve the hook"): the phone's Mixer showed that
//! panic in place of the mixer. `dx check` would catch it, but no `dx`
//! here matches this Dioxus, so this test does.

/// Every `.rs` file under `dir`, recursively.
fn sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Whether `rest` (what follows a `&&` or `||`) starts with a hook call: a
/// path whose last segment starts `use_` or `try_use_`, called.
fn hook_call(rest: &str) -> bool {
    let rest = rest.trim_start().trim_start_matches('!');
    let path: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == ':')
        .collect();
    // `try_use_context::<P>()`: the turbofish's `::` is not the path's.
    let path = path.trim_end_matches(':');
    let last = path.rsplit("::").next().unwrap_or("");
    let after = &rest[path.len()..];
    let called = after.starts_with('(') || after.starts_with("::<");
    called && (last.starts_with("use_") || last.starts_with("try_use_"))
}

/// The places in `text` where a hook is called behind a short circuit.
fn offences(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let code = line.split("//").next().unwrap_or("");
        for op in ["&&", "||"] {
            let mut from = 0;
            while let Some(at) = code[from..].find(op) {
                let after = from + at + op.len();
                if hook_call(&code[after..]) {
                    found.push((n + 1, line.trim().to_owned()));
                }
                from = after;
            }
        }
    }
    found
}

#[test]
fn the_rule_is_what_it_says() {
    assert_eq!(
        offences("let x = a && try_use_context::<P>().is_some();").len(),
        1
    );
    assert_eq!(offences("    || crate::touch::use_touch()").len(), 1);
    assert_eq!(
        offences("let p = try_use_context::<P>(); let x = a && p.is_some();").len(),
        0
    );
    assert_eq!(offences("a && reuse_thing()").len(), 0);
    assert_eq!(offences("// a && use_signal(|| 0)").len(), 0);
}

#[test]
fn no_hook_is_called_behind_a_short_circuit() {
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    sources(&here.join("src"), &mut files);
    sources(&here.join("../desktop/src"), &mut files);
    assert!(
        !files.is_empty(),
        "no sources found under {}",
        here.display()
    );
    let mut all = Vec::new();
    for file in files {
        // This file's own examples are offences on purpose.
        if file.ends_with("hook_rules.rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        for (line, code) in offences(&text) {
            all.push(format!("{}:{line}: {code}", file.display()));
        }
    }
    assert!(
        all.is_empty(),
        "hooks called behind `&&`/`||` (read them into a variable first):\n{}",
        all.join("\n")
    );
}
