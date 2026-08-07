#![no_std]

mod div;
mod element;
mod style;
mod units;

pub use div::*;
pub use element::*;
pub use style::*;
pub use units::*;

pub mod prelude {
    pub use crate::{IntoElement, ParentElement, Styled, div, px};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_api_compiles() {
        let _ = div()
            .flex()
            .flex_col()
            .w_full()
            .h_full()
            .p(px(8))
            .gap(px(4))
            .child("hello")
            .child(
                div()
                    .flex()
                    .flex_row()
                    .w(px(100))
                    .h(px(40))
                    .p(px(4))
                    .child("Nested"),
            );
    }
}
