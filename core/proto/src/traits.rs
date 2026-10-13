//! Core traits for the collection/variant architecture.
//!
//! - [`Variant`]: A named configuration within a collection.
//! - [`DefaultVariant`]: Factory for creating a default variant with a given name.
//! - [`Collection`]: An ordered group of [`Variant`]s with a designated default.
//! - [`HasMetadata`]: Provides access to [`Metadata`](crate::metadata::Metadata) for tagging/description.

use crate::metadata::{Metadata as MetadataModel, Tags};

// ─── Variant ────────────────────────────────────────────────────

/// A named state snapshot within a collection.
///
/// Variants are the leaves of the signal hierarchy — each one captures
/// a concrete configuration (parameters, block choices, overrides, etc.).
pub trait Variant {
    type Id: Clone + PartialEq;
    type BaseRef;
    type Override;

    fn id(&self) -> &Self::Id;
    fn name(&self) -> &str;
    fn set_name(&mut self, name: impl Into<String>);

    fn base_ref(&self) -> Option<&Self::BaseRef> {
        None
    }

    fn overrides(&self) -> Option<&[Self::Override]> {
        None
    }

    fn overrides_mut(&mut self) -> Option<&mut Vec<Self::Override>> {
        None
    }

    // Backward-compatible aliases during migration.
    fn variant_id(&self) -> &Self::Id {
        self.id()
    }

    fn variant_name(&self) -> &str {
        self.name()
    }
}

// ─── DefaultVariant ─────────────────────────────────────────────

/// Factory for producing a sensible default variant.
pub trait DefaultVariant: Variant {
    fn default_named(name: impl Into<String>) -> Self;
}

// ─── Collection ─────────────────────────────────────────────────

/// An ordered group of variants with a designated default.
///
/// # Default Normalization Contract
///
/// After any mutation the caller should invoke [`normalize_default`](Collection::normalize_default).
/// The rules are:
///
/// 1. **Empty collection** → inject a default variant via [`DefaultVariant::default_named`].
/// 2. **Non-empty but default missing** → promote the first variant.
/// 3. **Valid** → no-op.
pub trait Collection {
    type Variant: Variant;

    fn variants(&self) -> &[Self::Variant];
    fn variants_mut(&mut self) -> &mut Vec<Self::Variant>;

    fn default_variant_id(&self) -> &<Self::Variant as Variant>::Id;
    fn set_default_variant_id(&mut self, id: <Self::Variant as Variant>::Id);

    // Backward-compatible aliases aligned with "collection/entry" naming.
    fn entries(&self) -> &[Self::Variant] {
        self.variants()
    }

    fn entries_mut(&mut self) -> &mut Vec<Self::Variant> {
        self.variants_mut()
    }

    /// Look up the designated default variant.
    fn default_variant(&self) -> Option<&Self::Variant> {
        let default_id = self.default_variant_id();
        self.variants()
            .iter()
            .find(|v| v.variant_id() == default_id)
    }

    /// Mutable access to the designated default variant.
    fn default_variant_mut(&mut self) -> Option<&mut Self::Variant>
    where
        <Self::Variant as Variant>::Id: Clone,
    {
        let default_id = self.default_variant_id().clone();
        self.variants_mut()
            .iter_mut()
            .find(|v| v.variant_id() == &default_id)
    }

    /// Look up a variant by name (first match).
    fn variant_by_name(&self, name: &str) -> Option<&Self::Variant> {
        self.variants().iter().find(|v| v.name() == name)
    }

    /// Mutable lookup by name (first match).
    fn variant_by_name_mut(&mut self, name: &str) -> Option<&mut Self::Variant> {
        self.variants_mut().iter_mut().find(|v| v.name() == name)
    }

    /// Ensure the collection satisfies the normalization contract.
    fn normalize_default(&mut self)
    where
        Self::Variant: DefaultVariant,
        <Self::Variant as Variant>::Id: Clone,
    {
        if self.variants().is_empty() {
            let fallback = Self::Variant::default_named("Default");
            let id = fallback.variant_id().clone();
            self.variants_mut().push(fallback);
            self.set_default_variant_id(id);
            return;
        }

        let default_id = self.default_variant_id();
        let found = self.variants().iter().any(|v| v.variant_id() == default_id);
        if !found {
            if let Some(first) = self.variants().first() {
                let id = first.variant_id().clone();
                self.set_default_variant_id(id);
            }
        }
    }
}

