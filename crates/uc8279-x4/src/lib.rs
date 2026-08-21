#![no_std]

use embedded_hal_async::delay::DelayNs;
use epd_bus::{BusyPolarity, EpdInterface};

use crate::command::Command;

mod command;

const BUSY_POLARITY: BusyPolarity = BusyPolarity::ActiveLow;

const RESET_SETTLE_MS: u32 = 50;
const BUSY_SETTLE_MS: u32 = 1;

const STREAM_BUFFER_LEN: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelSettings {
    external_lut: [u8; 2],
    otp_lut: [u8; 2],
}

impl PanelSettings {
    pub const fn new(external_lut: [u8; 2], otp_lut: [u8; 2]) -> Self {
        Self {
            external_lut,
            otp_lut,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefreshTemperature {
    full: u8,
    fast: u8,
}

impl RefreshTemperature {
    pub const fn new(full: u8, fast: u8) -> Self {
        Self { full, fast }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VcomDataInterval {
    full: u8,
    fast: u8,
}

impl VcomDataInterval {
    pub const fn new(full: u8, fast: u8) -> Self {
        Self { full, fast }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    width: u16,
    visible_height: u16,
    addressed_height: u16,
    gate_offset: u16,

    panel_settings: PanelSettings,

    power_off_sequence: u8,
    pll: u8,

    gate_scan: u8,
    cascade_control: u8,

    temperature: RefreshTemperature,

    vcom: VcomDataInterval,
}

impl Config {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        width: u16,
        visible_height: u16,
        addressed_height: u16,
        gate_offset: u16,
        panel_settings: PanelSettings,
        power_off_sequence: u8,
        pll: u8,
        gate_scan: u8,
        cascade_control: u8,
        temperature: RefreshTemperature,
        vcom: VcomDataInterval,
    ) -> Self {
        Self {
            width,
            visible_height,
            addressed_height,
            gate_offset,
            panel_settings,
            power_off_sequence,
            pll,
            gate_scan,
            cascade_control,
            temperature,
            vcom,
        }
    }

    pub const fn width(self) -> u16 {
        self.width
    }

    pub const fn visible_height(self) -> u16 {
        self.visible_height
    }

    pub const fn addressed_height(self) -> u16 {
        self.addressed_height
    }

    pub const fn gate_offset(self) -> u16 {
        self.gate_offset
    }

    pub const fn buffer_len(self) -> usize {
        self.width as usize / 8 * self.visible_height as usize
    }
}

/// XTEINK X4 Pro 800×480 UC8279 configuration.
///
/// the controller scans 800×600 gates, with the bonded 480-row visible
/// region starting at gate 120.
pub const X4_PRO_800X480: Config = Config::new(
    800,
    480,
    600,
    120,
    PanelSettings::new([0x37, 0x4d], [0x17, 0x4d]),
    0x20,
    0x0e,
    0x02,
    0x02,
    RefreshTemperature::new(0x1e, 0x5a),
    VcomDataInterval::new(0x97, 0xd7),
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshMode {
    /// factory OTP GC/full refresh.
    Full,
    /// explicit cleaning refresh.
    ///
    /// UC8279-X4 does not expose the distinct "HALF" waveform used by
    /// SSD1677/UC8179, so this maps to a non-differential GC refresh.
    Clean,
    /// differential DU page/UI refresh.
    Fast,
}

#[derive(Debug)]
pub enum Error<E> {
    Bus(E),
    InvalidGeometry,
    InvalidFrameLength { expected: usize, actual: usize },
}

pub struct Uc8279X4 {
    config: Config,

    screen_on: bool,

    need_full_clear: bool,
    old_plane_valid: bool,

    dark_background: bool,
}

impl Uc8279X4 {
    pub const fn new(config: Config) -> Self {
        Self {
            config,
            screen_on: false,
            need_full_clear: true,
            old_plane_valid: false,
            dark_background: false,
        }
    }

    pub const fn config(&self) -> Config {
        self.config
    }

    pub fn set_dark_background(&mut self, enabled: bool) {
        self.dark_background = enabled;
    }

    pub fn request_clean(&mut self) {
        self.need_full_clear = true;
    }

    pub fn skip_initial_clean(&mut self) {
        self.need_full_clear = false;
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

        // FreeInk uses EpdBus::reset(50) on this controller: normal reset pulse
        // followed by another 50 ms settle.
        delay.delay_ms(RESET_SETTLE_MS).await;

        self.write_register(
            bus,
            Command::PanelSetting,
            &self.config.panel_settings.external_lut,
        )
        .await?;

        self.write_resolution(bus).await?;

        self.write_register(bus, Command::GateSourceStart, &[0x00, 0x00, 0x00, 0x00])
            .await?;

        self.write_register(
            bus,
            Command::PowerOffSequence,
            &[self.config.power_off_sequence],
        )
        .await?;

        self.write_register(bus, Command::Pll, &[self.config.pll])
            .await?;

        self.write_register(bus, Command::GateScan, &[self.config.gate_scan])
            .await?;

        self.screen_on = false;
        self.need_full_clear = true;
        self.old_plane_valid = false;

        Ok(())
    }

    pub async fn display<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        frame: &[u8],
        mode: RefreshMode,
        turn_off: bool,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.validate_frame(frame)?;

        // only an explicit FAST request may use DU.
        // FULL/CLEAN always force the non-differential GC path.
        let fast = mode == RefreshMode::Fast && !self.need_full_clear && self.old_plane_valid;

        // DTM2 = NEW frame.
        self.stream_plane(bus, Command::NewPlane, frame, false)
            .await?;

        if !fast {
            // full/initial refresh starts from a white OLD plane.
            self.fill_white_plane(bus, Command::OldPlane).await?;
        } else if self.dark_background {
            // re-drive every target pixel. This prevents unchanged black
            // regions from accumulating white residue over repeated DU refreshes.
            self.stream_plane(bus, Command::OldPlane, frame, true)
                .await?;
        }

        self.configure_refresh(bus, fast).await?;
        self.power_on_if_needed(bus, delay).await?;

        if fast {
            // this is essential on UC8279-X4:
            // PTIN without PTL runs a waveform but does not develop the image
            // on field hardware.
            self.command(bus, Command::PartialIn).await?;
            self.write_full_partial_window(bus).await?;
        }

        // UC8279-X4 reloads panel settings during PON. The vendor sequence
        // therefore rewrites PSR after PON and immediately before DRF.
        self.write_register(
            bus,
            Command::PanelSetting,
            &self.config.panel_settings.otp_lut,
        )
        .await?;

        self.command(bus, Command::DisplayRefresh).await?;

        self.wait_ready(bus, delay).await?;

        if fast {
            self.command(bus, Command::PartialOut).await?;
        }

        // seed DTM1 with the newly displayed frame so the next DU refresh
        // has the correct OLD state.
        self.stream_plane(bus, Command::OldPlane, frame, false)
            .await?;

        self.old_plane_valid = true;
        self.need_full_clear = false;

        if turn_off {
            self.power_off(bus, delay).await?;
        }

        Ok(())
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
            self.power_off(bus, delay).await?;
        }

        self.write_register(bus, Command::DeepSleep, &[0xa5])
            .await?;

        self.need_full_clear = true;
        self.old_plane_valid = false;

        Ok(())
    }

    async fn configure_refresh<B>(&self, bus: &mut B, fast: bool) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        let cdi = if fast {
            self.config.vcom.fast
        } else {
            self.config.vcom.full
        };

        self.write_register(bus, Command::VcomDataInterval, &[cdi])
            .await?;

        self.write_register(bus, Command::CascadeControl, &[self.config.cascade_control])
            .await?;

        let temperature = if fast {
            self.config.temperature.fast
        } else {
            self.config.temperature.full
        };

        self.write_register(bus, Command::Temperature, &[temperature])
            .await?;

        if fast {
            self.write_register(
                bus,
                Command::PowerOffSequence,
                &[self.config.power_off_sequence],
            )
            .await?;

            self.write_register(bus, Command::GateScan, &[self.config.gate_scan])
                .await?;
        }

        Ok(())
    }

    async fn write_resolution<B>(&self, bus: &mut B) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        self.write_register(
            bus,
            Command::Resolution,
            &[
                (self.config.width >> 8) as u8,
                self.config.width as u8,
                (self.config.addressed_height >> 8) as u8,
                self.config.addressed_height as u8,
            ],
        )
        .await
    }

    async fn write_full_partial_window<B>(&self, bus: &mut B) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        let x_end = self.config.width - 1;
        let y_start = self.config.gate_offset;
        let y_end = self.config.gate_offset + self.config.visible_height - 1;

        self.write_register(
            bus,
            Command::PartialWindow,
            &[
                0x00,
                0x00,
                (x_end >> 8) as u8,
                (x_end as u8) | 0x07,
                (y_start >> 8) as u8,
                y_start as u8,
                (y_end >> 8) as u8,
                y_end as u8,
                0x01,
            ],
        )
        .await
    }

    async fn stream_plane<B>(
        &self,
        bus: &mut B,
        command: Command,
        frame: &[u8],
        invert: bool,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        let row_bytes = self.config.width as usize / 8;

        self.command(bus, command).await?;

        // UC8279-X4 is different from UC8179 here:
        // - 600 gates are scanned, but the visible 480-row glass starts at gate
        // - 120. Everything before the visible area must therefore be padded white.
        for _ in 0..self.config.gate_offset {
            self.write_white_row(bus, row_bytes).await?;
        }

        // hardware-confirmed X4 Pro orientation:
        // rows forward, bytes as-is.
        // PSR.SHL handles the horizontal scan direction.
        for row in 0..self.config.visible_height as usize {
            let start = row * row_bytes;
            let end = start + row_bytes;
            let source = &frame[start..end];

            if invert {
                self.write_inverted_row(bus, source).await?;
            } else {
                bus.data(source).await.map_err(Error::Bus)?;
            }
        }

        let visible_end = self.config.gate_offset + self.config.visible_height;

        for _ in visible_end..self.config.addressed_height {
            self.write_white_row(bus, row_bytes).await?;
        }

        Ok(())
    }

    async fn fill_white_plane<B>(
        &self,
        bus: &mut B,
        command: Command,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        let row_bytes = self.config.width as usize / 8;

        self.command(bus, command).await?;

        for _ in 0..self.config.addressed_height {
            self.write_white_row(bus, row_bytes).await?;
        }

        Ok(())
    }

    async fn write_inverted_row<B>(&self, bus: &mut B, source: &[u8]) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        let mut buffer = [0u8; STREAM_BUFFER_LEN];
        let mut offset = 0;

        while offset < source.len() {
            let len = (source.len() - offset).min(STREAM_BUFFER_LEN);
            for index in 0..len {
                buffer[index] = !source[offset + index];
            }

            bus.data(&buffer[..len]).await.map_err(Error::Bus)?;

            offset += len;
        }

        Ok(())
    }

