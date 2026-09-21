macro_rules! define_named_u8_enum {
    (
        $vis:vis enum $type:ident {
            $( $variant:ident = $id:literal => $name:literal ),+ $(,)?
        }
    ) => {
        #[repr(u8)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        $vis enum $type {
            $(
                $variant = $id,
            )+
        }

        impl $type {
            $vis const ALL: &'static [Self] = &[
                $(
                    Self::$variant,
                )+
            ];

            $vis const COUNT: usize = Self::ALL.len();

            $vis const fn id(self) -> u8 {
                self as u8
            }

            $vis const fn from_id(id: u8) -> Option<Self> {
                match id {
                    $(
                        $id => Some(Self::$variant),
                    )+
                    _ => None,
                }
            }

            $vis const fn name(self) -> &'static str {
                match self {
                    $(
                        Self::$variant => $name,
                    )+
                }
            }
        }
    };
}

define_named_u8_enum! {
    pub enum DisplayPhase {
        Unknown = 0 => "unknown",

        PowerOn = 1 => "power_on",

        BinaryFullRefresh = 2 => "binary_full_refresh",
        BinaryFastRefresh = 3 => "binary_fast_refresh",

        GrayscaleBaseRefresh = 4 => "grayscale_base_refresh",
        GrayscalePrecondition = 5 => "grayscale_precondition",
        GrayscaleActivate = 6 => "grayscale_activate",
        GrayscaleRefresh = 7 => "grayscale_refresh",

        PowerOff = 8 => "power_off",
    }
}

define_named_u8_enum! {
    pub enum TraceEvent {
        Render = 0 => "render",
        Rebuild = 1 => "rebuild",
        Layout = 2 => "layout",
        Clear = 3 => "clear",
        Paint = 4 => "paint",
        Damage = 5 => "damage",

        TextRun = 6 => "text_run",
        Shape = 7 => "shape",
        Glyphs = 8 => "glyphs",
        LogicalShape = 9 => "logical_shape",
        VisualOrder = 10 => "visual_order",
        Positioning = 11 => "positioning",

        Present = 12 => "present",
        PresentBusy = 13 => "present_busy",
        DisplayPhase = 14 => "display_phase",

        LayoutResolveStyles = 15 => "layout_resolve_styles",
        LayoutClearMeasureCaches = 16 => "layout_clear_measure_caches",
        LayoutFlow = 17 => "layout_flow",
        LayoutFinishBounds = 18 => "layout_finish_bounds",

        ReaderChapterTransition = 19 => "reader_chapter_transition",
        ReaderChapterFind = 20 => "reader_chapter_find",
        ReaderChapterLoad = 21 => "reader_chapter_load",
        ReaderChapterStyles = 22 => "reader_chapter_styles",
        ReaderChapterImages = 23 => "reader_chapter_images",
        ReaderChapterPaginate = 24 => "reader_chapter_paginate",
        ReaderChapterRegisterImages = 25 => "reader_chapter_register_images",
        ReaderChapterApply = 26 => "reader_chapter_apply",
    }
}

define_named_u8_enum! {
    pub enum TraceMetric {
        TextMeasure = 0 => "text_measure",

        BidiBuildRuns = 1 => "bidi_build_runs",
        BidiResolveLevels = 2 => "bidi_resolve_levels",
        BidiMirror = 3 => "bidi_mirror",
        BidiReorder = 4 => "bidi_reorder",

        FontResolve = 5 => "font_resolve",
        PairPositioning = 6 => "pair_positioning",
        TtfFaceParse = 7 => "ttf_face_parse",
        GposPairLookup = 8 => "gpos_pair_lookup",
        LegacyKerning = 9 => "legacy_kerning",

        MarkAnchors = 10 => "mark_anchors",
        MarkMetrics = 11 => "mark_metrics",
    }
}

define_named_u8_enum! {
    pub enum TraceAggregate {
        ReaderMeasureText = 0 => "reader_measure_text",
        ReaderMeasureTextBytes = 1 => "reader_measure_text_bytes",

        ReaderNextBoundary = 2 => "reader_next_boundary",
        ReaderNextBoundaryBytes = 3 => "reader_next_boundary_bytes",

        ReaderLineHeight = 4 => "reader_line_height",

        ReaderPaginationBlocks = 5 => "reader_pagination_blocks",
        ReaderPaginationContentChars = 6 => "reader_pagination_content_chars",
        ReaderPaginationWords = 7 => "reader_pagination_words",
        ReaderPaginationWhitespaceRuns = 8 => "reader_pagination_whitespace_runs",
        ReaderPaginationOversizedWords = 9 => "reader_pagination_oversized_words",
        ReaderPaginationTextFragments = 10 => "reader_pagination_text_fragments",
        ReaderPaginationLines = 11 => "reader_pagination_lines",
        ReaderPaginationPages = 12 => "reader_pagination_pages",
        ReaderPaginationImages = 13 => "reader_pagination_images",

        ReaderMeasureResolveFont = 14 => "reader_measure_resolve_font",
        ReaderMeasureShape = 15 => "reader_measure_shape",

        ReaderPaginationTextRuns = 16 => "reader_pagination_text_runs",
        ReaderPaginationTextRunBytes = 17 => "reader_pagination_text_run_bytes",

        ReaderMeasureCacheHit = 18 => "reader_measure_cache_hit",
        ReaderMeasureCacheMiss = 19 => "reader_measure_cache_miss",
        ReaderMeasureCacheCollision = 20 => "reader_measure_cache_collision",
        ReaderMeasureCacheBypass = 21 => "reader_measure_cache_bypass",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_phase_ids_round_trip() {
        for phase in DisplayPhase::ALL {
            assert_eq!(DisplayPhase::from_id(phase.id()), Some(*phase));
        }

        assert_eq!(DisplayPhase::from_id(u8::MAX), None);
    }

    #[test]
    fn trace_event_ids_round_trip() {
        for event in TraceEvent::ALL {
            assert_eq!(TraceEvent::from_id(event.id()), Some(*event));
        }

        assert_eq!(TraceEvent::from_id(u8::MAX), None);
    }

    #[test]
    fn trace_metric_ids_round_trip() {
        for metric in TraceMetric::ALL {
            assert_eq!(TraceMetric::from_id(metric.id()), Some(*metric));
        }

        assert_eq!(TraceMetric::from_id(u8::MAX), None);
    }

    #[test]
    fn trace_aggregate_ids_round_trip() {
        for aggregate in TraceAggregate::ALL {
            assert_eq!(TraceAggregate::from_id(aggregate.id()), Some(*aggregate),);
        }

        assert_eq!(TraceAggregate::from_id(u8::MAX), None);
    }
}
