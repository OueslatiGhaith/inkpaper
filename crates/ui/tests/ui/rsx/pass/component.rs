#![deny(unused_braces)]

use inkpaper_ui::prelude::*;

struct Badge<'a> {
    label: &'a str,
}

struct BadgeProps<'a> {
    label: &'a str,
}

impl<'a> From<BadgeProps<'a>> for Badge<'a> {
    fn from(props: BadgeProps<'a>) -> Self {
        Self { label: props.label }
    }
}

impl RenderOnce for Badge<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        div().px(px(8)).child(self.label)
    }
}

fn assert_into_element(element: impl IntoElement) {
    let _ = element;
}

fn main() {
    let label = "New";

    let tree = rsx! {
        <Badge label={label} />
    };

    assert_into_element(tree);
}
