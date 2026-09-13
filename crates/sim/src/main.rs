use std::{fs::File, io::Read, path::Path};

use clap::Parser;
use embedded_graphics::{
    geometry::{Point as EgPoint, Size as EgSize},
    mono_font::ascii::FONT_6X10,
    pixelcolor::Rgb888,
};
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
};
use inkpaper_app::InkPaperApp;
use inkpaper_ui::{
    FontData, FontFace, TtfFont,
    backend::{CoverageMode, EmbeddedGraphicsPainter, MonoFontFace},
    prelude::*,
};
use static_cell::StaticCell;

use crate::{args::SimulatorArgs, runtime::new_runtime};

mod args;
mod runtime;

const DISPLAY_WIDTH: u32 = 480;
const DISPLAY_HEIGHT: u32 = 800;

const DISPLAY_SIZE_EG: EgSize = EgSize::new(DISPLAY_WIDTH, DISPLAY_HEIGHT);
const DISPLAY_SIZE: Size = Size::new(px(DISPLAY_WIDTH as i32), px(DISPLAY_HEIGHT as i32));

const MAX_RUNTIME_FONT_BYTES: usize = 5 * 1024 * 1024;

static RUNTIME_FONT_BYTES: StaticCell<Box<[u8]>> = StaticCell::new();
static RUNTIME_FONT: StaticCell<TtfFont> = StaticCell::new();

static FALLBACK_FONT: MonoFontFace<'static> = MonoFontFace::ascii(&FONT_6X10);

fn runtime_font_path(path: Option<&Path>) -> Option<&'static TtfFont<'static>> {
    let path = path?;

    let data = load_font_data(path);
    let font = TtfFont::parse(data, 0).unwrap_or_else(|error| {
        panic!(
            "{} is not a supported TTF/OTF face: {error:?}",
            path.display()
        )
    });

    let font = RUNTIME_FONT.init(font);

    eprintln!("runtime font: {} ({} bytes)", path.display(), data.len());

    Some(font)
}

fn load_font_data(path: &Path) -> FontData<'static> {
    let mut file = File::open(path)
        .unwrap_or_else(|error| panic!("failed to open font {}: {error}", path.display()));
    let metadata = file
        .metadata()
        .unwrap_or_else(|error| panic!("failed to stat font {}: {error}", path.display()));
    let file_len = usize::try_from(metadata.len()).expect("font file must fit usize");

    assert!(file_len > 0, "font {} is empty", path.display());
    assert!(
        file_len <= MAX_RUNTIME_FONT_BYTES,
        "font {} is {file_len} bytes, but simulator storage is limited to \
         {MAX_RUNTIME_FONT_BYTES} bytes",
        path.display()
    );

    let mut storage = vec![0; file_len].into_boxed_slice();
    file.read_exact(&mut storage)
        .unwrap_or_else(|error| panic!("failed to read font {}: {error}", path.display()));

    let storage: &'static [u8] = RUNTIME_FONT_BYTES.init(storage);

    FontData::new(storage)
}

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

fn paint_ui<R>(runtime: &mut R, display: &mut SimulatorDisplay<Rgb888>, damage: DamageRegion)
where
    R: RenderRuntimeApi,
    for<'target> EmbeddedGraphicsPainter<'target, SimulatorDisplay<Rgb888>>:
        ResourcePainter<R::Resources>,
{
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

fn rebuild_ui<R>(runtime: &mut R, display: &mut SimulatorDisplay<Rgb888>)
where
    R: RenderRuntimeApi,
    for<'target> EmbeddedGraphicsPainter<'target, SimulatorDisplay<Rgb888>>:
        ResourcePainter<R::Resources>,
{
    runtime.rebuild().unwrap();
    runtime.layout(DISPLAY_SIZE).unwrap();

    paint_ui(runtime, display, DamageRegion::full());
}

fn main() {
    let args = SimulatorArgs::parse();

    let mut runtime = new_runtime();

    let font: &'static dyn FontFace = match runtime_font_path(args.font.as_deref()) {
        Some(font) => font,
        None => &FALLBACK_FONT,
    };

    let font_id = runtime
        .register_font(font)
        .expect("simulator font slot must fit");

    assert_eq!(
        font_id,
        FontId::DEFAULT,
        "the mockup expects its default UI font to occupy font slot zero"
    );

    let _app = runtime
        .create_root(|_| InkPaperApp)
        .expect("InkPaper application root must fit");

    let mut display = SimulatorDisplay::<Rgb888>::new(DISPLAY_SIZE_EG);

    rebuild_ui(runtime.as_mut(), &mut display);

    let output_settings = OutputSettingsBuilder::new().scale(1).build();

    let mut window = Window::new("InkPaper", &output_settings);

    'running: loop {
        window.update(&display);

        for event in window.events() {
            if matches!(event, SimulatorEvent::Quit) {
                break 'running;
            }
        }
    }
}
