#![no_std]

use embedded_hal_async::delay::DelayNs;
use epd_bus::{
    BinaryOverGrayMode, BinaryUpdateMode, BusyPolarity, DataStream, DisplayPhase, EpdInterface,
    EpdPanel, GrayscaleUpdateMode, PanelCapabilities, PanelError, grayscale_base_byte,
    region_has_grayscale,
};

pub use epd_bus::{PanelError as Error, RefreshMode, Region};

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
                self.set_full_ram_area(bus).await?;
                self.write_plane(bus, Command::WriteRedRam, frame).await?;
            }
            RefreshMode::Fast => {
                if let Some(previous) = previous {
                    self.set_full_ram_area(bus).await?;
                    self.write_plane(bus, Command::WriteRedRam, previous)
                        .await?;
                }
            }
        }

        let refresh_phase = if mode == RefreshMode::Fast {
            DisplayPhase::BinaryFastRefresh
        } else {
            DisplayPhase::BinaryFullRefresh
        };
        self.refresh(bus, delay, mode, refresh_phase).await?;

        // BW RAM already holds `frame`. Only RED RAM needs synchronizing after
        // a single-buffer Fast refresh; Full/Clean already seeded RED above.
        if previous.is_none() && mode == RefreshMode::Fast {
            self.set_full_ram_area(bus).await?;
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

        self.activate_grayscale(
            bus,
            delay,
            &FACTORY_GRAYSCALE_LUT,
            FACTORY_GRAYSCALE_BORDER,
            DisplayPhase::GrayscaleRefresh,
        )
        .await?;

        // absolute grayscale does not preserve a normal B/W differential baseline.
        self.needs_initial_clean = true;
        self.overlay_grayscale_on_panel = false;

        if turn_off {
            self.power_off_after_grayscale(bus, delay).await?;
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
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

        let has_grayscale =
            region_has_grayscale(lsb, msb, self.config.width as usize / 8, base_region);
        let base_phase = if has_grayscale {
            DisplayPhase::GrayscaleBaseRefresh
        } else if base_mode == RefreshMode::Fast {
            DisplayPhase::BinaryFastRefresh
        } else {
            DisplayPhase::BinaryFullRefresh
        };

        self.paint_grayscale_base(
            bus,
            delay,
            lsb,
            msb,
            base_region,
            base_mode,
            base_phase,
            !has_grayscale,
        )
        .await?;

        self.needs_initial_clean = false;

        // if this particular update contains no gray, the B/W base pass already
        // produced its final result.
        // existing grayscale elsewhere on the physical panel is untouched because
        // the base update was windowed.
        if has_grayscale {
            self.write_overlay_masks(bus, lsb, msb, base_region).await?;

            self.activate_grayscale(
                bus,
                delay,
                &OVERLAY_GRAYSCALE_LUT,
                OVERLAY_GRAYSCALE_BORDER,
                DisplayPhase::GrayscaleActivate,
            )
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

    #[allow(clippy::too_many_arguments)]
    async fn paint_grayscale_base<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        lsb: &[u8],
        msb: &[u8],
        region: Region,
        mode: RefreshMode,
        phase: DisplayPhase,
        sync_red: bool,
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
            self.refresh(bus, delay, RefreshMode::Fast, phase).await?;

            // BW already holds the new base. Only synchronize RED when an
            // overlay pass is not about to overwrite both planes immediately.
            if sync_red {
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
            }

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

        // Both BW and RED already hold the new B/W base, so no post-refresh
        // re-write is needed.
        self.refresh(bus, delay, mode, phase).await
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

            bus.wait_busy_for(BUSY_POLARITY, DisplayPhase::PowerOff, delay)
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
        phase: DisplayPhase,
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

        bus.wait_busy_for(BUSY_POLARITY, phase, delay)
            .await
            .map_err(Error::Bus)?;

        self.screen_on = !profile.sequence.powers_off();

        Ok(())
    }

    async fn set_ram_area<B>(&self, bus: &mut B, region: Region) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        let x_start = region.x();
        let x_end = region.x() + region.width() - 1;

        // the panel gate order is physically reserved
        let controller_y = self.config.height - region.y() - region.height();
        let y_start = controller_y + region.height() - 1;
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

        let right = region.x() as u32 + region.width() as u32;
        let bottom = region.y() as u32 + region.height() as u32;
        if right > self.config.width as u32 || bottom > self.config.height as u32 {
            return Err(Error::RegionOutOfBounds { region });
        }

        let x = region.x() & !7;
        let right = ((right + 7) & !7).min(self.config.width as u32) as u16;

        Ok(Region::new(x, region.y(), right - x, region.height()))
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
        let x_byte = region.x() as usize / 8;
        let row_bytes = region.width() as usize / 8;

        self.command(bus, command).await?;
        let mut stream = DataStream::open(bus).await.map_err(Error::Bus)?;

        for row in 0..region.height() as usize {
            let framebuffer_y = region.y() as usize + row;
            let start = framebuffer_y * framebuffer_stride + x_byte;
            let end = start + row_bytes;

            stream.write(&frame[start..end]).await.map_err(Error::Bus)?;
        }

        stream.finish().map_err(Error::Bus)
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

        self.refresh(bus, delay, RefreshMode::Fast, DisplayPhase::BinaryFastRefresh)
            .await?;

        // in single-buffer mode RED RAM is our persistent differential
        // baseline. BW RAM already holds `frame` in `region`, so only sync RED.
        if previous.is_none() {
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
        let mut stream = DataStream::open(bus).await.map_err(Error::Bus)?;

        let mut buffer = [0u8; GRAYSCALE_STREAM_CHUNK];

        for source in frame.chunks(GRAYSCALE_STREAM_CHUNK) {
            for index in 0..source.len() {
                buffer[index] = !source[index];
            }

            stream
                .write(&buffer[..source.len()])
                .await
                .map_err(Error::Bus)?;
        }

        stream.finish().map_err(Error::Bus)
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
        phase: DisplayPhase,
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

        bus.wait_busy_for(BUSY_POLARITY, phase, delay)
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

        bus.wait_busy_for(BUSY_POLARITY, DisplayPhase::PowerOff, delay)
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
        let mut stream = DataStream::open(bus).await.map_err(Error::Bus)?;

        let mut buffer = [0u8; GRAYSCALE_STREAM_CHUNK];
        let mut offset = 0;

        while offset < lsb.len() {
            let len = (lsb.len() - offset).min(buffer.len());

            for index in 0..len {
                buffer[index] = transform(lsb[offset + index], msb[offset + index]);
            }

            stream.write(&buffer[..len]).await.map_err(Error::Bus)?;

            offset += len;
        }

        stream.finish().map_err(Error::Bus)
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
        let x_byte = region.x() as usize / 8;
        let row_bytes = region.width() as usize / 8;

        self.command(bus, command).await?;
        let mut stream = DataStream::open(bus).await.map_err(Error::Bus)?;

        let mut buffer = [0u8; GRAYSCALE_STREAM_CHUNK];

        for row in 0..region.height() as usize {
            let start = (region.y() as usize + row) * stride + x_byte;
            let mut offset = 0;

            while offset < row_bytes {
                let len = (row_bytes - offset).min(buffer.len());

                for (index, byte) in buffer[..len].iter_mut().enumerate() {
                    let source = start + offset + index;
                    *byte = transform(lsb[source], msb[source]);
                }

                stream.write(&buffer[..len]).await.map_err(Error::Bus)?;

                offset += len;
            }
        }

        stream.finish().map_err(Error::Bus)
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

        let x_start = region.x() as usize / 8;
        let x_end = x_start + region.width() as usize / 8;

        let y_start = region.y() as usize;
        let y_end = y_start + region.height() as usize;

        self.command(bus, command).await?;
        let mut stream = DataStream::open(bus).await.map_err(Error::Bus)?;

        let mut buffer = [0u8; GRAYSCALE_STREAM_CHUNK];

        for y in 0..self.config.height as usize {
            let row_start = y * stride;
            let mut x = 0;

            while x < stride {
                let len = (stride - x).min(buffer.len());

                for (index, byte) in buffer[..len].iter_mut().enumerate() {
                    let byte_x = x + index;

                    *byte = if y >= y_start && y < y_end && byte_x >= x_start && byte_x < x_end {
                        let source = row_start + byte_x;
                        transform(lsb[source], msb[source])
                    } else {
                        0x00
                    };
                }

                stream.write(&buffer[..len]).await.map_err(Error::Bus)?;

                x += len;
            }
        }

        stream.finish().map_err(Error::Bus)
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
}

impl EpdPanel for Ssd1677 {
    fn capabilities(&self) -> PanelCapabilities {
        PanelCapabilities::new(
            BinaryUpdateMode::Window,
            BinaryOverGrayMode::NativeWindow,
            GrayscaleUpdateMode::Window,
        )
    }

    async fn initialize<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        Ssd1677::initialize(self, bus, delay).await
    }

    async fn display_binary<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        frame: &[u8],
        damage: Option<Region>,
        mode: RefreshMode,
        _turn_off: bool,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        match (mode, damage) {
            (RefreshMode::Fast, Some(region)) => self
                .display_window(bus, delay, frame, None, region)
                .await
                .map(|_| ()),
            _ => self.display(bus, delay, frame, None, mode).await,
        }
    }

    async fn display_grayscale<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        lsb: &[u8],
        msb: &[u8],
        damage: Region,
        mode: RefreshMode,
        _turn_off: bool,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.display_grayscale_window(bus, delay, lsb, msb, damage, mode, true)
            .await
            .map(|_| ())
    }

    async fn display_binary_preserving_grayscale<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        lsb: &[u8],
        msb: &[u8],
        damage: Region,
        _turn_off: bool,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.display_grayscale_window(bus, delay, lsb, msb, damage, RefreshMode::Fast, true)
            .await
            .map(|_| ())
    }

    async fn deep_sleep<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        Ssd1677::deep_sleep(self, bus, delay).await
    }
}