    async fn write_white_row<B>(&self, bus: &mut B, row_bytes: usize) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        const WHITE: [u8; STREAM_BUFFER_LEN] = [0xff; STREAM_BUFFER_LEN];

        let mut remaining = row_bytes;

        while remaining != 0 {
            let len = remaining.min(WHITE.len());
            bus.data(&WHITE[..len]).await.map_err(Error::Bus)?;

            remaining -= len;
        }

        Ok(())
    }

    async fn power_on_if_needed<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        if self.screen_on {
            return Ok(());
        }

        self.command(bus, Command::PowerOn).await?;
        self.wait_ready(bus, delay).await?;
        self.screen_on = true;

        Ok(())
    }

    async fn power_off<B, D>(&mut self, bus: &mut B, delay: &mut D) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        if !self.screen_on {
            return Ok(());
        }

        self.command(bus, Command::PowerOff).await?;
        self.wait_ready(bus, delay).await?;
        self.screen_on = false;

        Ok(())
    }

    async fn wait_ready<B, D>(&self, bus: &mut B, delay: &mut D) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        // UC81xx production behavior:
        // BUSY_N is idle HIGH. Give the command one tick to assert BUSY LOW,
        // then wait until it returns HIGH.
        delay.delay_ms(BUSY_SETTLE_MS).await;

        bus.wait_busy(BUSY_POLARITY, delay)
            .await
            .map_err(Error::Bus)
    }

    async fn command<B>(&self, bus: &mut B, command: Command) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        bus.command(command.byte()).await.map_err(Error::Bus)
    }

    async fn write_register<B>(
        &self,
        bus: &mut B,
        command: Command,
        data: &[u8],
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        // match the X4 Pro production framing:
        // - command = one CS pulse
        // - payload = following data CS pulse
        self.command(bus, command).await?;

        bus.data(data).await.map_err(Error::Bus)
    }

    fn validate_geometry<E>(&self) -> Result<(), Error<E>> {
        let visible_end = self.config.gate_offset as u32 + self.config.visible_height as u32;

        if self.config.width == 0
            || !self.config.width.is_multiple_of(8)
            || self.config.visible_height == 0
            || self.config.addressed_height == 0
            || visible_end > self.config.addressed_height as u32
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
}
