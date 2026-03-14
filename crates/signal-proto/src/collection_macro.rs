//! Macro for reducing Collection / Variant trait boilerplate.
//!
//! Most domain types follow a common pattern: a **collection** struct holds a
//! `Vec` of **variant** structs plus a default-variant ID field. Both carry a
//! `metadata` field.  The [`impl_collection!`] macro generates the
//! [`typed_uuid_id!`](crate::typed_uuid_id!) calls for both ID types **and**
//! the four trait implementations ([`Variant`], [`DefaultVariant`],
//! [`Collection`], [`HasMetadata`] x 2) in a single invocation.
//!
//! # Usage
//!
//! ```ignore
//! impl_collection! {
//!     /// Doc comment for the collection ID.
//!     collection_id: SetlistId,
//!     /// Doc comment for the variant ID.
//!     variant_id: SetlistEntryId,
//!
//!     variant SetlistEntry {
//!         id: SetlistEntryId,
//!         base_ref: SongId => song_id,
//!         default_named: |name| Self::new(SetlistEntryId::new(), name, SongId::new()),
//!     }
//!
//!     collection Setlist {
//!         variant_type: SetlistEntry,
//!         variants_field: entries,
//!         default_id_field: default_entry_id,
//!     }
//! }
//! ```
//!
//! The `overrides` and `base_ref` arms are optional. When `overrides` is
//! omitted the trait methods return `None` (the default). Likewise `base_ref`
//! defaults to `None`.

/// Generate `typed_uuid_id!` calls and trait impls for a Collection / Variant pair.
///
/// See the [module-level documentation](self) for a full usage example.
#[macro_export]
macro_rules! impl_collection {
    (
        $(#[$coll_id_meta:meta])*
        collection_id: $CollId:ident,

        $(#[$var_id_meta:meta])*
        variant_id: $VarId:ident,

        variant $Variant:ident {
            id: $var_id_ty:ty
            $(, base_ref: $BaseRef:ty => $base_ref_field:ident)?
            $(, overrides: $Override:ty)?
            , default_named: |$dn_name:ident| $dn_body:expr
            $(,)?
        }

        collection $Collection:ident {
            variant_type: $VarType:ty,
            variants_field: $variants_field:ident,
            default_id_field: $default_id_field:ident
            $(,)?
        }
    ) => {
        // ── ID newtypes ──────────────────────────────────────────
        $crate::typed_uuid_id!($(#[$coll_id_meta])* $CollId);
        $crate::typed_uuid_id!($(#[$var_id_meta])* $VarId);

        // ── Variant ──────────────────────────────────────────────
        impl $crate::traits::Variant for $Variant {
            type Id = $var_id_ty;

            $crate::impl_collection!(@base_ref_type $($BaseRef)?);

            $crate::impl_collection!(@override_type $($Override)?);

            fn id(&self) -> &Self::Id {
                &self.id
            }
            fn name(&self) -> &str {
                &self.name
            }
            fn set_name(&mut self, name: impl Into<String>) {
                self.name = name.into();
            }

            $crate::impl_collection!(@base_ref_method $($base_ref_field)?);

            $crate::impl_collection!(@overrides_methods $($Override)?);
        }

        // ── DefaultVariant ───────────────────────────────────────
        impl $crate::traits::DefaultVariant for $Variant {
            fn default_named($dn_name: impl Into<String>) -> Self {
                let $dn_name = $dn_name.into();
                $dn_body
            }
        }

        // ── Collection ───────────────────────────────────────────
        impl $crate::traits::Collection for $Collection {
            type Variant = $VarType;

            fn variants(&self) -> &[$VarType] {
                &self.$variants_field
            }
            fn variants_mut(&mut self) -> &mut Vec<$VarType> {
                &mut self.$variants_field
            }
            fn default_variant_id(&self) -> &<$VarType as $crate::traits::Variant>::Id {
                &self.$default_id_field
            }
            fn set_default_variant_id(
                &mut self,
                id: <$VarType as $crate::traits::Variant>::Id,
            ) {
                self.$default_id_field = id;
            }
        }

        // ── HasMetadata (variant) ────────────────────────────────
        impl $crate::traits::HasMetadata for $Variant {
            fn metadata(&self) -> &$crate::metadata::Metadata {
                &self.metadata
            }
            fn metadata_mut(&mut self) -> &mut $crate::metadata::Metadata {
                &mut self.metadata
            }
        }

        // ── HasMetadata (collection) ─────────────────────────────
        impl $crate::traits::HasMetadata for $Collection {
            fn metadata(&self) -> &$crate::metadata::Metadata {
                &self.metadata
            }
            fn metadata_mut(&mut self) -> &mut $crate::metadata::Metadata {
                &mut self.metadata
            }
        }
    };

    // ── internal helpers: base_ref associated type ────────────────
    (@base_ref_type $BaseRef:ty) => {
        type BaseRef = $BaseRef;
    };
    (@base_ref_type) => {
        type BaseRef = ();
    };

    // ── internal helpers: override associated type ────────────────
    (@override_type $Override:ty) => {
        type Override = $Override;
    };
    (@override_type) => {
        type Override = ();
    };

    // ── internal helpers: base_ref method impl ───────────────────
    (@base_ref_method $base_ref_field:ident) => {
        fn base_ref(&self) -> Option<&Self::BaseRef> {
            Some(&self.$base_ref_field)
        }
    };
    (@base_ref_method) => {};

    // ── internal helpers: overrides methods ───────────────────────
    (@overrides_methods $Override:ty) => {
        fn overrides(&self) -> Option<&[Self::Override]> {
            Some(&self.overrides)
        }
        fn overrides_mut(&mut self) -> Option<&mut Vec<Self::Override>> {
            Some(&mut self.overrides)
        }
    };
    (@overrides_methods) => {};
}
