//! The drop log: every audio dropout the engine sees, written to
//! `logs/dropouts.log` with what the rig was doing when it happened — the
//! patch playing, how long since the last switch and from what, the render
//! time against the block's budget — so the drops can be counted, lined up
//! against switches and tuned away.
//!
//! The engine records drops lock- and allocation-free from the realtime
//! thread (daw `EngineStats::drops`); the pump collects them every tick and
//! writes them here. Each switch is marked (from wherever it happened) and,
//! ten seconds on, summarised: how many drops followed it and when — the
//! shape of "a burst of drops for the first few seconds after a switch".
//!
//! Lines (tab-separated after the time, for grep and cut):
//!
//! ```text
//! 2026-09-24 12:04:31.182  SWITCH  Clean → Drive Dotted  via stack  (settle 3.1 ms)
//! 2026-09-24 12:04:31.412  DROP  over-budget  render 3.41 ms / 2.67 ms (128)  +0.230 s after → Drive Dotted  patch Drive Dotted
//! 2026-09-24 12:04:41.182  AFTER  Clean → Drive Dotted  7 drops in 10 s  [0-1 s: 4, 1-3 s: 2, 3-6 s: 1, 6-10 s: 0]  worst 4.02 ms
//! ```

use std::io::Write;
use std::sync::Mutex;

use signal_rig_host::lock::LockExt;
use signal_sampler::{clock_ns, DropEvent, DropKind};

/// How long after a switch its drops are counted for its summary.
const WINDOW_NS: u64 = 10_000_000_000;
/// The summary's buckets, seconds after the switch.
const BUCKETS: [(f64, f64); 4] = [(0.0, 1.0), (1.0, 3.0), (3.0, 6.0), (6.0, 10.0)];

/// A patch switch, as the drop log saw it.
#[derive(Clone, Debug)]
struct Switch {
    at_ns: u64,
    from: String,
    to: String,
    via: String,
    drops: [u32; BUCKETS.len()],
    worst_ns: u64,
    reported: bool,
}

/// Switches marked from any thread, for the pump to match drops against.
static SWITCHES: Mutex<Vec<Switch>> = Mutex::new(Vec::new());

/// Mark a switch (`from` → `to`, via a stack / section / song / momentary),
/// and how long its follow-up took. Cheap; call from the switch path.
pub fn switched(from: &str, to: &str, via: &str, settle_ms: f64) {
    let at_ns = clock_ns();
    {
        let mut s = SWITCHES.lock_ok();
        s.push(Switch {
            at_ns,
            from: from.to_string(),
            to: to.to_string(),
            via: via.to_string(),
            drops: [0; BUCKETS.len()],
            worst_ns: 0,
            reported: false,
        });
        // Only the last minute's matter.
        let keep = at_ns.saturating_sub(60_000_000_000);
        s.retain(|w| w.at_ns >= keep || !w.reported);
    }
    write_line(at_ns, &format!("SWITCH\t{from} → {to}\tvia {via}\t(settle {settle_ms:.1} ms)"));
}

/// The pump's side: what it has collected so far.
#[derive(Default)]
pub struct DropLog {
    seen: u64,
    /// Drops in the current minute, for the per-minute rate line.
    minute_start_ns: u64,
    minute_drops: u32,
    minute_worst_ns: u64,
}

