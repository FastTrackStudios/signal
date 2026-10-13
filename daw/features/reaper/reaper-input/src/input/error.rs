//! Input module error types and helpers
//!
//! Provides error types and helper functions for safe Mutex operations,
//! avoiding panic-prone `.unwrap()` calls throughout the input module.
//!
//! Based on Rust10x error handling pattern.

use std::sync::{Mutex, MutexGuard, PoisonError};

// region: --- Error Type

/// Input module error type
#[derive(Debug)]
pub enum Error {
    /// Custom error with message
    Custom(String),

    /// IO error
    Io(std::io::Error),

    /// Mutex was poisoned
    LockPoisoned(String),

    /// Action not found
    ActionNotFound(String),

    /// Invalid key sequence
    InvalidKeySequence(String),

    /// CString creation failed (NUL byte in string)
    CStringError(std::ffi::NulError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Custom(msg) => write!(f, "{msg}"),
            Self::Io(e) => write!(f, "IO error: {e}"),
            Self::LockPoisoned(msg) => write!(f, "Lock poisoned: {msg}"),
            Self::ActionNotFound(action) => write!(f, "Action not found: {action}"),
            Self::InvalidKeySequence(seq) => write!(f, "Invalid key sequence: {seq}"),
            Self::CStringError(e) => write!(f, "CString error: {e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::CStringError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<std::ffi::NulError> for Error {
    fn from(e: std::ffi::NulError) -> Self {
        Self::CStringError(e)
    }
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Self::Custom(s)
    }
}

impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Self::Custom(s.to_string())
    }
}

impl<T> From<PoisonError<T>> for Error {
    fn from(e: PoisonError<T>) -> Self {
        Self::LockPoisoned(e.to_string())
    }
}

// endregion: --- Error Type

// region: --- Result Type

/// Input module result type
pub type Result<T> = std::result::Result<T, Error>;

// endregion: --- Result Type

// region: --- Mutex Helpers

/// Try to lock a Mutex, returning a default value on poison.
///
/// Use this when you need a copy of the value and can tolerate
/// getting a default on error.
pub fn lock_or_default<T: Default + Clone>(mutex: &Mutex<T>) -> T {
    mutex.lock().map(|guard| guard.clone()).unwrap_or_else(|e| {
        tracing::error!("Mutex poisoned, using default: {}", e);
        T::default()
    })
}

/// Try to lock a Mutex for mutation, logging on poison.
///
/// Use this when you need to mutate the value but can skip the
/// operation on poison.
pub fn lock_mut<T, F>(mutex: &Mutex<T>, f: F)
where
    F: FnOnce(&mut T),
{
    match mutex.lock() {
        Ok(mut guard) => f(&mut *guard),
        Err(e) => {
            tracing::error!("Mutex poisoned, skipping operation: {}", e);
        }
    }
}

/// Try to lock a Mutex for read-only access, logging on poison.
///
/// Use this when you need to read the value but can skip the
/// operation on poison.
pub fn lock_read<T, F, R>(mutex: &Mutex<T>, f: F) -> Option<R>
where
    F: FnOnce(&T) -> R,
{
    match mutex.lock() {
        Ok(guard) => Some(f(&*guard)),
        Err(e) => {
            tracing::error!("Mutex poisoned, skipping read: {}", e);
            None
        }
    }
}

/// Try to lock a Mutex, returning Result.
///
/// Use this when you need to propagate the error.
pub fn try_lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>> {
    mutex.lock().map_err(|e| Error::LockPoisoned(e.to_string()))
}

// endregion: --- Mutex Helpers

// region: --- CString Helpers

/// Create a CString safely, returning Result.
pub fn cstring(s: &str) -> Result<std::ffi::CString> {
    std::ffi::CString::new(s).map_err(Error::from)
}

/// Create a CString, returning empty string on error.
pub fn cstring_or_empty(s: &str) -> std::ffi::CString {
    std::ffi::CString::new(s).unwrap_or_else(|_| {
        tracing::warn!("CString creation failed for: {:?}, using empty", s);
        std::ffi::CString::new("").expect("empty string is valid")
    })
}

// endregion: --- CString Helpers
