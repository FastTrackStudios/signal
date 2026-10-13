//! Errors detected before changing configuration or processing state.

/// EQ configuration and processing errors. No heap storage is required.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidSampleRate,
    InvalidBlockSize,
    InvalidFrequency,
    InvalidQ,
    InvalidGain,
    InvalidSlope,
    UnsupportedFilter,
    InvalidDynamics,
    UnsupportedRouting,
    InvalidCoefficients,
    UnstableFilter,
    TooManySections,
    CapacityExceeded,
    UnknownBand,
    ChannelLengthMismatch,
    BlockTooLarge,
    IncompatiblePreparation,
    NotPrepared,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::InvalidSampleRate => "sample rate must be finite and positive",
            Self::InvalidBlockSize => "maximum block size must be nonzero",
            Self::InvalidFrequency => "frequency must be finite, positive and below Nyquist",
            Self::InvalidQ => "Q must be finite and positive",
            Self::InvalidGain => "gain must be finite",
            Self::UnsupportedFilter => "filter shape has no supported implementation",
            Self::InvalidSlope => "unsupported slope for this filter shape",
            Self::UnsupportedRouting => {
                "this dynamics mode does not support the requested stream or placement"
            }
            Self::InvalidDynamics => "invalid dynamics settings",
            Self::InvalidCoefficients => "coefficients must be finite with nonzero a0",
            Self::UnstableFilter => "filter poles must be strictly inside the unit circle",
            Self::TooManySections => "filter exceeds prepared section capacity",
            Self::CapacityExceeded => "EQ band capacity exceeded",
            Self::UnknownBand => "band ID is absent or has been removed",
            Self::ChannelLengthMismatch => "stereo channels must have equal lengths",
            Self::BlockTooLarge => "block exceeds prepared capacity",
            Self::IncompatiblePreparation => "update requires a different processing specification",
            Self::NotPrepared => "processor must be prepared before processing",
        })
    }
}

impl core::error::Error for Error {}
