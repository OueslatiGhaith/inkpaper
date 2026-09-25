#![no_std]

use embedded_hal_async::delay::DelayNs;
use epd_bus::{BusyPolarity, EpdInterface};

use crate::{
    command::Command,
    grayscale::{ABSOLUTE_DARK_GRAY_LUT, GRAY_LUTS, GRAY_PRE_BW_MID},
};

mod command;
mod grayscale;

const BUSY_POLARITY: BusyPolarity = BusyPolarity::ActiveLow;

const BUSY_ASSERT_SETTLE_MS: u32 = 1;
const EXTRA_RESET_SETTLE_MS: u32 = 50;

const STREAM_BUFFER_LEN: usize = 128;

const GRAY_LUT_REGISTERS: [Command; 5] = [
    Command::LutVcom,
    Command::LutWhite,
    Command::LutBlackToWhite,
    Command::LutWhiteToBlack,
    Command::LutBlack,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct BoosterSoftStart {
    bytes: [u8; 4],
}

impl BoosterSoftStart {
    pub const fn new(bytes: [u8; 4]) -> Self {
        Self { bytes }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum RefreshMode {
    Full,
    /// strong charge-scrubbing refresh.
    Clean,
    /// differential page/UI refresh.
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
    EmptyRegion,
    RegionOutOfBounds { region: Region },
}

pub struct Uc8179 {
    config: Config,

    screen_on: bool,
    power_off_pending: bool,
    power_on_pending: bool,

    need_full_clear: bool,
    old_plane_valid: bool,

    grayscale_on_panel: bool,

    dark_background: bool,
}

impl Uc8179 {
    pub const fn new(config: Config) -> Self {
        Self {
            config,
            screen_on: false,
            power_off_pending: false,
            power_on_pending: false,
            need_full_clear: true,
            old_plane_valid: false,
            grayscale_on_panel: false,
            dark_background: false,
        }
    }

    pub const fn config(&self) -> Config {
        self.config
    }

    pub const fn grayscale_on_panel(&self) -> bool {
        self.grayscale_on_panel
    }

    pub const fn power_off_pending(&self) -> bool {
        self.power_off_pending
    }

    pub const fn power_on_pending(&self) -> bool {
        self.power_on_pending
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

        self.finish_pending_power_off(bus, delay).await?;
        self.finish_pending_power_on(bus, delay).await?;

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
        self.power_off_pending = false;
        self.power_on_pending = false;
        self.need_full_clear = true;
        self.old_plane_valid = false;
        self.grayscale_on_panel = false;

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

        self.finish_pending_power_off(bus, delay).await?;
        self.finish_pending_power_on(bus, delay).await?;

        // a caller that bypasses display_grayscale_window() and asks for a Fast B/W update
        // while grayscale is physically present must not use an ordinary DU transition.
        let scrub =
            mode == RefreshMode::Clean || (mode == RefreshMode::Fast && self.grayscale_on_panel);

        let fast = mode == RefreshMode::Fast
            && !self.grayscale_on_panel
            && !self.need_full_clear
            && self.old_plane_valid;

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
        self.grayscale_on_panel = false;

        if turn_off {
            self.power_off(bus, delay).await?;
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

        self.finish_pending_power_off(bus, delay).await?;
        self.finish_pending_power_on(bus, delay).await?;

        self.display_absolute_grayscale_base(bus, delay, lsb, msb)
            .await?;

        self.stream_plane(bus, Command::OldPlane, lsb, false)
            .await?;

        self.stream_plane(bus, Command::NewPlane, msb, false)
            .await?;

        self.activate_grayscale(bus, delay, None).await?;

        // the selector planes are not a usable DU baseline.
        // restore the B/W base to both controller planes without another physical refresh.
        self.restore_grayscale_base(bus, lsb, msb).await?;

        self.old_plane_valid = true;
        self.need_full_clear = false;
        self.grayscale_on_panel = lsb != msb;

        if turn_off {
            self.power_off(bus, delay).await?;
        }

        Ok(())
    }

    /// performs a damage-scoped four-level grayscale update.
    ///
    /// the UC8179 PTL-scoped PRE_BW_MID + grayscale sequence was validated on XTEINK X4 Pro
    /// hardware with static grayscale content outside the update region.
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

        let region = self.normalize_region(region)?;

        self.finish_pending_power_off(bus, delay).await?;
        self.finish_pending_power_on(bus, delay).await?;

        let full = Region::new(0, 0, self.config.width, self.config.visible_height);

        // use the established whole-panel path whenever we do not have a valid previous B/W base.
        // also use it the first time grayscale appears. This follows the OEM/FreeInk model:
        // the first AA frame is established normally. Later AA pages can use the PRE_BW_MID transition.
        if mode != RefreshMode::Fast
            || self.need_full_clear
            || !self.old_plane_valid
            || (!self.grayscale_on_panel && lsb != msb)
        {
            self.display_grayscale(bus, delay, lsb, msb, turn_off)
                .await?;

            return Ok(full);
        }

        // DTM1 currently contains the preceding frame's clean B/W base. Put the new base into DTM2.
        self.stream_and_plane(bus, Command::NewPlane, lsb, msb)
            .await?;

        // drive only the damage rectangle from the previous base to the new base using
        // XTF_PRE_BW_MID.
        self.run_grayscale_precondition(bus, delay, region).await?;

        if self.region_has_grayscale(lsb, msb, region) {
            // our framebuffer already uses UC8179's absolute selector encoding:
            // black = 00
            // dark  = 10
            // light = 01
            // white = 11
            self.stream_plane(bus, Command::OldPlane, lsb, false)
                .await?;

            self.stream_plane(bus, Command::NewPlane, msb, false)
                .await?;

            // run the short custom grayscale waveform only over the damage window.
            self.activate_grayscale(bus, delay, Some(region)).await?;
        }

        // RAM must once again contain the B/W base, even though
        // physical grayscale remains visible.
        self.restore_grayscale_base(bus, lsb, msb).await?;

        self.old_plane_valid = true;
        self.need_full_clear = false;
        self.grayscale_on_panel = lsb != msb;

        if turn_off {
            self.power_off(bus, delay).await?;
        }

        Ok(region)
    }

    pub async fn display_binary_window_preserving_grayscale<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        lsb: &[u8],
        msb: &[u8],
        region: Region,
        turn_off: bool,
    ) -> Result<Region, Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.validate_frame(lsb)?;
        self.validate_frame(msb)?;

        let region = self.normalize_region(region)?;

        self.finish_pending_power_off(bus, delay).await?;
        self.finish_pending_power_on(bus, delay).await?;

        // This path relies on the clean B/W baseline established by the grayscale-window
        // implementation.
        if self.need_full_clear || !self.old_plane_valid || !self.grayscale_on_panel {
            return self
                .display_grayscale_window(bus, delay, lsb, msb, region, RefreshMode::Fast, turn_off)
                .await;
        }

        // a binary-preserving update is valid only if the damaged region itself contains
        // no native Gray4 pixels.
        if self.region_has_grayscale(lsb, msb, region) {
            return self
                .display_grayscale_window(bus, delay, lsb, msb, region, RefreshMode::Fast, turn_off)
                .await;
        }

        // DTM1 currently contains the preceding clean B/W base.
        // only replace DTM2 inside the damaged window. PTL causes the controller's DTM
        // write pointer to consume exactly the supplied window payload rather than
        // a complete screen.
        self.stream_and_plane_window(bus, Command::NewPlane, lsb, msb, region)
            .await?;

        // physically transition only that same region from the previous B/W base to
        // the new one using XTF_PRE_BW_MID.
        self.run_grayscale_precondition(bus, delay, region).await?;

        // no Gray4 selector waveform ran, so DTM2 still contains the correct new baseline.
        // update only the corresponding DTM1 window so both controller planes agree
        // again for the next differential transition.
        self.stream_and_plane_window(bus, Command::OldPlane, lsb, msb, region)
            .await?;

        self.old_plane_valid = true;
        self.need_full_clear = false;

        // native grayscale outside the updated window was deliberately left untouched.
        self.grayscale_on_panel = true;

        if turn_off {
            self.power_off(bus, delay).await?;
        }

        Ok(region)
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
        // a previously deferred shutdown must be complete before sending another
        // controller command.
        self.finish_pending_power_off(bus, delay).await?;
        self.finish_pending_power_on(bus, delay).await?;

        if self.screen_on {
            self.power_off(bus, delay).await?;

            // DeepSleep must not race the asynchronous PowerOff sequence.
            self.finish_pending_power_off(bus, delay).await?;
        }

        self.command_data(bus, Command::DeepSleep, &[0xa5]).await?;

        self.need_full_clear = true;
        self.old_plane_valid = false;
        self.grayscale_on_panel = false;

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

        self.command(bus, command).await?;
        bus.begin_data_stream().await.map_err(Error::Bus)?;

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

        let end_result = bus.end_data_stream().map_err(Error::Bus);

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

        self.command(bus, command).await?;
        bus.begin_data_stream().await.map_err(Error::Bus)?;

        let mut result = Ok(());

        for _ in 0..self.config.addressed_height {
            if let Err(error) = self.stream_white_row(bus, row_bytes).await {
                result = Err(error);
                break;
            }
        }

        let end_result = bus.end_data_stream().map_err(Error::Bus);

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

    pub async fn begin_power_off<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.power_off(bus, delay).await
    }

    async fn power_off<B, D>(&mut self, bus: &mut B, delay: &mut D) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.finish_pending_power_on(bus, delay).await?;

        if !self.screen_on {
            return Ok(());
        }

        self.command(bus, Command::PowerOff).await?;

        // PowerOff continues autonomously in the controller.
        //
        // do not wait here: there is no reason for the CPU to remain blocked after
        // the visible update has completed. Any later operation touching the controller
        // must resolve this pending state first.
        self.screen_on = false;
        self.power_off_pending = true;

        Ok(())
    }

    async fn finish_pending_power_off<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        if !self.power_off_pending {
            return Ok(());
        }

        // we still retain the normal assertion-settle delay here. In the usual page-turn
        // path PowerOff was issued far earlier, so BUSY will already be idle and this costs
        // only the 1 ms guard.
        //
        // keeping the guard also makes an immediate follow-up operation safe.
        self.wait_ready(bus, delay).await?;

        self.power_off_pending = false;

        Ok(())
    }

    pub async fn prepare_power_on<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        debug_assert!(!(self.power_off_pending && self.power_on_pending));

        if self.screen_on || self.power_on_pending {
            return Ok(());
        }

        if self.power_off_pending {
            // Do not synchronously wait for an in-flight PowerOff here.
            //
            // That would move the ~80 ms shutdown stall back in front of rendering
            // and undo the previous optimization.
            //
            // Wait only long enough to guarantee that BUSY has had time to assert,
            // then sample once. If shutdown is still running, rendering proceeds
            // immediately and the ordinary presentation path will finish it later.
            delay.delay_ms(BUSY_ASSERT_SETTLE_MS).await;

            if bus.is_busy(BUSY_POLARITY).map_err(Error::Bus)? {
                return Ok(());
            }

            self.power_off_pending = false;
        }

        self.command(bus, Command::PowerOn).await?;
        self.power_on_pending = true;

        Ok(())
    }

    async fn finish_pending_power_on<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        if !self.power_on_pending {
            return Ok(());
        }

        self.wait_ready(bus, delay).await?;

        self.power_on_pending = false;
        self.screen_on = true;

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
        self.command(bus, command).await?;
        bus.data(data).await.map_err(Error::Bus)
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

    async fn display_absolute_grayscale_base<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        lsb: &[u8],
        msb: &[u8],
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.stream_and_plane(bus, Command::NewPlane, lsb, msb)
            .await?;

        self.fill_white_plane(bus, Command::OldPlane).await?;

        self.configure_refresh(bus, false).await?;

        if !self.screen_on {
            self.command(bus, Command::PowerOn).await?;

            self.wait_ready(bus, delay).await?;

            self.screen_on = true;
        }

        self.command(bus, Command::DisplayRefresh).await?;

        self.wait_ready(bus, delay).await?;

        self.command_data(
            bus,
            Command::VcomDataInterval,
            &self.config.vcom_data_interval.idle,
        )
        .await
    }

    async fn stream_and_plane<B>(
        &self,
        bus: &mut B,
        command: Command,
        lhs: &[u8],
        rhs: &[u8],
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        let row_bytes = self.config.width as usize / 8;

        self.command(bus, command).await?;

        bus.begin_data_stream().await.map_err(Error::Bus)?;

        let mut result = Ok(());
        let mut buffer = [0u8; STREAM_BUFFER_LEN];

        for row in (0..self.config.visible_height as usize).rev() {
            let row_start = row * row_bytes;

            let mut offset = 0;

            while offset < row_bytes {
                let len = (row_bytes - offset).min(buffer.len());

                for index in 0..len {
                    let source_index = row_start + offset + index;

                    buffer[index] = grayscale_base_byte(lhs[source_index], rhs[source_index]);
                }

                if let Err(error) = bus.stream_data(&buffer[..len]).await.map_err(Error::Bus) {
                    result = Err(error);
                    break;
                }

                offset += len;
            }

            if result.is_err() {
                break;
            }
        }

        if result.is_ok() {
            let padding_rows = self.config.addressed_height - self.config.visible_height;

            for _ in 0..padding_rows {
                if let Err(error) = self.stream_white_row(bus, row_bytes).await {
                    result = Err(error);
                    break;
                }
            }
        }

        let end_result = bus.end_data_stream().map_err(Error::Bus);

        match (result, end_result) {
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) => Err(error),
            (Ok(()), Ok(())) => Ok(()),
        }
    }

    async fn stream_and_plane_window<B>(
        &self,
        bus: &mut B,
        command: Command,
        lhs: &[u8],
        rhs: &[u8],
        region: Region,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        let region = self.normalize_region(region)?;

        let stride = self.config.width as usize / 8;

        let first_byte = region.x as usize / 8;
        let row_bytes = region.width as usize / 8;

        debug_assert!(region.x.is_multiple_of(8));
        debug_assert!(region.width.is_multiple_of(8));
        debug_assert!(row_bytes > 0);

        self.enter_partial_window(bus, region).await?;

        self.command(bus, command).await?;

        bus.begin_data_stream().await.map_err(Error::Bus)?;

        let mut result = Ok(());
        let mut buffer = [0u8; STREAM_BUFFER_LEN];

        let first_row = region.y as usize;
        let end_row = first_row + region.height as usize;

        // UC8179/X4 Pro panel memory order is vertically reversed.
        //
        // partial_window_data() performs the same transformation for PTL, so the payload
        // must follow the same order as stream_and_plane():
        // bottom framebuffer row first, top framebuffer row last.
        for row in (first_row..end_row).rev() {
            let row_start = row * stride + first_byte;

            let mut offset = 0;

            while offset < row_bytes {
                let len = (row_bytes - offset).min(buffer.len());

                for index in 0..len {
                    let source_index = row_start + offset + index;

                    buffer[index] = grayscale_base_byte(lhs[source_index], rhs[source_index]);
                }

                if let Err(error) = bus.stream_data(&buffer[..len]).await.map_err(Error::Bus) {
                    result = Err(error);
                    break;
                }

                offset += len;
            }

            if result.is_err() {
                break;
            }
        }

        let end_result = bus.end_data_stream().map_err(Error::Bus);

        // always attempt to leave partial mode after opening it.
        let partial_out_result = self.command(bus, Command::PartialOut).await;

        match (result, end_result, partial_out_result) {
            (Err(error), _, _) => Err(error),
            (Ok(()), Err(error), _) => Err(error),
            (Ok(()), Ok(()), Err(error)) => Err(error),
            (Ok(()), Ok(()), Ok(())) => Ok(()),
        }
    }

    async fn load_grayscale_luts<B>(&self, bus: &mut B) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        for index in 0..GRAY_LUTS.len() {
            let data = if index == 3 {
                &ABSOLUTE_DARK_GRAY_LUT
            } else {
                &GRAY_LUTS[index]
            };

            self.command_data(bus, GRAY_LUT_REGISTERS[index], data)
                .await?;
        }

        Ok(())
    }

    async fn activate_grayscale<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        region: Option<Region>,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        if let Some(region) = region {
            self.enter_partial_window(bus, region).await?;
        }

        self.command_data(bus, Command::PanelSetting, &self.config.panel_settings.init)
            .await?;

        self.load_grayscale_luts(bus).await?;

        self.command_data(
            bus,
            Command::VcomDataInterval,
            &self.config.vcom_data_interval.active,
        )
        .await?;

        if !self.screen_on {
            self.command(bus, Command::PowerOn).await?;

            self.wait_ready(bus, delay).await?;

            self.screen_on = true;
        }

        self.command(bus, Command::DisplayRefresh).await?;

        self.wait_ready(bus, delay).await?;

        if region.is_some() {
            self.command(bus, Command::PartialOut).await?;
        }

        self.command_data(
            bus,
            Command::VcomDataInterval,
            &self.config.vcom_data_interval.idle,
        )
        .await
    }

    async fn run_grayscale_precondition<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        region: Region,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.enter_partial_window(bus, region).await?;

        self.command_data(bus, Command::PanelSetting, &self.config.panel_settings.init)
            .await?;

        self.command_data(
            bus,
            Command::PowerOffSequence,
            &[self.config.power_off_sequence],
        )
        .await?;

        self.command_data(bus, Command::GateScan, &[self.config.gate_scan])
            .await?;

        self.command_data(
            bus,
            Command::VcomDataInterval,
            &self.config.vcom_data_interval.active,
        )
        .await?;

        self.command_data(bus, Command::CascadeControl, &[self.config.cascade_control])
            .await?;

        self.command_data(bus, Command::Temperature, &[self.config.temperature.fast])
            .await?;

        for index in 0..GRAY_PRE_BW_MID.len() {
            self.command_data(bus, GRAY_LUT_REGISTERS[index], &GRAY_PRE_BW_MID[index])
                .await?;
        }

        if !self.screen_on {
            self.command(bus, Command::PowerOn).await?;

            self.wait_ready(bus, delay).await?;

            self.screen_on = true;
        }

        self.command(bus, Command::DisplayRefresh).await?;

        self.wait_ready(bus, delay).await?;

        self.command(bus, Command::PartialOut).await?;

        self.command_data(
            bus,
            Command::VcomDataInterval,
            &self.config.vcom_data_interval.idle,
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
        self.stream_and_plane(bus, Command::OldPlane, lsb, msb)
            .await?;
        self.stream_and_plane(bus, Command::NewPlane, lsb, msb)
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

    fn normalize_region<E>(&self, region: Region) -> Result<Region, Error<E>> {
        if region.is_empty() {
            return Err(Error::EmptyRegion);
        }

        let right = region.x as u32 + region.width as u32;
        let bottom = region.y as u32 + region.height as u32;
        if right > self.config.width as u32 || bottom > self.config.visible_height as u32 {
            return Err(Error::RegionOutOfBounds { region });
        }

        let x = region.x & !7;
        let right = ((right + 7) & !7).min(self.config.width as u32) as u16;

        Ok(Region::new(x, region.y, right - x, region.height))
    }

    fn partial_window_data(&self, region: Region) -> [u8; 9] {
        let x_start = region.x;
        let x_end = region.x + region.width - 1;

        // stream_plane() vertically reverses framebuffer rows.
        let y_start = self.config.visible_height - region.y - region.height;
        let y_end = y_start + region.height - 1;

        [
            (x_start >> 8) as u8,
            x_start as u8,
            (x_end >> 8) as u8,
            (x_end as u8) | 0x07,
            (y_start >> 8) as u8,
            y_start as u8,
            (y_end >> 8) as u8,
            y_end as u8,
            0x01,
        ]
    }

    async fn enter_partial_window<B>(
        &self,
        bus: &mut B,
        region: Region,
    ) -> Result<(), Error<B::Error>>
    where
        B: EpdInterface,
    {
        self.command(bus, Command::PartialIn).await?;

        let window = self.partial_window_data(region);

        self.command_data(bus, Command::PartialWindow, &window)
            .await
    }
}

