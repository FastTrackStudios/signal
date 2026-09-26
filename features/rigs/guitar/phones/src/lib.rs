//! The headphone mixer — the incoming monitor mix, played into the phones by
//! a process of its own.
//!
//! A player's in-ear mix (a band mix without their guitar, from the desk)
//! comes into two of the interface's inputs. The rig could blend it into the
//! phones itself, but then everything the rig does to its own audio thread —
//! an overrun on a heavy patch, a rebuild, a crash — happens to the mix too,
//! and the player loses the band. So the mix plays from here instead:
//! `signal-phones`, a small process that opens the same device, reads the mix
//! pair and writes it to the phones pair. The rig writes only the player's
//! own guitar to those outputs, and the device sums the two (CoreAudio mixes
//! every client of a device; each client's IO thread is its own, so one that
//! misses its deadline or dies drops only its own share).
//!
//! The rig still runs it: levels, channels and on/off go through a small
//! memory-mapped file of atomics ([`PhonesLink`]) that the mixer's audio
//! callback reads lock-free every block, and that the mixer writes its state
//! back into (streaming or not, its meters, the device's rate and block). The
//! file outlives both processes, so a mixer started with no rig — after a
//! crash, or before the app is up — plays at the last levels.
//!
//! One mixer at a time: it holds an `flock` on `<state>.lock` for as long as
//! it lives, which is also how the rig tells whether one is running (the
//! kernel drops the lock the moment a process dies, crash or not).

use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};

use memmap2::MmapMut;

/// `SPHN`.
const MAGIC: u32 = 0x5350_484E;
const VERSION: u32 = 1;
/// Bytes of device name the file carries.
const DEVICE_BYTES: usize = 128;

/// What the mixer is doing (`Shared::state`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixerState {
    /// Not started, or between devices.
    Idle,
    /// Playing the mix.
    Streaming,
    /// The device is not there (unplugged, or not yet up); retrying.
    NoDevice,
}

impl MixerState {
    fn from_u32(v: u32) -> Self {
        match v {
            1 => Self::Streaming,
            2 => Self::NoDevice,
            _ => Self::Idle,
        }
    }
    const fn to_u32(self) -> u32 {
        match self {
            Self::Idle => 0,
            Self::Streaming => 1,
            Self::NoDevice => 2,
        }
    }
}

/// The file's layout: plain atomics, so both processes read and write it
/// without a lock. `f32`s travel as their bits.
#[repr(C)]
pub struct Shared {
    magic: AtomicU32,
    version: AtomicU32,

    // ── The rig's side ────────────────────────────────────────────────────
    /// Play the mix (0 = the mixer quits).
    enabled: AtomicU32,
    /// The incoming mix's level, a fader position (see [`fader_gain`]).
    mix_level: AtomicU32,
    /// The phones' overall level, a fader position.
    volume: AtomicU32,
    /// 0-based input channels of the mix (the same twice = a mono mix, in
    /// both ears).
    mix_in_l: AtomicU32,
    mix_in_r: AtomicU32,
    /// 0-based output channels of the phones.
    out_l: AtomicU32,
    out_r: AtomicU32,
    /// Bumped after the device or a channel changes; the mixer reopens.
    config_seq: AtomicU32,
    /// The device, by name substring (UTF-8, NUL-padded).
    device: [AtomicU64; DEVICE_BYTES / 8],

    // ── The mixer's side ──────────────────────────────────────────────────
    pid: AtomicU32,
    /// [`MixerState`].
    state: AtomicU32,
    /// Blocks played — advancing means audio is flowing.
    heartbeat: AtomicU32,
    /// The incoming mix's meter (peak, falling ~20 dB/s), before its
    /// fader, linear.
    peak_l: AtomicU32,
    peak_r: AtomicU32,
    /// The device's rate and the mixer's block, once streaming.
    rate: AtomicU32,
    block: AtomicU32,
}

/// A fader position (0..=1) as a gain: silent at 0, unity at 0.75, +12 dB at
/// the top, 48 dB across the travel — how a console fader reads. The rig's
/// guitar-in-the-phones and the mixer's mix both use it, so the two faders
/// beside each other mean the same thing.
#[must_use]
pub fn fader_gain(pos: f32) -> f32 {
    if pos <= 0.001 {
        return 0.0;
    }
    10f32.powf(fader_db(pos) / 20.0)
}

/// A fader position's gain in dB (−∞ below the bottom stop).
#[must_use]
pub fn fader_db(pos: f32) -> f32 {
    if pos <= 0.001 {
        return f32::NEG_INFINITY;
    }
    (pos.clamp(0.0, 1.0) - 0.75) * 48.0
}

