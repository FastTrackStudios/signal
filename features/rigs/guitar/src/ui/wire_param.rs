//! A [`ParamHandle`] backed by the vox link — what lets the processor repo's
//! own FX widgets run in a detached remote.
//!
//! # Why this exists
//!
//! `eq-ui`, `pattern-ui` and the rest of the processor's editors are built on
//! `fts_audio_ui::ParamHandle`: a bundle of closures over one parameter —
//! read it normalized, write it, name it, print it. In a plugin those
//! closures wrap a `nice_plug` `ParamPtr`, which is a pointer into the
//! processor's own memory (`dsp::eq::ui::param_adapter` does that).
//!
//! A remote has no such pointer. The rig is in another process, sometimes on
//! another machine, and the only way to move a parameter is
//! `set_block_param` over the wire. So the closures wrap that instead, and
//! every widget written against `ParamHandle` works unchanged — which is the
//! whole point: the detached GUI is meant to be the same editor, not a
//! reduced one.
//!
//! # The network stays out of the closure
//!
//! `ParamHandle::new` wants `Send + Sync` closures, and a vox client on wasm
//! is `Rc`-backed and neither. So a handle's setter captures no client: it
//! pushes the edit into a sync signal, and [`use_wire_params`] drains that
//! over the wire from inside the component, where the client lives.
//!
//! That indirection is also what makes the handle cheap to build during
//! render — no clone of a client per knob, per frame.
//!
//! # Normalized in, real out
//!
//! A `ParamHandle` deals in 0..=1 because that is what a knob is. The rig's
//! `set_block_param` takes the value in the DSP's own units, and `BlockParam`
//! reports `min`/`max`, so this is where the two meet — the same conversion
//! the domain does at its own boundary, for the same reason.

use crate::ui::param_writer::WriteParam;
use std::sync::Arc;

use crate::proto::rig::RigClient;
use crate::proto::{BlockParam, LiveBlock};
use dioxus::prelude::*;
use dioxus::signals::SyncSignal;
use fts_audio_ui::prelude::ParamHandle;

/// One parameter write, queued for the wire: `(block id, param, real value)`.
type Edit = (String, String, f32);

/// Where a handle sends its edits.
///
/// A closure rather than a concrete queue, for two reasons: `ParamHandle`'s
/// closures must be `Send + Sync` (a vox client on wasm is neither), and a
/// caller that is not a component — a test — can supply somewhere to collect
/// them without a Dioxus runtime to hold a signal in.
pub type EditSink = Arc<dyn Fn(Edit) + Send + Sync>;

/// Start draining parameter edits to the rig, and hand back the queue to
/// bind handles against.
///
/// Call once in a component that has a `RigClient` in context; pass the
/// result to every [`WireParam::bind`] beneath it.
pub fn use_wire_params() -> EditSink {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut queue: SyncSignal<Vec<Edit>> = use_signal_sync(Vec::new);

    // Drain whatever the knobs have queued. An effect rather than a future so
    // it runs on every change, and the client is touched here — inside the
    // component — rather than from a widget's closure.
    use_effect(move || {
        let edits: Vec<Edit> = {
            let mut pending = queue.write();
            if pending.is_empty() {
                return;
            }
            std::mem::take(&mut pending)
        };
        let Some(rig) = rig.clone() else { return };
        spawn(async move {
            for (block, param, value) in edits {
                let _ = rig.write_param(block, param, value).await;
            }
        });
    });

    // The signal is what makes the effect above run: writing to it is a
    // reactive change, which is exactly the wake-up a queued edit needs.
    // `Signal` is `Copy`, so the closure takes its own handle and stays an
    // `Fn` — which is what `ParamHandle` requires.
    Arc::new(move |edit: Edit| {
        let mut queue = queue;
        queue.write().push(edit);
    })
}

/// Everything needed to reach one parameter of one block over the wire.
#[derive(Clone)]
pub struct WireParam {
    sink: EditSink,
    block_id: Arc<str>,
    param: Arc<str>,
    min: f32,
    max: f32,
    /// Where the value currently is, read from the live block. Held rather
    /// than re-read because a `ParamHandle`'s getter is called during render
    /// and must not touch the network.
    value: f32,
    unit: &'static str,
}

impl WireParam {
    /// Bind to a parameter of a live block, if the block reports it.
    ///
    /// `None` when the block has no such parameter — a UI should draw an
    /// inert control then ([`ParamHandle::inert`]) rather than a live one
    /// that writes nowhere.
    #[must_use]
    pub fn bind(sink: EditSink, block: &LiveBlock, param: &str) -> Option<Self> {
        let found: &BlockParam = block.params.iter().find(|p| p.name == param)?;
        Some(Self {
            sink,
            block_id: Arc::from(block.id.as_str()),
            param: Arc::from(param),
            min: found.min,
            max: found.max,
            value: found.value,
            unit: unit_for(param),
        })
    }

    /// The handle a widget takes.
    #[must_use]
    pub fn handle(self) -> ParamHandle {
        let (min, max, value) = (self.min, self.max, self.value);
        let span = max - min;
        let to_normalized = move |real: f32| {
            if span.abs() < f32::EPSILON {
                0.0
            } else {
                ((real - min) / span).clamp(0.0, 1.0)
            }
        };
        let to_real = move |normalized: f32| span.mul_add(normalized.clamp(0.0, 1.0), min);

        let shown = self.param.to_string();
        let unit = self.unit;
        let write = self.clone();

        ParamHandle::new(
            move || to_normalized(value),
            || {},
            move |normalized: f32| write.set(to_real(normalized)),
            || {},
            move || format_value(to_real(to_normalized(value)), unit),
            move || shown.clone(),
            move |text: &str| text.trim().parse::<f32>().ok().map(to_normalized),
        )
        .with_unit(unit)
        .with_bipolar(min < 0.0 && max > 0.0)
        .with_default(to_normalized(if min < 0.0 && max > 0.0 {
            0.0
        } else {
            min
        }))
    }

    /// Queue a value, in the DSP's units. The drain sends it.
    fn set(&self, real: f32) {
        (self.sink)((self.block_id.to_string(), self.param.to_string(), real));
    }
}

/// The unit a parameter reports, by what it is called.
///
/// The wire carries a name, a value and a range but not a unit, and a knob
/// that prints "5500" where it means "5.5 kHz" is a knob a player misreads.
/// The same naming conventions the DSP's own ranges use — see
/// `sampler::native::ranges`.
fn unit_for(param: &str) -> &'static str {
    let ends = |suffix: &str| param == suffix || param.ends_with(&format!("_{suffix}"));
    if ends("freq") || ends("cutoff") || ends("rate") || param.contains("side_") {
        "Hz"
    } else if ends("gain") || ends("thr") || ends("threshold") || ends("range") {
        "dB"
    } else if ends("q") {
        ""
    } else if ends("atk") || ends("rel") || ends("attack") || ends("release") {
        "ms"
    } else {
        ""
    }
}

/// A value as a player reads it: kHz above a kilohertz, one decimal for
/// small numbers, none for large.
fn format_value(value: f32, unit: &str) -> String {
    if unit == "Hz" && value >= 1000.0 {
        return format!("{:.2} kHz", value / 1000.0);
    }
    let decimals = usize::from(value.abs() < 100.0);
    match unit {
        "" => format!("{value:.decimals$}"),
        _ => format!("{value:.decimals$} {unit}"),
    }
}
