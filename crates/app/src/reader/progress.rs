use alloc::vec::Vec;

use inkpaper_epub::{ContentOffset, SpineIndex};

use crate::BookProgress;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BookProgressMap {
    /// `cumulative[n]` is the total readable resource weight before spine index n.
    ///
    /// the final element is the total book weight.
    cumulative: Vec<u64>,
}

impl BookProgressMap {
    pub(super) fn from_weights(weights: impl IntoIterator<Item = u64>) -> Self {
        let mut cumulative = Vec::new();

        cumulative.push(0);

        let mut total = 0u64;

        for weight in weights {
            total = total.saturating_add(weight);

            cumulative.push(total);
        }

        Self { cumulative }
    }

    pub(super) fn at(
        &self,
        spine: SpineIndex,
        current: ContentOffset,
        chapter_end: ContentOffset,
    ) -> BookProgress {
        let Some(index) = spine.as_usize() else {
            return BookProgress::ZERO;
        };

        let Some(start) = self.cumulative.get(index).copied() else {
            return BookProgress::ZERO;
        };

        let Some(end) = index
            .checked_add(1)
            .and_then(|index| self.cumulative.get(index))
            .copied()
        else {
            return BookProgress::ZERO;
        };

        let total = self.cumulative.last().copied().unwrap_or(0);

        if total == 0 {
            return BookProgress::ZERO;
        }

        let weight = end.saturating_sub(start);

        let chapter_end = chapter_end.get();

        let current = current.get().min(chapter_end);

        let consumed_in_chapter = if weight == 0 || chapter_end == 0 {
            0
        } else {
            u64::try_from((u128::from(weight) * u128::from(current)) / u128::from(chapter_end))
                .unwrap_or(weight)
                .min(weight)
        };

        let consumed = start.saturating_add(consumed_in_chapter).min(total);

        BookProgress::from_ratio(u128::from(consumed), u128::from(total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_is_weighted_by_spine_resource_size() {
        let progress = BookProgressMap::from_weights([100, 300, 600]);

        // 100 bytes from the first chapter plus half of the 300-byte second chapter:
        // 250 / 1000 = 25%.
        assert_eq!(
            progress
                .at(
                    SpineIndex::new(1),
                    ContentOffset::new(50),
                    ContentOffset::new(100),
                )
                .basis_points(),
            2_500,
        );
    }

    #[test]
    fn progress_reaches_complete_at_end_of_final_spine_item() {
        let progress = BookProgressMap::from_weights([100, 300]);

        assert_eq!(
            progress.at(
                SpineIndex::new(1),
                ContentOffset::new(200),
                ContentOffset::new(200),
            ),
            BookProgress::COMPLETE,
        );
    }

    #[test]
    fn zero_weight_spine_items_do_not_affect_progress() {
        let progress = BookProgressMap::from_weights([0, 100, 0, 300]);

        assert_eq!(
            progress
                .at(
                    SpineIndex::new(1),
                    ContentOffset::new(100),
                    ContentOffset::new(100),
                )
                .percent(),
            25,
        );
    }
}
