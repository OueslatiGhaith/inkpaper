use embassy_executor::Spawner;
use embassy_time::{Delay as AsyncDelay, Duration, Timer};
use epd_bus::SpiEpdBus;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    gpio::{Input, InputConfig, Pull},
    i2c::master::{Config as I2cConfig, I2c},
    interrupt::software::SoftwareInterruptControl,
    spi::{
        Mode,
        master::{Config as SpiConfig, Spi},
    },
    time::Rate,
    timer::timg::TimerGroup,
};
use esp_println::println;
use inkpaper_app::{AppModel, BookSummary, InkPaperApp, Route, theme::Theme};
use inkpaper_ui::prelude::*;
use static_cell::StaticCell;
use xteink_display_probe::{Verdict, detect_x4_controller};

use crate::firmware::{
    buttons::{Buttons, button_task},
    display::X4Panel,
    framebuffer::FRAMEBUFFER_LEN,
    input::{Button, ButtonEdge, ButtonEvent, INPUT_EVENTS, InputEvent, TouchEvent, TouchPosition},
    power::PowerRails,
    power_button::{ENTER_DEEP_SLEEP, power_button_task},
    presenter::{Presenter, UiRuntime},
    probe::ProbePins,
    touch::{TouchController, touch_task},
};

mod buttons;
mod display;
mod framebuffer;
mod input;
mod power;
mod power_button;
mod presenter;
mod probe;
mod touch;

esp_bootloader_esp_idf::esp_app_desc!();

static FRAMEBUFFER: StaticCell<[u8; FRAMEBUFFER_LEN]> = StaticCell::new();
static UI_RUNTIME: StaticCell<UiRuntime> = StaticCell::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputAction {
    Continue,
    Sleep,
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // establish the board's safe rail state before doing anything else
    let mut rails = PowerRails::new(peripherals.GPIO1, peripherals.GPIO2, peripherals.GPIO5);

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
        );
        spawner.spawn(button_task(buttons).unwrap());
        spawner.spawn(power_button_task(peripherals.GPIO3, peripherals.LPWR).unwrap());
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

    // touch power-up.
    // GPIO2 is active-low. Give the GT911 rail time to settle before
    // performing its address-select reset sequence.
    println!("powering GT911...");
    rails.enable_touch();
    Timer::after(Duration::from_millis(50)).await;

    let i2c = I2c::new(
        peripherals.I2C0,
        I2cConfig::default().with_frequency(Rate::from_khz(400)),
    )
    .unwrap()
    .with_sda(peripherals.GPIO39)
    .with_scl(peripherals.GPIO38)
    .into_async();

    let mut touch = TouchController::new(i2c, peripherals.GPIO4, peripherals.GPIO10);

    match touch.initialize(&mut delay).await {
        Ok(info) => {
            println!("GT911 initialized at 0x{:02x}", touch.address());
            println!("GT911 product: {:02x?}", info.product_id());
            println!("GT911 fw: 0x{:04x}", info.firmware_version());
            println!(
                "GT911 resolution: {}x{}",
                info.x_resolution(),
                info.y_resolution(),
            );
            println!("GT911 vendor: 0x{:02x}", info.vendor_id());

            spawner.spawn(touch_task(touch).unwrap());
        }
        Err(error) => {
            println!("GT911 initialization failed: {error:?}");
            rails.disable_touch();
        }
    }

    loop {
        // sleep completely until physical input arrives
        let event = INPUT_EVENTS.receive().await;
        let mut action = handle_input_event(runtime, app, event);

        // combine events accumulated while the e-ink panel was busy.
        while action == InputAction::Continue {
            let Ok(event) = INPUT_EVENTS.try_receive() else {
                break;
            };
            action = handle_input_event(runtime, app, event);
        }

        if action == InputAction::Sleep {
            println!("preparing display for deep sleep...");

            // ORDER MATTERS:
            // the panel receives DSLP while its control lines and rails are still alive
            panel.deep_sleep(&mut bus, &mut delay).await.unwrap();
            println!("display asleep");
            // only after the panel is asleep do we switch peripheral rails off
            rails.prepare_for_deep_sleep();
            println!("peripheral rails prepared");
            // tell the GPIO3 owner it may wait for button release and transfer the pin
            // to EXT0
            ENTER_DEEP_SLEEP.signal(());
            // `power_button_tasl()` owns the final SoC sleep transition.
            // once it calls `rtc.sleep_deep()`, this entire firmware instance disappears
            stay_alive().await;
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

fn handle_input_event(
    runtime: &mut UiRuntime,
    app: Entity<InkPaperApp>,
    event: InputEvent,
) -> InputAction {
    match event {
        InputEvent::Button(event) => handle_button_event(runtime, event),
        InputEvent::Touch(event) => {
            handle_touch_event(runtime, app, event);
            InputAction::Continue
        }
    }
}

fn handle_button_event(runtime: &mut UiRuntime, event: ButtonEvent) -> InputAction {
    match (event.button(), event.edge()) {
        (Button::Left, ButtonEdge::Pressed) => {
            println!("button: left");
            runtime.focus_previous();
            InputAction::Continue
        }
        (Button::Right, ButtonEdge::Pressed) => {
            println!("button: right");
            runtime.focus_next();
            InputAction::Continue
        }
        (Button::Power, ButtonEdge::Pressed) => {
            // this will become suspend/awake once the power lifecycle is implemented
            println!("button: power");
            InputAction::Sleep
        }
        (_, ButtonEdge::Released) => {
            // releases are intentionally retained by the hardware abstraction event
            // though focus navigation doesn't need them yet.
            // they will matter for long press and power handling
            InputAction::Continue
        }
    }
}

fn handle_touch_event(runtime: &mut UiRuntime, app: Entity<InkPaperApp>, event: TouchEvent) {
    match event {
        TouchEvent::Down(position) => {
            println!("touch down: {},{}", position.x(), position.y(),);
            runtime.begin_activation_at(to_ui_point(position));
        }
        TouchEvent::Up(position) => {
            println!("touch up: {},{}", position.x(), position.y(),);
            runtime
                .complete_activation_at(to_ui_point(position))
                .unwrap();
        }
        TouchEvent::HomeTap => {
            println!("home tap");
            runtime
                .update(app, |app, cx| {
                    app.navigate(Route::Home, cx);
                })
                .unwrap();
        }
    }
}

fn to_ui_point(position: TouchPosition) -> Point {
    Point::new(px(position.x() as i32), px(position.y() as i32))
}