fn overlay_lsb_byte(lsb: u8, msb: u8) -> u8 {
    lsb & !msb
}

fn overlay_msb_byte(lsb: u8, msb: u8) -> u8 {
    lsb ^ msb
}

#[cfg(test)]
mod tests {
    extern crate std;

    use core::{
        convert::Infallible,
        future::Future,
        pin::pin,
        task::{Context, Poll, Waker},
    };
    use std::vec;
    use std::vec::Vec;

    use embedded_hal_async::delay::DelayNs;
    use epd_bus::{BusyPolarity, DisplayPhase, EpdInterface, RefreshMode, Region};

    use super::{
        BoosterSoftStart, Config, RefreshProfile, Ssd1677, UpdateSequence, grayscale_base_byte,
        overlay_lsb_byte, overlay_msb_byte,
    };

    #[derive(Debug, PartialEq, Eq)]
    enum Op {
        Reset,
        Command(u8),
        Data(Vec<u8>),
        Stream(Vec<u8>),
        WaitBusy,
    }

    struct RecordingBus {
        ops: Vec<Op>,
        phases: Vec<DisplayPhase>,
        active_stream: Option<Vec<u8>>,
    }

    impl RecordingBus {
        fn new() -> Self {
            Self {
                ops: Vec::new(),
                phases: Vec::new(),
                active_stream: None,
            }
        }

