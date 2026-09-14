#![no_std]

use embedded_hal_async::delay::DelayNs;
use epd_bus::{BusyPolarity, EpdInterface};

use crate::{
    command::Command,
    grayscale::{
        FACTORY_GRAYSCALE_BORDER, FACTORY_GRAYSCALE_LUT, OVERLAY_GRAYSCALE_BORDER,
        OVERLAY_GRAYSCALE_LUT,
    },
};

mod command;
mod grayscale;

const BUSY_POLARITY: BusyPolarity = BusyPolarity::ActiveHigh;

const INTERNAL_TEMPERATURE_SENSOR: u8 = 0x80;
const DATA_ENTRY_X_INCREMENT_Y_DECREMENT: u8 = 0x01;
const DISPLAY_UPDATE_NORMAL: u8 = 0x00;
const DISPLAY_UPDATE_BYPASS_RED: u8 = 0x40;
const AUTO_WRITE_PATTERN: u8 = 0xf7;
const GRAYSCALE_STREAM_CHUNK: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum RefreshMode {
    Full,
    Clean,
    Fast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
    overlay_grayscale_on_panel: bool,
}

impl Ssd1677 {
    pub const fn new(config: Config) -> Self {
        Self {
            config,
            screen_on: false,
            needs_initial_clean: true,
            overlay_grayscale_on_panel: false,
        }
    }

    pub const fn config(&self) -> Config {
        self.config
    }

    pub const fn overlay_grayscale_on_panel(&self) -> bool {
        self.overlay_grayscale_on_panel
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
        self.overlay_grayscale_on_panel = false;

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

    pub async fn display_grayscale<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        lsb: &[u8],
        msb: &[u8],
        turn_off: bool,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.validate_frame(lsb)?;
        self.validate_frame(msb)?;

        self.set_full_ram_area(bus).await?;
        self.write_inverted_plane(bus, Command::WriteBlackWhiteRam, lsb)
            .await?;

        self.set_full_ram_area(bus).await?;
        self.write_inverted_plane(bus, Command::WriteRedRam, msb)
            .await?;

        self.activate_grayscale(bus, delay, &FACTORY_GRAYSCALE_LUT, FACTORY_GRAYSCALE_BORDER)
            .await?;

        // absolute grayscale does not preserve a normal B/W differential baseline.
        self.needs_initial_clean = true;
        self.overlay_grayscale_on_panel = false;

        if turn_off {
            self.power_off_after_grayscale(bus, delay).await?;
        }

        Ok(())
    }

