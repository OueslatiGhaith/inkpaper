use inkpaper_epub::SpineIndex;

use super::state::ReaderChapterDirection;

#[cfg(feature = "trace")]
use inkpaper_trace::{TraceEvent, async_begin, async_end};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChapterTraceStage {
    Find,
    Load,
    Styles,
    Images,
    Paginate,
    RegisterImages,
    Apply,
}

#[cfg(feature = "trace")]
impl ChapterTraceStage {
    const fn event(self) -> TraceEvent {
        match self {
            Self::Find => TraceEvent::ReaderChapterFind,
            Self::Load => TraceEvent::ReaderChapterLoad,
            Self::Styles => TraceEvent::ReaderChapterStyles,
            Self::Images => TraceEvent::ReaderChapterImages,
            Self::Paginate => TraceEvent::ReaderChapterPaginate,
            Self::RegisterImages => TraceEvent::ReaderChapterRegisterImages,
            Self::Apply => TraceEvent::ReaderChapterApply,
        }
    }
}

pub(crate) struct ChapterTraceSpan {
    #[cfg(feature = "trace")]
    event: TraceEvent,

    #[cfg(feature = "trace")]
    id: u32,

    #[cfg(feature = "trace")]
    active: bool,
}

impl ChapterTraceSpan {
    pub(crate) fn start(stage: ChapterTraceStage, id: u32, arg: u32) -> Self {
        #[cfg(feature = "trace")]
        {
            let event = stage.event();
            let active = async_begin(event, id, arg);

            return Self { event, id, active };
        }

        #[cfg(not(feature = "trace"))]
        {
            let _ = (stage, id, arg);

            Self {}
        }
    }
}

impl Drop for ChapterTraceSpan {
    fn drop(&mut self) {
        #[cfg(feature = "trace")]
        {
            if !self.active {
                return;
            }

            let _ = async_end(self.event, self.id);
            self.active = false;
        }
    }
}

pub(crate) const fn chapter_transition_id(
    from: SpineIndex,
    direction: ReaderChapterDirection,
) -> u32 {
    let direction = match direction {
        ReaderChapterDirection::Previous => 0,
        ReaderChapterDirection::Next => 1,
    };

    from.get().wrapping_mul(2).wrapping_add(direction)
}

pub(crate) const fn chapter_transition_arg(
    from: SpineIndex,
    direction: ReaderChapterDirection,
) -> u32 {
    let direction_bit = match direction {
        ReaderChapterDirection::Previous => 0,
        ReaderChapterDirection::Next => 1u32 << 31,
    };

    (from.get() & 0x7fff_ffff) | direction_bit
}

pub(crate) fn spine_trace_id(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}

pub(crate) fn begin_chapter_transition(from: SpineIndex, direction: ReaderChapterDirection) {
    #[cfg(feature = "trace")]
    {
        let _ = async_begin(
            TraceEvent::ReaderChapterTransition,
            chapter_transition_id(from, direction),
            chapter_transition_arg(from, direction),
        );
    }

    #[cfg(not(feature = "trace"))]
    {
        let _ = (from, direction);
    }
}

pub(crate) fn end_chapter_transition(from: SpineIndex, direction: ReaderChapterDirection) {
    #[cfg(feature = "trace")]
    {
        let _ = async_end(
            TraceEvent::ReaderChapterTransition,
            chapter_transition_id(from, direction),
        );
    }

    #[cfg(not(feature = "trace"))]
    {
        let _ = (from, direction);
    }
}
