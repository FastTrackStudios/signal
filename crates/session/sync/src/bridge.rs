//! One peer's engine, kept in step with the shared doc.
//!
//! The bridge remembers the model the engine last showed. A local edit is
//! a difference between that and a fresh read of the engine, and goes into
//! the doc; a remote edit is a difference between that and the doc, and
//! goes into the engine. Either way the remembered model moves to the
//! engine's new state, so the engine never hears its own edit back and the
//! doc never records a remote edit twice.
//!
//! Undo is the doc's too ([`Bridge::undo`]): this peer's own steps,
//! undone in the doc — where another peer's edits since stay as they are —
//! and the engine made to match.

use daw_control::Project;

use crate::diff::{Change, diff};
use crate::doc::{ORIGIN_LOCAL, SessionDoc, Touched, Watch};
use crate::engine::{self, Blobs, Direct, EngineIds, MediaRoot};
use crate::model::SessionModel;

pub struct Bridge {
    doc: SessionDoc,
    project: Project,
    media: MediaRoot,
    /// Where FX state is kept (the project's store).
    blobs: std::sync::Arc<dyn Blobs>,
    /// Which engine FX and item each doc FX and item is, here.
    ids: EngineIds,
    /// The engine's own services, when it runs in this process.
    direct: Option<std::sync::Arc<dyn Direct>>,
    /// What the engine showed after the last reconcile, chart included.
    shown: SessionModel,
    /// This peer's steps, to undo and redo — from when the bridge began,
    /// so opening a song is not a step.
    undo: loro::UndoManager,
    /// The name the next step pushed onto either stack is given.
    naming: std::sync::Arc<std::sync::Mutex<String>>,
    /// What changes to the doc have touched since the engine last matched
    /// it — so a remote edit or an undo reads back only that.
    watch: Watch,
    /// Steps the engine finished since this peer's last recorded one that
    /// changed nothing the doc holds, by name, last on top. Each is undone
    /// first, as nothing: an undo never reaches past a step the person
    /// took to the one before it.
    quiet: Vec<String>,
}

/// An undo manager on `doc` whose steps carry the name `naming` holds
/// when each is pushed.
fn undo_manager(doc: &SessionDoc) -> (loro::UndoManager, std::sync::Arc<std::sync::Mutex<String>>) {
    let naming = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let mut undo = loro::UndoManager::new(doc.loro());
    undo.set_max_undo_steps(UNDO_STEPS);
    let named = std::sync::Arc::clone(&naming);
    undo.set_on_push(Some(Box::new(move |_, _, _| {
        let name = named.lock().map(|n| n.clone()).unwrap_or_default();
        loro::undo::UndoItemMeta {
            value: loro::LoroValue::from(name),
            cursors: Vec::new(),
        }
    })));
    (undo, naming)
}

/// How many steps back a song can be undone.
const UNDO_STEPS: usize = 200;

fn name_of(value: Option<loro::LoroValue>) -> String {
    match value {
        Some(loro::LoroValue::String(name)) => name.to_string(),
        _ => String::new(),
    }
}

impl Bridge {
    /// Start a session from this engine: the engine is the truth, the doc
    /// is written from it. `chart` is the song's chart text.
    ///
    /// # Errors
    /// When the engine cannot be read or the doc cannot be written.
    pub async fn host(
        project: Project,
        media: MediaRoot,
        doc: SessionDoc,
        chart: String,
        blobs: std::sync::Arc<dyn Blobs>,
        direct: Option<std::sync::Arc<dyn Direct>>,
    ) -> eyre::Result<Self> {
        let ids = EngineIds::default();
        let mut shown =
            engine::read(&project, &media, blobs.as_ref(), &ids, direct.as_deref()).await?;
        shown.chart = chart;
        doc.write(&shown, ORIGIN_LOCAL)?;
        let (undo, naming) = undo_manager(&doc);
        let watch = doc.watch();
        let bridge = Self {
            doc,
            project,
            media,
            blobs,
            ids,
            direct,
            shown,
            undo,
            naming,
            watch,
            quiet: Vec::new(),
        };
        // What the engine showed is what the doc now holds.
        bridge.take_touched();
        Ok(bridge)
    }

    /// Join a session: `doc` already holds the host's state (imported
    /// before calling this), and the engine is made to match it —
    /// whatever it had open is replaced, never written into the doc.
    ///
    /// # Errors
    /// When the engine cannot be read or an engine call fails.
    pub async fn join(
        project: Project,
        media: MediaRoot,
        doc: SessionDoc,
        blobs: std::sync::Arc<dyn Blobs>,
        direct: Option<std::sync::Arc<dyn Direct>>,
    ) -> eyre::Result<Self> {
        let ids = EngineIds::default();
        let current =
            engine::read(&project, &media, blobs.as_ref(), &ids, direct.as_deref()).await?;
        let (undo, naming) = undo_manager(&doc);
        let watch = doc.watch();
        let mut bridge = Self {
            doc,
            project,
            media,
            blobs,
            ids,
            direct,
            shown: current,
            undo,
            naming,
            watch,
            quiet: Vec::new(),
        };
        bridge.remote_changed().await?;
        Ok(bridge)
    }

    /// The shared doc — for the sync driver to import into and export from.
    #[must_use]
    pub const fn doc(&self) -> &SessionDoc {
        &self.doc
    }

    /// What the engine showed at the last reconcile.
    #[must_use]
    pub const fn shown(&self) -> &SessionModel {
        &self.shown
    }