    pub async fn display_grayscale_window<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        lsb: &[u8],
        msb: &[u8],
        region: Region,
        mode: RefreshMode,
        turn_off: bool,
    ) -> Result<Region, Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.validate_frame(lsb)?;
        self.validate_frame(msb)?;

        let requested = self.normalize_region(region)?;

        let full = Region::new(0, 0, self.config.width, self.config.height);

        // a periodic/full clean must rebuild the B/W base for the whole panel. Likewise
        // the first update after boot cannot safely start from a differential window.
        let (base_region, base_mode) = if self.needs_initial_clean {
            (full, RefreshMode::Clean)
        } else {
            match mode {
                RefreshMode::Fast => (requested, RefreshMode::Fast),
                RefreshMode::Full => (full, RefreshMode::Full),
                RefreshMode::Clean => (full, RefreshMode::Clean),
            }
        };

        self.paint_grayscale_base(bus, delay, lsb, msb, base_region, base_mode)
            .await?;

        self.needs_initial_clean = false;

        // if this particular update contains no gray, the B/W base pass already
        // produced its final result.
        // existing grayscale elsewhere on the physical panel is untouched because
        // the base update was windowed.
        if self.region_has_grayscale(lsb, msb, base_region) {
            self.write_overlay_masks(bus, lsb, msb, base_region).await?;

            self.activate_grayscale(bus, delay, &OVERLAY_GRAYSCALE_LUT, OVERLAY_GRAYSCALE_BORDER)
                .await?;

            // the overlay activation temporarily replaces both controller RAM planes
            // with selector masks.
            // restore a clean binary baseline without activating another waveform. Physical
            // grayscale stays visible, but the next partial B/W base update has valid OLD RAM.
            self.restore_grayscale_base(bus, lsb, msb).await?;
        }

        self.overlay_grayscale_on_panel = lsb != msb;

        if turn_off {
            self.power_off_after_grayscale(bus, delay).await?;
        }

        Ok(base_region)
    }

    async fn paint_grayscale_base<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        lsb: &[u8],
        msb: &[u8],
        region: Region,
        mode: RefreshMode,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        if mode == RefreshMode::Fast {
            self.set_ram_area(bus, region).await?;

            self.write_transformed_window_plane(
                bus,
                Command::WriteBlackWhiteRam,
                lsb,
                msb,
                region,
                grayscale_base_byte,
            )
            .await?;

            // RED keeps the previous B/W base here, so the normal
            // FAST waveform performs the differential transition.
            self.refresh(bus, delay, RefreshMode::Fast).await?;

            // Synchronize the new base into both controller planes.
            self.set_ram_area(bus, region).await?;

            self.write_transformed_window_plane(
                bus,
                Command::WriteBlackWhiteRam,
                lsb,
                msb,
                region,
                grayscale_base_byte,
            )
            .await?;

            self.set_ram_area(bus, region).await?;

            self.write_transformed_window_plane(
                bus,
                Command::WriteRedRam,
                lsb,
                msb,
                region,
                grayscale_base_byte,
            )
            .await?;

            return Ok(());
        }

        debug_assert_eq!(
            region,
            Region::new(0, 0, self.config.width, self.config.height)
        );

        self.set_full_ram_area(bus).await?;

        self.write_transformed_plane(
            bus,
            Command::WriteBlackWhiteRam,
            lsb,
            msb,
            grayscale_base_byte,
        )
        .await?;

        self.set_full_ram_area(bus).await?;

        self.write_transformed_plane(bus, Command::WriteRedRam, lsb, msb, grayscale_base_byte)
            .await?;

        self.refresh(bus, delay, mode).await?;

        // match the ordinary single-buffer path: after activation, re-seed both RAM
        // planes with the displayed B/W baseline.
        self.restore_grayscale_base(bus, lsb, msb).await
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

    async fn write_inverted_plane<B>(
        &self,
        bus: &mut B,
        command: Command,
        frame: &[u8],
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        self.command(bus, command).await?;

        let mut buffer = [0u8; GRAYSCALE_STREAM_CHUNK];

        for source in frame.chunks(GRAYSCALE_STREAM_CHUNK) {
            for index in 0..source.len() {
                buffer[index] = !source[index];
            }

            bus.data(&buffer[..source.len()])
                .await
                .map_err(Error::Bus)?;
        }

        Ok(())
    }

    async fn load_grayscale_lut<B>(
        &self,
        bus: &mut B,
        lut: &[u8; 110],
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        self.command_data(bus, Command::WriteLut, &lut[..105])
            .await?;
        self.command_data(bus, Command::GateVoltage, &[lut[105]])
            .await?;
        self.command_data(bus, Command::SourceVoltage, &lut[106..109])
            .await?;
        self.command_data(bus, Command::WriteVcom, &[lut[109]])
            .await
    }

    async fn activate_grayscale<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        lut: &[u8; 110],
        border: u8,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.load_grayscale_lut(bus, lut).await?;

        self.command_data(bus, Command::BorderWaveform, &[border])
            .await?;

        // both selector planes must participate.
        self.command_data(
            bus,
            Command::DisplayUpdateControl1,
            &[DISPLAY_UPDATE_NORMAL],
        )
        .await?;

        self.command_data(bus, Command::DisplayUpdateControl2, &[0xcc])
            .await?;

        self.command(bus, Command::MasterActivation).await?;

        bus.wait_busy(BUSY_POLARITY, delay)
            .await
            .map_err(Error::Bus)?;

        self.screen_on = true;

        Ok(())
    }

    async fn power_off_after_grayscale<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        if !self.screen_on {
            return Ok(());
        }

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

        Ok(())
    }

    async fn write_transformed_plane<B>(
        &self,
        bus: &mut B,
        command: Command,
        lsb: &[u8],
        msb: &[u8],
        transform: fn(u8, u8) -> u8,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        self.command(bus, command).await?;

        let mut buffer = [0u8; GRAYSCALE_STREAM_CHUNK];
        let mut offset = 0;

        while offset < lsb.len() {
            let len = (lsb.len() - offset).min(buffer.len());

            for index in 0..len {
                buffer[index] = transform(lsb[offset + index], msb[offset + index]);
            }

            bus.data(&buffer[..len]).await.map_err(Error::Bus)?;

            offset += len;
        }

        Ok(())
    }

    async fn write_transformed_window_plane<B>(
        &self,
        bus: &mut B,
        command: Command,
        lsb: &[u8],
        msb: &[u8],
        region: Region,
        transform: fn(u8, u8) -> u8,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        let stride = self.config.width as usize / 8;

        let x_byte = region.x as usize / 8;

        let row_bytes = region.width as usize / 8;

        self.command(bus, command).await?;

        let mut buffer = [0u8; GRAYSCALE_STREAM_CHUNK];

        for row in 0..region.height as usize {
            let start = (region.y as usize + row) * stride + x_byte;

            let mut offset = 0;

            while offset < row_bytes {
                let len = (row_bytes - offset).min(buffer.len());

                for index in 0..len {
                    let source = start + offset + index;

                    buffer[index] = transform(lsb[source], msb[source]);
                }

                bus.data(&buffer[..len]).await.map_err(Error::Bus)?;

                offset += len;
            }
        }

        Ok(())
    }

    async fn write_overlay_mask_plane<B>(
        &self,
        bus: &mut B,
        command: Command,
        lsb: &[u8],
        msb: &[u8],
        region: Region,
        transform: fn(u8, u8) -> u8,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        let stride = self.config.width as usize / 8;

        let x_start = region.x as usize / 8;
        let x_end = x_start + region.width as usize / 8;

        let y_start = region.y as usize;
        let y_end = y_start + region.height as usize;

        self.command(bus, command).await?;

        let mut buffer = [0u8; GRAYSCALE_STREAM_CHUNK];

        for y in 0..self.config.height as usize {
            let row_start = y * stride;

            let mut x = 0;

            while x < stride {
                let len = (stride - x).min(buffer.len());

                for index in 0..len {
                    let byte_x = x + index;

                    buffer[index] =
                        if y >= y_start && y < y_end && byte_x >= x_start && byte_x < x_end {
                            let source = row_start + byte_x;

                            transform(lsb[source], msb[source])
                        } else {
                            0x00
                        };
                }

                bus.data(&buffer[..len]).await.map_err(Error::Bus)?;

                x += len;
            }
        }

        Ok(())
    }

    async fn write_overlay_masks<B>(
        &self,
        bus: &mut B,
        lsb: &[u8],
        msb: &[u8],
        region: Region,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        self.set_full_ram_area(bus).await?;

        self.write_overlay_mask_plane(
            bus,
            Command::WriteBlackWhiteRam,
            lsb,
            msb,
            region,
            overlay_lsb_byte,
        )
        .await?;

        self.set_full_ram_area(bus).await?;

        self.write_overlay_mask_plane(
            bus,
            Command::WriteRedRam,
            lsb,
            msb,
            region,
            overlay_msb_byte,
        )
        .await
    }

    async fn restore_grayscale_base<B>(
        &self,
        bus: &mut B,
        lsb: &[u8],
        msb: &[u8],
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        self.set_full_ram_area(bus).await?;

        self.write_transformed_plane(
            bus,
            Command::WriteBlackWhiteRam,
            lsb,
            msb,
            grayscale_base_byte,
        )
        .await?;

        self.set_full_ram_area(bus).await?;

        self.write_transformed_plane(bus, Command::WriteRedRam, lsb, msb, grayscale_base_byte)
            .await
    }

    fn region_has_grayscale(&self, lsb: &[u8], msb: &[u8], region: Region) -> bool {
        let stride = self.config.width as usize / 8;

        let x_byte = region.x as usize / 8;

        let row_bytes = region.width as usize / 8;

        for row in 0..region.height as usize {
            let start = (region.y as usize + row) * stride + x_byte;
            let end = start + row_bytes;

            if lsb[start..end] != msb[start..end] {
                return true;
            }
        }

        false
    }
}

