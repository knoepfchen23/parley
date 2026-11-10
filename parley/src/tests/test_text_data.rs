// Copyright 2025 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Unicode data handed to a [`LayoutContext`] through a provider
//! ([`LayoutContext::try_new_unstable`]): the dictionaries of the scripts that
//! set no space between words, and Parley's combined table taken from the
//! provider rather than from its crate.

use crate::analysis::Boundary;
use crate::data_provider::{CompositeProperties, ParleyCompositePropertiesV1};
use crate::{FontContext, LayoutContext};
use alloc::vec::Vec;
use icu_collections::codepointtrie::planes;
use icu_normalizer::provider::{
    NormalizerNfcV1, NormalizerNfdDataV1, NormalizerNfdSupplementV1, NormalizerNfdTablesV1,
};
use icu_properties::props::{BidiClass, GeneralCategory, GraphemeClusterBreak, Script};
use icu_properties::provider::{PropertyEnumBidiMirroringGlyphV1, PropertyNameShortScriptV1};
use icu_provider::{
    DataError, DataPayload, DataProvider, DataRequest, DataResponse, DataResponseMetadata,
};
use icu_segmenter::options::{LineBreakOptions, WordBreakInvariantOptions};
use icu_segmenter::provider::{
    SegmenterBreakGraphemeClusterV1, SegmenterBreakLineV1, SegmenterBreakWordOverrideV1,
    SegmenterBreakWordV1, SegmenterDictionaryAutoV1, SegmenterDictionaryExtendedV1,
};
use icu_segmenter::{LineSegmenter, WordSegmenter};
use parley_data::Properties;
use std::sync::OnceLock;

/// ICU4X's compiled data and a combined table, behind one provider, as an
/// application hands its own data to Parley.
struct TestData {
    composite: CompositeProperties<'static>,
}

macro_rules! delegate {
    ($baked:path => $($marker:ty),* $(,)?) => {
        $(
            impl DataProvider<$marker> for TestData {
                fn load(&self, req: DataRequest<'_>) -> Result<DataResponse<$marker>, DataError> {
                    $baked.load(req)
                }
            }
        )*
    };
}

delegate!(icu_segmenter::provider::Baked =>
    SegmenterBreakGraphemeClusterV1,
    SegmenterBreakWordV1,
    SegmenterBreakWordOverrideV1,
    SegmenterBreakLineV1,
    SegmenterDictionaryAutoV1,
    SegmenterDictionaryExtendedV1,
);
delegate!(icu_normalizer::provider::Baked =>
    NormalizerNfcV1,
    NormalizerNfdDataV1,
    NormalizerNfdTablesV1,
    NormalizerNfdSupplementV1,
);
delegate!(icu_properties::provider::Baked =>
    PropertyNameShortScriptV1,
    PropertyEnumBidiMirroringGlyphV1,
);

impl DataProvider<ParleyCompositePropertiesV1> for TestData {
    fn load(
        &self,
        _req: DataRequest<'_>,
    ) -> Result<DataResponse<ParleyCompositePropertiesV1>, DataError> {
        Ok(DataResponse {
            metadata: DataResponseMetadata::default(),
            payload: DataPayload::from_owned(self.composite.clone()),
        })
    }
}

/// The combined table built from ICU4X's compiled data, once: the same
/// values as the table compiled into `parley_data`.
fn compiled_table() -> &'static CompositeProperties<'static> {
    static TABLE: OnceLock<CompositeProperties<'static>> = OnceLock::new();
    TABLE.get_or_init(parley_data_gen::composite_properties)
}

/// A context with ICU4X's compiled data and the combined table handed to it.
pub(super) fn handed_context() -> LayoutContext {
    LayoutContext::try_new_unstable(&TestData {
        composite: compiled_table().clone(),
    })
    .expect("every datum Parley reads")
}

/// Whether `text` holds a character of a script ICU4X segments by a
/// dictionary, where data handed with the dictionaries break otherwise than
/// Parley's compiled segmenters.
pub(super) fn segmented_by_dictionary(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            Properties::get(c).script(),
            Script::Han
                | Script::Hiragana
                | Script::Katakana
                | Script::Thai
                | Script::Lao
                | Script::Khmer
                | Script::Myanmar
        )
    })
}

/// The boundaries Parley gives `text`, by the byte offset of each character.
fn boundaries(mut layout_context: LayoutContext, text: &str) -> Vec<(usize, Boundary)> {
    let mut font_context = FontContext::default();
    {
        let builder = layout_context.ranged_builder(&mut font_context, text, 1., true);
        _ = builder.build(text);
    }
    text.char_indices()
        .map(|(at, _)| at)
        .zip(layout_context.info.iter().map(|(info, _)| info.boundary))
        .collect()
}

