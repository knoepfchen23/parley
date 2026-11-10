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
#[cfg_attr(feature = "datagen", derive(serde::Serialize))]
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