fn grayscale_base_byte(lsb: u8, msb: u8) -> u8 {
    lsb & msb
}

fn overlay_lsb_byte(lsb: u8, msb: u8) -> u8 {
    lsb & !msb
}

fn overlay_msb_byte(lsb: u8, msb: u8) -> u8 {
    lsb ^ msb
}

#[cfg(test)]
mod tests {
    use super::{grayscale_base_byte, overlay_lsb_byte, overlay_msb_byte};

    #[test]
    fn absolute_pixels_convert_to_overlay_encoding() {
        // black: absolute 00 -> base 0, mask 00
        assert_eq!(grayscale_base_byte(0x00, 0x00), 0x00);
        assert_eq!(overlay_lsb_byte(0x00, 0x00), 0x00);
        assert_eq!(overlay_msb_byte(0x00, 0x00), 0x00);

        // dark: absolute 10 -> base 0, mask 11
        assert_eq!(grayscale_base_byte(0xff, 0x00), 0x00);
        assert_eq!(overlay_lsb_byte(0xff, 0x00), 0xff);
        assert_eq!(overlay_msb_byte(0xff, 0x00), 0xff);

        // light: absolute 01 -> base 0, mask 01
        assert_eq!(grayscale_base_byte(0x00, 0xff), 0x00);
        assert_eq!(overlay_lsb_byte(0x00, 0xff), 0x00);
        assert_eq!(overlay_msb_byte(0x00, 0xff), 0xff);

        // white: absolute 11 -> base 1, mask 00
        assert_eq!(grayscale_base_byte(0xff, 0xff), 0xff);
        assert_eq!(overlay_lsb_byte(0xff, 0xff), 0x00);
        assert_eq!(overlay_msb_byte(0xff, 0xff), 0x00);
    }
}
