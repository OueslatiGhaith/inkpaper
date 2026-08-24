#![no_std]

use embedded_hal_async::i2c::I2c;

pub const ADDRESS: u8 = 0x51;

const DATETIME_REGISTER: u8 = 0x02;

const SECOND_VOLTAGE_LOW: u8 = 0x80;

const SECOND_MASK: u8 = 0x7f;
const MINUTE_MASK: u8 = 0x7f;
const HOUR_MASK: u8 = 0x3f;
const DAY_MASK: u8 = 0x3f;
const WEEKDAY_MASK: u8 = 0x07;
const MONTH_MASK: u8 = 0x1f;
const CENTURY_MASK: u8 = 0x80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    year: u16,
    month: u8,
    day: u8,
    weekday: u8,
    hour: u8,
    minute: u8,
    second: u8,
}

impl DateTime {
    pub const fn new(
        year: u16,
        month: u8,
        day: u8,
        weekday: u8,
        hour: u8,
        minute: u8,
        second: u8,
    ) -> Result<Self, DateTimeError> {
        if year < 2000 || year > 2099 {
            return Err(DateTimeError::Year { value: year });
        }
        if month < 1 || month > 12 {
            return Err(DateTimeError::Month { value: month });
        }
        if day < 1 || day > 31 {
            return Err(DateTimeError::Day { value: day });
        }
        if weekday > 6 {
            return Err(DateTimeError::Weekday { value: weekday });
        }
        if hour > 23 {
            return Err(DateTimeError::Hour { value: hour });
        }
        if minute > 59 {
            return Err(DateTimeError::Minute { value: minute });
        }
        if second > 59 {
            return Err(DateTimeError::Second { value: second });
        }

        Ok(Self {
            year,
            month,
            day,
            weekday,
            hour,
            minute,
            second,
        })
    }

    pub const fn year(self) -> u16 {
        self.year
    }

    pub const fn month(self) -> u8 {
        self.month
    }

    pub const fn day(self) -> u8 {
        self.day
    }

    pub const fn weekday(self) -> u8 {
        self.weekday
    }

    pub const fn hour(self) -> u8 {
        self.hour
    }

    pub const fn minute(self) -> u8 {
        self.minute
    }

    pub const fn second(self) -> u8 {
        self.second
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading {
    datetime: DateTime,
    voltage_low: bool,
    century: bool,
}

impl Reading {
    pub const fn datetime(self) -> DateTime {
        self.datetime
    }

    pub const fn voltage_low(self) -> bool {
        self.voltage_low
    }

    pub const fn century(self) -> bool {
        self.century
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateTimeError {
    Year { value: u16 },
    Month { value: u8 },
    Day { value: u8 },
    Weekday { value: u8 },
    Hour { value: u8 },
    Minute { value: u8 },
    Second { value: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    InvalidBcd { register: u8, value: u8 },
    InvalidDateTime(DateTimeError),
}

#[derive(Debug)]
pub enum Error<E> {
    I2c(E),
    Decode(DecodeError),
}

pub struct Bm8563<I2C> {
    i2c: I2C,
}

impl<I2C> Bm8563<I2C> {
    pub const fn new(i2c: I2C) -> Self {
        Self { i2c }
    }

    pub fn release(self) -> I2C {
        self.i2c
    }
}

impl<I2C> Bm8563<I2C>
where
    I2C: I2c,
{
    pub async fn read(&mut self) -> Result<Reading, Error<I2C::Error>> {
        let mut registers = [0u8; 7];

        self.i2c
            .write_read(ADDRESS, &[DATETIME_REGISTER], &mut registers)
            .await
            .map_err(Error::I2c)?;

        decode_reading(registers).map_err(Error::Decode)
    }

    pub async fn set_datetime(&mut self, datetime: DateTime) -> Result<(), Error<I2C::Error>> {
        let year = datetime
            .year()
            .checked_sub(2000)
            .expect("validated DateTime year must be >= 2000");

        let data = [
            DATETIME_REGISTER,
            // writing seconds with bit 7 clear also clears the  voltage-low indication.
            encode_bcd(datetime.second()),
            encode_bcd(datetime.minute()),
            encode_bcd(datetime.hour()),
            encode_bcd(datetime.day()),
            datetime.weekday(),
            // This driver intentionally supports 2000..2099. Therefore the century
            // bit remains clear.
            encode_bcd(datetime.month()),
            encode_bcd(year as u8),
        ];

        self.i2c.write(ADDRESS, &data).await.map_err(Error::I2c)
    }
}

fn decode_reading(registers: [u8; 7]) -> Result<Reading, DecodeError> {
    let voltage_low = registers[0] & SECOND_VOLTAGE_LOW != 0;

    let century = registers[5] & CENTURY_MASK != 0;
    let second = decode_bcd(DATETIME_REGISTER, registers[0] & SECOND_MASK)?;
    let minute = decode_bcd(DATETIME_REGISTER + 1, registers[1] & MINUTE_MASK)?;
    let hour = decode_bcd(DATETIME_REGISTER + 2, registers[2] & HOUR_MASK)?;
    let day = decode_bcd(DATETIME_REGISTER + 3, registers[3] & DAY_MASK)?;
    let weekday = registers[4] & WEEKDAY_MASK;
    let month = decode_bcd(DATETIME_REGISTER + 5, registers[5] & MONTH_MASK)?;
    let year = decode_bcd(DATETIME_REGISTER + 6, registers[6])?;

    // the PCF8563 century bit is only a single rollover marker and its
    // absolute interpretation is user-defined in newer datasheets, so
    // exposing it separately is safer than pretending it uniquely identifies
    // an absolute century.
    let datetime = DateTime::new(
        2000 + year as u16,
        month,
        day,
        weekday,
        hour,
        minute,
        second,
    )
    .map_err(DecodeError::InvalidDateTime)?;

    Ok(Reading {
        datetime,
        voltage_low,
        century,
    })
}

fn decode_bcd(register: u8, value: u8) -> Result<u8, DecodeError> {
    let units = value & 0x0f;
    let tens = value >> 4;
    if units > 9 || tens > 9 {
        return Err(DecodeError::InvalidBcd { register, value });
    }

    Ok(tens * 10 + units)
}

const fn encode_bcd(value: u8) -> u8 {
    (value / 10) << 4 | (value % 10)
}
