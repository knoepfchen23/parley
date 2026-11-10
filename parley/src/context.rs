// Copyright 2021 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Context for layout.

use alloc::{vec, vec::Vec};

use super::FontContext;
use super::builder::{RangedBuilder, StyleRunBuilder};
use super::resolve::tree::TreeStyleBuilder;
use super::resolve::{RangedStyleBuilder, ResolveContext, ResolvedStyle, StyleRun};
use super::style::{Brush, TextStyle};

use crate::analysis::{AnalysisDataSources, CharInfo, TextDataProvider};
use crate::bidi::BidiResolver;
use crate::builder::TreeBuilder;
use crate::inline_box::InlineBox;
use crate::shape::ShapeContext;
use icu_provider::DataError;

/// Shared scratch space used when constructing text layouts.
///
/// This type is designed to be a global resource with only one per-application (or per-thread).
pub struct LayoutContext<B: Brush = [u8; 4]> {
    pub(crate) rcx: ResolveContext,
    pub(crate) style_table: Vec<ResolvedStyle<B>>,
    pub(crate) style_runs: Vec<StyleRun>,
    pub(crate) inline_boxes: Vec<InlineBox>,
    pub(crate) bidi: BidiResolver,

    // Reusable style builders (to amortise allocations)
    pub(crate) ranged_style_builder: RangedStyleBuilder<B>,
    pub(crate) tree_style_builder: TreeStyleBuilder<B>,

    // u16: style index for character
    pub(crate) info: Vec<(CharInfo, u16)>,
    pub(crate) scx: ShapeContext,

    // Unicode analysis data sources (provided by icu)
    pub(crate) analysis_data_sources: AnalysisDataSources,

    /// Further starts of words for the layout being built, by byte offset,
    /// beside those of the word segmenter.
    pub(crate) word_starts: Vec<usize>,
}

impl<B: Brush> LayoutContext<B> {
    /// A context that analyses text with the Unicode data compiled into
    /// Parley's crates: segmenters without the dictionaries of the scripts
    /// that set no space between words.
    pub fn new() -> Self {
        Self::with_sources(AnalysisDataSources::new())
    }

    /// A context that analyses text with the data of `provider`, built once
    /// here: ICU4X's segmenters with the dictionaries of Chinese, Japanese,
    /// Thai, Lao, Khmer and Burmese, its normalizers, the names of scripts,
    /// the mirrored brackets and Parley's combined table.
    ///
    /// An application that ships its Unicode data apart from its code hands
    /// them here, so that every program reads one version.
    ///
    /// # Errors
    ///
    /// Where `provider` lacks a datum Parley reads.
    pub fn try_new_unstable<P: TextDataProvider + ?Sized>(provider: &P) -> Result<Self, DataError> {
        Ok(Self::with_sources(AnalysisDataSources::try_new_unstable(
            provider,
        )?))
    }

    /// As [`Self::try_new_unstable`], with data serialized by ICU4X's
    /// exporters, as a postcard blob behind a `BufferProvider`.
    ///
    /// # Errors
    ///
    /// Where the provider lacks a datum Parley reads or one does not
    /// deserialize.
    #[cfg(feature = "serde")]
    pub fn try_new_with_buffer_provider(
        provider: &(impl icu_provider::buf::BufferProvider + ?Sized),
    ) -> Result<Self, DataError> {
        use icu_provider::buf::AsDeserializingBufferProvider;
        Self::try_new_unstable(&provider.as_deserializing())
    }

    fn with_sources(analysis_data_sources: AnalysisDataSources) -> Self {
        Self {
            rcx: ResolveContext::default(),
            style_table: vec![],
            style_runs: vec![],
            inline_boxes: vec![],
            bidi: BidiResolver::new(),
            ranged_style_builder: RangedStyleBuilder::default(),
            tree_style_builder: TreeStyleBuilder::default(),
            info: vec![],
            analysis_data_sources,
            scx: ShapeContext::default(),
            word_starts: vec![],
        }
    }

    fn resolve_style_set(
        &mut self,
        font_ctx: &mut FontContext,
        scale: f32,
        raw_style: &TextStyle<'_, '_, B>,
    ) -> ResolvedStyle<B> {
        self.rcx
            .resolve_entire_style_set(font_ctx, raw_style, scale)
    }