/// Where unity sits on a fader.
pub const UNITY: f32 = 0.75;

/// The mix's routing, as the rig sets it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MixerConfig {
    /// The device, by name substring (empty = the system default).
    pub device: String,
    /// 0-based input channels of the mix (equal = mono, in both ears).
    pub mix_in: (u32, u32),
    /// 0-based output channels of the phones.
    pub out: (u32, u32),
}

/// What the mixer reports.
#[derive(Clone, Debug, PartialEq)]
pub struct MixerStatus {
    /// A mixer process holds the lock.
    pub running: bool,
    pub state: MixerState,
    pub pid: u32,
    /// Blocks played so far (compare two reads to see audio flowing).
    pub heartbeat: u32,
    /// The incoming mix's meter, dBFS (−90 = silence).
    pub peak_db: (f32, f32),
    pub rate: u32,
    pub block: u32,
}

/// A handle on the shared file.
pub struct PhonesLink {
    map: MmapMut,
    path: PathBuf,
}

// SAFETY: every access goes through the atomics in `Shared`.
unsafe impl Send for PhonesLink {}
unsafe impl Sync for PhonesLink {}

impl PhonesLink {
    /// Open (creating, with defaults) the file at `path`.
    pub fn open(path: &Path) -> io::Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let f = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path)?;
        let want = std::mem::size_of::<Shared>().next_multiple_of(4096) as u64;
        let fresh = f.metadata()?.len() < want;
        if fresh {
            f.set_len(want)?;
        }
        // SAFETY: the file is ours and sized; every access is atomic.
        let map = unsafe { MmapMut::map_mut(&f)? };
        let link = Self { map, path: path.to_path_buf() };
        let s = link.shared();
        if fresh || s.magic.load(Relaxed) != MAGIC || s.version.load(Relaxed) != VERSION {
            link.reset();
        }
        Ok(link)
    }

    fn reset(&self) {
        let s = self.shared();
        s.enabled.store(0, Relaxed);
        s.mix_level.store(UNITY.to_bits(), Relaxed);
        s.volume.store(UNITY.to_bits(), Relaxed);
        s.mix_in_l.store(2, Relaxed);
        s.mix_in_r.store(3, Relaxed);
        s.out_l.store(0, Relaxed);
        s.out_r.store(1, Relaxed);
        s.config_seq.store(0, Relaxed);
        for w in &s.device {
            w.store(0, Relaxed);
        }
        s.state.store(0, Relaxed);
        s.version.store(VERSION, Relaxed);
        s.magic.store(MAGIC, Relaxed);
    }

    /// The file's atomics.
    #[must_use]
    pub fn shared(&self) -> &Shared {
        // SAFETY: page-aligned, at least `size_of::<Shared>()` long, and
        // `Shared` is only atomics (valid for any bit pattern).
        unsafe { &*self.map.as_ptr().cast::<Shared>() }
    }

    /// The file's path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    // ── The rig's side ────────────────────────────────────────────────────

    /// Set the mix's and the phones' fader positions (0..=1).
    pub fn set_levels(&self, mix_level: f32, volume: f32) {
        let s = self.shared();
        s.mix_level.store(mix_level.clamp(0.0, 1.0).to_bits(), Relaxed);
        s.volume.store(volume.clamp(0.0, 1.0).to_bits(), Relaxed);
    }

    /// Play the mix, or not (off = the mixer quits and frees the device).
    pub fn set_enabled(&self, on: bool) {
        self.shared().enabled.store(u32::from(on), Relaxed);
    }

    #[must_use]
    pub fn enabled(&self) -> bool {
        self.shared().enabled.load(Relaxed) != 0
    }

    /// Route the mix. A change makes the mixer reopen the device.
    pub fn configure(&self, cfg: &MixerConfig) {
        if self.config() == *cfg {
            return;
        }
        let s = self.shared();
        let mut bytes = [0u8; DEVICE_BYTES];
        let name = cfg.device.as_bytes();
        let n = name.len().min(DEVICE_BYTES - 1);
        bytes[..n].copy_from_slice(&name[..n]);
        for (i, w) in s.device.iter().enumerate() {
            let mut b = [0u8; 8];
            b.copy_from_slice(&bytes[i * 8..i * 8 + 8]);
            w.store(u64::from_le_bytes(b), Relaxed);
        }
        s.mix_in_l.store(cfg.mix_in.0, Relaxed);
        s.mix_in_r.store(cfg.mix_in.1, Relaxed);
        s.out_l.store(cfg.out.0, Relaxed);
        s.out_r.store(cfg.out.1, Relaxed);
        s.config_seq.fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    /// The routing as the file holds it.
    #[must_use]
    pub fn config(&self) -> MixerConfig {
        let s = self.shared();
        let mut bytes = Vec::with_capacity(DEVICE_BYTES);
        for w in &s.device {
            bytes.extend_from_slice(&w.load(Relaxed).to_le_bytes());
        }
        let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
        MixerConfig {
            device: String::from_utf8_lossy(&bytes[..end]).into_owned(),
            mix_in: (s.mix_in_l.load(Relaxed), s.mix_in_r.load(Relaxed)),
            out: (s.out_l.load(Relaxed), s.out_r.load(Relaxed)),
        }
    }

    /// The incoming mix's meter, dBFS (−90 = silence) — any number of
    /// readers, none of them taking it from another.
    #[must_use]
    pub fn meter_db(&self) -> (f32, f32) {
        let s = self.shared();
        (db(s.peak_l.load(Relaxed)), db(s.peak_r.load(Relaxed)))
    }

    /// What the mixer reports.
    #[must_use]
    pub fn status(&self) -> MixerStatus {
        let s = self.shared();
        let running = is_running(&self.path);
        MixerStatus {
            running,
            state: if running { MixerState::from_u32(s.state.load(Relaxed)) } else { MixerState::Idle },
            pid: s.pid.load(Relaxed),
            heartbeat: s.heartbeat.load(Relaxed),
            peak_db: self.meter_db(),
            rate: s.rate.load(Relaxed),
            block: s.block.load(Relaxed),
        }
    }

    // ── The mixer's side ──────────────────────────────────────────────────

    /// The config generation (a change means reopen).
    #[must_use]
    pub fn config_seq(&self) -> u32 {
        self.shared().config_seq.load(std::sync::atomic::Ordering::Acquire)
    }

    /// The fader positions `(mix, volume)`.
    #[must_use]
    pub fn levels(&self) -> (f32, f32) {
        let s = self.shared();
        (f32::from_bits(s.mix_level.load(Relaxed)), f32::from_bits(s.volume.load(Relaxed)))
    }

    /// Report the mixer's state.
    pub fn report(&self, state: MixerState, rate: u32, block: u32) {
        let s = self.shared();
        s.pid.store(std::process::id(), Relaxed);
        s.rate.store(rate, Relaxed);
        s.block.store(block, Relaxed);
        s.state.store(state.to_u32(), Relaxed);
    }

    /// The device's rate as the mixer reported it (0 before it streams).
    #[must_use]
    pub fn shared_rate(&self) -> u32 {
        self.shared().rate.load(Relaxed)
    }

    /// Blocks played so far.
    #[must_use]
    pub fn shared_heartbeat(&self) -> u32 {
        self.shared().heartbeat.load(Relaxed)
    }

    /// One block played, with the mix's meter (linear) — realtime-safe.
    pub fn beat(&self, meter_l: f32, meter_r: f32) {
        let s = self.shared();
        s.heartbeat.fetch_add(1, Relaxed);
        s.peak_l.store(meter_l.to_bits(), Relaxed);
        s.peak_r.store(meter_r.to_bits(), Relaxed);
    }
}