const fn grayscale_base_byte(lsb: u8, msb: u8) -> u8 {
    lsb & msb
}

const fn region_buffer_len(region: Region) -> usize {
    region.width as usize * region.height as usize / 8
}

#[cfg(test)]
mod tests {
    use crate::region_buffer_len;

    use super::{Error, Region, Uc8179, X4_PRO_800X480, grayscale_base_byte};

    #[test]
    fn absolute_grayscale_maps_to_expected_bw_base() {
        // black: 00 -> black base
        assert_eq!(grayscale_base_byte(0x00, 0x00), 0x00);

        // dark: 10 -> black base
        assert_eq!(grayscale_base_byte(0xff, 0x00), 0x00);

        // light: 01 -> black base
        assert_eq!(grayscale_base_byte(0x00, 0xff), 0x00);

        // white: 11 -> white base
        assert_eq!(grayscale_base_byte(0xff, 0xff), 0xff);
    }

    #[test]
    fn partial_region_is_byte_aligned() {
        let panel = Uc8179::new(X4_PRO_800X480);

        let region = panel
            .normalize_region::<()>(Region::new(13, 10, 11, 20))
            .unwrap();

        assert_eq!(region, Region::new(8, 10, 16, 20));
    }

    #[test]
    fn partial_region_rejects_empty_region() {
        let panel = Uc8179::new(X4_PRO_800X480);

        let result = panel.normalize_region::<()>(Region::new(10, 10, 0, 20));

        assert!(matches!(result, Err(Error::EmptyRegion)));
    }

