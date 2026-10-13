//! Provider-agnostic parameter handle.
//!
//! Audio widgets read and write a normalized `f32` in `[0.0, 1.0]`. They do
//! not know about nih_plug, vizia, or any specific param system — consumers
//! construct a [`ParamHandle`] from whatever backend they use.
//!
//! Each handle bundles the closures the widgets need:
//! - read the current normalized value
//! - bracket an edit gesture (`begin_edit` / `end_edit`) for host automation
//! - set the normalized value mid-gesture
//! - format the current value for display
//! - parse a user-typed string back to a normalized value
//!
//! Two handles compare equal iff they share the same `id`. Construct fresh
//! handles via [`ParamHandle::new`] (auto-assigns a unique id) so Dioxus's
//! prop memoization works correctly.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

type Getter<T> = Arc<dyn Fn() -> T + Send + Sync>;
type Action = Arc<dyn Fn() + Send + Sync>;
type Setter = Arc<dyn Fn(f32) + Send + Sync>;
type Parser = Arc<dyn Fn(&str) -> Option<f32> + Send + Sync>;

/// A handle that lets audio widgets drive a single parameter.
#[derive(Clone)]
pub struct ParamHandle {
    id: u64,
    pub(crate) normalized: Getter<f32>,
    pub(crate) begin_edit: Action,
    pub(crate) set_normalized: Setter,
    pub(crate) end_edit: Action,
    pub(crate) display_value: Getter<String>,
    pub(crate) name: Getter<String>,
    pub(crate) string_to_normalized: Parser,
    /// Default normalized value, used by widgets to reset on alt-click /
    /// double-click. Defaults to `0.5` (centre) when not provided.
    pub(crate) default_normalized: f32,
    /// Whether this parameter is bipolar (centred at zero, e.g. gain in
    /// dB). Affects visualization — knobs draw the value arc from the
    /// centre detent instead of from the start of the track.
    pub(crate) bipolar: bool,
    /// Number of discrete steps for a stepped (enum / int / bool) parameter;
    /// `None` for a continuous one. Wheel, keyboard and drag snap by it
    /// (`fx.control.stepped`).
    pub(crate) step_count: Option<usize>,
    /// Unit suffix as the parameter reports it (`"dB"`, `"Hz"`, `"ms"`,
    /// `"%"`, `""`). Drives the typed-value conventions in
    /// [`crate::gesture::parse_typed`].
    pub(crate) unit: Arc<str>,
}

impl ParamHandle {
    /// Build a handle from raw closures. Assigns a fresh equality id.
    /// Default normalized is `0.5`, bipolar is `false` — call
    /// [`Self::with_default`] / [`Self::with_bipolar`] to override.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        normalized: impl Fn() -> f32 + Send + Sync + 'static,
        begin_edit: impl Fn() + Send + Sync + 'static,
        set_normalized: impl Fn(f32) + Send + Sync + 'static,
        end_edit: impl Fn() + Send + Sync + 'static,
        display_value: impl Fn() -> String + Send + Sync + 'static,
        name: impl Fn() -> String + Send + Sync + 'static,
        string_to_normalized: impl Fn(&str) -> Option<f32> + Send + Sync + 'static,
    ) -> Self {
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            normalized: Arc::new(normalized),
            begin_edit: Arc::new(begin_edit),
            set_normalized: Arc::new(set_normalized),
            end_edit: Arc::new(end_edit),
            display_value: Arc::new(display_value),
            name: Arc::new(name),
            string_to_normalized: Arc::new(string_to_normalized),
            default_normalized: 0.5,
            bipolar: false,
            step_count: None,
            unit: Arc::from(""),
        }
    }

    /// A handle bound to nothing: reads a fixed position, ignores writes.
    ///
    /// For a control that is on the panel because the unit has it, but has no
    /// parameter behind it yet. Drawing those is deliberate — a faceplate is
    /// the specification for what the DSP still owes — and this is what lets a
    /// widget draw one without a special case for "no handle".
    pub fn inert(name: impl Into<String>, position: f32) -> Self {
        let name = name.into();
        let shown = name.clone();
        let position = position.clamp(0.0, 1.0);
        Self::new(
            move || position,
            || {},
            |_| {},
            || {},
            move || shown.clone(),
            move || name.clone(),
            |_| None,
        )
        .with_default(position)
    }

    /// Set the default normalized value (used for alt-click reset etc.).
    pub fn with_default(mut self, default_normalized: f32) -> Self {
        self.default_normalized = default_normalized.clamp(0.0, 1.0);
        self
    }

    /// Mark this parameter as bipolar (zero-centred). Knobs render the
    /// value arc from the centre detent toward the cursor instead of from
    /// the leftmost track position.
    pub fn with_bipolar(mut self, bipolar: bool) -> Self {
        self.bipolar = bipolar;
        self
    }

    /// Declare the parameter stepped (`Some(n)` steps) or continuous (`None`).
    pub fn with_step_count(mut self, step_count: Option<usize>) -> Self {
        self.step_count = step_count.filter(|n| *n >= 1);
        self
    }

    /// Declare the parameter's unit suffix (`"dB"`, `"Hz"`, …).
    pub fn with_unit(mut self, unit: impl AsRef<str>) -> Self {
        self.unit = Arc::from(unit.as_ref().trim());
        self
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn normalized(&self) -> f32 {
        (self.normalized)()
    }
    pub fn begin_edit(&self) {
        (self.begin_edit)()
    }
    pub fn set_normalized(&self, v: f32) {
        (self.set_normalized)(v.clamp(0.0, 1.0))
    }
    pub fn end_edit(&self) {
        (self.end_edit)()
    }
    pub fn display_value(&self) -> String {
        (self.display_value)()
    }
    pub fn name(&self) -> String {
        (self.name)()
    }
    pub fn string_to_normalized(&self, s: &str) -> Option<f32> {
        (self.string_to_normalized)(s)
    }
    pub fn default_normalized(&self) -> f32 {
        self.default_normalized
    }
    pub fn is_bipolar(&self) -> bool {
        self.bipolar
    }
    pub fn step_count(&self) -> Option<usize> {
        self.step_count
    }
    pub fn unit(&self) -> &str {
        &self.unit
    }

    /// Normalized value one step away in `direction` (+1 / −1). A stepped
    /// parameter moves exactly one step; a continuous one moves by `step`
    /// (a normalized amount).
    pub fn stepped_from(&self, from: f32, direction: f32, step: f32) -> f32 {
        match self.step_count {
            Some(n) if n >= 1 => {
                let n = n as f32;
                let idx = (from * n).round();
                ((idx + direction.signum()) / n).clamp(0.0, 1.0)
            }
            _ => (from + direction * step).clamp(0.0, 1.0),
        }
    }

    /// Set a value as one complete edit gesture (wheel notch, key press,
    /// typed entry).
    pub fn set_as_gesture(&self, v: f32) {
        self.begin_edit();
        self.set_normalized(v);
        self.end_edit();
    }

    /// Reset to the default normalized value, wrapped in begin/end edit so
    /// host automation captures it as a single gesture.
    pub fn reset_to_default(&self) {
        self.begin_edit();
        self.set_normalized(self.default_normalized);
        self.end_edit();
    }
}

impl PartialEq for ParamHandle {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl std::fmt::Debug for ParamHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParamHandle")
            .field("id", &self.id)
            .field("name", &self.name())
            .finish()
    }
}