/// A linear level in dBFS, −90 for silence.
fn db(bits: u32) -> f32 {
    let v = f32::from_bits(bits);
    if v > 3.2e-5 { 20.0 * v.log10() } else { -90.0 }
}

/// A meter's fall per block: ~20 dB a second at `rate` and `frames`.
#[must_use]
pub fn meter_fall(rate: u32, frames: usize) -> f32 {
    let secs = frames as f32 / rate.max(1) as f32;
    10f32.powf(-20.0 * secs / 20.0)
}

/// The lock file beside the state file.
fn lock_path(state: &Path) -> PathBuf {
    let mut p = state.as_os_str().to_owned();
    p.push(".lock");
    PathBuf::from(p)
}

/// The mixer's hold on being the one running: dropped when it exits.
pub struct Instance {
    _file: File,
}

/// Become the running mixer — `None` when another one already is.
pub fn claim(state: &Path) -> io::Result<Option<Instance>> {
    let f = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(lock_path(state))?;
    // SAFETY: a valid fd we own.
    let r = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    Ok((r == 0).then_some(Instance { _file: f }))
}

/// Whether a mixer is running on `state` (some process holds its lock).
#[must_use]
pub fn is_running(state: &Path) -> bool {
    let Ok(f) = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(lock_path(state)) else {
        return false;
    };
    // SAFETY: a valid fd we own; a shared probe lock, released on close.
    let r = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) };
    r != 0
}