        fn plane_writes(&self) -> Vec<(u8, usize)> {
            let mut writes = Vec::new();
            for pair in self.ops.windows(2) {
                match (&pair[0], &pair[1]) {
                    (Op::Command(cmd @ (0x24 | 0x26)), Op::Data(bytes))
                    | (Op::Command(cmd @ (0x24 | 0x26)), Op::Stream(bytes)) => {
                        writes.push((*cmd, bytes.len()));
                    }
                    _ => {}
                }
            }
            writes
        }
    }

    impl EpdInterface for RecordingBus {
        type Error = Infallible;

        async fn command(&mut self, command: u8) -> Result<(), Self::Error> {
            assert!(self.active_stream.is_none());
            self.ops.push(Op::Command(command));
            Ok(())
        }

        async fn data(&mut self, data: &[u8]) -> Result<(), Self::Error> {
            assert!(self.active_stream.is_none());
            self.ops.push(Op::Data(data.to_vec()));
            Ok(())
        }

        async fn reset<D>(&mut self, _delay: &mut D) -> Result<(), Self::Error>
        where
            D: DelayNs,
        {
            self.ops.push(Op::Reset);
            Ok(())
        }

        fn reset_high(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }

        fn is_busy(&mut self, _polarity: BusyPolarity) -> Result<bool, Self::Error> {
            Ok(false)
        }

