use embassy_executor::Spawner;
use embassy_time::{Delay as AsyncDelay, Duration, Timer};
use embedded_graphics::mono_font::{
    MonoFont,
    ascii::{FONT_6X10, FONT_10X20},
};
use epd_bus::SpiEpdBus;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    gpio::{Input, InputConfig},
    interrupt::software::SoftwareInterruptControl,
    spi::{
        Mode,
        master::{Config as SpiConfig, Spi},
    },
    time::Rate,
    timer::timg::TimerGroup,
};
use esp_println::println;
use inkpaper_app::{AppModel, BookSummary, InkPaperApp, theme::Theme};
use inkpaper_ui::{backend::EmbeddedGraphicsPainter, prelude::*};
use ssd1677::{GDEQ0426T82, RefreshMode as SsdRefreshMode, Ssd1677};
use static_cell::StaticCell;
use uc8179::{RefreshMode as Uc8179RefreshMode, Uc8179, X4_PRO_800X480 as UC8179_X4_PRO};
use uc8279_x4::{RefreshMode as Uc8279RefreshMode, Uc8279X4, X4_PRO_800X480 as UC8279_X4_PRO};
use xteink_display_probe::{Controller, Verdict, detect_x4_controller};

use crate::firmware::{
    framebuffer::{FRAMEBUFFER_LEN, Framebuffer, Orientation},
    power::PowerRails,
    probe::ProbePins,
};

mod framebuffer;
mod power;
mod probe;
mod test_pattern;

esp_bootloader_esp_idf::esp_app_desc!();

const DISPLAY_WIDTH: i32 = 480;
const DISPLAY_HEIGHT: i32 = 800;

const DISPLAY_SIZE: Size = Size::new(px(DISPLAY_WIDTH), px(DISPLAY_HEIGHT));

const FONTS: [&MonoFont; 2] = [&FONT_6X10, &FONT_10X20];

type UiRuntime = Runtime<
    4_096, // entity bytes
    8,     // entity slots
    2_048, // callback bytes
    16,    // callback slots
    96,    // frame nodes
    2_048, // frame text bytes
    32,    // persistent element states
    256,   // global bytes
    4,     // global slots
>;

static FRAMEBUFFER: StaticCell<[u8; FRAMEBUFFER_LEN]> = StaticCell::new();
static UI_RUNTIME: StaticCell<UiRuntime> = StaticCell::new();

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // establish the board's safe rail state before doing anything else
    let _rails = PowerRails::new(peripherals.GPIO1, peripherals.GPIO2, peripherals.GPIO5);

    let timer_group = TimerGroup::new(peripherals.TIMG0);
    let software_interrupt = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);

    esp_rtos::start(timer_group.timer0, software_interrupt.software_interrupt0);

    let mut delay = AsyncDelay;

    println!();
    println!("InkPaper X4 Pro");
    println!("----------------");
    println!("probing display controller...");

    let mut probe_io = ProbePins::new(
        peripherals.GPIO12,
        peripherals.GPIO11,
        peripherals.GPIO13,
        peripherals.GPIO18,
        peripherals.GPIO14,
    );

    let detection = detect_x4_controller(&mut probe_io, &mut delay)
        .await
        .unwrap();

    println!("controller: {:?}", detection.controller);
    println!("verdict: {:?}", detection.verdict);
    println!("VER: {:02x?}", detection.diagnostics.version.bytes());
    println!("FLG: 0x{:02x}", detection.diagnostics.flags.raw());
    println!("MTP read: {}", detection.diagnostics.mtp.is_some());

    // an inconclusive result means something on the desplay bus responded, but not
    // consistently enough to identify it safely. Do not send an SSD1677 initialization
    // sequence to potential UC silicon
    if detection.verdict == Verdict::Inconclusive {
        println!("display probe inconclusive, refusing to drive panel");
        stay_alive().await;
    }

    // the probe has finished using the display pins
    let (sclk, mosi, cs, dc, reset) = probe_io.into_parts();
    let spi = Spi::new(
        peripherals.SPI2,
        SpiConfig::default()
            .with_frequency(Rate::from_mhz(5))
            .with_mode(Mode::_0),
    )
    .unwrap()
    .with_sck(sclk.into_peripheral_output())
    .with_mosi(mosi.into_peripheral_output())
    .into_async();

    let busy = Input::new(peripherals.GPIO6, InputConfig::default());
    let mut bus = SpiEpdBus::new(spi, cs, dc, reset, busy).unwrap();

    println!("painting 800x480 diagnostic pattern...");
    println!("expected: 1 block TL, 2 bars TR, 3 bars BL, 4 squares BR");

    let frame = FRAMEBUFFER.init_with(|| [0xff; FRAMEBUFFER_LEN]);
    let runtime = UI_RUNTIME.init_with(UiRuntime::default);

    runtime.set_global(Theme::EINK).unwrap();
    let model = demo_model();
    let app = runtime.create(move |_| InkPaperApp::new(model)).unwrap();

    println!("building InkPaper application...");
    render_ui(runtime, app, frame);
    println!("InkPaper UI rendered to framebuffer");
    println!("painting 480x800 portrait UI...");

    match detection.controller {
        Controller::Ssd1677 => {
            println!("initializing SSD1677");
            let mut panel = Ssd1677::new(GDEQ0426T82);
            panel.initialize(&mut bus, &mut delay).await.unwrap();
            println!("SSD1677 initialized");

            panel
                .display(&mut bus, &mut delay, frame, None, SsdRefreshMode::Full)
                .await
                .unwrap();
        }
        Controller::Uc8179 => {
            println!("initializing UC8179...");
            let mut panel = Uc8179::new(UC8179_X4_PRO);
            panel.initialize(&mut bus, &mut delay).await.unwrap();
            println!("UC8179 initialized");

            panel
                .display(&mut bus, &mut delay, frame, Uc8179RefreshMode::Full, true)
                .await
                .unwrap();
        }
        Controller::Uc8279 => {
            println!("initializing UC8279-X4...");
            let mut panel = Uc8279X4::new(UC8279_X4_PRO);
            panel.initialize(&mut bus, &mut delay).await.unwrap();
            println!("UC8279-X4 initialized");

            panel
                .display(&mut bus, &mut delay, frame, Uc8279RefreshMode::Full, true)
                .await
                .unwrap();
        }
    };

    println!("display refresh complete");
    println!("bring-up successful");

    stay_alive().await
}

fn demo_model() -> AppModel {
    let book = BookSummary::try_new("The Left Hand of Darkness", "Ursula K. Le Guin", 68)
        .expect("demo book metadata must fit");

    AppModel::new(73, book)
}

fn render_ui(runtime: &mut UiRuntime, app: Entity<InkPaperApp>, frame: &mut [u8; FRAMEBUFFER_LEN]) {
    let mut display = Framebuffer::new(frame, Orientation::Portrait);
    let mut painter = EmbeddedGraphicsPainter::new(&mut display, FONTS, []);

    runtime.rebuild(app).unwrap();
    runtime.layout(DISPLAY_SIZE, &painter).unwrap();

    let damage = DamageRegion::full();

    painter.clear_damage(damage, Color::WHITE).unwrap();

    runtime
        .paint_with_damage(damage, &mut painter)
        .unwrap()
        .unwrap();
}

async fn stay_alive() -> ! {
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
