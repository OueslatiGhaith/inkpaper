#![deny(unused_braces)]

use inkpaper_ui::prelude::*;

fn assert_into_element(element: impl IntoElement) {
    let _ = element;
}

fn main() {
    let source = ImageSource::new(ImageId::new(1), Size::new(px(120), px(180)));

    let title = "Continue reading";

    let tree = rsx! {
        <div class="flex flex-col gap-4 p-4">
            <text class="text-lg text-zinc-700">
                {title}
            </text>

            <image
                source={source}
                class="w-24 h-32 object-cover grayscale"
            />
        </div>
    };

    assert_into_element(tree);
}