        async fn wait_busy<D>(
            &mut self,
            _polarity: BusyPolarity,
            _delay: &mut D,
        ) -> Result<(), Self::Error>
        where
            D: DelayNs,
        {
            self.ops.push(Op::WaitBusy);
            Ok(())
        }

        async fn wait_busy_for<D>(
            &mut self,
            polarity: BusyPolarity,
            phase: DisplayPhase,
            delay: &mut D,
        ) -> Result<(), Self::Error>
        where
            D: DelayNs,
        {
            self.phases.push(phase);
            self.wait_busy(polarity, delay).await
        }

        async fn begin_data_stream(&mut self) -> Result<(), Self::Error> {
            assert!(self.active_stream.is_none());
            self.active_stream = Some(Vec::new());
            Ok(())
        }

        async fn stream_data(&mut self, data: &[u8]) -> Result<(), Self::Error> {
            self.active_stream
                .as_mut()
                .expect("stream must be open")
                .extend_from_slice(data);
            Ok(())
        }

        fn end_data_stream(&mut self) -> Result<(), Self::Error> {
            let stream = self.active_stream.take().expect("stream must be open");
            self.ops.push(Op::Stream(stream));
            Ok(())
        }
    }

    struct NoopDelay;

    impl DelayNs for NoopDelay {
        async fn delay_ns(&mut self, _ns: u32) {}
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = pin!(future);
        let mut cx = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("unexpected pending future in mock test"),
        }
    }

    const TINY_CONFIG: Config = Config::new(
        32,
        8,
        BoosterSoftStart::new([0xae, 0xc7, 0xc3, 0xc0, 0x80]),
        0x02,
        0x80,
        RefreshProfile::new(UpdateSequence::new(0xf7), 0xc0, None),
        RefreshProfile::new(UpdateSequence::new(0xd7), 0xc0, Some(0x5a)),
        RefreshProfile::new(UpdateSequence::new(0xfc), 0xc0, None),
    );

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

    #[test]
    fn full_and_fast_binary_display_skip_redundant_post_refresh_planes() {
        let mut panel = Ssd1677::new(TINY_CONFIG);
        let mut bus = RecordingBus::new();
        let mut delay = NoopDelay;
        let frame = vec![0xaa; TINY_CONFIG.buffer_len()];

        block_on(panel.initialize(&mut bus, &mut delay)).unwrap();
        bus.ops.clear();

        // Full refresh writes BW + RED before activation and 0 planes afterwards.
        block_on(panel.display(&mut bus, &mut delay, &frame, None, RefreshMode::Full)).unwrap();
        assert_eq!(bus.plane_writes(), vec![(0x24, 32), (0x26, 32)]);

        bus.ops.clear();
        // Fast refresh writes BW before activation and only RED afterwards.
        block_on(panel.display(&mut bus, &mut delay, &frame, None, RefreshMode::Fast)).unwrap();
        assert_eq!(bus.plane_writes(), vec![(0x24, 32), (0x26, 32)]);

        bus.ops.clear();
        // Windowed fast refresh streams BW before and only RED afterwards (2 streams, 8 bytes each).
        let region = Region::new(8, 2, 16, 4);
        block_on(panel.display_window(&mut bus, &mut delay, &frame, None, region)).unwrap();
        assert_eq!(bus.plane_writes(), vec![(0x24, 8), (0x26, 8)]);
        let stream_count = bus.ops.iter().filter(|op| matches!(op, Op::Stream(_))).count();
        assert_eq!(stream_count, 2);
    }

    #[test]
    fn grayscale_window_skips_redundant_base_sync_before_overlay() {
        let mut panel = Ssd1677::new(TINY_CONFIG);
        let mut bus = RecordingBus::new();
        let mut delay = NoopDelay;
        let white = vec![0xff; TINY_CONFIG.buffer_len()];

        block_on(panel.initialize(&mut bus, &mut delay)).unwrap();
        block_on(panel.display(&mut bus, &mut delay, &white, None, RefreshMode::Full)).unwrap();

        // Fast window with no gray: BW window before refresh, RED window after refresh.
        bus.ops.clear();
        let region = Region::new(8, 2, 16, 4);
        block_on(panel.display_grayscale_window(
            &mut bus,
            &mut delay,
            &white,
            &white,
            region,
            RefreshMode::Fast,
            false,
        ))
        .unwrap();
        assert_eq!(bus.plane_writes(), vec![(0x24, 8), (0x26, 8)]);

        // Fast window with gray: only 1 BW window before overlay masks (no wasted RED sync),
        // then 2 full overlay mask planes + 2 full restored baseline planes.
        let mut lsb = white.clone();
        lsb[2 * 4 + 1] = 0x00; // light gray inside `region`
        bus.ops.clear();
        block_on(panel.display_grayscale_window(
            &mut bus,
            &mut delay,
            &lsb,
            &white,
            region,
            RefreshMode::Fast,
            false,
        ))
        .unwrap();
        assert_eq!(
            bus.plane_writes(),
            vec![(0x24, 8), (0x24, 32), (0x26, 32), (0x24, 32), (0x26, 32)]
        );
    }

    #[test]
    fn reports_explicit_display_phases() {
        let mut panel = Ssd1677::new(TINY_CONFIG);
        let mut bus = RecordingBus::new();
        let mut delay = NoopDelay;
        let white = vec![0xff; TINY_CONFIG.buffer_len()];
        let mut lsb = white.clone();
        lsb[2 * 4 + 1] = 0x00;

        block_on(panel.initialize(&mut bus, &mut delay)).unwrap();
        bus.phases.clear();

        block_on(panel.display(&mut bus, &mut delay, &white, None, RefreshMode::Full)).unwrap();
        block_on(panel.display(&mut bus, &mut delay, &white, None, RefreshMode::Fast)).unwrap();
        block_on(panel.display_grayscale_window(
            &mut bus,
            &mut delay,
            &lsb,
            &white,
            Region::new(8, 2, 16, 4),
            RefreshMode::Fast,
            true,
        ))
        .unwrap();
        block_on(panel.display_grayscale(&mut bus, &mut delay, &lsb, &white, true)).unwrap();

        assert_eq!(
            bus.phases,
            vec![
                DisplayPhase::BinaryFullRefresh,
                DisplayPhase::BinaryFastRefresh,
                DisplayPhase::GrayscaleBaseRefresh,
                DisplayPhase::GrayscaleActivate,
                DisplayPhase::PowerOff,
                DisplayPhase::GrayscaleRefresh,
                DisplayPhase::PowerOff,
            ]
        );
    }
}