    #[test]
    fn partial_region_rejects_out_of_bounds_region() {
        let panel = Uc8179::new(X4_PRO_800X480);

        let region = Region::new(760, 100, 80, 40);

        let result = panel.normalize_region::<()>(region);

        assert!(matches!(
            result,
            Err(
                Error::RegionOutOfBounds {
                    region: actual,
                }
            ) if actual == region
        ));
    }

    #[test]
    fn partial_window_maps_reversed_rows() {
        let panel = Uc8179::new(X4_PRO_800X480);

        let region = panel
            .normalize_region::<()>(Region::new(13, 10, 11, 20))
            .unwrap();

        assert_eq!(
            panel.partial_window_data(region),
            [0x00, 0x08, 0x00, 0x17, 0x01, 0xc2, 0x01, 0xd5, 0x01],
        );
    }

    #[test]
    fn full_partial_window_covers_visible_panel() {
        let panel = Uc8179::new(X4_PRO_800X480);

        let region = Region::new(0, 0, 800, 480);

        assert_eq!(
            panel.partial_window_data(region),
            [0x00, 0x00, 0x03, 0x1f, 0x00, 0x00, 0x01, 0xdf, 0x01],
        );
    }

    #[test]
    fn partial_region_payload_contains_only_window_bytes() {
        let panel = Uc8179::new(X4_PRO_800X480);

        let region = panel
            .normalize_region::<()>(Region::new(13, 10, 11, 20))
            .unwrap();

        assert_eq!(region, Region::new(8, 10, 16, 20));
        assert_eq!(region_buffer_len(region), 40);
    }

}
