#![no_std]

use embedded_hal_async::i2c::I2c;

pub const PRIMARY_ADDRESS: u8 = 0x5d;
pub const ALTERNATE_ADDRESS: u8 = 0x14;

pub const MAX_TOUCHES: usize = 5;

const PRODUCT_INFO_REGISTER: u16 = 0x8140;
const STATUS_REGISTER: u16 = 0x814e;
const POINTS_REGISTER: u16 = 0x8150;

const STATUS_READY: u8 = 0x80;
const STATUS_HOME_KEY: u8 = 0x10;
const STATUS_TOUCH_COUNT_MASK: u8 = 0x0f;

const PRODUCT_INFO_LEN: usize = 11;
const POINT_BYTES: usize = 8;
const POINT_BUFFER_LEN: usize = MAX_TOUCHES * POINT_BYTES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointLayout {
    /// X-lo, X-hi, Y-lo, Y-hi, ...
    ///
    /// this is the layout used by the XTEINK X4 Pro.
    CoordinatesAtByte0,
    /// track ID, X-lo, X-hi, Y-lo, Y-hi, ...
    ///
    /// this is the usual GT911 point layout.
    CoordinatesAtByte1,
}

impl PointLayout {
    const fn coordinate_offset(self) -> usize {
        match self {
            Self::CoordinatesAtByte0 => 0,
            Self::CoordinatesAtByte1 => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductInfo {
    product_id: [u8; 4],
    firmware_version: u16,
    x_resolution: u16,
    y_resolution: u16,
    vendor_id: u8,
}

impl ProductInfo {
    fn from_raw(raw: [u8; PRODUCT_INFO_LEN]) -> Self {
        Self {
            product_id: [raw[0], raw[1], raw[2], raw[3]],

            firmware_version: u16::from_le_bytes([raw[4], raw[5]]),

            x_resolution: u16::from_le_bytes([raw[6], raw[7]]),

            y_resolution: u16::from_le_bytes([raw[8], raw[9]]),

            vendor_id: raw[10],
        }
    }

    pub const fn product_id(self) -> [u8; 4] {
        self.product_id
    }

    pub const fn firmware_version(self) -> u16 {
        self.firmware_version
    }

    pub const fn x_resolution(self) -> u16 {
        self.x_resolution
    }

    pub const fn y_resolution(self) -> u16 {
        self.y_resolution
    }

    pub const fn vendor_id(self) -> u8 {
        self.vendor_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TouchPoint {
    x: u16,
    y: u16,
    size: u16,
}

impl TouchPoint {
    const EMPTY: Self = Self {
        x: 0,
        y: 0,
        size: 0,
    };

    pub const fn new(x: u16, y: u16, size: u16) -> Self {
        Self { x, y, size }
    }

    pub const fn x(self) -> u16 {
        self.x
    }

    pub const fn y(self) -> u16 {
        self.y
    }

    pub const fn size(self) -> u16 {
        self.size
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TouchFrame {
    points: [TouchPoint; MAX_TOUCHES],
    count: u8,
    home_key: bool,
}

impl TouchFrame {
    pub const fn count(self) -> usize {
        self.count as usize
    }

    pub const fn home_key(self) -> bool {
        self.home_key
    }

    pub fn points(&self) -> &[TouchPoint] {
        &self.points[..self.count as usize]
    }

    pub fn first_point(&self) -> Option<TouchPoint> {
        self.points().first().copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error<E> {
    I2c(E),
    TooManyTouches { count: u8 },
}

pub struct Gt911<I2C> {
    i2c: I2C,
    address: u8,
    point_layout: PointLayout,
}

impl<I2C> Gt911<I2C> {
    pub const fn new(i2c: I2C, address: u8, point_layout: PointLayout) -> Self {
        Self {
            i2c,
            address,
            point_layout,
        }
    }

    pub const fn address(&self) -> u8 {
        self.address
    }

    pub fn set_address(&mut self, address: u8) {
        self.address = address;
    }

    pub fn release(self) -> I2C {
        self.i2c
    }
}

impl<I2C> Gt911<I2C>
where
    I2C: I2c,
{
    pub async fn product_info(&mut self) -> Result<ProductInfo, Error<I2C::Error>> {
        let mut raw = [0u8; PRODUCT_INFO_LEN];
        self.read_register(PRODUCT_INFO_REGISTER, &mut raw).await?;

        Ok(ProductInfo::from_raw(raw))
    }

    pub async fn poll(&mut self) -> Result<Option<TouchFrame>, Error<I2C::Error>> {
        let mut status = [0u8; 1];
        self.read_register(STATUS_REGISTER, &mut status).await?;

        let status = status[0];
        if status & STATUS_READY == 0 {
            return Ok(None);
        }

        let count = status & STATUS_TOUCH_COUNT_MASK;
        let home_key = status & STATUS_HOME_KEY != 0;
        if count as usize > MAX_TOUCHES {
            self.clear_status().await?;

            return Err(Error::TooManyTouches { count });
        }

        let mut points = [TouchPoint::EMPTY; MAX_TOUCHES];

        if count != 0 {
            let mut raw = [0u8; POINT_BUFFER_LEN];
            let byte_count = count as usize * POINT_BYTES;

            self.read_register(POINTS_REGISTER, &mut raw[..byte_count])
                .await?;

            for (index, point) in points.iter_mut().enumerate() {
                let start = index * POINT_BYTES;
                let bytes: &[u8; POINT_BYTES] = raw[start..start + POINT_BYTES]
                    .try_into()
                    .expect("GT911 point chunks are always eight bytes");

                *point = parse_touch_point(self.point_layout, bytes);
            }
        }

        self.clear_status().await?;

        Ok(Some(TouchFrame {
            points,
            count,
            home_key,
        }))
    }

    async fn read_register(
        &mut self,
        register: u16,
        data: &mut [u8],
    ) -> Result<(), Error<I2C::Error>> {
        let register = register.to_be_bytes();

        self.i2c
            .write_read(self.address, &register, data)
            .await
            .map_err(Error::I2c)
    }

    async fn write_register_u8(
        &mut self,
        register: u16,
        value: u8,
    ) -> Result<(), Error<I2C::Error>> {
        let register = register.to_be_bytes();
        let data = [register[0], register[1], value];

        self.i2c
            .write(self.address, &data)
            .await
            .map_err(Error::I2c)
    }

    async fn clear_status(&mut self) -> Result<(), Error<I2C::Error>> {
        self.write_register_u8(STATUS_REGISTER, 0).await
    }
}

fn parse_touch_point(layout: PointLayout, bytes: &[u8; POINT_BYTES]) -> TouchPoint {
    let offset = layout.coordinate_offset();
    let x = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
    let y = u16::from_le_bytes([bytes[offset + 2], bytes[offset + 3]]);
    let size = u16::from_le_bytes([bytes[offset + 4], bytes[offset + 5]]);

    TouchPoint::new(x, y, size)
}
