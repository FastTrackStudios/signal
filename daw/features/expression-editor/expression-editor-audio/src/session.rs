//! An audio editing session against a DAW take.
//!
//! The audio counterpart of `expression_editor_daw::Session`: that one
//! loads a MIDI take and writes notes back, this one reads an audio
//! item's samples, analyses them, and renders the edits.
//!
//! Depends on the `daw` **facade**, never a backend, so the same
//! session drives a live REAPER item and a standalone project.
//!
//! ## The one structural difference from the MIDI session
//!
//! A MIDI take *is* the document — write it back and you are done. An
//! audio take is a recording that the document *describes*, so the
//! original samples have to be kept for the whole session: every render
//! reads them, and a second edit after a first render must resynthesise
//! from the recording rather than from the last render. Rendering a
//! render is how a vocal turns to glass after three passes.

use daw::service::audio_accessor::{AudioAccessors, GetSamplesRequest};
use daw::service::item::Items;
use daw::service::{ItemRef, ProjectContext, TakeRef};
use expression_editor_core::{Editor, Mode, Viewport};

use crate::analyze::{Analysis, TakeConfig, analyze_take, to_mono};

/// Where a take's audio lives.
///
/// No `PartialEq`: the facade's `ItemRef`/`TakeRef` do not have it, and
/// a location is compared by what it points at rather than by value.
#[derive(Clone, Debug)]
pub struct AudioTakeLocation {
    pub project: ProjectContext,
    pub item: ItemRef,
    pub take: TakeRef,
}

/// A loaded audio take, its analysis, and the editor over it.
pub struct AudioSession {
    pub location: AudioTakeLocation,
    pub editor: Editor,
    /// The recording. Kept for the session's lifetime — see the module
    /// note on why a render must never read a previous render.
    source: Vec<f64>,
    sample_rate: f64,
    /// Analysis paired with `editor.doc`; `commit` reconciles them.
    analysis: Analysis,
    /// The document as loaded, for a dirty check.
    baseline: expression_editor_core::ExpressionDoc,
}

/// How much audio one read pulls, in samples per channel.
///
/// Accessors are read in chunks because a five-minute stereo take at 48
/// kHz is a third of a gigabyte in `f64`, and asking for it in one call
/// makes the host allocate all of it before returning any. 64 k frames
/// is what SneakPeak uses against the same REAPER API, and there is no
/// reason to differ from a number already proven against real takes.
const CHUNK_SAMPLES: u32 = 1 << 16;

impl AudioSession {
    /// Load the first selected item as audio.
    ///
    /// Returns `None` when nothing is selected or the item yields no
    /// samples — an editor opened on nothing looks like a failed load,
    /// and saying so is better than showing an empty roll.
    pub fn load_selected<D: AudioAccessors + Items>(
        daw: &D,
        project: ProjectContext,
        viewport: Viewport,
        cfg: TakeConfig,
    ) -> Option<Self> {
        let item = daw.get_selected_items(project.clone()).into_iter().next()?;
        let location = AudioTakeLocation {
            project,
            item: ItemRef::Guid(item.guid.clone()),
            take: TakeRef::Active,
        };
        Self::load(
            daw,
            location,
            item.length.as_seconds(),
            item.volume,
            viewport,
            cfg,
        )
    }

