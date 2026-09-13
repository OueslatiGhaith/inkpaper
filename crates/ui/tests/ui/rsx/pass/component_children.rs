#![deny(unused_braces)]

use inkpaper_ui::prelude::*;

struct Card<'a, C = NoChildren> {
    title: &'a str,
    children: C,
}

struct CardProps<'a> {
    title: &'a str,
}

impl<'a> From<CardProps<'a>> for Card<'a, NoChildren> {
    fn from(props: CardProps<'a>) -> Self {
        Self {
            title: props.title,
            children: NoChildren,
        }
    }
}

impl<'a, C> ComponentChildren<C> for Card<'a, NoChildren>
where
    C: Children,
{
    type WithChildren = Card<'a, C>;

    fn with_children(self, children: C) -> Self::WithChildren {
        Card {
            title: self.title,
            children,
        }
    }
}

impl<C> RenderOnce for Card<'_, C>
where
    C: Children,
{
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        div().child(self.title).child_sequence(self.children)
    }
}

fn assert_into_element(element: impl IntoElement) {
    let _ = element;
}

fn main() {
    let visible = true;
    let items = ["One", "Two"];

    let empty = rsx! {
        <Card title="Empty" />
    };

    let populated = rsx! {
        <Card title="Library">
            <text>"Before"</text>

            {#if visible}
                <text>"Visible"</text>
            {/if}

            {#each items as item}
                <text>{item}</text>
            {/each}
        </Card>
    };

    assert_into_element(empty);
    assert_into_element(populated);
}
