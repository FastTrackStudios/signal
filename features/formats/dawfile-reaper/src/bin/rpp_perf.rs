use std::alloc::{GlobalAlloc, Layout, System};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use dawfile_reaper::{DecodeOptions, ReaperProject, parse_rpp_file};

struct CountingAllocator;

static ALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        let delta = new_size.saturating_sub(layout.size()) as u64;
        ALLOC_BYTES.fetch_add(delta, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[derive(Debug)]
struct Config {
    fixtures: Vec<PathBuf>,
    warmup: usize,
    repeat: usize,
    typed_mode: TypedMode,
}

#[derive(Debug, Clone, Copy)]
enum TypedMode {
    Full,
    Summary,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fixtures: vec![
                PathBuf::from("tests/fixtures/tempo-map-advanced.RPP"),
                PathBuf::from("tests/fixtures/local/Goodness of God.RPP"),
            ],
            warmup: 1,
            repeat: 3,
            typed_mode: TypedMode::Full,
        }
    }
}

#[derive(Debug)]
struct FixtureResult {
    path: PathBuf,
    bytes: usize,
    parse_avg: Duration,
    typed_avg: Duration,
    throughput_mb_s: f64,
    peak_rss_mb: Option<f64>,
    parse_alloc_calls_avg: u64,
    parse_alloc_mb_avg: f64,
    typed_alloc_calls_avg: u64,
    typed_alloc_mb_avg: f64,
}

fn parse_args() -> Result<Config, String> {
    let mut cfg = Config::default();
    cfg.fixtures.clear();

    let mut args = std::env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fixture" => {
                let v = args
                    .next()
                    .ok_or("--fixture expects a path argument".to_string())?;
                cfg.fixtures.push(PathBuf::from(v));
            }
            "--warmup" => {
                let v = args
                    .next()
                    .ok_or("--warmup expects an integer".to_string())?;
                cfg.warmup = v
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --warmup value: {v}"))?;
            }
            "--repeat" => {
                let v = args
                    .next()
                    .ok_or("--repeat expects an integer".to_string())?;
                cfg.repeat = v
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --repeat value: {v}"))?;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            "--typed-mode" => {
                let v = args
                    .next()
                    .ok_or("--typed-mode expects full|summary".to_string())?;
                cfg.typed_mode = match v.as_str() {
                    "full" => TypedMode::Full,
                    "summary" => TypedMode::Summary,
                    _ => return Err(format!("invalid --typed-mode value: {v}")),
                };
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    if cfg.fixtures.is_empty() {
        cfg.fixtures = Config::default().fixtures;
    }
    if cfg.repeat == 0 {
        return Err("--repeat must be >= 1".to_string());
    }
    Ok(cfg)
}

fn print_help() {
    println!("rpp_perf - parse/decode performance utility for dawfile-reaper");
    println!();
    println!("Usage:");
    println!("  cargo run -p dawfile-reaper --bin rpp_perf -- [options]");
    println!();
    println!("Options:");
    println!("  --fixture <path>   Add fixture path (repeatable)");
    println!("  --warmup <n>       Warmup iterations per fixture (default: 1)");
    println!("  --repeat <n>       Measured iterations per fixture (default: 3)");
    println!("  --typed-mode <m>   full|summary typed decode mode (default: full)");
}

fn read_fixture(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("failed reading {}: {e}", path.display()))
}

fn mb(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn reset_alloc_counters() {
    ALLOC_CALLS.store(0, Ordering::Relaxed);
    ALLOC_BYTES.store(0, Ordering::Relaxed);
}

fn read_alloc_counters() -> (u64, u64) {
    (
        ALLOC_CALLS.load(Ordering::Relaxed),
        ALLOC_BYTES.load(Ordering::Relaxed),
    )
}

#[cfg(unix)]
fn peak_rss_bytes() -> Option<u64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if rc != 0 {
        return None;
    }
    let usage = unsafe { usage.assume_init() };
    #[cfg(target_os = "macos")]
    {
        Some(usage.ru_maxrss as u64)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Some((usage.ru_maxrss as u64) * 1024)
    }
}

#[cfg(not(unix))]
fn peak_rss_bytes() -> Option<u64> {
    None
}

