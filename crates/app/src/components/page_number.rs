use alloc::format;
use inkpaper_ui::prelude::*;

/// Where a paged list is, as "page/count".
#[component]
pub(crate) struct PageNumber {
    page: usize,
    page_count: usize,
}

impl RenderOnce for PageNumber {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let label = format!("{}/{}", self.page + 1, self.page_count);

        rsx! {
            <text class="text-base no-wrap">
                {label}
            </text>
        }
    }
}
