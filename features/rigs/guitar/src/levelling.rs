//! Patch levelling without a running rig.
//!
//! The rig's own pass (`Rig::level_patches`) measures the installed chains.
//! This one builds them from the profile on disk and renders them offline,
//! so a terminal — or an agent that just wrote a profile — can level every
//! patch before the app is ever opened.

use crate::library::RigLibrary;

/// Apply the rig's NAM level calibration (from its audio prefs) to every
/// chain built from now on — the live rig and offline levelling alike, so
/// what is measured is what is heard.
pub fn apply_nam_calibration() -> Option<f32> {
    let cal = signal_sampler::RigManager::load(crate::session::AUDIO_RIG_NAME)
        .audio
        .nam_calibration();
    signal_sampler::nam::set_interface_calibration_dbu(cal);
    cal
}
use crate::nodes::profile_from_library;

/// Every core this machine has — the default for [`par_map`].
#[must_use]
pub fn cores() -> usize {
    std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get)
}

/// `f` over `items` on up to `threads` threads (`0` = [`cores`]), results in
/// input order.
///
/// A measurement renders the whole DI reference through a chain — seconds of
/// NAM per patch — and the patches are independent, so they run side by
/// side. Scoped threads rather than a pool: the work is a few dozen long
/// jobs, not many short ones.
pub fn par_map<T: Sync, R: Send>(
    items: &[T],
    threads: usize,
    f: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    let threads = if threads == 0 { cores() } else { threads };
    let threads = threads.clamp(1, items.len().max(1));
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results: Vec<std::sync::Mutex<Option<R>>> =
        items.iter().map(|_| std::sync::Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(item) = items.get(i) else { break };
                    let r = f(item);
                    *results[i]
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(r);
                }
            });
        }
    });
    results
        .into_iter()
        .map(|m| {
            m.into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .expect("every item is taken exactly once")
        })
        .collect()
}

/// One patch's measurement.
#[derive(Clone, Debug)]
pub struct Levelled {
    pub patch: String,
    /// Integrated loudness of the raw patch; `None` when it did not render.
    pub lufs: Option<f32>,
    /// The calibration written to `level_db` (unchanged when `lufs` is `None`).
    pub level_db: f32,
}

/// Level every patch of the named profile (the active one when `None`) and,
/// unless `dry_run`, write each `level_db` back to its file.
///
/// Measured with the patch's calibration AND the player's trim zeroed: the
/// chain carries both on its trim block, and measuring through them would
/// make a second pass correct the first instead of repeating it. The
/// player's `trim_db` is never touched — it is set by ear, on top.
///
/// # Errors
///
/// When no profile has that name.
pub fn level_profile(
    name: Option<&str>,
    sample_rate: u32,
    dry_run: bool,
) -> Result<Vec<Levelled>, String> {
    apply_nam_calibration();
    let lib = RigLibrary::load_or_bootstrap();
    let mut def = match name {
        None => lib.profile.clone(),
        Some(n) => lib
            .profiles
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(n))
            .cloned()
            .ok_or_else(|| {
                let names: Vec<&str> = lib.profiles.iter().map(|p| p.name.as_str()).collect();
                format!("no profile named {n:?} (have: {})", names.join(", "))
            })?,
    };

    let mut raw = def.clone();
    for p in &mut raw.patches {
        p.level_db = 0.0;
        p.trim_db = 0.0;
    }
    let mut built = profile_from_library(&raw, &lib.drive_presets);
    let comp = RigLibrary::load_compositions();
    // Each patch with its macro knobs where it keeps them — as it plays.
    for patch in &mut built.patches {
        if let Some(d) = raw.patches.iter().find(|d| d.name.eq_ignore_ascii_case(&patch.name)) {
            crate::macros::apply_positions(d, &comp, patch);
        }
    }

    // Measured on the rig itself (see `crate::measure`), a patch per core.
    let names: Vec<String> = def.patches.iter().map(|p| p.name.clone()).collect();
    let measured = par_map(&names, cores(), |name| {
        built
            .patches
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .and_then(|p| crate::measure::patch_lufs(p, sample_rate))
    });

    let mut out = Vec::with_capacity(def.patches.len());
    for (patch, lufs) in def.patches.iter_mut().zip(measured) {
        if let Some(l) = lufs {
            let target = crate::compose::loudness_target(&comp, &patch.rig_preset, &patch.snapshot);
            // Same clamp as the rig's pass: more than this is a patch built
            // wrong, and the makeup would only amplify noise.
            patch.level_db = (target - l).clamp(-24.0, 24.0);
        }
        out.push(Levelled {
            patch: patch.name.clone(),
            lufs,
            level_db: patch.level_db,
        });
    }
    if !dry_run {
        RigLibrary::save_profile(&def);
    }
    Ok(out)
}

