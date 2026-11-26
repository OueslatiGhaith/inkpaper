#![no_std]

use embedded_hal_async::delay::DelayNs;
use epd_bus::{
    BinaryOverGrayMode, BinaryUpdateMode, BusyPolarity, DataStream, DisplayPhase, EpdInterface,
    EpdPanel, GrayscaleUpdateMode, PanelCapabilities, PanelError, RefreshMode, grayscale_base_byte,
    region_has_grayscale,
};

pub use epd_bus::{PanelError as Error, RefreshMode as UcRefreshMode, Region};

use crate::{
    command::Command,
    grayscale::{UC8179_GRAY_LUTS, UC8179_GRAY_PRE_BW_MID, UC8279_QUALITY_GRAYSCALE_LUTS},
};

mod command;
mod grayscale;

const BUSY_POLARITY: BusyPolarity = BusyPolarity::ActiveLow;

const BUSY_ASSERT_SETTLE_MS: u32 = 1;
const EXTRA_RESET_SETTLE_MS: u32 = 50;

const STREAM_BUFFER_LEN: usize = 128;

const UC8179_GRAY_LUT_REGISTERS: [Command; 5] = [
    Command::LutVcom,
    Command::LutWhite,
    Command::LutBlackToWhite,
    Command::LutWhiteToBlack,
    Command::LutBlack,
];

