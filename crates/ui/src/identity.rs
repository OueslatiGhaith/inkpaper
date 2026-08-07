#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementId {
    Name(&'static str),
    Value(u64),
    NamedValue(&'static str, u64),
}

pub trait IntoElementId {
    fn into_element_id(self) -> ElementId;
}

impl IntoElementId for &'static str {
    fn into_element_id(self) -> ElementId {
        ElementId::Name(self)
    }
}

macro_rules! impl_integer_element_id {
    ($($ty:ty),* $(,)?) => {
        $(
            impl IntoElementId for $ty {
                fn into_element_id(self) -> ElementId {
                    ElementId::Value(self as u64)
                }
            }

            impl IntoElementId for (&'static str, $ty) {
                fn into_element_id(self) -> ElementId {
                    ElementId::NamedValue(self.0, self.1 as u64)
                }
            }
        )*
    };
}

impl_integer_element_id! { u8, u16, u32, u64, usize }