    /// Load a specific take.
    ///
    /// `volume` is the item's gain, which a take accessor does **not**
    /// apply — it hands back the source audio. Analysing without it
    /// would read a quiet item at the wrong level, which matters for
    /// more than looks: the silence floor separating consonants from
    /// gaps is an absolute threshold, so a fader-down item would have
    /// every frame below it and no sibilants at all.
    pub fn load<D: AudioAccessors>(
        daw: &D,
        location: AudioTakeLocation,
        length_secs: f64,
        volume: f64,
        viewport: Viewport,
        cfg: TakeConfig,
    ) -> Option<Self> {
        let accessor = daw.create_take_accessor(
            location.project.clone(),
            location.item.clone(),
            location.take.clone(),
        )?;
        let read = read_all(daw, &accessor, length_secs);
        daw.destroy_accessor(&accessor);

        let mut source = to_mono(&read.samples, read.channels.max(1) as usize);
        if source.is_empty() {
            return None;
        }
        if volume != 1.0 && volume > 0.0 {
            for s in &mut source {
                *s *= volume;
            }
        }
        let analysis = analyze_take(&source, read.sample_rate, cfg);

        let mut editor = Editor::new(analysis.doc.clone(), viewport);
        // The take is audio, so the surface is the audio one. Setting it
        // here rather than leaving it to the caller means a host cannot
        // load a vocal into the MIDI editor by omission.
        editor.set_mode(Mode::PitchedAudio);
        let baseline = editor.doc.clone();

        Some(Self {
            location,
            editor,
            source,
            sample_rate: read.sample_rate,
            analysis,
            baseline,
        })
    }

    /// Whether the document differs from what was analysed.
    pub fn is_dirty(&self) -> bool {
        self.editor.doc != self.baseline
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// The original recording.
    pub fn source(&self) -> &[f64] {
        &self.source
    }

    /// Reconcile the analysis with the edited document.
    ///
    /// Pitch only. Timing is *not* folded into the analysis, because it
    /// leaves as stretch markers and a render that also applied the
    /// warp would apply the timing twice — once baked in, once by the
    /// markers on top. See [`crate::retime`].
    pub fn commit(&mut self) {
        self.analysis.doc = self.editor.doc.clone();
        crate::apply_to(&self.analysis.doc, &mut self.analysis.pitch);
        self.analysis.pitch.markers.clear();
    }

    /// Where the take sits, for converting document time to marker
    /// coordinates.
    pub fn placement<D: daw::service::Takes + daw::service::item::Items>(
        &self,
        daw: &D,
    ) -> crate::TakePlacement {
        let item = daw.get_item(self.location.project.clone(), self.location.item.clone());
        let takes = daw.get_takes(self.location.project.clone(), self.location.item.clone());
        let take = match &self.location.take {
            TakeRef::Active => takes.iter().find(|t| t.is_active),
            TakeRef::Index(i) => takes.get(*i as usize),
            TakeRef::Guid(g) => takes.iter().find(|t| t.guid == *g),
        };
        crate::TakePlacement {
            item_position: item.map(|i| i.position.as_seconds()).unwrap_or(0.0),
            start_offset: take.map(|t| t.start_offset.as_seconds()).unwrap_or(0.0),
            play_rate: take.map(|t| t.play_rate).unwrap_or(1.0),
            frame_rate: self.analysis.frame_rate,
        }
    }

    /// Render the edited take from the **original** recording.
    ///
    /// Always from `source`, never from a previous render: WORLD is
    /// analysis-resynthesis, and each pass costs a little of the top
    /// end and a little of the transients. Chaining them is how a vocal
    /// ends up sounding like glass after three edits.
    #[cfg(feature = "render")]
    pub fn render(&mut self) -> Vec<f64> {
        self.commit();
        self.analysis.render(&self.source)
    }

    /// Re-analyse the recording, discarding all edits.
    pub fn reanalyze(&mut self, cfg: TakeConfig) {
        self.analysis = analyze_take(&self.source, self.sample_rate, cfg);
        self.editor.doc = self.analysis.doc.clone();
        self.baseline = self.editor.doc.clone();
    }

    /// The analysis, for a host that wants the blobs directly.
    pub fn analysis(&self) -> &Analysis {
        &self.analysis
    }
}

struct Read {
    samples: Vec<f64>,
    sample_rate: f64,
    channels: u32,
}

/// Pull a whole take through an accessor, in chunks.
fn read_all<D: AudioAccessors>(daw: &D, accessor: &str, length_secs: f64) -> Read {
    // One probe read to learn the rate and channel count the host will
    // give us; asking for a rate it does not have would resample, and
    // analysing a resampled take then writing back at the original rate
    // puts every edit slightly out of place.
    let probe = daw.get_samples(GetSamplesRequest {
        accessor_id: accessor.to_string(),
        sample_rate: 0.0,
        num_channels: 0,
        start_time: 0.0,
        num_samples: 1,
    });
    let sample_rate = if probe.sample_rate > 0.0 {
        probe.sample_rate
    } else {
        48_000.0
    };
    let channels = probe.num_channels.max(1);

    let total = (length_secs.max(0.0) * sample_rate).ceil() as u32;
    let mut samples = Vec::with_capacity((total as usize) * channels as usize);
    let mut done = 0u32;
    while done < total {
        let want = CHUNK_SAMPLES.min(total - done);
        let chunk = daw.get_samples(GetSamplesRequest {
            accessor_id: accessor.to_string(),
            sample_rate,
            num_channels: channels,
            start_time: done as f64 / sample_rate,
            num_samples: want,
        });
        if chunk.samples.is_empty() {
            // A host that returns nothing mid-take has hit the end of
            // the source; stopping beats looping forever on an item
            // whose reported length overshoots its audio.
            break;
        }
        samples.extend_from_slice(&chunk.samples);
        done += want;
    }
    Read {
        samples,
        sample_rate,
        channels,
    }
}

/// What a write-back did.
#[derive(Clone, Debug, PartialEq)]
pub enum WriteOutcome {
    /// Nothing had changed.
    Unchanged,
    /// Only timing changed, so the audio was left alone entirely and
    /// the warp went to the host as stretch markers. The good case: no
    /// resynthesis, nothing to undo but a marker list.
    Retimed { markers: usize },
    /// Pitch changed, so the take was resynthesised. Any timing still
    /// went to markers rather than being baked into the render.
    Rendered { path: String, markers: usize },
}

/// Why a write-back could not complete.
#[derive(Clone, Debug, PartialEq)]
pub enum WriteError {
    /// The take no longer reports a source file — it was replaced or
    /// removed underneath the session.
    NoSource,
    /// The rendered audio could not be written next to the original.
    Io(String),
    /// The host refused to repoint the take.
    Host(String),
}

impl core::fmt::Display for WriteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            WriteError::NoSource => write!(f, "the take has no source file"),
            WriteError::Io(e) => write!(f, "writing the rendered take: {e}"),
            WriteError::Host(e) => write!(f, "pointing the take at the render: {e}"),
        }
    }
}

