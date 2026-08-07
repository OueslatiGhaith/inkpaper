#![no_std]

mod div;
mod element;
mod identity;
mod stateful;
mod style;
mod units;

pub use div::*;
pub use element::*;
pub use identity::*;
pub use stateful::*;
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

    #[test]
    fn id_makes_div_stateful() {
        let element = div().id("foo");

        assert_eq!(element.element_id(), ElementId::Name("foo"));
    }

    #[test]
    fn stateful_elements_preserve_capabilities() {
        let _ = div()
            .flex()
            .w_full()
            .child("before")
            .id("panel")
            .flex_col()
            .h_full()
            .p(px(8))
            .child("after");
    }

    #[test]
    fn stateful_child_can_be_nested() {
        let _ = div().child(div().id("button").p(px(4)).child("Press me"));
    }

    #[test]
    fn element_ids_support_values() {
        let a = div().id(42u32);
        assert_eq!(a.element_id(), ElementId::Value(42),);

        let b = div().id(("row", 7usize));
        assert_eq!(b.element_id(), ElementId::NamedValue("row", 7),);
    }
}