// ─── HasMetadata ────────────────────────────────────────────────

/// Provides access to metadata (tags, description, notes).
pub trait HasMetadata {
    fn metadata(&self) -> &MetadataModel;
    fn metadata_mut(&mut self) -> &mut MetadataModel;
}

/// Domain metadata trait alias for cleaner generic bounds.
pub trait Metadata: HasMetadata {}

impl<T: HasMetadata> Metadata for T {}

/// Explicit accessors for long-form text fields.
pub trait Notes: HasMetadata {
    fn notes(&self) -> Option<&str> {
        self.metadata().notes.as_deref()
    }

    fn set_notes(&mut self, value: Option<String>) {
        self.metadata_mut().notes = value;
    }
}

/// Convenience trait for items that are tagged.
pub trait Tagged: HasMetadata {
    fn tags(&self) -> &Tags {
        &self.metadata().tags
    }
}

/// Convenience trait for items that carry a description.
pub trait Described: HasMetadata {
    fn description(&self) -> Option<&str> {
        self.metadata().description.as_deref()
    }

    fn set_description(&mut self, value: Option<String>) {
        self.metadata_mut().description = value;
    }
}

// Blanket impls
impl<T: HasMetadata> Tagged for T {}
impl<T: HasMetadata> Described for T {}
impl<T: HasMetadata> Notes for T {}

#[cfg(test)]
mod tests {
    use super::*;

    // Minimal test types

    #[derive(Debug, Clone, PartialEq)]
    struct TestId(String);

    #[derive(Debug, Clone)]
    struct TestVariant {
        id: TestId,
        name: String,
    }

    impl Variant for TestVariant {
        type Id = TestId;
        type BaseRef = ();
        type Override = ();
        fn id(&self) -> &TestId {
            &self.id
        }
        fn name(&self) -> &str {
            &self.name
        }
        fn set_name(&mut self, name: impl Into<String>) {
            self.name = name.into();
        }
    }

    impl DefaultVariant for TestVariant {
        fn default_named(name: impl Into<String>) -> Self {
            let name = name.into();
            Self {
                id: TestId(format!("{}-default", name.to_lowercase())),
                name,
            }
        }
    }

    struct TestCollection {
        variants: Vec<TestVariant>,
        default_id: TestId,
    }

    impl Collection for TestCollection {
        type Variant = TestVariant;

        fn variants(&self) -> &[TestVariant] {
            &self.variants
        }
        fn variants_mut(&mut self) -> &mut Vec<TestVariant> {
            &mut self.variants
        }
        fn default_variant_id(&self) -> &TestId {
            &self.default_id
        }
        fn set_default_variant_id(&mut self, id: TestId) {
            self.default_id = id;
        }
    }

    #[test]
    fn normalize_injects_default_when_empty() {
        let mut coll = TestCollection {
            variants: vec![],
            default_id: TestId("gone".into()),
        };
        coll.normalize_default();
        assert_eq!(coll.variants().len(), 1);
        assert_eq!(coll.variants()[0].variant_name(), "Default");
        assert_eq!(coll.default_variant_id(), coll.variants()[0].variant_id());
    }

    #[test]
    fn normalize_promotes_first_when_default_missing() {
        let a = TestVariant {
            id: TestId("a".into()),
            name: "Alpha".into(),
        };
        let b = TestVariant {
            id: TestId("b".into()),
            name: "Beta".into(),
        };
        let mut coll = TestCollection {
            variants: vec![a, b],
            default_id: TestId("gone".into()),
        };
        coll.normalize_default();
        assert_eq!(coll.default_variant_id(), &TestId("a".into()));
    }

    #[test]
    fn normalize_noop_when_valid() {
        let a = TestVariant {
            id: TestId("a".into()),
            name: "Alpha".into(),
        };
        let mut coll = TestCollection {
            variants: vec![a],
            default_id: TestId("a".into()),
        };
        coll.normalize_default();
        assert_eq!(coll.variants().len(), 1);
        assert_eq!(coll.default_variant_id(), &TestId("a".into()));
    }
}
