use embassy_executor::Spawner;
use embassy_time::{Delay as AsyncDelay, Duration, Timer};
use epd_bus::SpiEpdBus;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    gpio::{Input, InputConfig, Pull},
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
use static_cell::StaticCell;
use xteink_display_probe::{Verdict, detect_x4_controller};

use crate::firmware::{
    buttons::{BUTTON_EVENTS, Button, ButtonEdge, ButtonEvent, Buttons, button_task},
    display::X4Panel,
    framebuffer::FRAMEBUFFER_LEN,
    power::PowerRails,
    presenter::{Presenter, UiRuntime},
    probe::ProbePins,
};

mod buttons;
mod display;
mod framebuffer;
mod power;
mod presenter;
mod probe;

esp_bootloader_esp_idf::esp_app_desc!();

static FRAMEBUFFER: StaticCell<[u8; FRAMEBUFFER_LEN]> = StaticCell::new();
static UI_RUNTIME: StaticCell<UiRuntime> = StaticCell::new();

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // establish the board's safe rail state before doing anything else
    let _rails = PowerRails::new(peripherals.GPIO1, peripherals.GPIO2, peripherals.GPIO5);

    let timer_group = TimerGroup::new(peripherals.TIMG0);
    let software_interrupt = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);

    esp_rtos::start(timer_group.timer0, software_interrupt.software_interrupt0);

    {
        let buttons = Buttons::new(
            Input::new(
                peripherals.GPIO0,
                InputConfig::default().with_pull(Pull::Up),
            ),
            Input::new(
                peripherals.GPIO7,
                InputConfig::default().with_pull(Pull::Up),
            ),
            Input::new(
                peripherals.GPIO3,
                InputConfig::default().with_pull(Pull::Up),
            ),
        );
        spawner.spawn(button_task(buttons).unwrap());
    }

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

    let mut panel = X4Panel::new(detection.controller);

    println!("initializing display...");
    panel.initialize(&mut bus, &mut delay).await.unwrap();
    println!("display initialized");

    let frame = FRAMEBUFFER.init_with(|| [0xff; FRAMEBUFFER_LEN]);
    let runtime = UI_RUNTIME.init_with(UiRuntime::default);
    runtime.set_global(Theme::EINK).unwrap();

    let model = demo_model();
    let app = runtime.create(move |_| InkPaperApp::new(model)).unwrap();
    let mut presenter = Presenter::default();

    println!("building initial InkPaper frame...");
    let update = presenter.render_initial(runtime, app, frame);
    let damage = update.physical_damage();
    println!(
        "physical damage: x={} y={} w={} h={}",
        damage.x, damage.y, damage.width, damage.height,
    );

    println!("presenting initial frame...");
    panel
        .present(&mut bus, &mut delay, frame, update)
        .await
        .unwrap();
    println!("initial display complete");

    loop {
        // sleep completely until physical input arrives
        let event = BUTTON_EVENTS.receive().await;
        handle_button_event(runtime, event);

        // if the user pressed buttons while the previous e-ink refresh was running,
        // coalesce all queued input before painting
        while let Ok(event) = BUTTON_EVENTS.try_receive() {
            handle_button_event(runtime, event);
        }

        let Some(update) = presenter.render_pending(runtime, app, frame) else {
            continue;
        };

        let damage = update.physical_damage();
        println!(
            "UI update: {:?}, physical x={} y={} w={} h={}",
            update.refresh(),
            damage.x,
            damage.y,
            damage.width,
            damage.height,
        );

        panel
            .present(&mut bus, &mut delay, frame, update)
            .await
            .unwrap();
    }
}

fn demo_model() -> AppModel {
    let book = BookSummary::try_new("The Left Hand of Darkness", "Ursula K. Le Guin", 68)
        .expect("demo book metadata must fit");

    AppModel::new(72, book)
}

async fn stay_alive() -> ! {
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

fn handle_button_event(runtime: &mut UiRuntime, event: ButtonEvent) {
    match (event.button(), event.edge()) {
        (Button::Left, ButtonEdge::Pressed) => {
            println!("button: left");
            runtime.focus_previous();
        }
        (Button::Right, ButtonEdge::Pressed) => {
            println!("button: right");
            runtime.focus_next();
        }
        (Button::Power, ButtonEdge::Pressed) => {
            // this will become suspend/awake once the power lifecycle is implemented
            println!("button: power");
        }
        (_, ButtonEdge::Released) => {
            // releases are intentionally retained by the hardware abstraction event
            // though focus navigation doesn't need them yet.
            // they will matter for long press and power handling
        }
    }
}