/// One module snapshot or drive option levelled by [`level_modules`].
#[derive(Clone, Debug)]
pub struct ModuleLevel {
    /// `Amp · <preset> · <snapshot>` (with ` · R` for a second amp), or
    /// `Drive · <pedal> · <option>`.
    pub what: String,
    /// Its loudness with no Output Level applied; `None` when it did not
    /// render.
    pub lufs: Option<f32>,
    /// The Output Level written (unchanged when it did not render).
    pub level_db: f32,
}

/// Level the building blocks on their own, before any patch:
///
/// - every **amp** module snapshot, rendered as its amp through its own cab
///   (the amp's Output Level puts every amp at the rig's target loudness);
/// - every **drive** option, rendered alone at drive 0.5 (its Output Level
///   makes engaging it unity: as loud as the guitar going in).
///
/// Measured on the rig's own engine (`crate::measure`), a job per core, and
/// written to `modules.styx` and `drive-presets.styx` unless `dry_run`. The
/// chain builder writes these levels into each block
/// (`nodes::apply_block_levels`), so with them set, presets and patches only
/// balance what is left — the effects around the amp.
#[must_use]
pub fn level_modules(sample_rate: u32, dry_run: bool) -> Vec<ModuleLevel> {
    use signal_proto::BlockType;
    use signal_sampler::rig::RigBlock;
    apply_nam_calibration();
    let lib = RigLibrary::load_or_bootstrap();
    let mut comp = RigLibrary::load_compositions();
    let mut drives = lib.drive_presets.clone();
    let target = signal_sampler::patch_level::TARGET_LUFS as f32;
    let di = signal_sampler::nam_calibrate::DiReference::load_or_synthetic(f64::from(sample_rate));
    let di_lufs = signal_sampler::loudness::integrated_lufs(&di.samples, di.sample_rate) as f32;

    enum Job {
        Amp { m: usize, s: usize, second: bool, nam: String, cab: String },
        Drive { p: usize, o: usize, nam: String },
    }
    let mut jobs = Vec::new();
    for (m, module) in comp.modules.iter().enumerate() {
        if !module.module.eq_ignore_ascii_case("Amp") {
            continue;
        }
        for (s, snap) in module.snapshots.iter().enumerate() {
            if !snap.nam.is_empty() {
                jobs.push(Job::Amp { m, s, second: false, nam: snap.nam.clone(), cab: snap.cab.clone() });
            }
            if !snap.nam2.is_empty() {
                jobs.push(Job::Amp { m, s, second: true, nam: snap.nam2.clone(), cab: snap.cab2.clone() });
            }
        }
    }
    for (p, pedal) in drives.iter().enumerate() {
        for (o, option) in pedal.options.iter().enumerate() {
            if !option.nam.is_empty() {
                jobs.push(Job::Drive { p, o, nam: option.nam.clone() });
            }
        }
    }

    let measured = par_map(&jobs, 0, |job| {
        let chain = match job {
            Job::Amp { nam, cab, .. } => {
                let mut chain = vec![RigBlock::nam(nam.clone()).named("Amp L")];
                if !cab.is_empty() {
                    chain.push(RigBlock::cab_ir(cab.clone()).named("Cab L"));
                }
                chain
            }
            Job::Drive { nam, .. } => {
                vec![RigBlock::of_type(BlockType::Drive).with_nam(nam.clone()).named("Drive")]
            }
        };
        let mut patch = signal_sampler::rig_profile::RigPatch::new("module");
        patch.chain = chain;
        crate::measure::patch_lufs(&patch, sample_rate)
    });

    let mut out = Vec::with_capacity(jobs.len());
    for (job, lufs) in jobs.iter().zip(measured) {
        let lufs = lufs.filter(|l| *l > signal_sampler::loudness::SILENCE_LUFS as f32);
        match job {
            Job::Amp { m, s, second, .. } => {
                let module = &mut comp.modules[*m];
                let what = format!(
                    "Amp · {} · {}{}",
                    module.name,
                    module.snapshots[*s].name,
                    if *second { " · R" } else { "" }
                );
                let snap = &mut module.snapshots[*s];
                let level = if *second { &mut snap.level2_db } else { &mut snap.level_db };
                if let Some(l) = lufs {
                    *level = (target - l).clamp(-40.0, 40.0);
                }
                out.push(ModuleLevel { what, lufs, level_db: *level });
            }
            Job::Drive { p, o, .. } => {
                let pedal = &mut drives[*p];
                let what = format!("Drive · {} · {}", pedal.name, pedal.options[*o].name);
                let option = &mut pedal.options[*o];
                if let Some(l) = lufs {
                    option.level_db = (di_lufs - l).clamp(-40.0, 40.0);
                }
                out.push(ModuleLevel { what, lufs, level_db: option.level_db });
            }
        }
    }
    if !dry_run {
        RigLibrary::save_compositions(&comp);
        RigLibrary::save_drive_presets(&drives);
    }
    out
}

