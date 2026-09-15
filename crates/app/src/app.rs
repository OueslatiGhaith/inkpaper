use inkpaper_ui::{FontRegistryError, prelude::*};

use crate::screens::recent_books::{RecentBooksScreen, RecentBooksScreenProps};

pub struct InkPaperApp;

impl Default for InkPaperApp {
    fn default() -> Self {
        Self
    }
}

impl InkPaperApp {
    pub fn register_resources<'resource>(
        runtime: &mut impl ResourceRuntimeApi<'resource>,
    ) -> Result<(), FontRegistryError> {
        crate::typography::register(runtime)
    }
}

impl Render for InkPaperApp {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        // TODO: screen selection, navigation, and app behavior

        rsx! {
            <RecentBooksScreen />
        }
    }
}
