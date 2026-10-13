//! Core `AudioNode` trait and `ProcessContext`.

// ── ProcessContext ───────────────────────────────────────────────────────────

/// Per-block context passed to every [`AudioNode::process`] call.
///
/// Audio is processed **in-place**: nodes read from the mutable slices,
/// apply their effect, and write the result back. All slices are exactly
/// `block_size` long.
///
/// # Future extension
///
/// Fields for MIDI events and parameter-change events will be added here
/// in Phase 3 (standalone audio I/O) when real-time control is wired up.
pub struct ProcessContext<'a> {
    /// Left channel samples (in-place).
    pub left: &'a mut [f32],
    /// Right channel samples (in-place).
    pub right: &'a mut [f32],
    /// Current sample rate in Hz.
    pub sample_rate: f64,
    /// Number of valid samples in this block (≤ `left.len()`).
    pub block_size: usize,
}

// ── AudioNode ────────────────────────────────────────────────────────────────

/// Unified interface for all audio processing nodes in the graph.
///
/// Implementations include:
/// - Built-in FTS effects via `daw-builtin-fx` (`BuiltinNode<P>`)
/// - External CLAP plugins via `daw-plugin-host` (`ClapNode`) — Phase 6
/// - Test utilities: `GainNode`, `SineNode`, `PassthroughNode`
///
/// # Thread safety
///
/// All nodes must be [`Send`]. The audio graph executes all nodes
/// sequentially on the audio callback thread.
pub trait AudioNode: Send {
    /// Called whenever the sample rate or maximum block size changes.
    ///
    /// Implementations should pre-allocate internal delay lines,
    /// scratch buffers, and recalculate coefficients here.
    fn reset(&mut self, sample_rate: f64, max_block_size: usize);

    /// Process one block of audio in-place.
    ///
    /// The node reads from `ctx.left`/`ctx.right`, applies its effect,
    /// and writes the result back to the same slices.
    fn process(&mut self, ctx: &mut ProcessContext<'_>);

    /// Algorithmic latency introduced by this node, in samples.
    ///
    /// Used by the graph compiler to insert PDC delay lines on shorter
    /// parallel paths (Phase 3+).
    fn latency_samples(&self) -> u32 {
        0
    }

    /// Tail length after input silence, in samples (e.g. reverb decay).
    ///
    /// The graph keeps the node processing until the tail expires even
    /// after the scene graph switches.
    fn tail_samples(&self) -> u32 {
        0
    }
}