/// Where the rig and the mixer meet: `SIGNAL_PHONES_STATE`, else
/// `<config>/signal/rigs/phones-mixer.state` (`XDG_CONFIG_HOME`, else
/// `~/.config`).
#[must_use]
pub fn default_state_path() -> PathBuf {
    if let Some(p) = std::env::var_os("SIGNAL_PHONES_STATE").filter(|p| !p.is_empty()) {
        return p.into();
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("signal").join("rigs").join("phones-mixer.state")
}

/// The `signal-phones` binary: `SIGNAL_PHONES_BIN`, else beside this
/// executable (an app bundle's `MacOS/`, or `target/<profile>/`).
#[must_use]
pub fn find_binary() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("SIGNAL_PHONES_BIN").filter(|p| !p.is_empty()) {
        return Some(p.into());
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let here = dir.join("signal-phones");
    if here.is_file() {
        return Some(here);
    }
    // An example or test binary one level down (`target/<profile>/examples`).
    let up = dir.parent()?.join("signal-phones");
    up.is_file().then_some(up)
}

/// Start a mixer on `state`, detached — its own session, so it outlives the
/// process that started it. Its output goes to `log`.
pub fn spawn(bin: &Path, state: &Path, log: &Path) -> io::Result<u32> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let out = OpenOptions::new().create(true).append(true).open(log)?;
    let err = out.try_clone()?;
    let mut cmd = Command::new(bin);
    cmd.arg("--state").arg(state).stdin(Stdio::null()).stdout(out).stderr(err);
    // SAFETY: setsid is async-signal-safe.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = cmd.spawn()?;
    let pid = child.id();
    // Reap it whenever it exits, so it does not linger as a zombie while
    // this process lives.
    std::thread::Builder::new()
        .name("phones-mixer-reap".into())
        .spawn(move || {
            let _ = child.wait();
        })?;
    Ok(pid)
}

/// The mixer's log: `/Volumes/dev-drive/logs/phones-mixer.log` when that
/// drive is there, else the temp dir.
#[must_use]
pub fn default_log_path() -> PathBuf {
    let drive = Path::new("/Volumes/dev-drive/logs");
    if drive.is_dir() {
        return drive.join("phones-mixer.log");
    }
    std::env::temp_dir().join("signal-phones-mixer.log")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fader_reads_like_a_console() {
        assert_eq!(fader_gain(0.0), 0.0);
        assert!((fader_gain(UNITY) - 1.0).abs() < 1e-6);
        assert!((fader_db(1.0) - 12.0).abs() < 1e-4);
        assert!((fader_db(0.5) + 12.0).abs() < 1e-4);
    }

    /// The rig's writes are what the mixer reads, through the file.
    #[test]
    fn the_rig_and_the_mixer_meet_in_the_file() {
        let dir = std::env::temp_dir().join(format!("phones-{}", std::process::id()));
        let path = dir.join("m.state");
        let rig = PhonesLink::open(&path).unwrap();
        let mixer = PhonesLink::open(&path).unwrap();
        let seq = mixer.config_seq();
        rig.configure(&MixerConfig { device: "MiniFuse".into(), mix_in: (2, 3), out: (0, 1) });
        assert_ne!(mixer.config_seq(), seq, "a routing change bumps the generation");
        assert_eq!(mixer.config().device, "MiniFuse");
        let again = mixer.config_seq();
        rig.configure(&MixerConfig { device: "MiniFuse".into(), mix_in: (2, 3), out: (0, 1) });
        assert_eq!(mixer.config_seq(), again, "the same routing does not reopen the device");
        rig.set_levels(0.5, 0.9);
        assert_eq!(mixer.levels(), (0.5, 0.9));
        mixer.beat(0.5, 0.25);
        assert!((rig.status().peak_db.0 + 6.02).abs() < 0.1);
        assert!((rig.meter_db().0 + 6.02).abs() < 0.1, "reading the meter does not take it");

        assert!(!is_running(&path));
        let held = claim(&path).unwrap().expect("first claim");
        assert!(is_running(&path));
        assert!(claim(&path).unwrap().is_none(), "one mixer at a time");
        drop(held);
        assert!(!is_running(&path));
        let _ = std::fs::remove_dir_all(dir);
    }
}