fn run_fixture(
    path: PathBuf,
    content: &str,
    warmup: usize,
    repeat: usize,
    typed_mode: TypedMode,
) -> FixtureResult {
    let decode_opts = match typed_mode {
        TypedMode::Full => DecodeOptions::full(),
        TypedMode::Summary => DecodeOptions::summary(),
    };
    for _ in 0..warmup {
        let parsed = parse_rpp_file(content).expect("warmup parse failed");
        let _typed = ReaperProject::from_rpp_project_with_options(&parsed, decode_opts)
            .expect("warmup typed decode failed");
    }

    let mut parse_total = Duration::ZERO;
    let mut typed_total = Duration::ZERO;
    let mut parse_alloc_calls_total = 0u64;
    let mut parse_alloc_bytes_total = 0u64;
    let mut typed_alloc_calls_total = 0u64;
    let mut typed_alloc_bytes_total = 0u64;
    for _ in 0..repeat {
        reset_alloc_counters();
        let t0 = Instant::now();
        let parsed = parse_rpp_file(content).expect("parse failed");
        let t1 = Instant::now();
        let (parse_calls, parse_bytes) = read_alloc_counters();

        reset_alloc_counters();
        let _typed = ReaperProject::from_rpp_project_with_options(&parsed, decode_opts)
            .expect("typed conversion failed");
        let t2 = Instant::now();
        let (typed_calls, typed_bytes) = read_alloc_counters();

        parse_total += t1 - t0;
        typed_total += t2 - t1;
        parse_alloc_calls_total += parse_calls;
        parse_alloc_bytes_total += parse_bytes;
        typed_alloc_calls_total += typed_calls;
        typed_alloc_bytes_total += typed_bytes;
    }

    let parse_avg = parse_total / repeat as u32;
    let typed_avg = typed_total / repeat as u32;
    let throughput_mb_s = mb(content.len()) / parse_avg.as_secs_f64().max(0.000_001);
    let peak_rss_mb = peak_rss_bytes().map(|v| v as f64 / (1024.0 * 1024.0));

    FixtureResult {
        path,
        bytes: content.len(),
        parse_avg,
        typed_avg,
        throughput_mb_s,
        peak_rss_mb,
        parse_alloc_calls_avg: parse_alloc_calls_total / repeat as u64,
        parse_alloc_mb_avg: mb((parse_alloc_bytes_total / repeat as u64) as usize),
        typed_alloc_calls_avg: typed_alloc_calls_total / repeat as u64,
        typed_alloc_mb_avg: mb((typed_alloc_bytes_total / repeat as u64) as usize),
    }
}

fn main() -> Result<(), String> {
    let cfg = parse_args()?;
    println!(
        "rpp_perf: fixtures={}, warmup={}, repeat={}, typed_mode={:?}",
        cfg.fixtures.len(),
        cfg.warmup,
        cfg.repeat,
        cfg.typed_mode
    );

    let mut results = Vec::new();
    for path in cfg.fixtures {
        if !path.exists() {
            println!("skip (missing): {}", path.display());
            continue;
        }

        let content = read_fixture(&path)?;
        let result = run_fixture(path, &content, cfg.warmup, cfg.repeat, cfg.typed_mode);
        results.push(result);
    }

    if results.is_empty() {
        return Err("no fixtures were found; pass --fixture <path>".to_string());
    }

    println!();
    println!("=== Results ===");
    for r in results {
        println!("fixture: {}", r.path.display());
        println!("  size_mb: {:.2}", mb(r.bytes));
        println!("  parse_avg_s: {:.4}", r.parse_avg.as_secs_f64());
        println!("  typed_avg_s: {:.4}", r.typed_avg.as_secs_f64());
        println!("  parse_throughput_mb_s: {:.2}", r.throughput_mb_s);
        match r.peak_rss_mb {
            Some(v) => println!("  peak_rss_mb: {:.2}", v),
            None => println!("  peak_rss_mb: n/a"),
        }
        println!("  parse_alloc_calls_avg: {}", r.parse_alloc_calls_avg);
        println!("  parse_alloc_mb_avg: {:.2}", r.parse_alloc_mb_avg);
        println!("  typed_alloc_calls_avg: {}", r.typed_alloc_calls_avg);
        println!("  typed_alloc_mb_avg: {:.2}", r.typed_alloc_mb_avg);
    }

    Ok(())
}
