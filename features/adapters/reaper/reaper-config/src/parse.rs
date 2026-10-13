//! Parsing helpers for converting between INI string values and Rust types.

use crate::ConfigError;
use crate::ini::IniFile;

/// Helper to read a typed value from an INI section.
pub(crate) fn get_u32(ini: &IniFile, section: &str, key: &str) -> Option<u32> {
    ini.get(section, key).and_then(|v| v.parse().ok())
}

pub(crate) fn get_i32(ini: &IniFile, section: &str, key: &str) -> Option<i32> {
    ini.get(section, key).and_then(|v| v.parse().ok())
}

pub(crate) fn get_f64(ini: &IniFile, section: &str, key: &str) -> Option<f64> {
    ini.get(section, key).and_then(|v| v.parse().ok())
}

pub(crate) fn get_bool(ini: &IniFile, section: &str, key: &str) -> Option<bool> {
    ini.get(section, key).map(|v| v != "0")
}

pub(crate) fn get_string(ini: &IniFile, section: &str, key: &str) -> Option<String> {
    ini.get(section, key).map(|v| v.to_string())
}

/// Write helpers — only writes if the value differs from default / is Some.
pub(crate) fn set_u32(ini: &mut IniFile, section: &str, key: &str, value: u32) {
    ini.set(section, key, &value.to_string());
}

pub(crate) fn set_i32(ini: &mut IniFile, section: &str, key: &str, value: i32) {
    ini.set(section, key, &value.to_string());
}

pub(crate) fn set_f64(ini: &mut IniFile, section: &str, key: &str, value: f64) {
    ini.set(section, key, &value.to_string());
}

pub(crate) fn set_bool(ini: &mut IniFile, section: &str, key: &str, value: bool) {
    ini.set(section, key, if value { "1" } else { "0" });
}

pub(crate) fn set_string(ini: &mut IniFile, section: &str, key: &str, value: &str) {
    ini.set(section, key, value);
}

pub(crate) fn set_opt_u32(ini: &mut IniFile, section: &str, key: &str, value: Option<u32>) {
    if let Some(v) = value {
        set_u32(ini, section, key, v);
    }
}

pub(crate) fn set_opt_i32(ini: &mut IniFile, section: &str, key: &str, value: Option<i32>) {
    if let Some(v) = value {
        set_i32(ini, section, key, v);
    }
}

pub(crate) fn set_opt_f64(ini: &mut IniFile, section: &str, key: &str, value: Option<f64>) {
    if let Some(v) = value {
        set_f64(ini, section, key, v);
    }
}

pub(crate) fn set_opt_string(ini: &mut IniFile, section: &str, key: &str, value: &Option<String>) {
    if let Some(v) = value {
        set_string(ini, section, key, v);
    }
}

/// Read an `i32` from the INI and convert to an enum via `From<i32>`.
pub(crate) fn get_enum_i32<T: From<i32>>(ini: &IniFile, section: &str, key: &str) -> Option<T> {
    get_i32(ini, section, key).map(T::from)
}

/// Read a `u32` from the INI and convert to an enum via `From<u32>`.
pub(crate) fn get_enum_u32<T: From<u32>>(ini: &IniFile, section: &str, key: &str) -> Option<T> {
    get_u32(ini, section, key).map(T::from)
}

/// Write an optional enum value as its `i32` representation.
pub(crate) fn set_opt_enum_i32<T: Into<i32> + Copy>(
    ini: &mut IniFile,
    section: &str,
    key: &str,
    value: Option<T>,
) {
    if let Some(v) = value {
        set_i32(ini, section, key, v.into());
    }
}

/// Write an optional enum value as its `u32` representation.
pub(crate) fn set_opt_enum_u32<T: Into<u32> + Copy>(
    ini: &mut IniFile,
    section: &str,
    key: &str,
    value: Option<T>,
) {
    if let Some(v) = value {
        set_u32(ini, section, key, v.into());
    }
}

/// Read an `i32` from the INI and convert to `bool` (0 = false, non-zero = true).
pub(crate) fn get_bool_from_i32(ini: &IniFile, section: &str, key: &str) -> Option<bool> {
    get_i32(ini, section, key).map(|v| v != 0)
}

/// Write an optional `bool` as `1`/`0`.
pub(crate) fn set_opt_bool_as_i32(
    ini: &mut IniFile,
    section: &str,
    key: &str,
    value: Option<bool>,
) {
    if let Some(v) = value {
        set_i32(ini, section, key, if v { 1 } else { 0 });
    }
}

/// Write an optional `bool` as `1`/`0`.
pub(crate) fn set_opt_bool(ini: &mut IniFile, section: &str, key: &str, value: Option<bool>) {
    if let Some(v) = value {
        set_bool(ini, section, key, v);
    }
}
