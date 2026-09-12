pub mod border;
pub mod radius;
pub mod spacing;
pub mod typography;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Spacing,
    BorderWidth,
    Radius,
    Integer,
    FontSize,
    LineHeight,
    Color,
}
