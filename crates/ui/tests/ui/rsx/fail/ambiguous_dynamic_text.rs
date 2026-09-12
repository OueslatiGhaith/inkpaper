use inkpaper_ui::prelude::*;

fn main() {
    let value = Color::BLACK;

    let _ = rsx! {
        <text class="text-{value}">
            "Hello"
        </text>
    };
}
