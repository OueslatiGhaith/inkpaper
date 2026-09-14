use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point as EgPoint, Size as EgSize},
    pixelcolor::Rgb888,
    primitives::Rectangle,
};
use embedded_hal_async::delay::DelayNs;
use epd_bus::EpdInterface;
use inkpaper_ui::prelude::{DamageRegion, PaintReport};

use crate::firmware::{
    display::X4Panel,
    framebuffer::{
        Framebuffer, FramebufferStorage, Orientation, PHYSICAL_HEIGHT, PHYSICAL_WIDTH, Region,
    },
    presenter::FrameUpdate,
    refresh_policy::RefreshRequest,
};

const BLACK: Rgb888 = Rgb888::new(0, 0, 0);
const DARK_GRAY: Rgb888 = Rgb888::new(85, 85, 85);
const LIGHT_GRAY: Rgb888 = Rgb888::new(170, 170, 170);
const WHITE: Rgb888 = Rgb888::new(255, 255, 255);

const DYNAMIC_REGION: Region = Region::new(160, 360, 160, 160);
const STEP_DELAY_MS: u32 = 1_200;

const TEST_SEQUENCE: [Rgb888; 12] = [
    DARK_GRAY, LIGHT_GRAY, BLACK, WHITE, DARK_GRAY, BLACK, LIGHT_GRAY, WHITE, BLACK, DARK_GRAY,
    LIGHT_GRAY, BLACK,
];

pub async fn run<B, D>(
    panel: &mut X4Panel,
    bus: &mut B,
    delay: &mut D,
    frame: &mut FramebufferStorage,
) where
    B: EpdInterface,
    D: DelayNs,
{
    defmt::info!("grayscale validation: preparing reference frame");

    draw_reference_frame(frame);

    let full = Region::new(0, 0, PHYSICAL_WIDTH as u16, PHYSICAL_HEIGHT as u16);

    let initial_update = FrameUpdate::new(
        RefreshRequest::Full,
        full,
        frame.has_grayscale_in(full),
        frame.has_grayscale(),
        PaintReport::new(DamageRegion::full()),
    );

    if panel
        .present(bus, delay, frame, initial_update)
        .await
        .is_err()
    {
        panic!("initial grayscale validation paint failed");
    }

    if !panel.supports_partial_grayscale() {
        defmt::warn!("grayscale validation: controller does not yet support partial grayscale");

        park(delay).await;
    }

    defmt::info!("grayscale validation: starting partial update sequence");

    for (step, color) in TEST_SEQUENCE.iter().copied().enumerate() {
        delay.delay_ms(STEP_DELAY_MS).await;

        let physical_damage = draw_dynamic_region(frame, color);
        let damage_has_grayscale = frame.has_grayscale_in(physical_damage);
        let frame_has_grayscale = frame.has_grayscale();

        defmt::info!(
            "grayscale validation step={} level={} gray_damage={} gray_frame={} x={} y={} width={} height={}",
            step,
            level_name(color),
            damage_has_grayscale,
            frame_has_grayscale,
            physical_damage.x,
            physical_damage.y,
            physical_damage.width,
            physical_damage.height,
        );

        let update = FrameUpdate::new(
            RefreshRequest::Fast,
            physical_damage,
            damage_has_grayscale,
            frame_has_grayscale,
            PaintReport::default(),
        );

        if panel.present(bus, delay, frame, update).await.is_err() {
            panic!("partial grayscale validation paint failed");
        }
    }

    defmt::info!("grayscale validation complete; parking");

    park(delay).await;
}

fn draw_reference_frame(frame: &mut FramebufferStorage) {
    let mut framebuffer = Framebuffer::new(frame, Orientation::Portrait);

    framebuffer.clear(WHITE).unwrap();

    // four reference tones across the top.
    fill_region(&mut framebuffer, Region::new(20, 40, 90, 160), BLACK);
    fill_region(&mut framebuffer, Region::new(130, 40, 90, 160), DARK_GRAY);
    fill_region(&mut framebuffer, Region::new(240, 40, 90, 160), LIGHT_GRAY);
    fill_region(&mut framebuffer, Region::new(350, 40, 90, 160), WHITE);

    // static grayscale guards.
    // these must remain visually unchanged while the dynamic rectangle below is refreshed.
    fill_region(&mut framebuffer, Region::new(20, 240, 420, 40), DARK_GRAY);
    fill_region(&mut framebuffer, Region::new(20, 290, 420, 40), LIGHT_GRAY);

    // initial dynamic value.
    fill_region(&mut framebuffer, DYNAMIC_REGION, DARK_GRAY);
}

fn draw_dynamic_region(frame: &mut FramebufferStorage, color: Rgb888) -> Region {
    let mut framebuffer = Framebuffer::new(frame, Orientation::Portrait);

    fill_region(&mut framebuffer, DYNAMIC_REGION, color);

    framebuffer
        .physical_damage_region(DYNAMIC_REGION)
        .expect("validation region must be on-screen")
}

fn fill_region(framebuffer: &mut Framebuffer<'_>, region: Region, color: Rgb888) {
    framebuffer
        .fill_solid(
            &Rectangle::new(
                EgPoint::new(region.x as i32, region.y as i32),
                EgSize::new(region.width as u32, region.height as u32),
            ),
            color,
        )
        .unwrap();
}

fn level_name(color: Rgb888) -> &'static str {
    if color == BLACK {
        "black"
    } else if color == DARK_GRAY {
        "dark"
    } else if color == LIGHT_GRAY {
        "light"
    } else {
        "white"
    }
}

async fn park<D>(delay: &mut D) -> !
where
    D: DelayNs,
{
    loop {
        delay.delay_ms(60_000).await;
    }
}
