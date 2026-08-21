#![no_std]

use embedded_hal_async::delay::DelayNs;
use epd_bus::{BusyPolarity, EpdInterface};

use crate::command::Command;

mod command;

const BUSY_POLARITY: BusyPolarity = BusyPolarity::ActiveLow;

const BUSY_ASSERT_SETTLE_MS: u32 = 1;
const EXTRA_RESET_SETTLE_MS: u32 = 50;

const STREAM_BUFFER_LEN: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelSettings {
    init: [u8; 2],
    otp: [u8; 2],
}

impl PanelSettings {
    pub const fn new(init: [u8; 2], otp: [u8; 2]) -> Self {
        Self { init, otp }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoosterSoftStart {
    bytes: [u8; 4],
}

impl BoosterSoftStart {
    pub const fn new(bytes: [u8; 4]) -> Self {
        Self { bytes }
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
    active: [u8; 2],
    idle: [u8; 2],
}

impl VcomDataInterval {
    pub const fn new(active: [u8; 2], idle: [u8; 2]) -> Self {
        Self { active, idle }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    width: u16,
    visible_height: u16,
    addressed_height: u16,

    panel_settings: PanelSettings,

    power_off_sequence: u8,
    booster: BoosterSoftStart,

    gate_scan: u8,
    cascade_control: u8,

    temperature: RefreshTemperature,

    vcom_data_interval: VcomDataInterval,

    power_save: Option<u8>,
}

impl Config {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        width: u16,
        visible_height: u16,
        addressed_height: u16,
        panel_settings: PanelSettings,
        power_off_sequence: u8,
        booster: BoosterSoftStart,
        gate_scan: u8,
        cascade_control: u8,
        temperature: RefreshTemperature,
        vcom_data_interval: VcomDataInterval,
        power_save: Option<u8>,
    ) -> Self {
        Self {
            width,
            visible_height,
            addressed_height,
            panel_settings,
            power_off_sequence,
            booster,
            gate_scan,
            cascade_control,
            temperature,
            vcom_data_interval,
            power_save,
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

    pub const fn buffer_len(self) -> usize {
        self.width as usize / 8 * self.visible_height as usize
    }
}

/// X4/X4 Pro UC8179 configuration recovered from the OEM firmware and used by
/// FreeInk's current UC8179 implementation.
pub const X4_PRO_800X480: Config = Config::new(
    800,
    480,
    600,
    PanelSettings::new([0x3f, 0x0a], [0x1f, 0x0a]),
    0x20,
    BoosterSoftStart::new([0x25, 0x25, 0x3c, 0x25]),
    0x02,
    0x02,
    RefreshTemperature::new(0x1e, 0x5a),
    VcomDataInterval::new([0x29, 0x07], [0xa9, 0x07]),
    Some(0x22),
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshMode {
    Full,
    /// strong charge-scrubbing refresh.
    Clean,
    /// differential page/UI refresh.
    Fast,
}

#[derive(Debug)]
pub enum Error<E> {
    Bus(E),
    InvalidGeometry,
    InvalidFrameLength { expected: usize, actual: usize },
}

pub struct Uc8179 {
    config: Config,

    screen_on: bool,

    need_full_clear: bool,
    old_plane_valid: bool,

    dark_background: bool,
}

impl Uc8179 {
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

        // FreeInk's UC8179 path calls EpdBus::reset(50), i.e. the normal
        // reset sequence plus another 50 ms of settling before controller
        // initialization.
        delay.delay_ms(EXTRA_RESET_SETTLE_MS).await;

        self.command_data(bus, Command::PanelSetting, &self.config.panel_settings.init)
            .await?;

        self.write_resolution(bus).await?;

        self.command_data(bus, Command::GateSourceStart, &[0x00, 0x00, 0x00, 0x00])
            .await?;

        self.command_data(
            bus,
            Command::PowerOffSequence,
            &[self.config.power_off_sequence],
        )
        .await?;

        self.command_data(bus, Command::BoosterSoftStart, &self.config.booster.bytes)
            .await?;

        self.command_data(bus, Command::GateScan, &[self.config.gate_scan])
            .await?;

        if let Some(power_save) = self.config.power_save {
            self.command_data(bus, Command::PowerSave, &[power_save])
                .await?;
        }

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

        let scrub = mode == RefreshMode::Clean;

        let fast = mode == RefreshMode::Fast && !self.need_full_clear && self.old_plane_valid;

        // DTM2 is always the NEW frame.
        self.stream_plane(bus, Command::NewPlane, frame, false)
            .await?;

        if !fast {
            if scrub {
                // a clean pass gives every target pixel an actual transition:
                // white target -> OLD black / NEW white
                // black target -> OLD white / NEW black
                //
                // this avoids latent AA charge surviving in unchanged cells.
                self.stream_plane(bus, Command::OldPlane, frame, true)
                    .await?;
            } else {
                // full / first refresh starts from an absolute white OLD plane.
                self.fill_white_plane(bus, Command::OldPlane).await?;
            }
        } else if self.dark_background {
            // re-drive every pixel for dark-background content instead of
            // allowing unchanged dark pixels to accumulate light residue.
            self.stream_plane(bus, Command::OldPlane, frame, true)
                .await?;
        }

        self.configure_refresh(bus, fast).await?;

        if !self.screen_on {
            self.command(bus, Command::PowerOn).await?;
            self.wait_ready(bus, delay).await?;

            self.screen_on = true;
        }

        if fast {
            self.command(bus, Command::PartialIn).await?;
        }

        self.command(bus, Command::DisplayRefresh).await?;
        self.wait_ready(bus, delay).await?;

        if fast {
            self.command(bus, Command::PartialOut).await?;
        }

        self.command_data(
            bus,
            Command::VcomDataInterval,
            &self.config.vcom_data_interval.idle,
        )
        .await?;

        // DTM1 becomes the baseline for the next differential refresh.
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

        self.command_data(bus, Command::DeepSleep, &[0xa5]).await?;

        self.need_full_clear = true;
        self.old_plane_valid = false;

        Ok(())
    }

    async fn configure_refresh<B>(&self, bus: &mut B, fast: bool) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        self.command_data(
            bus,
            Command::VcomDataInterval,
            &self.config.vcom_data_interval.active,
        )
        .await?;

        self.command_data(bus, Command::CascadeControl, &[self.config.cascade_control])
            .await?;

        let temperature = if fast {
            self.config.temperature.fast
        } else {
            self.config.temperature.full
        };

        self.command_data(bus, Command::Temperature, &[temperature])
            .await?;

        // the refresh path clears REG and uses the controller's factory OTP
        // waveforms. SHL remains set for our framebuffer orientation.
        self.command_data(bus, Command::PanelSetting, &self.config.panel_settings.otp)
            .await?;

        if fast {
            self.command_data(
                bus,
                Command::PowerOffSequence,
                &[self.config.power_off_sequence],
            )
            .await?;

            self.command_data(bus, Command::GateScan, &[self.config.gate_scan])
                .await?;
        }

        Ok(())
    }

    async fn write_resolution<B>(&self, bus: &mut B) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        self.command_data(
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

        bus.begin_stream(command.byte()).await.map_err(Error::Bus)?;

        let mut result = Ok(());

        // UC8179/X4 Pro panel order is vertically reversed. PSR.SHL handles
        // the horizontal direction.
        for row in (0..self.config.visible_height as usize).rev() {
            let start = row * row_bytes;
            let end = start + row_bytes;
            let source = &frame[start..end];

            let write_result = if invert {
                self.stream_inverted(bus, source).await
            } else {
                bus.stream_data(source).await.map_err(Error::Bus)
            };

            if let Err(error) = write_result {
                result = Err(error);
                break;
            }
        }

        // TRES is 800x600 even though only 480 rows are visible. The remaining
        // 120 gate rows are transferred as white.
        if result.is_ok() {
            let padding_rows = self.config.addressed_height - self.config.visible_height;

            for _ in 0..padding_rows {
                if let Err(error) = self.stream_white_row(bus, row_bytes).await {
                    result = Err(error);
                    break;
                }
            }
        }

        let end_result = bus.end_stream().map_err(Error::Bus);

        match (result, end_result) {
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) => Err(error),
            (Ok(()), Ok(())) => Ok(()),
        }
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

        bus.begin_stream(command.byte()).await.map_err(Error::Bus)?;

        let mut result = Ok(());

        for _ in 0..self.config.addressed_height {
            if let Err(error) = self.stream_white_row(bus, row_bytes).await {
                result = Err(error);
                break;
            }
        }

        let end_result = bus.end_stream().map_err(Error::Bus);

        match (result, end_result) {
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) => Err(error),
            (Ok(()), Ok(())) => Ok(()),
        }
    }

    async fn stream_inverted<B>(&self, bus: &mut B, source: &[u8]) -> Result<(), Error<B::Error>>
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

            bus.stream_data(&buffer[..len]).await.map_err(Error::Bus)?;

            offset += len;
        }

        Ok(())
    }

    async fn stream_white_row<B>(
        &self,
        bus: &mut B,
        row_bytes: usize,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        const WHITE: [u8; STREAM_BUFFER_LEN] = [0xff; STREAM_BUFFER_LEN];

        let mut remaining = row_bytes;

        while remaining != 0 {
            let len = remaining.min(WHITE.len());

            bus.stream_data(&WHITE[..len]).await.map_err(Error::Bus)?;

            remaining -= len;
        }

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
        // the production UC8179 sequence waits a scheduler tick before
        // polling BUSY_N. Sampling immediately can see the previous idle-high
        // state before the controller has asserted BUSY low.
        delay.delay_ms(BUSY_ASSERT_SETTLE_MS).await;
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

    async fn command_data<B>(
        &self,
        bus: &mut B,
        command: Command,
        data: &[u8],
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        bus.command_data(command.byte(), data)
            .await
            .map_err(Error::Bus)
    }

    fn validate_geometry<E>(&self) -> Result<(), Error<E>> {
        if self.config.width == 0
            || !self.config.width.is_multiple_of(8)
            || self.config.visible_height == 0
            || self.config.addressed_height < self.config.visible_height
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