#[cfg(feature = "render")]
impl AudioSession {
    /// Render the edits and repoint the take at the result.
    ///
    /// **A new file beside the original, never over it.** Two reasons,
    /// and both are the difference between a tool you can trust with a
    /// vocal and one you cannot: the recording is the only copy of a
    /// performance, and WORLD is lossy analysis-resynthesis, so
    /// overwriting would make every edit permanent *and* degrade the
    /// source that later edits read from. The original stays on disk
    /// and the take is simply pointed elsewhere, which is undoable by
    /// pointing it back.
    ///
    /// Returns the path written.
    pub fn write_back<D>(&mut self, daw: &D) -> Result<WriteOutcome, WriteError>
    where
        D: daw::service::Takes + daw::service::item::Items + daw::service::StretchMarkers,
    {
        if !self.is_dirty() {
            return Ok(WriteOutcome::Unchanged);
        }
        // Asked before `commit`, which rewrites the blobs to match the
        // document and would make the answer always "no".
        let needs_render = crate::retime::pitch_changed(&self.baseline, &self.editor.doc);
        self.commit();

        // Timing first, and always as markers.
        let placement = self.placement(daw);
        let markers = crate::stretch_markers(&self.analysis.doc, &self.analysis.pitch, placement);
        let location = daw::service::StretchTakeRef::new(
            self.location.project.clone(),
            self.location.item.clone(),
            self.location.take.clone(),
        );
        daw.set_stretch_markers(location, markers.clone())
            .map_err(|e| WriteError::Host(format!("{e:?}")))?;

        // A take whose pitch is untouched is never resynthesised: that
        // would cost a generation of quality for an edit the host can
        // do losslessly.
        if !needs_render {
            self.baseline = self.editor.doc.clone();
            return Ok(WriteOutcome::Retimed {
                markers: markers.len(),
            });
        }

        let source = self.current_source(daw).ok_or(WriteError::NoSource)?;
        let out = next_render_path(&source);
        let audio = self.render();
        write_wav_f32(&out, &audio, self.sample_rate).map_err(|e| WriteError::Io(e.to_string()))?;

        daw.set_source_file(
            self.location.project.clone(),
            self.location.item.clone(),
            self.location.take.clone(),
            out.clone(),
        )
        .map_err(|e| WriteError::Host(format!("{e:?}")))?;

        // The document now describes what is on disk, so a second write
        // with no further edits is correctly a no-op.
        self.baseline = self.editor.doc.clone();
        Ok(WriteOutcome::Rendered {
            path: out,
            markers: markers.len(),
        })
    }
}

