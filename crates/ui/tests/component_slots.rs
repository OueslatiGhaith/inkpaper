use inkpaper_ui::prelude::*;

type TestRuntime = Runtime<
    4096, // entity bytes
    1,    // entity slots
    4096, // callback bytes
    16,   // callback slots
    64,   // frame nodes
    1024, // frame text bytes
    32,   // element states
>;

struct Card<'a, Content = EmptySlot> {
    title: &'a str,
    content: ComponentSlot<Content>,
}

impl<'a> Card<'a, EmptySlot> {
    fn new(title: &'a str) -> Self {
        Self {
            title,
            content: ComponentSlot::empty(),
        }
    }

    fn child<Content>(self, content: Content) -> Card<'a, Content>
    where
        Content: IntoElement,
    {
        Card {
            title: self.title,
            content: self.content.fill(content),
        }
    }
}

impl<Content> RenderOnce for Card<'_, Content>
where
    Content: IntoElement,
{
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        div()
            .w_full()
            .p(px(6))
            .gap(px(4))
            .bg(Color::rgb(35, 40, 50))
            .rounded(px(4))
            .child(text(self.title).text_color(Color::WHITE))
            .child(self.content.into_inner())
    }
}

struct App;

impl Render for App {
    fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
        div()
            .w_full()
            .h_full()
            .child(Card::new("Settings").child("Typed card content"))
    }
}

#[test]
fn component_can_require_typed_child_content() {
    let mut runtime = TestRuntime::default();

    runtime.create_root(|_| App).unwrap();

    runtime.rebuild().unwrap();

    assert!(runtime.frame_node_count() >= 4);
    assert!(runtime.frame_text_bytes_used() > 0);
}

struct Badge<'a> {
    label: &'a str,
}

impl RenderOnce for Badge<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        div()
            .p(px(3))
            .bg(Color::rgb(50, 110, 80))
            .rounded(px(3))
            .child(self.label)
    }
}

#[test]
fn component_slot_accepts_render_once_content() {
    struct BadgeApp;

    impl Render for BadgeApp {
        fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div()
                .w_full()
                .h_full()
                .child(Card::new("Connection").child(Badge { label: "Connected" }))
        }
    }

    let mut runtime = TestRuntime::default();

    runtime.create_root(|_| BadgeApp).unwrap();

    runtime.rebuild().unwrap();

    assert!(runtime.frame_node_count() >= 5);
    assert!(runtime.frame_text_bytes_used() > 0);
}

struct SplitRow<Leading = EmptySlot, Trailing = EmptySlot> {
    leading: ComponentSlot<Leading>,
    trailing: ComponentSlot<Trailing>,
}

impl SplitRow<EmptySlot, EmptySlot> {
    fn new() -> Self {
        Self {
            leading: ComponentSlot::empty(),
            trailing: ComponentSlot::empty(),
        }
    }
}

impl<Trailing> SplitRow<EmptySlot, Trailing> {
    fn leading<Leading>(self, leading: Leading) -> SplitRow<Leading, Trailing>
    where
        Leading: IntoElement,
    {
        SplitRow {
            leading: self.leading.fill(leading),
            trailing: self.trailing,
        }
    }
}

impl<Leading> SplitRow<Leading, EmptySlot> {
    fn trailing<Trailing>(self, trailing: Trailing) -> SplitRow<Leading, Trailing>
    where
        Trailing: IntoElement,
    {
        SplitRow {
            leading: self.leading,
            trailing: self.trailing.fill(trailing),
        }
    }
}

impl<Leading, Trailing> RenderOnce for SplitRow<Leading, Trailing>
where
    Leading: IntoElement,
    Trailing: IntoElement,
{
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(self.leading.into_inner())
            .child(self.trailing.into_inner())
    }
}

#[test]
fn component_can_have_multiple_named_typed_slots() {
    struct RowApp;

    impl Render for RowApp {
        fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div().w_full().h_full().child(
                SplitRow::new()
                    .leading("Wi-Fi")
                    .trailing(Badge { label: "Connected" }),
            )
        }
    }

    let mut runtime = TestRuntime::default();

    runtime.create_root(|_| RowApp).unwrap();

    runtime.rebuild().unwrap();

    assert!(runtime.frame_node_count() >= 5);
    assert!(runtime.frame_text_bytes_used() > 0);
}

#[test]
fn named_slots_can_be_filled_in_either_order() {
    struct ReverseSlotApp;

    impl Render for ReverseSlotApp {
        fn render<'a>(&'a mut self, _: &mut Context<'_, Self>) -> impl IntoElement + 'a {
            div()
                .w_full()
                .h_full()
                .child(SplitRow::new().trailing("Ready").leading("Status"))
        }
    }

    let mut runtime = TestRuntime::default();

    runtime.create_root(|_| ReverseSlotApp).unwrap();

    runtime.rebuild().unwrap();

    assert!(runtime.frame_node_count() >= 4);
    assert!(runtime.frame_text_bytes_used() > 0);
}
