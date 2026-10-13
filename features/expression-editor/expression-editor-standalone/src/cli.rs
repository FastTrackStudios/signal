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
    /// The command line, or the environment when there is no command
    /// line to read.
    ///
    /// `dx serve` owns argv — it has `--cargo-args` and `--rustc-args`
    /// but nothing that reaches the app — so a served window cannot be
    /// told what to open the usual way. `EXPRESSION_EDITOR_ARGS` is that
    /// channel, holding exactly what would have followed `--`:
    ///
    /// ```sh
    /// EXPRESSION_EDITOR_ARGS='song.rpp --drums --size 1600x900' \
    ///     dx serve -p expression-editor-standalone --example workstation
    /// ```
    ///
    /// Real arguments win, so nothing about running an example directly
    /// changes. Splitting is on whitespace with quotes respected, because
    /// the one argument that matters here is a path and paths have spaces
    /// in them — "set in stone.practice.RPP" being the case in hand.
    pub fn from_env() -> Result<Self, ArgsError> {
        let argv: Vec<String> = std::env::args().skip(1).collect();
        let Ok(from_env) = std::env::var("EXPRESSION_EDITOR_ARGS") else {
            return Self::parse(argv);
        };
        // The environment goes first, so a real argument can still add
        // flags beside it — `dx serve` supplies neither, but `--example
        // stress` passes `--out` and would otherwise lose its project.
        let mut combined = split_args(&from_env);
        combined.extend(argv.iter().cloned());
        match Self::parse(combined) {
            // Both named a source, which is a person being explicit on
            // the command line over a variable they probably forgot was
            // exported. Argv wins.
            Err(ArgsError::Bad(_)) => Self::parse(argv),
            other => other,
        }
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

/// Split a command line held in one string, respecting `'` and `"`.
///
/// Not a shell: no escapes, no expansion, no operators. Just enough that a
/// quoted path with spaces survives, which is the whole reason this exists.
fn split_args(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut has_content = false;
    for c in line.chars() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => current.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                // An empty pair of quotes is still an argument.
                has_content = true;
            }
            (None, c) if c.is_whitespace() => {
                if has_content || !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                    has_content = false;
                }
            }
            (None, _) => current.push(c),
        }
    }
    if has_content || !current.is_empty() {
        out.push(current);
    }
    out
}

fn parse_size(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.split_once(['x', 'X'])?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

#[cfg(test)]
mod arg_string_tests {
    use super::split_args;

    #[test]
    fn a_quoted_path_with_spaces_stays_one_argument() {
        assert_eq!(
            split_args("'/tmp/set in stone.practice.RPP' --drums --size 1600x900"),
            [
                "/tmp/set in stone.practice.RPP",
                "--drums",
                "--size",
                "1600x900",
            ]
        );
        // Double quotes too — a Justfile writes whichever is convenient.
        assert_eq!(
            split_args("\"/tmp/a b.RPP\" --drums"),
            ["/tmp/a b.RPP", "--drums"]
        );
    }

    #[test]
    fn unquoted_words_split_on_any_run_of_whitespace() {
        assert_eq!(split_args("  a   b\tc  "), ["a", "b", "c"]);
        assert_eq!(split_args(""), Vec::<String>::new());
        assert_eq!(split_args("   "), Vec::<String>::new());
    }

    #[test]
    fn an_empty_quoted_argument_survives() {
        // Otherwise `--track ''` would silently eat the flag's value and
        // the next flag would be read as it.
        assert_eq!(split_args("--track '' --drums"), ["--track", "", "--drums"]);
    }
}

#[cfg(test)]
mod env_args_tests {
    use super::{Args, Source};

    /// `EXPRESSION_EDITOR_ARGS` is process-wide, so these run as one test
    /// rather than racing each other over it.
    #[test]
    fn the_environment_supplies_a_project_and_argv_still_adds_flags() {
        let restore = std::env::var("EXPRESSION_EDITOR_ARGS").ok();
        // SAFETY: single-threaded test, and the variable is put back below.
        unsafe {
            std::env::set_var("EXPRESSION_EDITOR_ARGS", "'/tmp/a b.RPP' --size 800x400");
        }

        let args = Args::parse(
            super::split_args("'/tmp/a b.RPP' --size 800x400")
                .into_iter()
                .chain(["--out".into(), "/tmp/out".into()]),
        )
        .expect("environment plus a flag from argv");
        assert!(matches!(&args.source, Source::Rpp(p) if p.to_str() == Some("/tmp/a b.RPP")));
        assert_eq!((args.width, args.height), (800, 400));
        assert_eq!(args.out.as_deref(), Some(std::path::Path::new("/tmp/out")));

        // Two sources is the case `from_env` resolves by dropping the
        // environment; the parser itself must still reject it.
        assert!(
            Args::parse(["/tmp/a.RPP".to_string(), "/tmp/b.RPP".to_string()]).is_err(),
            "two sources must not silently pick one"
        );

        unsafe {
            match restore {
                Some(v) => std::env::set_var("EXPRESSION_EDITOR_ARGS", v),
                None => std::env::remove_var("EXPRESSION_EDITOR_ARGS"),
            }
        }
    }
}
