//! The dioxus-mcp probe, off unless it is asked for.
//!
//! `dioxus_mcp_probe::install()` puts a `tracing` layer under a bare
//! `Registry` with no level filter, so every `trace!` in `dioxus_core`
//! fires — including the diff's per-node ones, whose fields are whole
//! `VNode` trees recorded with `?`. Each event then costs a `Debug`
//! format of that tree, an RFC 3339 timestamp string, a
//! `serde_json::Map`, and — in the layer's target test — up to six
//! `format!("{target}::")` allocations, on the thread that is trying to
//! render.
//!
//! Measured on the drum workstation, doing nothing but playing back:
//! **18,000 events a second, 17 MB/s of JSON**, out of ninety renders of
//! three trivial components. A render of the drum stack is ~1900 nodes,
//! so a single scrolled frame emits tens of thousands of these — which is
//! where a 6 fps drag went.
//!
//! So it is opt-in. `FTS_DIOXUS_PROBE=1` turns the full probe on when
//! `runtime_events` is the tool for the job; otherwise a panic hook
//! writes the one record that has to survive either way. `dx serve`
//! captures the child's console into its own TUI, so a crash in the
//! window otherwise reaches nobody — that, not the render log, is why
//! anything is installed at startup at all.

use std::io::Write as _;
use std::path::PathBuf;

/// What was installed, kept alive for the process.
///
/// The full probe's handle stops its writer thread when dropped, so a
/// caller must hold (or leak) this.
pub enum Probe {
    /// `FTS_DIOXUS_PROBE=1`: the whole thing.
    Full(dioxus_mcp_probe::ProbeHandle),
    /// The default: panics only.
    PanicsOnly,
}

/// Install the probe the environment asks for.
///
/// Leak the return value — it should outlive every frame.
#[must_use]
pub fn install() -> Probe {
    if wanted() {
        // The frame meter's line has to survive this branch. The probe
        // installs itself as the global `tracing` subscriber, so turning
        // it on takes the fmt layer below out of the process — and the
        // one reading an A/B is decided by would go missing in exactly
        // the arm that is the control. `extra_targets` puts it in the
        // probe's own log instead, so `ui.fps` is readable either way.
        let mut config = dioxus_mcp_probe::ProbeConfig::default();
        config
            .extra_targets
            .push("expression_editor_ui::frame_meter".into());
        return Probe::Full(dioxus_mcp_probe::install_with(config));
    }
    install_panic_hook();
    install_tracing();
    Probe::PanicsOnly
}

/// An ordinary `RUST_LOG` subscriber, because turning the probe off
/// would otherwise turn every `tracing` line in the process off with it:
/// the probe's layer sits under a `Registry` installed as the global
/// default, so it has always been the only subscriber these windows had.
///
/// Off by default the way any `RUST_LOG` is — an unset filter means
/// `warn`, which is what a window with no terminal should say.
fn install_tracing() {
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::layer::SubscriberExt as _;
    use tracing_subscriber::util::SubscriberInitExt as _;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .with(filter)
        .try_init();
}

/// Whether the full probe was asked for.
fn wanted() -> bool {
    std::env::var("FTS_DIOXUS_PROBE")
        .map(|v| matches!(v.trim(), "1" | "true" | "full" | "on"))
        .unwrap_or(false)
}

/// Where `runtime_events` reads from, resolved the way the probe does.
fn log_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("target")
        .join("dioxus-mcp")
        .join("events.jsonl")
}

/// Append one `kind: "panic"` record per panic, in the probe's schema, so
/// `runtime_events` reads a crash out of the same file either way.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        write_panic(info);
        previous(info);
    }));
}

fn write_panic(info: &std::panic::PanicHookInfo<'_>) {
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&'static str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "<non-string panic payload>".to_string());
    let (file, line) = info
        .location()
        .map_or((String::new(), 0), |l| (l.file().to_string(), l.line()));

    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(mut out) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    let record = serde_json::json!({
        "v": 1,
        "ts": now_rfc3339(),
        "kind": "panic",
        "message": message,
        "file": file,
        "line": line,
    });
    let _ = writeln!(out, "{record}");
    let _ = out.flush();
}

/// An RFC 3339 stamp, which is what the schema's `ts` is and what
/// `runtime_events`' `since` compares against.
///
/// Hand-rolled from the epoch rather than pulling a calendar crate in for
/// one line that runs once, at a panic. Civil-date arithmetic only — days
/// since 1970 to a Gregorian date, seconds to a clock.
fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Howard Hinnant's `civil_from_days`: days since the Unix epoch to a
/// proleptic Gregorian `(year, month, day)`.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_formats_as_the_epoch() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn a_leap_day_is_a_leap_day() {
        // 2024-02-29 is 19782 days after the epoch.
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
    }

    #[test]
    fn a_stamp_is_rfc_3339_shaped() {
        let ts = now_rfc3339();
        assert_eq!(ts.len(), 20, "{ts}");
        assert!(ts.ends_with('Z'), "{ts}");
        assert!(ts.starts_with("20"), "{ts}");
    }
}
