use inkpaper_ui::prelude::*;

use crate::keyboard::{Key, Layer};

// printable ASCII, so a key's label can borrow its character
const PRINTABLE: &str = " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~";

/// A touch keyboard, laid out like crosspoint's. Every key shares `on_key`;
/// the key's code is its element id.
#[component]
pub(crate) struct Keyboard<'a> {
    rows: [&'static [Key]; 5],
    layer: Layer,
    shift: bool,
    submit_label: &'a str,
    can_submit: bool,
    on_key: Listener<ActivateEvent>,
}

impl RenderOnce for Keyboard<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let Self {
            rows,
            layer,
            shift,
            submit_label,
            can_submit,
            on_key,
        } = self;

        let rows = rows.into_iter().map(move |row| {
            let keys = row.iter().map(move |&key| {
                KeyButton::from(KeyButtonProps {
                    key,
                    label: label(key, layer, shift, submit_label),
                    width: width(key),
                    inverted: (key == Key::Shift && shift) || (key == Key::Submit && can_submit),
                    // the submit key waits for enough text
                    on_key: (key != Key::Submit || can_submit).then_some(on_key),
                })
            });

            div()
                .w_full()
                .h(px(58))
                .flex()
                .justify_center()
                .gap(px(4))
                .children(keys)
        });

        div().w_full().flex().flex_col().gap(px(4)).children(rows)
    }
}

fn label(key: Key, layer: Layer, shift: bool, submit_label: &str) -> &str {
    match key {
        Key::Char(b' ') => "space",
        Key::Char(byte) => {
            let byte = if shift {
                byte.to_ascii_uppercase()
            } else {
                byte
            };
            let index = usize::from(byte - b' ');

            PRINTABLE.get(index..index + 1).unwrap_or("")
        }
        Key::Shift => "Shift",
        Key::Symbols => "?123",
        Key::Letters => "abc",
        Key::Page => match layer {
            Layer::Symbols(0) => "1/2",
            _ => "2/2",
        },
        Key::Delete => "Del",
        Key::Submit => submit_label,
    }
}

// a character key is 42 px; ten of them with 4 px gaps span 456 px
fn width(key: Key) -> Pixels {
    px(match key {
        Key::Char(b' ') => 272,
        Key::Char(_) => 42,
        Key::Shift | Key::Page | Key::Delete => 63,
        Key::Symbols | Key::Letters | Key::Submit => 88,
    })
}

#[component]
struct KeyButton<'a> {
    key: Key,
    label: &'a str,
    width: Pixels,
    /// black, for a Shift that's on and a submit key that can be pressed
    inverted: bool,
    on_key: Option<Listener<ActivateEvent>>,
}

impl RenderOnce for KeyButton<'_> {
    fn render(self, _: &AppContext<'_>) -> impl IntoElement {
        let (background, ink) = if self.inverted {
            (Color::BLACK, Color::WHITE)
        } else if self.on_key.is_some() {
            (Color::WHITE, Color::BLACK)
        } else {
            (Color::WHITE, Color::rgb(170, 170, 170))
        };

        let width = self.width;

        rsx! {
            <div class="relative h-[58px] w-{width}">
                <div class="absolute left-0 top-0 w-{width} h-[58px] rounded-md border-2 border-{color:ink} bg-{background} flex items-center justify-center">
                    <text class="text-xl no-wrap text-{color:ink}">
                        {self.label}
                    </text>
                </div>

                {#if let Some(listener) = self.on_key}
                    <div
                        id={("key", self.key.code())}
                        on:activate={listener}
                        class="absolute left-0 top-0 w-{width} h-[58px]"
                    />
                {/if}
            </div>
        }
    }
}
