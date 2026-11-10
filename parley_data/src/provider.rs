// Copyright 2025 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The combined properties of [`Properties`](crate::Properties) as data an
//! ICU4X provider carries.
//!
//! Without it, Parley looks characters up in the table compiled into this
//! crate. An application that ships its Unicode data apart from its code, so
//! that every program reads one version and an update needs no rebuild,
//! carries the same table under [`ParleyCompositePropertiesV1`] beside
//! ICU4X's own data and hands the provider to Parley.

use icu_collections::codepointtrie::CodePointTrie;

/// The combined properties of every code point, each value the bits of a
/// [`Properties`](crate::Properties).
#[derive(Clone, Debug, Eq, PartialEq, yoke::Yokeable, zerofrom::ZeroFrom)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(
    feature = "datagen",
    derive(serde::Serialize, databake::Bake),
    databake(path = parley_data::provider)
)]
pub struct CompositeProperties<'data> {
    /// The table: a code point to the bits of its properties.
    #[cfg_attr(feature = "serde", serde(borrow))]
    pub trie: CodePointTrie<'data, u32>,
}

icu_provider::data_struct!(
    CompositeProperties<'_>,
    #[cfg(feature = "datagen")]
);

icu_provider::data_marker!(
    /// The combined properties of Parley's analysis, one table for all code
    /// points.
    ParleyCompositePropertiesV1,
    CompositeProperties<'static>,
    is_singleton = true
);

#[cfg(all(test, feature = "datagen"))]
mod tests {
    extern crate std;

    use super::*;
    use core::hash::{Hash, Hasher};
    use icu_collections::codepointtrie::planes;
    use icu_provider::DataPayload;
    use icu_provider::dynutil::UpcastDataPayload;
    use icu_provider::export::ExportMarker;

    /// The table meets what ICU4X's exporters ask of a data struct.
    #[test]
    fn the_table_can_be_exported() {
        let trie = planes::get_planes_trie()
            .try_alloc_map_value(|plane| Ok::<u32, ()>(u32::from(plane)))
            .unwrap();
        let payload =
            DataPayload::<ParleyCompositePropertiesV1>::from_owned(CompositeProperties { trie });
        let exported = ExportMarker::upcast(payload);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        exported.hash(&mut hasher);
        let _ = hasher.finish();
    }
}
