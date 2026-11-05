use embedded_graphics::{
    geometry::{Point as EgPoint, Size as EgSize},
    pixelcolor::Rgb888,
};
use embedded_graphics_simulator::SimulatorDisplay;
use inkpaper_ui::{
    backend::{CoverageMode, EmbeddedGraphicsPainter},
    prelude::*,
};

const DISPLAY_WIDTH: u32 = 480;
const DISPLAY_HEIGHT: u32 = 800;

pub(super) const DISPLAY_SIZE_EG: EgSize = EgSize::new(DISPLAY_WIDTH, DISPLAY_HEIGHT);
const DISPLAY_SIZE: Size = Size::new(px(DISPLAY_WIDTH as i32), px(DISPLAY_HEIGHT as i32));

fn read_simulator_pixel(display: &SimulatorDisplay<Rgb888>, point: EgPoint) -> Option<Rgb888> {
    if point.x < 0
        || point.y < 0
        || point.x >= DISPLAY_WIDTH as i32
        || point.y >= DISPLAY_HEIGHT as i32
    {
        return None;
    }

    Some(display.get_pixel(point))
}

/// paints what changed since the last frame
pub(super) fn render_ui<R>(runtime: &mut R, display: &mut SimulatorDisplay<Rgb888>)
where
    R: RenderRuntimeApi,
{
    let damage = runtime
        .prepare_frame(DISPLAY_SIZE)
        .expect("UI rebuild failed");
    if damage.is_none() {
        return;
    }

    let mut painter = EmbeddedGraphicsPainter::new(display)
        .with_coverage_mode(CoverageMode::alpha_blend(read_simulator_pixel));

    painter.clear_damage(damage, Color::WHITE).unwrap();

    match runtime.paint_with_damage(damage, &mut painter) {
        Ok(Some(_)) => {}
        Ok(None) => panic!("painting requires a mounted root"),
        Err(_) => panic!("UI painting failed"),
    }
}

pub(super) fn ui_point(point: EgPoint) -> Point {
    Point::new(px(point.x), px(point.y))
}
