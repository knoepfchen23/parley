// Copyright 2025 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The Unicode properties `HarfRust` shapes with, from the data handed to a
//! [`LayoutContext`](crate::LayoutContext), so that shaping reads the same
//! version of the data as the analysis.

use icu_normalizer::properties::{
    CanonicalCombiningClassMap, CanonicalComposition, CanonicalDecomposition, Decomposed,
};
use icu_properties::props::{
    BidiMirroringGlyph, ExtendedPictographic, GeneralCategory as IcuCategory, Script,
};
use icu_properties::{CodePointMapData, CodePointSetData, PropertyNamesShort};
use icu_provider::{DataError, DataPayload};
use parley_data::Properties;
use parley_data::provider::ParleyCompositePropertiesV1;

use harfrust::GeneralCategory as HbCategory;

use super::TextDataProvider;

/// `HarfRust`'s `UnicodeFuncs` over the handed data.
pub(crate) struct ShapingProperties {
    composite: DataPayload<ParleyCompositePropertiesV1>,
    combining_classes: CanonicalCombiningClassMap,
    mirroring: CodePointMapData<BidiMirroringGlyph>,
    extended_pictographic: CodePointSetData,
    composition: CanonicalComposition,
    decomposition: CanonicalDecomposition,
    /// `HarfRust`'s script for each of `ICU4X`'s, by `ICU4C`'s value.
    scripts: alloc::vec::Vec<harfrust::Script>,
}

impl core::fmt::Debug for ShapingProperties {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ShapingProperties")
    }
}

impl ShapingProperties {
    pub(crate) fn try_new_unstable<P: TextDataProvider + ?Sized>(
        provider: &P,
    ) -> Result<Self, DataError> {
        let names = PropertyNamesShort::<Script>::try_new_unstable(provider)?;
        let names = names.as_borrowed();
        let scripts = (0..=u16::from(u8::MAX))
            .map(|value| {
                names
                    .get(Script::from_icu4c_value(value))
                    .and_then(|name| <[u8; 4]>::try_from(name.as_bytes()).ok())
                    .and_then(|tag| harfrust::Script::from_iso15924_tag(harfrust::Tag::new(&tag)))
                    .unwrap_or(harfrust::script::UNKNOWN)
            })
            .collect();
        Ok(Self {
            composite: provider.load(icu_provider::DataRequest::default())?.payload,
            combining_classes: CanonicalCombiningClassMap::try_new_unstable(provider)?,
            mirroring: CodePointMapData::try_new_unstable(provider)?,
            extended_pictographic: CodePointSetData::try_new_unstable::<ExtendedPictographic>(
                provider,
            )?,
            composition: CanonicalComposition::try_new_unstable(provider)?,
            decomposition: CanonicalDecomposition::try_new_unstable(provider)?,
            scripts,
        })
    }

    fn properties(&self, u: u32) -> Properties {
        Properties::from(self.composite.get().trie.get32(u))
    }
}

/// `category` in `HarfBuzz`'s numbering.
fn to_harfbuzz(category: IcuCategory) -> HbCategory {
    match category {
        IcuCategory::Unassigned => HbCategory::UNASSIGNED,
        IcuCategory::UppercaseLetter => HbCategory::UPPERCASE_LETTER,
        IcuCategory::LowercaseLetter => HbCategory::LOWERCASE_LETTER,
        IcuCategory::TitlecaseLetter => HbCategory::TITLECASE_LETTER,
        IcuCategory::ModifierLetter => HbCategory::MODIFIER_LETTER,
        IcuCategory::OtherLetter => HbCategory::OTHER_LETTER,
        IcuCategory::NonspacingMark => HbCategory::NON_SPACING_MARK,
        IcuCategory::EnclosingMark => HbCategory::ENCLOSING_MARK,
        IcuCategory::SpacingMark => HbCategory::SPACING_MARK,
        IcuCategory::DecimalNumber => HbCategory::DECIMAL_NUMBER,
        IcuCategory::LetterNumber => HbCategory::LETTER_NUMBER,
        IcuCategory::OtherNumber => HbCategory::OTHER_NUMBER,
        IcuCategory::SpaceSeparator => HbCategory::SPACE_SEPARATOR,
        IcuCategory::LineSeparator => HbCategory::LINE_SEPARATOR,
        IcuCategory::ParagraphSeparator => HbCategory::PARAGRAPH_SEPARATOR,
        IcuCategory::Control => HbCategory::CONTROL,
        IcuCategory::Format => HbCategory::FORMAT,
        IcuCategory::PrivateUse => HbCategory::PRIVATE_USE,
        IcuCategory::Surrogate => HbCategory::SURROGATE,
        IcuCategory::DashPunctuation => HbCategory::DASH_PUNCTUATION,
        IcuCategory::OpenPunctuation => HbCategory::OPEN_PUNCTUATION,
        IcuCategory::ClosePunctuation => HbCategory::CLOSE_PUNCTUATION,
        IcuCategory::ConnectorPunctuation => HbCategory::CONNECT_PUNCTUATION,
        IcuCategory::OtherPunctuation => HbCategory::OTHER_PUNCTUATION,
        IcuCategory::MathSymbol => HbCategory::MATH_SYMBOL,
        IcuCategory::CurrencySymbol => HbCategory::CURRENCY_SYMBOL,
        IcuCategory::ModifierSymbol => HbCategory::MODIFIER_SYMBOL,
        IcuCategory::OtherSymbol => HbCategory::OTHER_SYMBOL,
        IcuCategory::InitialPunctuation => HbCategory::INITIAL_PUNCTUATION,
        IcuCategory::FinalPunctuation => HbCategory::FINAL_PUNCTUATION,
    }
}

fn to_char(u: u32) -> Option<char> {
    char::from_u32(u)
}

impl harfrust::UnicodeFuncs for ShapingProperties {
    fn general_category(&self, u: u32) -> HbCategory {
        to_harfbuzz(self.properties(u).general_category())
    }

    fn combining_class(&self, u: u32) -> u8 {
        self.combining_classes.as_borrowed().get32_u8(u)
    }

    fn mirroring(&self, u: u32) -> Option<u32> {
        let c = to_char(u)?;
        self.mirroring
            .as_borrowed()
            .get(c)
            .mirroring_glyph
            .map(u32::from)
    }

    fn script(&self, u: u32) -> harfrust::Script {
        let value = self.properties(u).script().to_icu4c_value();
        self.scripts
            .get(usize::from(value))
            .copied()
            .unwrap_or(harfrust::script::UNKNOWN)
    }

    fn is_extended_pictographic(&self, u: u32) -> bool {
        self.extended_pictographic.as_borrowed().contains32(u)
    }

    fn compose(&self, a: u32, b: u32) -> Option<u32> {
        self.composition
            .as_borrowed()
            .compose(to_char(a)?, to_char(b)?)
            .map(u32::from)
    }

    fn decompose(&self, ab: u32) -> Option<(u32, u32)> {
        match self.decomposition.as_borrowed().decompose(to_char(ab)?) {
            Decomposed::Default => None,
            Decomposed::Singleton(a) => Some((u32::from(a), 0)),
            Decomposed::Expansion(a, b) => Some((u32::from(a), u32::from(b))),
        }
    }
}