/// The offsets inside `text` where a line may break, by ICU4X's segmenter
/// with the dictionaries.
fn icu_line_breaks(text: &str) -> Vec<usize> {
    LineSegmenter::new_dictionary(LineBreakOptions::default())
        .segment_str(text)
        .filter(|&at| at != 0 && at != text.len())
        .collect()
}

/// The offsets inside `text` where a word begins, by ICU4X's segmenter with
/// the dictionaries.
fn icu_word_starts(text: &str) -> Vec<usize> {
    WordSegmenter::new_dictionary(WordBreakInvariantOptions::default())
        .segment_str(text)
        .filter(|&at| at != 0 && at != text.len())
        .collect()
}

fn offsets_of(boundaries: &[(usize, Boundary)], kinds: &[Boundary]) -> Vec<usize> {
    boundaries
        .iter()
        .filter(|(at, boundary)| *at != 0 && kinds.contains(boundary))
        .map(|(at, _)| *at)
        .collect()
}

#[test]
fn a_line_of_thai_breaks_between_its_words() {
    // "Thai is quite easy": four words, no space between them.
    let text = "ภาษาไทยง่ายนิดเดียว";
    let expected = icu_line_breaks(text);
    assert!(expected.len() >= 3, "ICU4X finds the words: {expected:?}");

    let handed = boundaries(handed_context(), text);
    assert_eq!(offsets_of(&handed, &[Boundary::Line]), expected);

    let compiled = boundaries(LayoutContext::new(), text);
    assert_eq!(
        offsets_of(&compiled, &[Boundary::Line]),
        Vec::<usize>::new(),
        "without the dictionaries no line breaks inside the run"
    );
}

#[test]
fn japanese_words_begin_where_the_dictionary_says() {
    // "Write a Japanese text": 日本語 / の / 文章 / を / 書く.
    let text = "日本語の文章を書く";
    let starts = icu_word_starts(text);
    let bunsho = text.find("文章").unwrap();
    assert!(starts.contains(&bunsho), "ICU4X finds 文章: {starts:?}");

    let handed = boundaries(handed_context(), text);
    let found = offsets_of(&handed, &[Boundary::Word, Boundary::Line]);
    for start in &starts {
        assert!(found.contains(start), "a boundary at {start}: {found:?}");
    }
}

#[test]
fn burmese_khmer_and_lao_are_segmented_too() {
    // "Myanmar lies in Southeast Asia", "I love Cambodia" and "Lao
    // language", each without spaces.
    for text in ["မြန်မာနိုင်ငံသည်အရှေ့တောင်အာရှတွင်ရှိသည်", "ខ្ញុំស្រលាញ់ប្រទេសកម្ពុជា", "ພາສາລາວ"]
    {
        let expected = icu_line_breaks(text);
        assert!(!expected.is_empty(), "ICU4X finds the words of {text}");
        let handed = boundaries(handed_context(), text);
        assert_eq!(offsets_of(&handed, &[Boundary::Line]), expected, "{text}");
    }
}

#[test]
fn a_handed_table_is_the_one_looked_up() {
    // A table that calls every character of the first plane Thai, made from
    // ICU4X's table of planes so that its values stand apart from any
    // compiled one.
    let thai: u32 = Properties::new(
        Script::Thai,
        GeneralCategory::OtherLetter,
        GraphemeClusterBreak::Other,
        BidiClass::LeftToRight,
        false,
        false,
        false,
        false,
    )
    .into();
    let trie = planes::get_planes_trie()
        .try_alloc_map_value(|plane| Ok::<u32, ()>(if plane == 0 { thai } else { 0 }))
        .unwrap();
    let mut layout_context: LayoutContext = LayoutContext::try_new_unstable(&TestData {
        composite: CompositeProperties { trie },
    })
    .unwrap();
    let mut font_context = FontContext::default();
    let text = "AB";
    {
        let builder = layout_context.ranged_builder(&mut font_context, text, 1., true);
        _ = builder.build(text);
    }
    let scripts: Vec<_> = layout_context
        .info
        .iter()
        .map(|(info, _)| info.script)
        .collect();
    assert_eq!(scripts, [Script::Thai, Script::Thai]);
}

#[test]
fn the_built_table_equals_the_compiled_one() {
    let table = compiled_table();
    for cp in 0_u32..=0x10FFFF {
        let Some(c) = char::from_u32(cp) else {
            continue;
        };
        assert_eq!(
            table.trie.get32(cp),
            u32::from(Properties::get(c)),
            "U+{cp:04X}"
        );
    }
}