impl AudioSession {
    /// The take's source path as the host currently reports it.
    fn current_source<D: daw::service::Takes>(&self, daw: &D) -> Option<String> {
        let takes = daw.get_takes(self.location.project.clone(), self.location.item.clone());
        let index = match &self.location.take {
            TakeRef::Active => takes.iter().position(|t| t.is_active).unwrap_or(0),
            TakeRef::Index(i) => *i as usize,
            TakeRef::Guid(g) => takes.iter().position(|t| t.guid == *g)?,
        };
        takes.get(index)?.source_file_path.clone()
    }
}

/// A render path beside `source` that does not exist yet.
///
/// Numbered rather than timestamped so a directory of them sorts in the
/// order they were made, and so a second edit of the same take is
/// visibly the next version of it rather than an unrelated file.
fn next_render_path(source: &str) -> String {
    let p = std::path::Path::new(source);
    let dir = p.parent().unwrap_or_else(|| std::path::Path::new("."));
    let raw = p.file_stem().and_then(|s| s.to_str()).unwrap_or("take");
    // A second edit reads the take's *current* source, which after the
    // first write is already a render. Without stripping the suffix the
    // names compound — `vox-fts-001-fts-001` and worse — and the chain
    // stops being readable at exactly the point it matters.
    let stem = strip_render_suffix(raw);
    for n in 1..10_000 {
        let candidate = dir.join(format!("{stem}-fts-{n:03}.wav"));
        if !candidate.exists() {
            return candidate.to_string_lossy().into_owned();
        }
    }
    dir.join(format!("{stem}-fts.wav"))
        .to_string_lossy()
        .into_owned()
}

/// Drop a trailing `-fts-NNN`, if there is one.
fn strip_render_suffix(stem: &str) -> &str {
    let Some((head, tail)) = stem.rsplit_once("-fts-") else {
        return stem;
    };
    if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) {
        head
    } else {
        stem
    }
}

/// Write mono 32-bit float WAV via the shared `fts-sample` writer.
///
/// Float rather than 16- or 24-bit int because this is an intermediate
/// a later edit will read back: there is no headroom decision to make,
/// nothing to dither, and a round trip costs nothing.
fn write_wav_f32(path: &str, samples: &[f64], sample_rate: f64) -> std::io::Result<()> {
    let rate = sample_rate.max(1.0) as u32;
    let mono: Vec<f32> = samples.iter().map(|&s| s as f32).collect();
    fts_sample::write_wav_f32(std::path::Path::new(path), rate, &[mono]).map_err(|e| match e {
        fts_sample::SamplerError::Io(e) => e,
        other => std::io::Error::new(std::io::ErrorKind::InvalidData, other.to_string()),
    })
}
