//! What does each of Pro-C 3's hundred stored floats mean?
//!
//! A `.ffp` preset names its parameters and the binary state does not, but
//! both carry exactly a hundred values in the same order, so the names are
//! free. The units are not: `Ratio=0.56` and `Attack=0.0993` are positions
//! along curves the plugin owns, and translating them by eye is how a
//! converter ends up confidently wrong.
//!
//! So ask the plugin what a value means. `value_to_text` is a pure query — no
//! parameter set, no render, no state save — so sweeping a parameter's
//! declared range through it gives the whole curve directly.
//!
//! Two earlier versions of this are worth not repeating. The first swept a
//! normalized 0..1 and mostly measured its own confusion: the host's
//! `set_param` takes a **plain** value, and the stored float, the `.ffp` text
//! value and what `set_param` takes are all the same number — confirmed by
//! setting a parameter and reading the saved state back. The second did
//! set/render/save at every point, and took over an hour across the yabridge
//! boundary for nineteen parameters, which is not a tool anyone iterates
//! with. Ask the cheap question instead.
//!
//! ```sh
//! cargo run --release -p signal-analyzer --example proc3_params -- \
//!     --plugin ~/.clap/yabridge/"FabFilter Pro-C 3.clap" [--only Ratio,Attack]
//! ```

use signal_plugin_host::HostedPlugin;

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;
/// Points sampled across each parameter's declared range, unless `--steps`
/// says otherwise. Sixty is enough to see the shape of a curve and to
/// interpolate it to well under the plugin's own display precision.
const DEFAULT_STEPS: usize = 60;

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

fn main() {
    let Some(path) = arg("--plugin") else {
        eprintln!(
            "usage: plugin_params --plugin <path> [--only a,b] [--rust <name>] [--steps n]"
        );
        std::process::exit(2);
    };
    let steps: usize = arg("--steps").and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_STEPS);
    // `--rust` emits an interpolation table for one parameter instead of a
    // human listing, so a measured curve reaches the code without anyone
    // retyping sixty pairs of numbers.
    let as_rust = arg("--rust").map(|s| s.to_lowercase());
    let only: Option<Vec<String>> =
        arg("--only").map(|s| s.split(',').map(|p| p.trim().to_lowercase()).collect());

    let mut plugin = if let Ok(Some(mut p)) = HostedPlugin::load(&path) {
        p.prepare(SR, BLOCK as u32).expect("prepare");
        p
    } else {
        eprintln!("{path}: could not load");
        std::process::exit(1);
    };

    let all = plugin.params();
    eprintln!("{} parameters", all.len());
    let params: Vec<_> = all
        .into_iter()
        .enumerate()
        .filter(|(_, p)| match &only {
            None => true,
            Some(only) => only.iter().any(|w| p.name.to_lowercase().contains(w)),
        })
        .collect();

    for (index, p) in &params {
        let samples: Vec<(f64, String)> = (0..=steps)
            .map(|step| {
                let stored = (p.max - p.min).mul_add(step as f64 / steps as f64, p.min);
                (stored, plugin.value_to_text(p.id, stored).unwrap_or_default())
            })
            .collect();

        if as_rust.as_deref() == Some(&p.name.to_lowercase()) {
            emit_rust_table(&p.name, samples);
            continue;
        }

        println!("[{index:>3}] {}   stored {} .. {}", p.name, p.min, p.max);
        let mut last = String::new();
        for (stored, shown) in samples {
            // A switch or a mode repeats itself across most of its range;
            // printing only the changes makes a fourteen-way selector read as
            // fourteen lines instead of sixty.
            if shown != last {
                println!("        {stored:>10.4}  {shown}");
                last = shown;
            }
        }
    }
}

/// Emit a measured curve as a Rust table, ready to paste into a decoder.
///
/// The display text is parsed for a leading number and a unit, so
/// `"56.50 ms"` and `"1.325 sec"` land in the same table in milliseconds and
/// `"4.00:1"` lands as 4. A line that carries no number at all — an enum, a
/// switch — is passed through as a comment rather than dropped, because a
/// selector masquerading as a curve is exactly the thing you want to notice.
fn emit_rust_table(name: &str, samples: Vec<(f64, String)>) {
    let mut rows: Vec<(f64, f64)> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    for (stored, shown) in &samples {
        let mut it = shown.split_whitespace();
        let head = it.next().unwrap_or("");
        let numeric: String = head
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+')
            .collect();
        match numeric.parse::<f64>() {
            Ok(v) => {
                // Seconds and percent are the two units that would otherwise
                // put two scales in one table.
                let scale = match it.next().unwrap_or("") {
                    "sec" | "s" => 1000.0,
                    _ => 1.0,
                };
                rows.push((*stored, v * scale));
            }
            Err(_) => notes.push(format!("{stored:.4} = {shown}")),
        }
    }

    let ident: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
        .collect();
    println!("/// `{name}`, measured off the plugin with `plugin_params --rust`.");
    if !notes.is_empty() {
        println!("///");
        println!("/// Values that are not numbers, so not part of the curve:");
        for n in notes.iter().take(20) {
            println!("///   {n}");
        }
    }
    println!("const {ident}_CURVE: [(f64, f64); {}] = [", rows.len());
    for chunk in rows.chunks(4) {
        let row: Vec<String> = chunk.iter().map(|(x, y)| format!("({x:.4}, {y:.6})")).collect();
        println!("    {},", row.join(", "));
    }
    println!("];");
}
