#![no_std]

use embedded_hal_async::delay::DelayNs;
use epd_bus::{BusyPolarity, EpdInterface};

use crate::command::Command;

mod command;

const BUSY_POLARITY: BusyPolarity = BusyPolarity::ActiveHigh;

const INTERNAL_TEMPERATURE_SENSOR: u8 = 0x80;
const DATA_ENTRY_X_INCREMENT_Y_DECREMENT: u8 = 0x01;
const DISPLAY_UPDATE_NORMAL: u8 = 0x00;
const DISPLAY_UPDATE_BYPASS_RED: u8 = 0x40;
const AUTO_WRITE_PATTERN: u8 = 0xf7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateSequence(u8);

impl UpdateSequence {
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    const fn raw(self) -> u8 {
        self.0
    }

    const fn powers_off(self) -> bool {
        self.0 & 0x03 != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoosterSoftStart([u8; 5]);

impl BoosterSoftStart {
    pub const fn new(values: [u8; 5]) -> Self {
        Self(values)
    }

    const fn bytes(&self) -> &[u8; 5] {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefreshProfile {
    sequence: UpdateSequence,
    border_waveform: u8,
    temperature: Option<u8>,
}

impl RefreshProfile {
    pub const fn new(
        sequence: UpdateSequence,
        border_waveform: u8,
        temperature: Option<u8>,
    ) -> Self {
        Self {
            sequence,
            border_waveform,
            temperature,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    width: u16,
    height: u16,

    booster: BoosterSoftStart,

    driver_output_scan: u8,
    border_waveform_init: u8,

    full: RefreshProfile,
    clean: RefreshProfile,
    fast: RefreshProfile,
}

impl Config {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        width: u16,
        height: u16,
        booster: BoosterSoftStart,
        driver_output_scan: u8,
        border_waveform_init: u8,
        full: RefreshProfile,
        clean: RefreshProfile,
        fast: RefreshProfile,
    ) -> Self {
        Self {
            width,
            height,
            booster,
            driver_output_scan,
            border_waveform_init,
            full,
            clean,
            fast,
        }
    }

    pub const fn width(self) -> u16 {
        self.width
    }

    pub const fn height(self) -> u16 {
        self.height
    }

    pub const fn buffer_len(self) -> usize {
        self.width as usize / 8 * self.height as usize
    }
}

pub const GDEQ0426T82: Config = Config::new(
    800,
    480,
    BoosterSoftStart::new([0xae, 0xc7, 0xc3, 0xc0, 0x80]),
    0x02,
    0x80,
    RefreshProfile::new(UpdateSequence::new(0xf7), 0xc0, None),
    RefreshProfile::new(UpdateSequence::new(0xd7), 0xc0, Some(0x5a)),
    RefreshProfile::new(UpdateSequence::new(0xfc), 0xc0, None),
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshMode {
    Full,
    Clean,
    Fast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

impl Region {
    pub const fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub const fn x(self) -> u16 {
        self.x
    }

    pub const fn y(self) -> u16 {
        self.y
    }

    pub const fn width(self) -> u16 {
        self.width
    }

    pub const fn height(self) -> u16 {
        self.height
    }

    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }
}

#[derive(Debug)]
pub enum Error<E> {
    Bus(E),
    InvalidGeometry,
    InvalidFrameLength { expected: usize, actual: usize },
    InvalidPreviousFrameLength { expected: usize, actual: usize },
    EmptyRegion,
    RegionOutOfBounds { region: Region },
}

pub struct Ssd1677 {
    config: Config,

    screen_on: bool,
    needs_initial_clean: bool,
}

impl Ssd1677 {
    pub const fn new(config: Config) -> Self {
        Self {
            config,
            screen_on: false,
            needs_initial_clean: true,
        }
    }

    pub const fn config(&self) -> Config {
        self.config
    }

    pub async fn initialize<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.validate_geometry()?;
        bus.reset(delay).await.map_err(Error::Bus)?;
        self.command(bus, Command::SoftReset).await?;

        // x4 pro requires a fixed post-SWRESET settle. BUSY can still be deasserted on
        // the first sample, so waiting only on BUSY is not equivalent
        delay.delay_ms(10).await;
        bus.wait_busy(BUSY_POLARITY, delay)
            .await
            .map_err(Error::Bus)?;

        self.command_data(
            bus,
            Command::TemperatureSensorControl,
            &[INTERNAL_TEMPERATURE_SENSOR],
        )
        .await?;

        self.command_data(bus, Command::BoosterSoftStart, self.config.booster.bytes())
            .await?;

        let height_minus_one = self.config.height - 1;

        self.command_data(
            bus,
            Command::DriverOutputControl,
            &[
                height_minus_one as u8,
                (height_minus_one >> 8) as u8,
                self.config.driver_output_scan,
            ],
        )
        .await?;

        self.command_data(
            bus,
            Command::BorderWaveform,
            &[self.config.border_waveform_init],
        )
        .await?;

        self.set_full_ram_area(bus).await?;

        self.command_data(bus, Command::AutoWriteBlackWhiteRam, &[AUTO_WRITE_PATTERN])
            .await?;

        bus.wait_busy(BUSY_POLARITY, delay)
            .await
            .map_err(Error::Bus)?;

        self.command_data(bus, Command::AutoWriteRedRam, &[AUTO_WRITE_PATTERN])
            .await?;

        bus.wait_busy(BUSY_POLARITY, delay)
            .await
            .map_err(Error::Bus)?;

        self.screen_on = false;
        self.needs_initial_clean = true;

        Ok(())
    }

    pub async fn display<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        frame: &[u8],
        previous: Option<&[u8]>,
        mut mode: RefreshMode,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.validate_frame(frame)?;
        if let Some(previous) = previous {
            self.validate_previous_frame(previous)?;
        }

        // a differential refresh cannot clean arbitrary physical pixels left
        // on the panel from before boot/deep sleep.
        if self.needs_initial_clean {
            if mode == RefreshMode::Fast {
                mode = RefreshMode::Clean;
            }

            self.needs_initial_clean = false;
        }

        self.set_full_ram_area(bus).await?;
        self.write_plane(bus, Command::WriteBlackWhiteRam, frame)
            .await?;

        match mode {
            RefreshMode::Full | RefreshMode::Clean => {
                self.write_plane(bus, Command::WriteRedRam, frame).await?;
            }
            RefreshMode::Fast => {
                if let Some(previous) = previous {
                    self.write_plane(bus, Command::WriteRedRam, previous)
                        .await?;
                }
            }
        }

        self.refresh(bus, delay, mode).await?;

        if previous.is_none() {
            self.set_full_ram_area(bus).await?;

            self.write_plane(bus, Command::WriteBlackWhiteRam, frame)
                .await?;

            self.write_plane(bus, Command::WriteRedRam, frame).await?;
        }

        Ok(())
    }

    pub async fn seed_previous_frame<B>(
        &mut self,
        bus: &mut B,
        frame: &[u8],
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        self.validate_frame(frame)?;
        self.set_full_ram_area(bus).await?;
        self.write_plane(bus, Command::WriteRedRam, frame).await
    }

    pub async fn deep_sleep<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        if self.screen_on {
            self.command_data(
                bus,
                Command::BorderWaveform,
                &[self.config.border_waveform_init],
            )
            .await?;

            self.command_data(bus, Command::DisplayUpdateControl2, &[0x03])
                .await?;
            self.command(bus, Command::MasterActivation).await?;
            delay.delay_ms(200).await;

            bus.wait_busy(BUSY_POLARITY, delay)
                .await
                .map_err(Error::Bus)?;

            self.screen_on = false;
        }

        self.command_data(bus, Command::DeepSleep, &[0x03]).await?;

        self.needs_initial_clean = true;

        Ok(())
    }

    async fn refresh<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        mode: RefreshMode,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        let profile = self.refresh_profile(mode);
        let control1 = match mode {
            RefreshMode::Fast => DISPLAY_UPDATE_NORMAL,
            RefreshMode::Full | RefreshMode::Clean => DISPLAY_UPDATE_BYPASS_RED,
        };

        self.command_data(bus, Command::DisplayUpdateControl1, &[control1])
            .await?;
        self.command_data(bus, Command::BorderWaveform, &[profile.border_waveform])
            .await?;

        if let Some(temperature) = profile.temperature {
            self.command_data(bus, Command::WriteTemperature, &[temperature])
                .await?;
        }

        self.command_data(
            bus,
            Command::DisplayUpdateControl2,
            &[profile.sequence.raw()],
        )
        .await?;

        self.command(bus, Command::MasterActivation).await?;

        bus.wait_busy(BUSY_POLARITY, delay)
            .await
            .map_err(Error::Bus)?;

        self.screen_on = !profile.sequence.powers_off();

        Ok(())
    }

    async fn set_ram_area<B>(&self, bus: &mut B, region: Region) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        let x_start = region.x;
        let x_end = region.x + region.width - 1;

        // the panel gate order is physically reserved
        let controller_y = self.config.height - region.y - region.height;
        let y_start = controller_y + region.height - 1;
        let y_end = controller_y;

        self.command_data(
            bus,
            Command::DataEntryMode,
            &[DATA_ENTRY_X_INCREMENT_Y_DECREMENT],
        )
        .await?;

        self.command_data(
            bus,
            Command::SetRamXRange,
            &[
                x_start as u8,
                (x_start >> 8) as u8,
                x_end as u8,
                (x_end >> 8) as u8,
            ],
        )
        .await?;

        self.command_data(
            bus,
            Command::SetRamYRange,
            &[
                y_start as u8,
                (y_start >> 8) as u8,
                y_end as u8,
                (y_end >> 8) as u8,
            ],
        )
        .await?;

        self.command_data(
            bus,
            Command::SetRamXCounter,
            &[x_start as u8, (x_start >> 8) as u8],
        )
        .await?;

        self.command_data(
            bus,
            Command::SetRamYCounter,
            &[y_start as u8, (y_start >> 8) as u8],
        )
        .await
    }

    async fn set_full_ram_area<B>(&self, bus: &mut B) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        self.set_ram_area(
            bus,
            Region::new(0, 0, self.config.width, self.config.height),
        )
        .await
    }

