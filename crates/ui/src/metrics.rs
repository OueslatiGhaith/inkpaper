#[cfg(feature = "metrics")]
use core::cell::Cell;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PerformanceMetrics {
    pub entity_render_calls: u64,
    pub nodes_mounted: u64,

    pub measure_node_calls: u64,
    pub text_measurements: u64,
    pub flex_base_main_size_calls: u64,
    pub flex_item_main_size_calls: u64,
    pub flex_sibling_visits: u64,
    pub nodes_laid_out: u64,

    pub visual_nodes_visited: u64,
    pub visible_nodes: u64,
    pub nodes_painted: u64,
}

#[cfg(feature = "metrics")]
#[derive(Debug, Default)]
pub(crate) struct PerformanceMetricsCell {
    value: Cell<PerformanceMetrics>,
}

#[cfg(feature = "metrics")]
impl PerformanceMetricsCell {
    pub(crate) fn reset(&self) {
        self.value.set(PerformanceMetrics::default());
    }

    pub(crate) fn snapshot(&self) -> PerformanceMetrics {
        self.value.get()
    }

    pub(crate) fn increment(&self, update: impl FnOnce(&mut PerformanceMetrics)) {
        let mut metrics = self.value.get();

        update(&mut metrics);

        self.value.set(metrics);
    }
}

macro_rules! count_metric {
    ($frame:expr, $field:ident) => {{
        #[cfg(feature = "metrics")]
        {
            $frame.metrics.increment(|metrics| {
                metrics.$field = metrics.$field.saturating_add(1);
            });
        }
    }};
}

pub(crate) use count_metric;
