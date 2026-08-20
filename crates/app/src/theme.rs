use inkpaper_ui::prelude::*;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub paper: Color,
    pub ink: Color,
    pub surface: Color,
    pub subtle: Color,
    pub muted: Color,
}

impl Global for Theme {}

impl Theme {
    pub const EINK: Self = Self {
        paper: Color::WHITE,
        ink: Color::BLACK,
        surface: Color::rgb(242, 242, 242),
        subtle: Color::rgb(210, 210, 210),
        muted: Color::rgb(96, 96, 96),
    };
}
