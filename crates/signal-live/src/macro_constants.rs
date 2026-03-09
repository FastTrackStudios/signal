//! Configuration constants for the macro system.
//!
//! Centralized configuration for tuning macro system behavior,
//! performance, and limits without code changes.

/// Performance and concurrency configuration.
pub mod performance {
    /// Maximum number of macro bindings per knob.
    /// If exceeded, a warning is logged and binding is skipped.
    /// Typical value: 1-10 bindings per knob.
    pub const MAX_TARGETS_PER_KNOB: usize = 100;

    /// Maximum number of knobs in a macro bank.
    /// Large banks can slow down parameter resolution.
    pub const MAX_KNOBS_PER_BANK: usize = 1000;

    /// Maximum number of records to keep in memory per recording session.
    /// Prevents unbounded memory growth during long recording sessions.
    /// At 3400 records per minute, this allows ~15 minutes of recording.
    pub const MAX_RECORDING_SIZE: usize = 50_000;

    /// Recommended update frequency for smooth parameter modulation.
    /// In Hz. 200 Hz provides smooth visual curves without excessive RPC overhead.
    pub const RECOMMENDED_UPDATE_FREQUENCY_HZ: u32 = 200;

    /// Time window for "simultaneously" setting parameters in parallel.
    /// In milliseconds. All parameters set within this window are batched
    /// in one join_all() for concurrent DAW RPC calls.
    pub const PARALLEL_BATCH_WINDOW_MS: u64 = 5;
}

/// Validation and error handling configuration.
pub mod validation {
    /// Minimum length for knob IDs.
    /// Empty strings are rejected during validation.
    pub const MIN_KNOB_ID_LENGTH: usize = 1;

    /// Maximum length for knob IDs.
    /// Prevents memory issues from pathologically long IDs.
    pub const MAX_KNOB_ID_LENGTH: usize = 256;

    /// Maximum nesting depth for sub-macros.
    /// Prevents infinite recursion or pathological hierarchies.
    pub const MAX_KNOB_NESTING_DEPTH: usize = 10;

    /// Minimum and maximum valid parameter values (normalized).
    /// All parameter mappings should fall within [0.0, 1.0].
    pub const MIN_PARAM_VALUE: f32 = 0.0;
    pub const MAX_PARAM_VALUE: f32 = 1.0;
}

/// Logging and debugging configuration.
pub mod logging {
    /// Log parameter resolution mismatches (missing parameters).
    /// Level: INFO (visible in normal operation).
    pub const LOG_MISSING_PARAMETERS: bool = true;

    /// Log successful parameter resolution.
    /// Level: DEBUG (only in development).
    pub const LOG_RESOLUTION_SUCCESS: bool = false;

    /// Log every macro change (knob move).
    /// Level: TRACE (verbose, only when debugging).
    /// WARNING: Can spam logs at 200+ Hz!
    pub const LOG_MACRO_CHANGES: bool = false;

    /// Log registry statistics on every lookup.
    /// Level: TRACE.
    pub const LOG_REGISTRY_STATS: bool = false;
}

/// Macro system feature flags.
pub mod features {
    /// Enable parallel parameter updates using join_all().
    /// If false, updates are sequential (slower but simpler debugging).
    pub const ENABLE_PARALLEL_UPDATES: bool = true;

    /// Enable automatic recording when macro knobs move.
    /// If false, recording must be explicitly started.
    pub const ENABLE_AUTO_RECORDING: bool = false;

    /// Enable registry stats tracking (counts, averages).
    /// Minimal performance impact but adds memory usage.
    pub const ENABLE_REGISTRY_STATS: bool = true;

    /// Enable validation before parameter resolution.
    /// If false, invalid inputs may cause panics.
    pub const ENABLE_INPUT_VALIDATION: bool = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants_are_reasonable() {
        // Sanity checks for configuration values
        assert!(performance::MAX_KNOBS_PER_BANK > 0);
        assert!(performance::RECOMMENDED_UPDATE_FREQUENCY_HZ > 0);
        assert!(validation::MIN_PARAM_VALUE == 0.0);
        assert!(validation::MAX_PARAM_VALUE == 1.0);
        assert!(validation::MIN_KNOB_ID_LENGTH >= 1);
    }

    #[test]
    fn test_feature_flags_are_boolean() {
        // Just ensure they compile and have expected types
        let _ = features::ENABLE_PARALLEL_UPDATES;
        let _ = features::ENABLE_AUTO_RECORDING;
        let _ = features::ENABLE_REGISTRY_STATS;
        let _ = features::ENABLE_INPUT_VALIDATION;
    }
}
