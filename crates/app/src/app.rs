use inkpaper_ui::prelude::*;

use crate::screens::home::{HomeScreen, HomeScreenProps};

pub struct InkPaperApp;

impl Default for InkPaperApp {
    fn default() -> Self {
        Self
    }
}

impl Render for InkPaperApp {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        // TODO: screen selection, navigation, and app behavior

        rsx! {
            <HomeScreen />
        }
    }
}
