#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pixels(pub i32);

pub const fn px(value: i32) -> Pixels {
    Pixels(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Length {
    Auto,
    Pixels(Pixels),
    Fill,
}

impl From<Pixels> for Length {
    fn from(value: Pixels) -> Self {
        Self::Pixels(value)
    }
}
