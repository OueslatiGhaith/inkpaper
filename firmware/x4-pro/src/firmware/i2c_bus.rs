use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use esp_hal::{Async, i2c::master::I2c};
use static_cell::StaticCell;

pub type HardwareI2c = I2c<'static, Async>;
pub type SharedI2cBus = Mutex<CriticalSectionRawMutex, HardwareI2c>;
pub type SharedI2cDevice = I2cDevice<'static, CriticalSectionRawMutex, HardwareI2c>;

static I2C_BUS: StaticCell<SharedI2cBus> = StaticCell::new();

pub fn init(i2c: HardwareI2c) -> &'static SharedI2cBus {
    I2C_BUS.init(Mutex::new(i2c))
}

pub fn device(bus: &'static SharedI2cBus) -> SharedI2cDevice {
    I2cDevice::new(bus)
}