    /// The engine may have changed locally: read it, and record any
    /// difference in the doc. Returns whether anything was recorded.
    ///
    /// # Errors
    /// When the engine cannot be read or the doc cannot be written.
    pub async fn local_changed(&mut self) -> eyre::Result<bool> {
        let mut now = engine::read(
            &self.project,
            &self.media,
            self.blobs.as_ref(),
            &self.ids,
            self.direct.as_deref(),
        )
        .await?;
        now.chart.clone_from(&self.shown.chart);
        if now == self.shown {
            return Ok(false);
        }
        self.name_next("Edits");
        self.doc.write(&now, ORIGIN_LOCAL)?;
        self.shown = now;
        self.quiet.clear();
        Ok(true)
    }

    /// The engine finished an edit step named `label`: read it, and record
    /// what changed as one commit of that name — only what differs
    /// written. Returns whether anything was recorded.
    ///
    /// A step that changed nothing the doc holds (the doc holds the whole
    /// project, so: nothing a collaborator or an undo could see) records
    /// no commit — there is nothing to send anyone — but is kept as a
    /// quiet step: the next [`Self::undo`] undoes it, as nothing, rather
    /// than the step before it.
    ///
    /// # Errors
    /// When the engine cannot be read or the doc cannot be written.
    pub async fn local_step(&mut self, label: &str) -> eyre::Result<bool> {
        let mut now = engine::read(
            &self.project,
            &self.media,
            self.blobs.as_ref(),
            &self.ids,
            self.direct.as_deref(),
        )
        .await?;
        now.chart.clone_from(&self.shown.chart);
        if now == self.shown {
            if self.quiet.len() >= UNDO_STEPS {
                self.quiet.remove(0);
            }
            self.quiet.push(label.to_owned());
            return Ok(false);
        }
        self.name_next(label);
        self.doc
            .write_step(&self.shown, &now, ORIGIN_LOCAL, label)?;
        self.shown = now;
        self.quiet.clear();
        Ok(true)
    }

    /// Undo this peer's last step: in the doc, then in the engine. Returns
    /// its name, or `None` with nothing to undo.
    ///
    /// # Errors
    /// When the doc cannot undo or the engine cannot be made to match.
    pub async fn undo(&mut self) -> eyre::Result<Option<String>> {
        // A step that changed nothing is undone as nothing.
        if let Some(name) = self.quiet.pop() {
            return Ok(Some(name));
        }
        if !self.undo.can_undo() {
            return Ok(None);
        }
        let name = name_of(self.undo.top_undo_value());
        // Its redo goes by the same name.
        self.name_next(&name);
        self.undo.undo()?;
        self.remote_changed().await?;
        Ok(Some(name))
    }

    /// Redo the step last undone: in the doc, then in the engine. Returns
    /// its name, or `None` with nothing to redo.
    ///
    /// # Errors
    /// When the doc cannot redo or the engine cannot be made to match.
    pub async fn redo(&mut self) -> eyre::Result<Option<String>> {
        if !self.undo.can_redo() {
            return Ok(None);
        }
        let name = name_of(self.undo.top_redo_value());
        self.name_next(&name);
        self.undo.redo()?;
        self.remote_changed().await?;
        Ok(Some(name))
    }

    /// The name of the step that would be undone next, if any.
    #[must_use]
    pub fn next_undo(&self) -> Option<String> {
        self.quiet.last().cloned().or_else(|| {
            self.undo
                .can_undo()
                .then(|| name_of(self.undo.top_undo_value()))
        })
    }

    /// The name of the step that would be redone next, if any.
    #[must_use]
    pub fn next_redo(&self) -> Option<String> {
        self.undo
            .can_redo()
            .then(|| name_of(self.undo.top_redo_value()))
    }

    fn name_next(&self, name: &str) {
        if let Ok(mut naming) = self.naming.lock() {
            name.clone_into(&mut naming);
        }
    }

    /// The chart was edited locally.
    ///
    /// # Errors
    /// When the doc cannot be written.
    pub fn chart_changed(&mut self, chart: &str) -> eyre::Result<()> {
        if self.shown.chart == chart {
            return Ok(());
        }
        self.shown.chart = chart.to_string();
        self.name_next("Edit chart");
        self.doc.write(&self.shown, ORIGIN_LOCAL)?;
        self.quiet.clear();
        Ok(())
    }

    /// The doc changed remotely: make the engine match it. Returns the
    /// steps taken — a caller redraws from them, and a `ChartChanged`
    /// among them is the chart editor's to show.
    ///
    /// # Errors
    /// When an engine call fails. The engine is re-read either way, so a
    /// half-applied change is reconciled on the next call.
    pub async fn remote_changed(&mut self) -> eyre::Result<Vec<Change>> {
        let touched = self.take_touched();
        if touched.is_empty() {
            return Ok(Vec::new());
        }
        let target = self.doc.read_touched(&self.shown, &touched);
        let changes = diff(&self.shown, &target);
        if changes.is_empty() {
            return Ok(changes);
        }
        let applied = engine::apply(
            &self.project,
            &self.media,
            &changes,
            &target,
            self.blobs.as_ref(),
            &mut self.ids,
            self.direct.as_deref(),
        )
        .await;
        // The tracks read again only if a change could have touched them.
        let keep =
            (!touched.everything && !touched.parts.tracks).then(|| self.shown.tracks.clone());
        let mut now = engine::read_keeping(
            &self.project,
            &self.media,
            self.blobs.as_ref(),
            &self.ids,
            self.direct.as_deref(),
            keep,
        )
        .await?;
        now.chart = target.chart;
        self.shown = now;
        applied?;
        Ok(changes)
    }

    /// What the doc's changes have touched since last asked, cleared.
    fn take_touched(&self) -> Touched {
        self.watch
            .touched
            .lock()
            .map_or_else(|_| Touched::everything(), |mut t| std::mem::take(&mut *t))
    }
}
