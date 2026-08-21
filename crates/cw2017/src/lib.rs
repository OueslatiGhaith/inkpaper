use embedded_hal_async::i2c::I2c;

use crate::{
    measurement::{StateOfCharge, Temperature, Version, Voltage},
    register::{Register, SocRegister, VcellRegister},
};

mod measurement;
mod register;

/// `CW2017` default 7-bit I2C address
pub const ADDRESS: u8 = 0x63;

/// errors returned by the `CW2017` driver
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error<E> {
    I2c(E),
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

    async fn read_u8(&mut self, register: Register) -> Result<u8, Error<I2C::Error>> {
        let mut value = [0];
        self.i2c
            .write_read(ADDRESS, &[register.address()], &mut value)
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
}