impl DropLog {
    /// Write the engine's new drops (`events`, collected from the rig) with
    /// the rig's state (`patch` playing), and any switch summaries now due.
    pub fn record(&mut self, events: &[DropEvent], patch: &str) {
        let now = clock_ns();
        for e in events {
            if self.seen > 0 && e.seq > self.seen {
                write_line(e.at_ns, &format!("LOST\t{} drops not logged (the ring overflowed)", e.seq - self.seen));
            }
            self.seen = e.seq + 1;
            let (kind, detail) = match e.kind {
                DropKind::OverBudget => (
                    "over-budget",
                    format!(
                        "render {:.2} ms / {:.2} ms ({})",
                        e.render_ns as f64 / 1e6,
                        e.budget_ns as f64 / 1e6,
                        e.frames
                    ),
                ),
                DropKind::DeviceOverload => ("device-overload", format!("({} frames)", e.frames)),
            };
            // The switch it followed, if one was recent.
            let after = {
                let mut s = SWITCHES.lock_ok();
                let hit = s.iter_mut().rev().find(|w| w.at_ns <= e.at_ns && e.at_ns - w.at_ns < WINDOW_NS);
                hit.map(|w| {
                    let dt = (e.at_ns - w.at_ns) as f64 / 1e9;
                    if let Some(b) = BUCKETS.iter().position(|(lo, hi)| dt >= *lo && dt < *hi) {
                        w.drops[b] += 1;
                    }
                    w.worst_ns = w.worst_ns.max(e.render_ns);
                    format!("+{dt:.3} s after → {}", w.to)
                })
            }
            .unwrap_or_else(|| "no recent switch".to_string());
            write_line(e.at_ns, &format!("DROP\t{kind}\t{detail}\t{after}\tpatch {patch}"));

            if e.at_ns.saturating_sub(self.minute_start_ns) >= 60_000_000_000 {
                self.flush_minute(e.at_ns);
            }
            self.minute_drops += 1;
            self.minute_worst_ns = self.minute_worst_ns.max(e.render_ns);
        }
        // Switch summaries whose window has closed.
        let due: Vec<Switch> = {
            let mut s = SWITCHES.lock_ok();
            s.iter_mut()
                .filter(|w| !w.reported && now.saturating_sub(w.at_ns) >= WINDOW_NS)
                .map(|w| {
                    w.reported = true;
                    w.clone()
                })
                .collect()
        };
        for w in due {
            let total: u32 = w.drops.iter().sum();
            let buckets = BUCKETS
                .iter()
                .zip(w.drops)
                .map(|((lo, hi), n)| format!("{lo:.0}-{hi:.0} s: {n}"))
                .collect::<Vec<_>>()
                .join(", ");
            write_line(
                w.at_ns + WINDOW_NS,
                &format!(
                    "AFTER\t{} → {}\tvia {}\t{total} drops in 10 s\t[{buckets}]\tworst {:.2} ms",
                    w.from,
                    w.to,
                    w.via,
                    w.worst_ns as f64 / 1e6
                ),
            );
        }
        if self.minute_drops > 0 && now.saturating_sub(self.minute_start_ns) >= 60_000_000_000 {
            self.flush_minute(now);
        }
    }

    fn flush_minute(&mut self, now: u64) {
        if self.minute_drops > 0 {
            write_line(
                now,
                &format!(
                    "MINUTE\t{} drops in the last minute\tworst {:.2} ms",
                    self.minute_drops,
                    self.minute_worst_ns as f64 / 1e6
                ),
            );
        }
        self.minute_start_ns = now;
        self.minute_drops = 0;
        self.minute_worst_ns = 0;
    }
}

/// Where the log lives: `SIGNAL_DROP_LOG` (a file), else
/// `/Volumes/dev-drive/logs/dropouts.log` when that drive is there, else
/// the temp dir.
fn path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("SIGNAL_DROP_LOG") {
        if !p.is_empty() {
            return p.into();
        }
    }
    let drive = std::path::Path::new("/Volumes/dev-drive/logs");
    if drive.is_dir() {
        return drive.join("dropouts.log");
    }
    std::env::temp_dir().join("signal-dropouts.log")
}

/// Append one line, stamped with wall-clock time for `at_ns` (on
/// [`clock_ns`]'s base).
fn write_line(at_ns: u64, text: &str) {
    let now_ns = clock_ns();
    let wall = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_nanos(now_ns.saturating_sub(at_ns)))
        .unwrap_or_else(std::time::SystemTime::now);
    let stamp = format_local(wall);
    let line = format!("{stamp}  {text}\n");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path()) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// `YYYY-MM-DD HH:MM:SS.mmm` in UTC (the log's other timestamps, tracing's,
/// are UTC too).
pub(crate) fn format_local(t: std::time::SystemTime) -> String {
    let d = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() as i64;
    let ms = d.subsec_millis();
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    let (y, m, dd) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{dd:02} {:02}:{:02}:{:02}.{ms:03}Z",
        sod / 3600,
        (sod % 3600) / 60,
        sod % 60
    )
}

/// Days since 1970-01-01 → (year, month, day) — Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
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
    fn dates_are_civil() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_720), (2026, 9, 24));
    }

    /// A drop after a switch lands in the switch's bucket.
    #[test]
    fn drops_are_counted_against_the_switch_they_follow() {
        let dir = std::env::temp_dir().join(format!("droplog-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // SAFETY: tests in this module run on their own thread; nothing else
        // reads SIGNAL_DROP_LOG.
        unsafe { std::env::set_var("SIGNAL_DROP_LOG", dir.join("d.log")) };
        switched("Clean", "Drive", "stack", 1.0);
        let at = clock_ns();
        let mut log = DropLog::default();
        log.record(
            &[DropEvent { seq: 0, at_ns: at + 1, kind: DropKind::OverBudget, render_ns: 3_000_000, budget_ns: 2_666_000, frames: 128 }],
            "Drive",
        );
        let s = SWITCHES.lock_ok();
        let w = s.iter().rev().find(|w| w.to == "Drive").unwrap();
        assert_eq!(w.drops[0], 1);
        let text = std::fs::read_to_string(dir.join("d.log")).unwrap();
        assert!(text.contains("SWITCH\tClean → Drive"));
        assert!(text.contains("DROP\tover-budget"));
    }
}