    async fn write_plane<B>(
        &self,
        bus: &mut B,
        command: Command,
        frame: &[u8],
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        self.command(bus, command).await?;
        bus.data(frame).await.map_err(Error::Bus)
    }

    async fn command<B>(&self, bus: &mut B, command: Command) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        bus.command(command.byte()).await.map_err(Error::Bus)
    }

    async fn command_data<B>(
        &self,
        bus: &mut B,
        command: Command,
        data: &[u8],
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        self.command(bus, command).await?;
        bus.data(data).await.map_err(Error::Bus)
    }

    const fn refresh_profile(&self, mode: RefreshMode) -> RefreshProfile {
        match mode {
            RefreshMode::Full => self.config.full,
            RefreshMode::Clean => self.config.clean,
            RefreshMode::Fast => self.config.fast,
        }
    }

    fn validate_geometry<E>(&self) -> Result<(), Error<E>> {
        if self.config.width == 0 || self.config.height == 0 || !self.config.width.is_multiple_of(8)
        {
            return Err(Error::InvalidGeometry);
        }

        Ok(())
    }

    fn validate_frame<E>(&self, frame: &[u8]) -> Result<(), Error<E>> {
        let expected = self.config.buffer_len();
        if frame.len() != expected {
            return Err(Error::InvalidFrameLength {
                expected,
                actual: frame.len(),
            });
        }

        Ok(())
    }

    fn validate_previous_frame<E>(&self, frame: &[u8]) -> Result<(), Error<E>> {
        let expected = self.config.buffer_len();
        if frame.len() != expected {
            return Err(Error::InvalidPreviousFrameLength {
                expected,
                actual: frame.len(),
            });
        }

        Ok(())
    }

    fn normalize_region<E>(&self, region: Region) -> Result<Region, Error<E>> {
        if region.is_empty() {
            return Err(Error::EmptyRegion);
        }

        let right = region.x as u32 + region.width as u32;
        let bottom = region.y as u32 + region.height as u32;
        if right > self.config.width as u32 || bottom > self.config.height as u32 {
            return Err(Error::RegionOutOfBounds { region });
        }

        let x = region.x & !7;
        let right = ((right + 7) & !7).min(self.config.width as u32) as u16;

        Ok(Region::new(x, region.y, right - x, region.height))
    }

    async fn write_window_plane<B>(
        &self,
        bus: &mut B,
        command: Command,
        frame: &[u8],
        region: Region,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        let framebuffer_stride = self.config.width as usize / 8;
        let x_byte = region.x as usize / 8;
        let row_bytes = region.width as usize / 8;

        self.command(bus, command).await?;

        for row in 0..region.height as usize {
            let framebuffer_y = region.y as usize + row;
            let start = framebuffer_y * framebuffer_stride + x_byte;
            let end = start + row_bytes;

            bus.data(&frame[start..end]).await.map_err(Error::Bus)?;
        }

        Ok(())
    }

    pub async fn display_window<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        frame: &[u8],
        previous: Option<&[u8]>,
        region: Region,
    ) -> Result<Region, Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.validate_frame(frame)?;
        if let Some(previous) = previous {
            self.validate_previous_frame(previous)?;
        }

        let region = self.normalize_region(region)?;

        // we cannot safely perform a differential window update until the
        // controller RAM and physical panel have a known baseline.
        //
        // we already have the complete target framebuffer, so recover by
        // performing a clean full-screen update rather than exposing this
        // state-management requirement to the caller.
        if self.needs_initial_clean {
            self.display(bus, delay, frame, previous, RefreshMode::Clean)
                .await?;

            return Ok(Region::new(0, 0, self.config.width, self.config.height));
        }

        self.set_ram_area(bus, region).await?;
        self.write_window_plane(bus, Command::WriteBlackWhiteRam, frame, region)
            .await?;

        if let Some(previous) = previous {
            self.set_ram_area(bus, region).await?;
            self.write_window_plane(bus, Command::WriteRedRam, previous, region)
                .await?;
        }

        self.refresh(bus, delay, RefreshMode::Fast).await?;

        // in single-buffer mode RED RAM is our persistent differential
        // baseline. Synchronize both planes after the waveform so the next
        // update compares against the frame we just displayed.
        //
        // when the caller supplies `previous`, it owns that baseline and will
        // explicitly provide it again on the next update, so there is no need
        // for the extra write.
        if previous.is_none() {
            self.set_ram_area(bus, region).await?;
            self.write_window_plane(bus, Command::WriteBlackWhiteRam, frame, region)
                .await?;
            self.set_ram_area(bus, region).await?;
            self.write_window_plane(bus, Command::WriteRedRam, frame, region)
                .await?;
        }

        Ok(region)
    }
}