/// The UC8279-X4 factory quality waveform uses the BW/WB registers in exchanged order.
const UC8279_GRAY_LUT_REGISTERS: [Command; 5] = [
    Command::LutVcom,
    Command::LutWhite,
    Command::LutWhiteToBlack,
    Command::LutBlackToWhite,
    Command::LutBlack,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Variant {
    Uc8179,
    Uc8279X4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum RowOrder {
    Forward,
    Reversed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
pub enum VcomDataInterval {
    /// Two-byte active/idle CDI sequence (UC8179).
    ActiveIdle { active: [u8; 2], idle: [u8; 2] },
    /// One-byte full/fast CDI value (UC8279-X4).
    FullFast { full: u8, fast: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Config {
    variant: Variant,

    width: u16,
    visible_height: u16,
    addressed_height: u16,
    gate_offset: u16,
    row_order: RowOrder,

    panel_settings: PanelSettings,

    power_off_sequence: u8,
    booster: Option<[u8; 4]>,
    pll: Option<u8>,

    gate_scan: u8,
    cascade_control: u8,

    temperature: RefreshTemperature,
    vcom_data_interval: VcomDataInterval,

    power_save: Option<u8>,
}

impl Config {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        variant: Variant,
        width: u16,
        visible_height: u16,
        addressed_height: u16,
        gate_offset: u16,
        row_order: RowOrder,
        panel_settings: PanelSettings,
        power_off_sequence: u8,
        booster: Option<[u8; 4]>,
        pll: Option<u8>,
        gate_scan: u8,
        cascade_control: u8,
        temperature: RefreshTemperature,
        vcom_data_interval: VcomDataInterval,
        power_save: Option<u8>,
    ) -> Self {
        Self {
            variant,
            width,
            visible_height,
            addressed_height,
            gate_offset,
            row_order,
            panel_settings,
            power_off_sequence,
            booster,
            pll,
            gate_scan,
            cascade_control,
            temperature,
            vcom_data_interval,
            power_save,
        }
    }

    pub const fn variant(self) -> Variant {
        self.variant
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

/// X4/X4 Pro UC8179 configuration recovered from the OEM firmware and used by
/// FreeInk's UC8179 implementation.
pub const UC8179_X4_PRO_800X480: Config = Config::new(
    Variant::Uc8179,
    800,
    480,
    600,
    0,
    RowOrder::Reversed,
    PanelSettings::new([0x3f, 0x0a], [0x1f, 0x0a]),
    0x20,
    Some([0x25, 0x25, 0x3c, 0x25]),
    None,
    0x02,
    0x02,
    RefreshTemperature::new(0x1e, 0x5a),
    VcomDataInterval::ActiveIdle {
        active: [0x29, 0x07],
        idle: [0xa9, 0x07],
    },
    Some(0x22),
);

/// XTEINK X4 Pro 800×480 UC8279 configuration.
///
/// The controller scans 800×600 gates, with the bonded 480-row visible region
/// starting at gate 120.
pub const UC8279_X4_PRO_800X480: Config = Config::new(
    Variant::Uc8279X4,
    800,
    480,
    600,
    120,
    RowOrder::Forward,
    PanelSettings::new([0x37, 0x4d], [0x17, 0x4d]),
    0x20,
    None,
    Some(0x0e),
    0x02,
    0x02,
    RefreshTemperature::new(0x1e, 0x5a),
    VcomDataInterval::FullFast {
        full: 0x97,
        fast: 0xd7,
    },
    None,
);

pub struct Uc8xxx {
    config: Config,

    screen_on: bool,
    power_off_pending: bool,
    power_on_pending: bool,

    need_full_clear: bool,
    old_plane_valid: bool,

    grayscale_on_panel: bool,
    dark_background: bool,
}

impl Uc8xxx {
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

    pub const fn variant(&self) -> Variant {
        self.config.variant
    }

    const fn is_uc8179(&self) -> bool {
        matches!(self.config.variant, Variant::Uc8179)
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
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.validate_geometry()?;

        self.finish_pending_power_off(bus, delay).await?;
        self.finish_pending_power_on(bus, delay).await?;

        bus.reset(delay).await.map_err(PanelError::Bus)?;

        // FreeInk calls EpdBus::reset(50) on both UC8179 and UC8279-X4: the
        // standard reset pulse followed by 50 ms of extra settling.
        delay.delay_ms(EXTRA_RESET_SETTLE_MS).await;

        self.command_data(
            bus,
            Command::PanelSetting,
            &self.config.panel_settings.external_lut,
        )
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

        if let Some(booster) = self.config.booster {
            self.command_data(bus, Command::BoosterSoftStart, &booster)
                .await?;
        }

        if let Some(pll) = self.config.pll {
            self.command_data(bus, Command::Pll, &[pll]).await?;
        }

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
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.validate_frame(frame)?;

        self.finish_pending_power_off(bus, delay).await?;
        self.finish_pending_power_on(bus, delay).await?;

        // A caller that bypasses display_grayscale_window() and asks for a Fast
        // B/W update while grayscale is physically present must not use an
        // ordinary DU transition. UC8279-X4 has no distinct scrub waveform.
        let scrub = self.is_uc8179()
            && (mode == RefreshMode::Clean
                || (mode == RefreshMode::Fast && self.grayscale_on_panel));

        let fast = mode == RefreshMode::Fast
            && !self.grayscale_on_panel
            && !self.need_full_clear
            && self.old_plane_valid;

        // DTM2 is always the NEW frame.
        self.stream_plane(bus, Command::NewPlane, frame, false)
            .await?;

        if !fast {
            if scrub {
                // Clean pass gives every target pixel an actual transition so
                // latent AA charge cannot survive in unchanged cells.
                self.stream_plane(bus, Command::OldPlane, frame, true)
                    .await?;
            } else {
                // Full / initial refresh starts from a white OLD plane.
                self.fill_white_plane(bus, Command::OldPlane).await?;
            }
        } else if self.dark_background {
            // Re-drive every pixel for dark-background content instead of
            // allowing unchanged dark pixels to accumulate light residue.
            self.stream_plane(bus, Command::OldPlane, frame, true)
                .await?;
        }

        self.configure_refresh(bus, fast).await?;
        self.power_on_if_needed(bus, delay).await?;

        if fast {
            // PTL keeps the last programmed window on UC8179, and on UC8279-X4
            // PTIN without PTL does not develop the image on field hardware.
            let full = Region::new(0, 0, self.config.width, self.config.visible_height);
            self.enter_partial_window(bus, full).await?;
        }

        if !self.is_uc8179() {
            // UC8279-X4 reloads panel settings during PON; rewrite PSR
            // immediately before DRF.
            self.command_data(bus, Command::PanelSetting, &self.config.panel_settings.otp_lut)
                .await?;
        }

        let refresh_phase = if fast {
            DisplayPhase::BinaryFastRefresh
        } else {
            DisplayPhase::BinaryFullRefresh
        };
        self.command(bus, Command::DisplayRefresh).await?;
        self.wait_ready(bus, refresh_phase, delay).await?;

        if fast {
            self.command(bus, Command::PartialOut).await?;
        }

        self.write_idle_vcom(bus).await?;

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
    ) -> Result<(), PanelError<B::Error>>
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

        match self.config.variant {
            Variant::Uc8179 => {
                self.stream_plane(bus, Command::OldPlane, lsb, false)
                    .await?;
                self.stream_plane(bus, Command::NewPlane, msb, false)
                    .await?;

                self.activate_uc8179_grayscale(bus, delay, None).await?;

                // Selector planes are not a usable DU baseline; restore the B/W
                // base to both controller planes without another refresh.
                self.restore_grayscale_base(bus, lsb, msb).await?;

                self.old_plane_valid = true;
                self.need_full_clear = false;
                self.grayscale_on_panel = lsb != msb;
            }
            Variant::Uc8279X4 => {
                // UC8279-X4 uses inverted RAM polarity for the absolute selector planes.
                self.stream_plane(bus, Command::OldPlane, lsb, true).await?;
                self.stream_plane(bus, Command::NewPlane, msb, true).await?;

                self.command_data(
                    bus,
                    Command::PanelSetting,
                    &self.config.panel_settings.external_lut,
                )
                .await?;

                for (register, lut) in UC8279_GRAY_LUT_REGISTERS
                    .iter()
                    .zip(UC8279_QUALITY_GRAYSCALE_LUTS.iter())
                {
                    self.command_data(bus, *register, lut).await?;
                }

                self.write_active_vcom(bus, false).await?;
                self.power_on_if_needed(bus, delay).await?;

                // PON reloads MTP state on UC8279-X4, so reassert external-LUT
                // PSR immediately before DRF.
                self.command_data(
                    bus,
                    Command::PanelSetting,
                    &self.config.panel_settings.external_lut,
                )
                .await?;

                self.command(bus, Command::DisplayRefresh).await?;
                self.wait_ready(bus, DisplayPhase::GrayscaleRefresh, delay)
                    .await?;

                self.need_full_clear = true;
                self.old_plane_valid = false;
                self.grayscale_on_panel = false;
            }
        }

        if turn_off {
            self.power_off(bus, delay).await?;
        }

        Ok(())
    }

    /// Performs a damage-scoped four-level grayscale update on UC8179, or falls
    /// back to a full-panel grayscale refresh on UC8279-X4.
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
    ) -> Result<Region, PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.validate_frame(lsb)?;
        self.validate_frame(msb)?;

        let full = Region::new(0, 0, self.config.width, self.config.visible_height);

        if !self.is_uc8179() {
            self.display_grayscale(bus, delay, lsb, msb, turn_off)
                .await?;
            return Ok(full);
        }

        let region = self.normalize_region(region)?;

        self.finish_pending_power_off(bus, delay).await?;
        self.finish_pending_power_on(bus, delay).await?;

        if mode != RefreshMode::Fast
            || self.need_full_clear
            || !self.old_plane_valid
            || (!self.grayscale_on_panel && lsb != msb)
        {
            self.display_grayscale(bus, delay, lsb, msb, turn_off)
                .await?;
            return Ok(full);
        }

        // DTM1 holds the preceding frame's clean B/W base; update only `region` in DTM2.
        self.stream_and_plane_window(bus, Command::NewPlane, lsb, msb, region)
            .await?;

        self.run_grayscale_precondition(bus, delay, region).await?;

        if region_has_grayscale(lsb, msb, self.config.width as usize / 8, region) {
            self.stream_plane_window(bus, Command::OldPlane, lsb, region, false)
                .await?;
            self.stream_plane_window(bus, Command::NewPlane, msb, region, false)
                .await?;

            self.activate_uc8179_grayscale(bus, delay, Some(region))
                .await?;

            // Restore only `region` in both planes; pixels outside `region` still
            // hold their clean B/W base from the preceding frame.
            self.stream_and_plane_window(bus, Command::OldPlane, lsb, msb, region)
                .await?;
            self.stream_and_plane_window(bus, Command::NewPlane, lsb, msb, region)
                .await?;
        } else {
            // NewPlane already contains the updated B/W base for `region`; sync OldPlane.
            self.stream_and_plane_window(bus, Command::OldPlane, lsb, msb, region)
                .await?;
        }

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
    ) -> Result<Region, PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.validate_frame(lsb)?;
        self.validate_frame(msb)?;

        if !self.is_uc8179() {
            self.display_grayscale(bus, delay, lsb, msb, turn_off)
                .await?;
            return Ok(Region::new(
                0,
                0,
                self.config.width,
                self.config.visible_height,
            ));
        }

        let region = self.normalize_region(region)?;

        self.finish_pending_power_off(bus, delay).await?;
        self.finish_pending_power_on(bus, delay).await?;

        if self.need_full_clear || !self.old_plane_valid || !self.grayscale_on_panel {
            return self
                .display_grayscale_window(bus, delay, lsb, msb, region, RefreshMode::Fast, turn_off)
                .await;
        }

        if region_has_grayscale(lsb, msb, self.config.width as usize / 8, region) {
            return self
                .display_grayscale_window(bus, delay, lsb, msb, region, RefreshMode::Fast, turn_off)
                .await;
        }

        self.stream_and_plane_window(bus, Command::NewPlane, lsb, msb, region)
            .await?;

        self.run_grayscale_precondition(bus, delay, region).await?;

        self.stream_and_plane_window(bus, Command::OldPlane, lsb, msb, region)
            .await?;

        self.old_plane_valid = true;
        self.need_full_clear = false;
        self.grayscale_on_panel = true;

        if turn_off {
            self.power_off(bus, delay).await?;
        }

        Ok(region)
    }

    pub async fn begin_power_off<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        if !self.is_uc8179() {
            return Ok(());
        }

        self.power_off(bus, delay).await
    }

    pub async fn prepare_power_on<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        if !self.is_uc8179() {
            return Ok(());
        }

        debug_assert!(!(self.power_off_pending && self.power_on_pending));

        if self.screen_on || self.power_on_pending {
            return Ok(());
        }

        if self.power_off_pending {
            delay.delay_ms(BUSY_ASSERT_SETTLE_MS).await;

            if bus.is_busy(BUSY_POLARITY).map_err(PanelError::Bus)? {
                return Ok(());
            }

            self.power_off_pending = false;
        }

        self.command(bus, Command::PowerOn).await?;
        self.power_on_pending = true;

        Ok(())
    }

    pub async fn deep_sleep<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.finish_pending_power_off(bus, delay).await?;
        self.finish_pending_power_on(bus, delay).await?;

        if self.screen_on {
            self.power_off(bus, delay).await?;
            self.finish_pending_power_off(bus, delay).await?;
        }

        self.command_data(bus, Command::DeepSleep, &[0xa5]).await?;

        self.need_full_clear = true;
        self.old_plane_valid = false;
        self.grayscale_on_panel = false;

        Ok(())
    }

    async fn configure_refresh<B>(
        &self,
        bus: &mut B,
        fast: bool,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
    {
        self.write_active_vcom(bus, fast).await?;

        self.command_data(bus, Command::CascadeControl, &[self.config.cascade_control])
            .await?;

        let temperature = if fast {
            self.config.temperature.fast
        } else {
            self.config.temperature.full
        };
        self.command_data(bus, Command::Temperature, &[temperature])
            .await?;

        if self.is_uc8179() {
            // UC8179 selects OTP waveforms before PON.
            self.command_data(bus, Command::PanelSetting, &self.config.panel_settings.otp_lut)
                .await?;
        }

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

    async fn write_active_vcom<B>(
        &self,
        bus: &mut B,
        fast: bool,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
    {
        match self.config.vcom_data_interval {
            VcomDataInterval::ActiveIdle { active, .. } => {
                self.command_data(bus, Command::VcomDataInterval, &active)
                    .await
            }
            VcomDataInterval::FullFast { full, fast: fast_v } => {
                let cdi = if fast { fast_v } else { full };
                self.command_data(bus, Command::VcomDataInterval, &[cdi])
                    .await
            }
        }
    }

    async fn write_idle_vcom<B>(&self, bus: &mut B) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
    {
        match self.config.vcom_data_interval {
            VcomDataInterval::ActiveIdle { idle, .. } => {
                self.command_data(bus, Command::VcomDataInterval, &idle)
                    .await
            }
            VcomDataInterval::FullFast { .. } => Ok(()),
        }
    }

    async fn write_resolution<B>(&self, bus: &mut B) -> Result<(), PanelError<B::Error>>
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
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
    {
        let row_bytes = self.config.width as usize / 8;

        self.command(bus, command).await?;
        let mut stream = DataStream::open(bus).await.map_err(PanelError::Bus)?;

        for _ in 0..self.config.gate_offset {
            Self::stream_white_row(&mut stream, row_bytes).await?;
        }

        let visible = self.config.visible_height as usize;
        for step in 0..visible {
            let row = match self.config.row_order {
                RowOrder::Forward => step,
                RowOrder::Reversed => visible - 1 - step,
            };
            let start = row * row_bytes;
            let source = &frame[start..start + row_bytes];

            if invert {
                Self::stream_inverted(&mut stream, source).await?;
            } else {
                stream.write(source).await.map_err(PanelError::Bus)?;
            }
        }

        let visible_end = self.config.gate_offset + self.config.visible_height;
        for _ in visible_end..self.config.addressed_height {
            Self::stream_white_row(&mut stream, row_bytes).await?;
        }

        stream.finish().map_err(PanelError::Bus)
    }

    async fn fill_white_plane<B>(
        &self,
        bus: &mut B,
        command: Command,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
    {
        let row_bytes = self.config.width as usize / 8;

        self.command(bus, command).await?;
        let mut stream = DataStream::open(bus).await.map_err(PanelError::Bus)?;

        for _ in 0..self.config.addressed_height {
            Self::stream_white_row(&mut stream, row_bytes).await?;
        }

        stream.finish().map_err(PanelError::Bus)
    }

    async fn stream_inverted<B>(
        stream: &mut DataStream<'_, B>,
        source: &[u8],
    ) -> Result<(), PanelError<B::Error>>
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

            stream
                .write(&buffer[..len])
                .await
                .map_err(PanelError::Bus)?;
            offset += len;
        }

        Ok(())
    }

    async fn stream_white_row<B>(
        stream: &mut DataStream<'_, B>,
        row_bytes: usize,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
    {
        const WHITE: [u8; STREAM_BUFFER_LEN] = [0xff; STREAM_BUFFER_LEN];

        let mut remaining = row_bytes;
        while remaining != 0 {
            let len = remaining.min(WHITE.len());
            stream.write(&WHITE[..len]).await.map_err(PanelError::Bus)?;
            remaining -= len;
        }

        Ok(())
    }

    async fn power_on_if_needed<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        if self.screen_on {
            return Ok(());
        }

        self.command(bus, Command::PowerOn).await?;
        self.wait_ready(bus, DisplayPhase::PowerOn, delay).await?;
        self.screen_on = true;

        Ok(())
    }

    async fn power_off<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.finish_pending_power_on(bus, delay).await?;

        if !self.screen_on {
            return Ok(());
        }

        self.command(bus, Command::PowerOff).await?;
        self.screen_on = false;

        if self.is_uc8179() {
            // PowerOff continues autonomously in UC8179; resolve before the next
            // controller operation instead of stalling here.
            self.power_off_pending = true;
        } else {
            self.wait_ready(bus, DisplayPhase::PowerOff, delay).await?;
        }

        Ok(())
    }

    async fn finish_pending_power_off<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        if !self.power_off_pending {
            return Ok(());
        }

        self.wait_ready(bus, DisplayPhase::PowerOff, delay).await?;
        self.power_off_pending = false;

        Ok(())
    }

    async fn finish_pending_power_on<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        if !self.power_on_pending {
            return Ok(());
        }

        self.wait_ready(bus, DisplayPhase::PowerOn, delay).await?;
        self.power_on_pending = false;
        self.screen_on = true;

        Ok(())
    }

    async fn wait_ready<B, D>(
        &self,
        bus: &mut B,
        phase: DisplayPhase,
        delay: &mut D,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        delay.delay_ms(BUSY_ASSERT_SETTLE_MS).await;
        bus.wait_busy_for(BUSY_POLARITY, phase, delay)
            .await
            .map_err(PanelError::Bus)
    }

    async fn command<B>(&self, bus: &mut B, command: Command) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
    {
        bus.command(command.byte()).await.map_err(PanelError::Bus)
    }

    async fn command_data<B>(
        &self,
        bus: &mut B,
        command: Command,
        data: &[u8],
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
    {
        self.command(bus, command).await?;
        bus.data(data).await.map_err(PanelError::Bus)
    }

    fn validate_geometry<E>(&self) -> Result<(), PanelError<E>> {
        let visible_end = self.config.gate_offset as u32 + self.config.visible_height as u32;

        if self.config.width == 0
            || !self.config.width.is_multiple_of(8)
            || self.config.visible_height == 0
            || self.config.addressed_height == 0
            || visible_end > self.config.addressed_height as u32
        {
            return Err(PanelError::InvalidGeometry);
        }

        Ok(())
    }

    fn validate_frame<E>(&self, frame: &[u8]) -> Result<(), PanelError<E>> {
        let expected = self.config.buffer_len();
        if frame.len() != expected {
            return Err(PanelError::InvalidFrameLength {
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
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.stream_and_plane(bus, Command::NewPlane, lsb, msb)
            .await?;
        self.fill_white_plane(bus, Command::OldPlane).await?;

        self.configure_refresh(bus, false).await?;
        self.power_on_if_needed(bus, delay).await?;

        if !self.is_uc8179() {
            self.command_data(bus, Command::PanelSetting, &self.config.panel_settings.otp_lut)
                .await?;
        }

        self.command(bus, Command::DisplayRefresh).await?;
        self.wait_ready(bus, DisplayPhase::GrayscaleBaseRefresh, delay)
            .await?;

        self.write_idle_vcom(bus).await
    }

    async fn stream_and_plane<B>(
        &self,
        bus: &mut B,
        command: Command,
        lhs: &[u8],
        rhs: &[u8],
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
    {
        let row_bytes = self.config.width as usize / 8;

        self.command(bus, command).await?;
        let mut stream = DataStream::open(bus).await.map_err(PanelError::Bus)?;

        for _ in 0..self.config.gate_offset {
            Self::stream_white_row(&mut stream, row_bytes).await?;
        }

        let mut buffer = [0u8; STREAM_BUFFER_LEN];
        let visible = self.config.visible_height as usize;

        for step in 0..visible {
            let row = match self.config.row_order {
                RowOrder::Forward => step,
                RowOrder::Reversed => visible - 1 - step,
            };
            let row_start = row * row_bytes;
            let mut offset = 0;

            while offset < row_bytes {
                let len = (row_bytes - offset).min(buffer.len());
                for (index, byte) in buffer[..len].iter_mut().enumerate() {
                    let source_index = row_start + offset + index;
                    *byte = grayscale_base_byte(lhs[source_index], rhs[source_index]);
                }

                stream
                    .write(&buffer[..len])
                    .await
                    .map_err(PanelError::Bus)?;
                offset += len;
            }
        }

        let visible_end = self.config.gate_offset + self.config.visible_height;
        for _ in visible_end..self.config.addressed_height {
            Self::stream_white_row(&mut stream, row_bytes).await?;
        }

        stream.finish().map_err(PanelError::Bus)
    }

    async fn stream_plane_window<B>(
        &self,
        bus: &mut B,
        command: Command,
        frame: &[u8],
        region: Region,
        invert: bool,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
    {
        let region = self.normalize_region(region)?;
        let stride = self.config.width as usize / 8;
        let first_byte = region.x() as usize / 8;
        let row_bytes = region.width() as usize / 8;

        debug_assert!(region.x().is_multiple_of(8));
        debug_assert!(region.width().is_multiple_of(8));
        debug_assert!(row_bytes > 0);

        self.enter_partial_window(bus, region).await?;

        let write_result = async {
            self.command(bus, command).await?;
            let mut stream = DataStream::open(bus).await.map_err(PanelError::Bus)?;

            let first_row = region.y() as usize;
            let height = region.height() as usize;

            for step in 0..height {
                let row = match self.config.row_order {
                    RowOrder::Forward => first_row + step,
                    RowOrder::Reversed => first_row + height - 1 - step,
                };
                let row_start = row * stride + first_byte;
                let source = &frame[row_start..row_start + row_bytes];

                if invert {
                    Self::stream_inverted(&mut stream, source).await?;
                } else {
                    stream.write(source).await.map_err(PanelError::Bus)?;
                }
            }

            stream.finish().map_err(PanelError::Bus)
        }
        .await;

        let partial_out_result = self.command(bus, Command::PartialOut).await;

        match (write_result, partial_out_result) {
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
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
    {
        let region = self.normalize_region(region)?;
        let stride = self.config.width as usize / 8;
        let first_byte = region.x() as usize / 8;
        let row_bytes = region.width() as usize / 8;

        debug_assert!(region.x().is_multiple_of(8));
        debug_assert!(region.width().is_multiple_of(8));
        debug_assert!(row_bytes > 0);

        self.enter_partial_window(bus, region).await?;

        let write_result = async {
            self.command(bus, command).await?;
            let mut stream = DataStream::open(bus).await.map_err(PanelError::Bus)?;

            let mut buffer = [0u8; STREAM_BUFFER_LEN];
            let first_row = region.y() as usize;
            let height = region.height() as usize;

            for step in 0..height {
                let row = match self.config.row_order {
                    RowOrder::Forward => first_row + step,
                    RowOrder::Reversed => first_row + height - 1 - step,
                };
                let row_start = row * stride + first_byte;
                let mut offset = 0;

                while offset < row_bytes {
                    let len = (row_bytes - offset).min(buffer.len());
                    for (index, byte) in buffer[..len].iter_mut().enumerate() {
                        let source_index = row_start + offset + index;
                        *byte = grayscale_base_byte(lhs[source_index], rhs[source_index]);
                    }

                    stream
                        .write(&buffer[..len])
                        .await
                        .map_err(PanelError::Bus)?;
                    offset += len;
                }
            }

            stream.finish().map_err(PanelError::Bus)
        }
        .await;

        let partial_out_result = self.command(bus, Command::PartialOut).await;

        match (write_result, partial_out_result) {
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) => Err(error),
            (Ok(()), Ok(())) => Ok(()),
        }
    }

    async fn load_uc8179_grayscale_luts<B>(&self, bus: &mut B) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
    {
        for (register, lut) in UC8179_GRAY_LUT_REGISTERS
            .iter()
            .zip(UC8179_GRAY_LUTS.iter())
        {
            self.command_data(bus, *register, lut).await?;
        }

        Ok(())
    }

    async fn activate_uc8179_grayscale<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        region: Option<Region>,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        if let Some(region) = region {
            self.enter_partial_window(bus, region).await?;
        }

        self.command_data(
            bus,
            Command::PanelSetting,
            &self.config.panel_settings.external_lut,
        )
        .await?;

        self.load_uc8179_grayscale_luts(bus).await?;
        self.write_active_vcom(bus, false).await?;
        self.power_on_if_needed(bus, delay).await?;

        self.command(bus, Command::DisplayRefresh).await?;
        self.wait_ready(bus, DisplayPhase::GrayscaleActivate, delay)
            .await?;

        if region.is_some() {
            self.command(bus, Command::PartialOut).await?;
        }

        self.write_idle_vcom(bus).await
    }

    async fn run_grayscale_precondition<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        region: Region,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        self.enter_partial_window(bus, region).await?;

        self.command_data(
            bus,
            Command::PanelSetting,
            &self.config.panel_settings.external_lut,
        )
        .await?;

        self.command_data(
            bus,
            Command::PowerOffSequence,
            &[self.config.power_off_sequence],
        )
        .await?;

        self.command_data(bus, Command::GateScan, &[self.config.gate_scan])
            .await?;

        self.write_active_vcom(bus, true).await?;

        self.command_data(bus, Command::CascadeControl, &[self.config.cascade_control])
            .await?;

        self.command_data(bus, Command::Temperature, &[self.config.temperature.fast])
            .await?;

        for (register, lut) in UC8179_GRAY_LUT_REGISTERS
            .iter()
            .zip(UC8179_GRAY_PRE_BW_MID.iter())
        {
            self.command_data(bus, *register, lut).await?;
        }

        self.power_on_if_needed(bus, delay).await?;

        self.command(bus, Command::DisplayRefresh).await?;
        self.wait_ready(bus, DisplayPhase::GrayscalePrecondition, delay)
            .await?;

        self.command(bus, Command::PartialOut).await?;
        self.write_idle_vcom(bus).await
    }

    async fn restore_grayscale_base<B>(
        &self,
        bus: &mut B,
        lsb: &[u8],
        msb: &[u8],
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
    {
        self.stream_and_plane(bus, Command::OldPlane, lsb, msb)
            .await?;
        self.stream_and_plane(bus, Command::NewPlane, lsb, msb)
            .await
    }

    fn normalize_region<E>(&self, region: Region) -> Result<Region, PanelError<E>> {
        if region.is_empty() {
            return Err(PanelError::EmptyRegion);
        }

        let right = region.x() as u32 + region.width() as u32;
        let bottom = region.y() as u32 + region.height() as u32;
        if right > self.config.width as u32 || bottom > self.config.visible_height as u32 {
            return Err(PanelError::RegionOutOfBounds { region });
        }

        let x = region.x() & !7;
        let right = ((right + 7) & !7).min(self.config.width as u32) as u16;

        Ok(Region::new(x, region.y(), right - x, region.height()))
    }

    fn partial_window_data(&self, region: Region) -> [u8; 9] {
        let x_start = region.x();
        let x_end = region.x() + region.width() - 1;

        let y_start = match self.config.row_order {
            RowOrder::Forward => self.config.gate_offset + region.y(),
            RowOrder::Reversed => {
                self.config.gate_offset + self.config.visible_height - region.y() - region.height()
            }
        };
        let y_end = y_start + region.height() - 1;

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
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
    {
        self.command(bus, Command::PartialIn).await?;

        let window = self.partial_window_data(region);
        self.command_data(bus, Command::PartialWindow, &window)
            .await
    }
}

impl EpdPanel for Uc8xxx {
    fn capabilities(&self) -> PanelCapabilities {
        match self.config.variant {
            Variant::Uc8179 => PanelCapabilities::new(
                BinaryUpdateMode::FullPlane,
                BinaryOverGrayMode::PreconditionedWindow,
                GrayscaleUpdateMode::Window,
            ),
            Variant::Uc8279X4 => PanelCapabilities::new(
                BinaryUpdateMode::FullPlane,
                BinaryOverGrayMode::Unsupported,
                GrayscaleUpdateMode::FullPlane,
            ),
        }
    }

    fn supports_idle_power_hold(&self) -> bool {
        self.is_uc8179()
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
        Uc8xxx::initialize(self, bus, delay).await
    }

    async fn prepare_power_on<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        Uc8xxx::prepare_power_on(self, bus, delay).await
    }

    async fn begin_idle_power_off<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        Uc8xxx::begin_power_off(self, bus, delay).await
    }

    async fn display_binary<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        frame: &[u8],
        _damage: Option<Region>,
        mode: RefreshMode,
        turn_off: bool,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        let turn_off = if self.is_uc8179() { turn_off } else { true };
        self.display(bus, delay, frame, mode, turn_off).await
    }

    async fn display_grayscale<B, D>(
        &mut self,
        bus: &mut B,
        delay: &mut D,
        lsb: &[u8],
        msb: &[u8],
        damage: Region,
        mode: RefreshMode,
        turn_off: bool,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        let turn_off = if self.is_uc8179() { turn_off } else { true };
        self.display_grayscale_window(bus, delay, lsb, msb, damage, mode, turn_off)
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
        turn_off: bool,
    ) -> Result<(), PanelError<B::Error>>
    where
        B: EpdInterface,
        D: DelayNs,
    {
        let turn_off = if self.is_uc8179() { turn_off } else { true };
        self.display_binary_window_preserving_grayscale(bus, delay, lsb, msb, damage, turn_off)
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
        Uc8xxx::deep_sleep(self, bus, delay).await
    }
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
    use std::vec::Vec;

    use embedded_hal_async::delay::DelayNs;
    use epd_bus::{BusyPolarity, DisplayPhase, EpdInterface, PanelError, RefreshMode, Region};

    use super::{
        Config, PanelSettings, RefreshTemperature, RowOrder, UC8179_X4_PRO_800X480,
        UC8279_X4_PRO_800X480, Uc8xxx, Variant, VcomDataInterval,
    };

    #[derive(Debug, PartialEq, Eq)]
    enum Op {
        Reset,
        Command(u8),
        Data(Vec<u8>),
        Stream(Vec<u8>),
        WaitBusy(BusyPolarity),
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
            polarity: BusyPolarity,
            _delay: &mut D,
        ) -> Result<(), Self::Error>
        where
            D: DelayNs,
        {
            self.ops.push(Op::WaitBusy(polarity));
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

    const fn region_buffer_len(region: Region) -> usize {
        region.width() as usize * region.height() as usize / 8
    }

    #[test]
    fn partial_region_is_byte_aligned() {
        let panel = Uc8xxx::new(UC8179_X4_PRO_800X480);

        let region = panel
            .normalize_region::<()>(Region::new(13, 10, 11, 20))
            .unwrap();

        assert_eq!(region, Region::new(8, 10, 16, 20));
        assert_eq!(region_buffer_len(region), 40);
    }

    #[test]
    fn partial_region_rejects_empty_and_out_of_bounds() {
        let panel = Uc8xxx::new(UC8179_X4_PRO_800X480);

        assert!(matches!(
            panel.normalize_region::<()>(Region::new(10, 10, 0, 20)),
            Err(PanelError::EmptyRegion)
        ));

        let oob = Region::new(760, 100, 80, 40);
        assert!(matches!(
            panel.normalize_region::<()>(oob),
            Err(PanelError::RegionOutOfBounds { region }) if region == oob
        ));
    }

    #[test]
    fn partial_window_maps_uc8179_reversed_and_uc8279_offset() {
        let uc8179 = Uc8xxx::new(UC8179_X4_PRO_800X480);
        let region = uc8179
            .normalize_region::<()>(Region::new(13, 10, 11, 20))
            .unwrap();
        assert_eq!(
            uc8179.partial_window_data(region),
            [0x00, 0x08, 0x00, 0x17, 0x01, 0xc2, 0x01, 0xd5, 0x01],
        );
        assert_eq!(
            uc8179.partial_window_data(Region::new(0, 0, 800, 480)),
            [0x00, 0x00, 0x03, 0x1f, 0x00, 0x00, 0x01, 0xdf, 0x01],
        );

        let uc8279 = Uc8xxx::new(UC8279_X4_PRO_800X480);
        assert_eq!(
            uc8279.partial_window_data(Region::new(0, 0, 800, 480)),
            [0x00, 0x00, 0x03, 0x1f, 0x00, 0x78, 0x02, 0x57, 0x01],
        );
    }

    #[test]
    fn uc8179_init_writes_booster_and_power_save() {
        let mut panel = Uc8xxx::new(UC8179_X4_PRO_800X480);
        let mut bus = RecordingBus::new();
        let mut delay = NoopDelay;

        block_on(panel.initialize(&mut bus, &mut delay)).unwrap();

        assert!(bus.ops.contains(&Op::Command(0x06))); // BoosterSoftStart
        assert!(bus.ops.contains(&Op::Command(0xe3))); // PowerSave
        assert!(!bus.ops.contains(&Op::Command(0x30))); // No PLL
    }

    #[test]
    fn uc8279_init_writes_pll_and_streams_gate_offset_padding() {
        let tiny = Config::new(
            Variant::Uc8279X4,
            16,
            2,
            4,
            1,
            RowOrder::Forward,
            PanelSettings::new([0x37, 0x4d], [0x17, 0x4d]),
            0x20,
            None,
            Some(0x0e),
            0x02,
            0x02,
            RefreshTemperature::new(0x1e, 0x5a),
            VcomDataInterval::FullFast {
                full: 0x97,
                fast: 0xd7,
            },
            None,
        );

        let mut panel = Uc8xxx::new(tiny);
        let mut bus = RecordingBus::new();
        let mut delay = NoopDelay;

        block_on(panel.initialize(&mut bus, &mut delay)).unwrap();
        assert!(bus.ops.contains(&Op::Command(0x30))); // PLL
        assert!(!bus.ops.contains(&Op::Command(0x06))); // No BoosterSoftStart

        bus.ops.clear();
        let frame = [0x11, 0x22, 0x33, 0x44];
        block_on(panel.display(&mut bus, &mut delay, &frame, RefreshMode::Full, true)).unwrap();

        // First stream is NewPlane (0x13): 1 leading white row, 2 forward visible rows, 1 trailing white row.
        let first_stream = bus
            .ops
            .iter()
            .find_map(|op| match op {
                Op::Stream(bytes) => Some(bytes.as_slice()),
                _ => None,
            })
            .expect("expected streamed NewPlane");

        assert_eq!(
            first_stream,
            &[0xff, 0xff, 0x11, 0x22, 0x33, 0x44, 0xff, 0xff]
        );
        // UC8279-X4 must synchronously power off (not leave power_off_pending).
        assert!(!panel.power_off_pending());
        // PSR (0x00) with OTP LUT must appear after PowerOn (0x04) and before DisplayRefresh (0x12).
        let pon = bus.ops.iter().position(|op| *op == Op::Command(0x04)).unwrap();
        let drf = bus.ops.iter().position(|op| *op == Op::Command(0x12)).unwrap();
        assert!(bus.ops[pon..drf].contains(&Op::Command(0x00)));
    }

    #[test]
    fn uc8179_grayscale_window_streams_only_damaged_region() {
        let tiny = Config::new(
            Variant::Uc8179,
            32,
            8,
            10,
            0,
            RowOrder::Reversed,
            PanelSettings::new([0x3f, 0x0a], [0x1f, 0x0a]),
            0x20,
            Some([0x25, 0x25, 0x3c, 0x25]),
            None,
            0x02,
            0x02,
            RefreshTemperature::new(0x1e, 0x5a),
            VcomDataInterval::ActiveIdle {
                active: [0x29, 0x07],
                idle: [0xa9, 0x07],
            },
            Some(0x22),
        );

        let mut panel = Uc8xxx::new(tiny);
        let mut bus = RecordingBus::new();
        let mut delay = NoopDelay;

        let mut lsb = std::vec![0xff; tiny.buffer_len()];
        let msb = std::vec![0xff; tiny.buffer_len()];
        lsb[0] = 0x00; // establish grayscale on panel

        block_on(panel.initialize(&mut bus, &mut delay)).unwrap();
        block_on(panel.display_grayscale(&mut bus, &mut delay, &lsb, &msb, false)).unwrap();

        let region = Region::new(8, 2, 16, 4); // 2 bytes × 4 rows = 8 bytes (vs 40 bytes full plane)

        // Window WITH grayscale inside: 5 windowed streams of 8 bytes each.
        lsb[2 * 4 + 1] = 0x00;
        bus.ops.clear();
        block_on(panel.display_grayscale_window(
            &mut bus,
            &mut delay,
            &lsb,
            &msb,
            region,
            RefreshMode::Fast,
            false,
        ))
        .unwrap();

        let stream_lens: Vec<usize> = bus
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Stream(bytes) => Some(bytes.len()),
                _ => None,
            })
            .collect();
        assert_eq!(stream_lens, std::vec![8, 8, 8, 8, 8]);

        // Window WITHOUT grayscale inside: only 2 windowed streams (NewPlane + OldPlane).
        lsb[2 * 4 + 1] = 0xff;
        bus.ops.clear();
        block_on(panel.display_grayscale_window(
            &mut bus,
            &mut delay,
            &lsb,
            &msb,
            region,
            RefreshMode::Fast,
            false,
        ))
        .unwrap();

        let stream_lens: Vec<usize> = bus
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Stream(bytes) => Some(bytes.len()),
                _ => None,
            })
            .collect();
        assert_eq!(stream_lens, std::vec![8, 8]);
    }

    #[test]
    fn reports_explicit_display_phases_for_uc8179_and_uc8279() {
        let mut uc8179 = Uc8xxx::new(UC8179_X4_PRO_800X480);
        let mut bus = RecordingBus::new();
        let mut delay = NoopDelay;
        let mut lsb = std::vec![0xff; UC8179_X4_PRO_800X480.buffer_len()];
        let msb = std::vec![0xff; UC8179_X4_PRO_800X480.buffer_len()];
        lsb[0] = 0x00;

        block_on(uc8179.initialize(&mut bus, &mut delay)).unwrap();
        bus.phases.clear();

        block_on(uc8179.display(&mut bus, &mut delay, &msb, RefreshMode::Full, false)).unwrap();
        block_on(uc8179.display(&mut bus, &mut delay, &msb, RefreshMode::Fast, true)).unwrap();
        // Deferred PowerOff finishes at the start of the next operation.
        block_on(uc8179.display_grayscale(&mut bus, &mut delay, &lsb, &msb, false)).unwrap();
        block_on(uc8179.display_grayscale_window(
            &mut bus,
            &mut delay,
            &lsb,
            &msb,
            Region::new(0, 0, 16, 4),
            RefreshMode::Fast,
            false,
        ))
        .unwrap();

        assert_eq!(
            bus.phases,
            std::vec![
                DisplayPhase::PowerOn,
                DisplayPhase::BinaryFullRefresh,
                DisplayPhase::BinaryFastRefresh,
                DisplayPhase::PowerOff,
                DisplayPhase::PowerOn,
                DisplayPhase::GrayscaleBaseRefresh,
                DisplayPhase::GrayscaleActivate,
                DisplayPhase::GrayscalePrecondition,
                DisplayPhase::GrayscaleActivate,
            ]
        );

        let mut uc8279 = Uc8xxx::new(UC8279_X4_PRO_800X480);
        let mut bus = RecordingBus::new();
        block_on(uc8279.initialize(&mut bus, &mut delay)).unwrap();
        bus.phases.clear();

        block_on(uc8279.display_grayscale(&mut bus, &mut delay, &lsb, &msb, true)).unwrap();
        assert_eq!(
            bus.phases,
            std::vec![
                DisplayPhase::PowerOn,
                DisplayPhase::GrayscaleBaseRefresh,
                DisplayPhase::GrayscaleRefresh,
                DisplayPhase::PowerOff,
            ]
        );
    }
}
