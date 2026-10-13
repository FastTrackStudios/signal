//! The runner's command line.
//!
//! Hand-parsed rather than derived, because both examples share it and
//! a `clap` dependency in the library would be paid for by every
//! consumer of the loading half. It is a dozen flags; the derive earns
//! nothing here.

use std::path::PathBuf;

use expression_editor_core::{Mode, Viewport};

use crate::{LoadError, Source, Target, scene_names};

/// Everything the runner takes.
#[derive(Clone, Debug, PartialEq)]
pub struct Args {
    pub source: Source,
    pub target: Target,
    /// Override the mode the source implies — how the screenshot
    /// harness reaches all seven surfaces from one document.
    pub mode: Option<Mode>,
    pub width: u32,
    pub height: u32,
    /// Where `--example shot` writes its PNG.
    pub out: Option<PathBuf>,
}

/// Window size. Roughly a plugin editor, which is the geometry the
/// canvas was laid out against.
pub const DEFAULT_WIDTH: u32 = 1200;
pub const DEFAULT_HEIGHT: u32 = 760;

/// What `--help` prints.
pub const USAGE: &str = "\
The expression editor, standalone.

USAGE:
    cargo run -p expression-editor-standalone --example editor -- [SOURCE] [OPTIONS]

SOURCE:
    <scene>          a demo scene name (default: phrase; --list to see them all)
    <path>.rpp       a REAPER project — its first editable item, or --track/--item
    <path>.mid       a standard MIDI file
    <path>.gp        a Guitar Pro transcription (gp3/4/5, gpx, gp)

OPTIONS:
    --track <name|N> pick a track by name (substring) or 1-based index
    --item <N>       pick the Nth item (0-based) among the candidates
    --drums [folder] open a .rpp as a drum workspace: every track under the
                     kit folder, folded into role lanes. The optional value
                     names the folder (put it after the source); without it
                     the first folder named like a kit (Drums, Kit) is used
    --mode <mode>    midi | mpe | vocals | drums | guitar | pitched-audio | unpitched-audio
    --size <WxH>     window size (default 1200x760)
    --out <path>     PNG destination (--example shot only)
    --list           list the demo scenes and exit
    --help           this
";

/// Parse failed, or the user asked for something that is not a run.
#[derive(Debug)]
pub enum ArgsError {
    /// Print [`USAGE`] and exit successfully.
    Help,
    /// Print the scene list and exit successfully.
    List,
    Bad(String),
    Load(LoadError),
}

impl std::fmt::Display for ArgsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArgsError::Help => f.write_str(USAGE),
            ArgsError::List => {
                for (name, label) in scene_names() {
                    writeln!(f, "  {name:<16} {label}")?;
                }
                Ok(())
            }
            ArgsError::Bad(m) => write!(f, "{m}"),
            ArgsError::Load(e) => write!(f, "{e}"),
        }
    }
}

impl Args {
    /// Parse the process arguments, skipping `argv[0]`.
    pub fn from_env() -> Result<Self, ArgsError> {
        Self::parse(std::env::args().skip(1))
    }

    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, ArgsError> {
        let mut positional: Option<String> = None;
        let mut target = Target::default();
        let mut mode = None;
        let mut width = DEFAULT_WIDTH;
        let mut height = DEFAULT_HEIGHT;
        let mut out = None;

        let mut it = args.into_iter().peekable();
        while let Some(arg) = it.next() {
            let mut value = |flag: &str| -> Result<String, ArgsError> {
                it.next()
                    .ok_or_else(|| ArgsError::Bad(format!("{flag} needs a value")))
            };
            match arg.as_str() {
                "--help" | "-h" => return Err(ArgsError::Help),
                "--list" => return Err(ArgsError::List),
                "--track" => target.track = Some(value("--track")?),
                "--item" => {
                    let v = value("--item")?;
                    target.item =
                        Some(v.parse().map_err(|_| {
                            ArgsError::Bad(format!("--item {v:?} is not a number"))
                        })?);
                }
                "--mode" => {
                    let v = value("--mode")?;
                    mode = Some(parse_mode(&v).ok_or_else(|| {
                        ArgsError::Bad(format!(
                            "--mode {v:?} is not one of: midi, mpe, vocals, drums, \
                             guitar, pitched-audio, unpitched-audio"
                        ))
                    })?);
                }
                "--size" => {
                    let v = value("--size")?;
                    let (w, h) = parse_size(&v).ok_or_else(|| {
                        ArgsError::Bad(format!("--size {v:?} is not WxH, e.g. 1200x760"))
                    })?;
                    width = w;
                    height = h;
                }
                // r[impl drums.open.runner]
                "--drums" => {
                    // The value is optional: bare `--drums` means "find
                    // the kit folder yourself". A following token is the
                    // folder name only when the source has already been
                    // given — otherwise `--drums song.rpp` would eat the
                    // source as a folder name, which is the likelier typo.
                    let folder = match it.peek() {
                        Some(next) if !next.starts_with('-') && positional.is_some() => {
                            Some(it.next().expect("peeked"))
                        }
                        _ => None,
                    };
                    target.drums = Some(folder);
                }
                "--out" => out = Some(PathBuf::from(value("--out")?)),
                other if other.starts_with('-') => {
                    return Err(ArgsError::Bad(format!("unknown flag {other:?}")));
                }
                other => {
                    if positional.is_some() {
                        return Err(ArgsError::Bad(format!(
                            "expected one source, also got {other:?}"
                        )));
                    }
                    positional = Some(other.to_string());
                }
            }
        }

        let source = match positional {
            // No source at all is the commonest run: someone wants to
            // see the editor. Opening on a scene beats printing usage.
            None => Source::parse("phrase").expect("`phrase` is a scene"),
            Some(arg) => Source::parse(&arg).map_err(ArgsError::Load)?,
        };

        Ok(Args {
            source,
            target,
            mode,
            width,
            height,
            out,
        })
    }

    /// The viewport the document is laid out against.
    ///
    /// The UI crate owns its chrome arithmetic — asking it keeps the
    /// runner honest when a bar is added or removed. The old local
    /// constant here (224 px) was measured against a two-row toolbar
    /// that no longer exists, and every headless shot carried ~120 px
    /// of dead canvas at the bottom because of it.
    pub fn viewport(&self) -> Viewport {
        expression_editor_ui::viewport_in(self.width as f64, self.height as f64)
    }
}

fn parse_mode(s: &str) -> Option<Mode> {
    match s.trim().to_ascii_lowercase().replace('_', "-").as_str() {
        "midi" => Some(Mode::Midi),
        "mpe" => Some(Mode::Mpe),
        "vocals" | "vocal" => Some(Mode::Vocals),
        "drums" | "drum" => Some(Mode::Drums),
        "guitar" => Some(Mode::Guitar),
        "pitched-audio" | "pitched" | "audio" => Some(Mode::PitchedAudio),
        "unpitched-audio" | "unpitched" | "percussive" => Some(Mode::UnpitchedAudio),
        _ => None,
    }
}

fn parse_size(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.split_once(['x', 'X'])?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}
