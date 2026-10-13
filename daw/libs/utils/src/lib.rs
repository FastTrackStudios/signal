//! Shared utility macros and types for FastTrackStudio.
//!
//! This is a leaf crate with no internal dependencies — only external crates
//! like `uuid`, `serde`, and `facet`. It provides the ID-generation macros
//! (`typed_uuid_id!`, `typed_string_id!`) used by both `signal-proto` and
//! `session-proto` without introducing a dependency between them.

pub mod paths;
pub mod prefs;

/// Creates a branded string ID type with Display, From, and AsRef impls.
/// Used for categorical IDs like `RigTypeId` that remain human-readable strings.
#[macro_export]
macro_rules! typed_string_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, facet::Facet)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_string())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

/// Creates a branded UUID ID type backed by String for Facet compatibility.
/// Generates v7 UUIDs on `new()`, validates UUID format on `From<&str>`.
/// Used for all entity IDs that need global uniqueness for online sharing.
#[macro_export]
macro_rules! typed_uuid_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, facet::Facet)]
        pub struct $name(String);

        impl $name {
            /// Generate a new random v7 UUID.
            pub fn new() -> Self {
                Self(uuid::Uuid::now_v7().to_string())
            }

            /// Wrap an existing UUID.
            pub fn from_uuid(uuid: uuid::Uuid) -> Self {
                Self(uuid.to_string())
            }

            /// Get the string representation.
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Parse back to a UUID value.
            ///
            /// # Panics
            /// Panics if the inner string is not a valid UUID. Prefer
            /// [`try_to_uuid`](Self::try_to_uuid) at untrusted boundaries.
            pub fn to_uuid(&self) -> uuid::Uuid {
                self.0.parse().expect(concat!(
                    "corrupted UUID in ",
                    stringify!($name)
                ))
            }

            /// Try to parse back to a UUID value, returning an error on invalid data.
            pub fn try_to_uuid(&self) -> Result<uuid::Uuid, uuid::Error> {
                self.0.parse()
            }

            /// Parse a string into this ID type, returning an error if it is not a valid UUID.
            ///
            /// Use this at untrusted boundaries (deserialization, user input) instead of
            /// `From<String>` which panics on invalid input.
            pub fn try_parse(value: impl Into<String>) -> Result<Self, uuid::Error> {
                let s = value.into();
                let _: uuid::Uuid = s.parse()?;
                Ok(Self(s))
            }

            /// Consume and return the inner string.
            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<uuid::Uuid> for $name {
            fn from(value: uuid::Uuid) -> Self {
                Self(value.to_string())
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                // Validate UUID format
                let _: uuid::Uuid = value.parse().expect(concat!(
                    "invalid UUID string for ",
                    stringify!($name)
                ));
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                // Validate UUID format
                let _: uuid::Uuid = value.parse().expect(concat!(
                    "invalid UUID string for ",
                    stringify!($name)
                ));
                Self(value.to_string())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}
