#![deny(unused_braces)]

use inkpaper_ui::prelude::*;

#[derive(Debug, Clone, Copy)]
struct BookSelectedEvent;

struct App;

impl App {
    fn activated(&mut self, _: &ActivateEvent, _: &mut Context<'_, Self>) {}
    fn book_selected(&mut self, _: &BookSelectedEvent, _: &mut Context<'_, Self>) {}
}

impl Render for App {
    fn render<'a>(&'a mut self, cx: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        let activate = cx.listener(Self::activated);
        let selected = cx.listener(Self::book_selected);

        rsx! {
            <div
                id="book"
                on:activate={activate}
                on:BookSelectedEvent={selected}
            />
        }
    }
}

fn main() {}