    /// Create a ranged style layout builder.
    ///
    /// Set `quantize` as `true` to have the layout coordinates aligned to pixel boundaries.
    /// That is the easiest way to avoid blurry text and to receive ready-to-paint layout metrics.
    ///
    /// For advanced rendering use cases you can set `quantize` as `false` and receive
    /// fractional coordinates. This ensures the most accurate results if you want to perform
    /// some post-processing on the coordinates before painting. To avoid blurry text you will
    /// still need to quantize the coordinates just before painting.
    ///
    /// Your should round at least the following:
    /// * Glyph run baseline
    /// * Inline box baseline
    ///   - `box.y = (box.y + box.height).round() - box.height`
    /// * Selection geometry's `y0` & `y1`
    /// * Cursor geometry's `y0` & `y1`
    ///
    /// Keep in mind that for the simple `f32::round` to be effective,
    /// you need to first ensure the coordinates are in physical pixel space.
    pub fn ranged_builder<'a>(
        &'a mut self,
        fcx: &'a mut FontContext,
        text: &'a str,
        scale: f32,
        quantize: bool,
    ) -> RangedBuilder<'a, B> {
        self.begin();

        let resolved_root_style = self.resolve_style_set(fcx, scale, &TextStyle::default());
        self.ranged_style_builder
            .begin(resolved_root_style, text.len());

        fcx.source_cache.prune(128, false);

        RangedBuilder {
            scale,
            quantize,
            lcx: self,
            fcx,
        }
    }

    /// Create a builder for constructing a layout from indexed style runs.
    ///
    /// Unlike [`Self::ranged_builder`], this builder expects callers to provide:
    /// - a style table of fully specified [`TextStyle`] values (via [`StyleRunBuilder::push_style`])
    /// - a complete sequence of **contiguous**, **non-overlapping** spans that cover
    ///   `0..text.len()` and reference style indices (via [`StyleRunBuilder::push_style_run`])
    ///
    /// Parley then skips its internal range-splitting logic.
    pub fn style_run_builder<'a>(
        &'a mut self,
        fcx: &'a mut FontContext,
        text: &'a str,
        scale: f32,
        quantize: bool,
    ) -> StyleRunBuilder<'a, B> {
        self.begin();

        fcx.source_cache.prune(128, false);

        StyleRunBuilder {
            scale,
            quantize,
            len: text.len(),
            lcx: self,
            fcx,
            cursor: 0,
        }
    }

    /// Create a tree style layout builder.
    ///
    /// Set `quantize` as `true` to have the layout coordinates aligned to pixel boundaries.
    /// That is the easiest way to avoid blurry text and to receive ready-to-paint layout metrics.
    ///
    /// For advanced rendering use cases you can set `quantize` as `false` and receive
    /// fractional coordinates. This ensures the most accurate results if you want to perform
    /// some post-processing on the coordinates before painting. To avoid blurry text you will
    /// still need to quantize the coordinates just before painting.
    ///
    /// Your should round at least the following:
    /// * Glyph run baseline
    /// * Inline box baseline
    ///   - `box.y = (box.y + box.height).round() - box.height`
    /// * Selection geometry's `y0` & `y1`
    /// * Cursor geometry's `y0` & `y1`
    ///
    /// Keep in mind that for the simple `f32::round` to be effective,
    /// you need to first ensure the coordinates are in physical pixel space.
    pub fn tree_builder<'a>(
        &'a mut self,
        fcx: &'a mut FontContext,
        scale: f32,
        quantize: bool,
        root_style: &TextStyle<'_, '_, B>,
    ) -> TreeBuilder<'a, B> {
        self.begin();

        let resolved_root_style = self.resolve_style_set(fcx, scale, root_style);
        self.tree_style_builder.begin(resolved_root_style);

        fcx.source_cache.prune(128, false);

        TreeBuilder {
            scale,
            quantize,
            lcx: self,
            fcx,
        }
    }

    fn begin(&mut self) {
        self.rcx.clear();
        self.style_table.clear();
        self.style_runs.clear();
        self.inline_boxes.clear();
        self.info.clear();
        self.bidi.clear();
        self.word_starts.clear();
    }
}

impl<B: Brush> Default for LayoutContext<B> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: Brush> Clone for LayoutContext<B> {
    fn clone(&self) -> Self {
        // None of the internal state is visible so just return a new instance.
        Self::new()
    }
}
