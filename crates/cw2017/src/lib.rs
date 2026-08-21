#![no_std]

use embedded_hal_async::{delay::DelayNs, i2c::I2c};

use crate::{
    measurement::{StateOfCharge, Temperature, Version, Voltage},
    profile::{BatteryProfile, InitializationStats},
    register::{ConfigRegister, Register, SocAlertRegister, SocRegister, VcellRegister},
};

mod measurement;
mod profile;
mod register;

/// `CW2017` default 7-bit I2C address
pub const ADDRESS: u8 = 0x63;

const RESET_DELAY_MS: u32 = 20;
const PROFILE_UPDATE_DELAY_MS: u32 = 20;
const READY_POLL_DELAY_MS: u32 = 20;
const READY_POLL_ATTEMPTS: usize = 50;

/// errors returned by the `CW2017` driver
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error<E> {
    I2c(E),
    ProfileVerificationFailed,
    InitializationTimeout,
}

/// async driver for the `CellWize CW2017` fuel gauge.
pub struct Cw2017<I2C> {
    i2c: I2C,
}

impl<I2C> Cw2017<I2C> {
    pub const fn new(i2c: I2C) -> Self {
        Self { i2c }
    }

    pub fn release(self) -> I2C {
        self.i2c
    }
}

impl<I2C> Cw2017<I2C>
where
    I2C: I2c,
{
    /// read the `CW2017` silicon version
    pub async fn version(&mut self) -> Result<Version, Error<I2C::Error>> {
        let raw = self.read_u8(Register::Version).await?;
        Ok(Version::from_raw(raw))
    }

    /// read the battery terminal voltage
    pub async fn voltage(&mut self) -> Result<Voltage, Error<I2C::Error>> {
        let raw = self.read_u16(Register::Vcell).await?;
        let register = VcellRegister::from(raw);

        Ok(Voltage::from_register(register))
    }

    /// read battery state of charge
    pub async fn state_of_charge(&mut self) -> Result<StateOfCharge, Error<I2C::Error>> {
        let raw = self.read_u16(Register::StateOfCharge).await?;
        let register = SocRegister::from(raw);

        Ok(StateOfCharge::from_register(register))
    }

    pub async fn temperature(&mut self) -> Result<Temperature, Error<I2C::Error>> {
        let raw = self.read_u8(Register::Temperature).await?;
        Ok(Temperature::from_raw(raw))
    }

    pub async fn initialize<D>(
        &mut self,
        delay: &mut D,
        profile: &BatteryProfile,
    ) -> Result<InitializationStats, Error<I2C::Error>>
    where
        D: DelayNs,
    {
        let version = self.version().await?;
        if !version.is_running() {
            self.reset(delay).await?;
        }

        let alert = self.read_soc_alert().await?;
        if alert.profile_updated() && self.profile_matches(profile).await? {
            return Ok(InitializationStats::AlreadyLoaded);
        }

        self.write_profile(profile).await?;
        if !self.profile_matches(profile).await? {
            return Err(Error::ProfileVerificationFailed);
        }

        let alert = alert.with_profile_updated(true);
        self.write_soc_alert(alert).await?;
        delay.delay_ms(PROFILE_UPDATE_DELAY_MS).await;

        self.reset(delay).await?;

        for _ in 0..READY_POLL_ATTEMPTS {
            let charge = self.state_of_charge().await?;
            if charge.is_valid() {
                return Ok(InitializationStats::Updated);
            }

            delay.delay_ms(READY_POLL_DELAY_MS).await;
        }

        Err(Error::InitializationTimeout)
    }

    async fn reset<D>(&mut self, delay: &mut D) -> Result<(), Error<I2C::Error>>
    where
        D: DelayNs,
    {
        self.write_config(ConfigRegister::sleep_command()).await?;
        delay.delay_ms(RESET_DELAY_MS).await;

        self.write_config(ConfigRegister::reset_command()).await?;
        delay.delay_ms(RESET_DELAY_MS).await;

        self.write_config(ConfigRegister::normal_command()).await?;
        delay.delay_ms(RESET_DELAY_MS).await;

        Ok(())
    }

    async fn read_soc_alert(&mut self) -> Result<SocAlertRegister, Error<I2C::Error>> {
        let raw = self.read_u8(Register::SocAlert).await?;
        Ok(SocAlertRegister::from(raw))
    }

    async fn write_soc_alert(
        &mut self,
        register: SocAlertRegister,
    ) -> Result<(), Error<I2C::Error>> {
        let raw = register.into();
        self.write_u8(Register::SocAlert, raw).await
    }

    async fn write_config(&mut self, register: ConfigRegister) -> Result<(), Error<I2C::Error>> {
        let raw = register.into();
        self.write_u8(Register::Config, raw).await
    }

    async fn profile_matches(
        &mut self,
        profile: &BatteryProfile,
    ) -> Result<bool, Error<I2C::Error>> {
        for (index, expected) in profile.as_bytes().iter().copied().enumerate() {
            let address = Register::BatteryProfile.offset(index as u8);
            let actual = self.read_u8_at(address).await?;
            if actual != expected {
                return Ok(false);
            }
        }

        Ok(true)
    }

    async fn write_profile(&mut self, profile: &BatteryProfile) -> Result<(), Error<I2C::Error>> {
        for (index, value) in profile.as_bytes().iter().copied().enumerate() {
            let address = Register::BatteryProfile.offset(index as u8);
            self.write_u8_at(address, value).await?;
        }

        Ok(())
    }

    async fn read_u8(&mut self, register: Register) -> Result<u8, Error<I2C::Error>> {
        let mut value = [0];
        self.i2c
            .write_read(ADDRESS, &[register.address()], &mut value)
            .await
            .map_err(Error::I2c)?;

        Ok(value[0])
    }

    async fn read_u8_at(&mut self, address: u8) -> Result<u8, Error<I2C::Error>> {
        let mut value = [0];
        self.i2c
            .write_read(ADDRESS, &[address], &mut value)
            .await
            .map_err(Error::I2c)?;

        Ok(value[0])
    }

    async fn read_u16(&mut self, register: Register) -> Result<u16, Error<I2C::Error>> {
        let mut value = [0; 2];
        self.i2c
            .write_read(ADDRESS, &[register.address()], &mut value)
            .await
            .map_err(Error::I2c)?;

        Ok(u16::from_be_bytes(value))
    }

    async fn write_u8(&mut self, register: Register, value: u8) -> Result<(), Error<I2C::Error>> {
        self.write_u8_at(register.address(), value).await
    }

    async fn write_u8_at(&mut self, address: u8, value: u8) -> Result<(), Error<I2C::Error>> {
        self.i2c
            .write(ADDRESS, &[address, value])
            .await
            .map_err(Error::I2c)
    }
}
