//! Merge a rig library into the app's shipped default config: every module,
//! preset, block preset, tone, drive pedal, song and setlist the shipped
//! files do not have yet is added (by name); what they have stays exactly as
//! it is. Profiles the shipped config does not have are added, and the ones
//! named with `--take` replace the shipped copy.
//!
//! Run by `default-config/ship-library.sh`, which then copies the captures
//! the result references and makes their paths relative.
//!
//! ```sh
//! cargo run -p signal-guitar --example ship_library -- <library dir> <default-config dir> [--take Worship]…
//! ```

use std::path::{Path, PathBuf};

use facet::Facet;
use signal_guitar::compose::{BlockLib, ModuleLib, PresetLib, ToneLib};
use signal_guitar::library::{DrivePresetLib, SetlistLib, SongLib};
use signal_guitar::profiles::ProfileDef;

fn read<T: for<'a> Facet<'a>>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    match facet_styx::from_str::<T>(&text) {
        Ok(v) => Some(v),
        Err(e) => panic!("{} does not parse: {e}", path.display()),
    }
}

fn write<T: for<'a> Facet<'a>>(path: &Path, value: &T) {
    let text = facet_styx::to_string(value).expect("serializes");
    std::fs::write(path, text).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

/// Add `from`'s entries whose key `to` lacks; how many.
fn union<E: Clone>(to: &mut Vec<E>, from: &[E], key: impl Fn(&E) -> String) -> usize {
    let mut added = 0;
    for e in from {
        let k = key(e).to_lowercase();
        if !to.iter().any(|x| key(x).to_lowercase() == k) {
            to.push(e.clone());
            added += 1;
        }
    }
    added
}

macro_rules! merge {
    ($lib:ty, $file:expr, $field:ident, $key:expr, $src:expr, $dst:expr) => {{
        let from: Option<$lib> = read(&$src.join($file));
        let to_path = $dst.join($file);
        if let Some(from) = from {
            match read::<$lib>(&to_path) {
                Some(mut to) => {
                    let n = union(&mut to.$field, &from.$field, $key);
                    if n > 0 {
                        write(&to_path, &to);
                    }
                    println!("{:<20} +{n}", $file);
                }
                None => {
                    write(&to_path, &from);
                    println!("{:<20} new ({})", $file, from.$field.len());
                }
            }
        }
    }};
}

fn main() {
    let mut args = std::env::args().skip(1);
    let src = PathBuf::from(args.next().expect("the library dir"));
    let dst = PathBuf::from(args.next().expect("the default-config dir"));
    let mut take: Vec<String> = Vec::new();
    while let Some(a) = args.next() {
        if a == "--take" {
            take.push(args.next().expect("a profile name").to_lowercase());
        }
    }

    merge!(ModuleLib, "modules.styx", presets, |p: &signal_guitar::compose::ModulePresetDef| format!("{}\t{}", p.module, p.name), src, dst);
    merge!(PresetLib, "presets.styx", presets, |p: &signal_guitar::compose::RigPresetDef| p.name.clone(), src, dst);
    merge!(BlockLib, "blocks.styx", presets, |p: &signal_guitar::compose::BlockPresetDef| format!("{}\t{}", p.block_type, p.name), src, dst);
    merge!(ToneLib, "tones.styx", tones, |t: &signal_guitar::compose::ToneDef| t.name.clone(), src, dst);
    merge!(DrivePresetLib, "drive-presets.styx", presets, |p: &signal_guitar::profiles::DrivePresetDef| p.name.clone(), src, dst);
    merge!(SongLib, "songs.styx", songs, |s: &signal_guitar::profiles::SongDef| s.name.clone(), src, dst);
    merge!(SetlistLib, "setlists.styx", setlists, |s: &signal_guitar::profiles::SetlistDef| s.name.clone(), src, dst);

    // Profiles: new ones added; `--take` ones replace the shipped copy.
    let shipped: Vec<String> = std::fs::read_dir(dst.join("profiles"))
        .map(|rd| rd.filter_map(Result::ok).filter_map(|e| read::<ProfileDef>(&e.path())).map(|p| p.name.to_lowercase()).collect())
        .unwrap_or_default();
    for entry in std::fs::read_dir(src.join("profiles")).expect("the library's profiles").filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("styx") {
            continue;
        }
        let Some(p) = read::<ProfileDef>(&path) else { continue };
        let name = p.name.to_lowercase();
        if shipped.contains(&name) && !take.contains(&name) {
            println!("profile {:<12} kept", p.name);
            continue;
        }
        std::fs::copy(&path, dst.join("profiles").join(entry.file_name())).expect("copy the profile");
        println!("profile {:<12} {}", p.name, if shipped.contains(&name) { "taken" } else { "added" });
    }
}
