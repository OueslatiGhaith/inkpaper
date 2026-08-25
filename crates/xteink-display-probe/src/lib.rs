#![no_std]

use embedded_hal_async::delay::DelayNs;

const COMMAND_VERSION: u8 = 0x70;
const COMMAND_FLAG: u8 = 0x71;
const COMMAND_READ_MTP: u8 = 0xa2;

const MTP_KEY: u8 = 0xa5;
const MTP_LEN: usize = 48;

const SHORT_RESET_MS: u32 = 1;
const CONFIRM_RESET_MS: u32 = 50;
const RESET_SETTLE_MS: u32 = 30;
const BETWEEN_PASSES_MS: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Controller {
    Ssd1677,
    Uc8179,
    Uc8279,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Verdict {
    /// neither probe pass found a UC81xx response.
    ///
    /// on X4/X4 Pro the conservative primary controller is SSD1677.
    PrimaryAssumed,
    /// two probe passes confirmed a UC81xx-family controller.
    Confirmed,
    /// the probe observed something inconsistent.
    ///
    /// callers should conservatively use the primary controller and avoid
    /// persisting the result.
    Inconclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Version {
    bytes: [u8; 5],
}

impl Version {
    pub const fn bytes(self) -> [u8; 5] {
        self.bytes
    }

    pub const fn chip_version(self) -> u8 {
        self.bytes[1]
    }

    pub const fn lut_version(self) -> u8 {
        self.bytes[2]
    }

    pub const fn lut_version_bytes(self) -> [u8; 3] {
        [self.bytes[2], self.bytes[3], self.bytes[4]]
    }

    fn is_uniform(self) -> bool {
        self.bytes[1..].iter().all(|byte| *byte == self.bytes[0])
    }

    fn is_all_ff(self) -> bool {
        self.bytes == [0xff; 5]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Flags(u8);

impl Flags {
    pub const fn raw(self) -> u8 {
        self.0
    }

    pub const fn busy_n(self) -> bool {
        self.0 & 0x01 != 0
    }

    fn is_driven_idle(self) -> bool {
        self.0 != 0x00 && self.0 != 0xff && self.busy_n()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ProbeDiagnostics {
    pub version: Version,
    pub flags: Flags,
    pub mtp: Option<[u8; MTP_LEN]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ProbeResult {
    pub controller: Controller,
    pub verdict: Verdict,
    pub diagnostics: ProbeDiagnostics,
}

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error<E> {
    Io(E),
}

/// GPIO operations required for the pre-SPI display-controller probe.
///
/// the display data line is bidirectional during the probe. The board adapter
/// therefore needs to be able to switch it between push-pull output and
/// pull-up input.
///
/// `clock_delay` should provide roughly a 1 µs blocking delay. Keeping this
/// tiny timing delay synchronous avoids yielding to an async executor for every
/// individual SPI edge. The longer reset/settle delays remain asynchronous.
pub trait ProbeIo {
    type Error;

    fn cs_high(&mut self) -> Result<(), Self::Error>;
    fn cs_low(&mut self) -> Result<(), Self::Error>;

    fn dc_high(&mut self) -> Result<(), Self::Error>;
    fn dc_low(&mut self) -> Result<(), Self::Error>;

    fn clock_high(&mut self) -> Result<(), Self::Error>;
    fn clock_low(&mut self) -> Result<(), Self::Error>;

    fn reset_high(&mut self) -> Result<(), Self::Error>;
    fn reset_low(&mut self) -> Result<(), Self::Error>;

    fn data_output(&mut self) -> Result<(), Self::Error>;
    fn data_input_pullup(&mut self) -> Result<(), Self::Error>;

    fn data_high(&mut self) -> Result<(), Self::Error>;
    fn data_low(&mut self) -> Result<(), Self::Error>;
    fn data_is_high(&mut self) -> Result<bool, Self::Error>;

    /// short, blocking bit-bang timing delay.
    fn clock_delay(&mut self);

    /// release display pins after probing so the real SPI peripheral can claim
    /// them afterwards.
    fn release(&mut self) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
struct ProbePass {
    version: Version,
    flags: Flags,
    matched: bool,
}

pub async fn detect_x4_controller<IO, D>(
    io: &mut IO,
    delay: &mut D,
) -> Result<ProbeResult, Error<IO::Error>>
where
    IO: ProbeIo,
    D: DelayNs,
{
    let result = detect_x4_controller_inner(io, delay).await;
    let release_result = io.release().map_err(Error::Io);

    match (result, release_result) {
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Ok(result), Ok(())) => Ok(result),
    }
}

async fn detect_x4_controller_inner<IO, D>(
    io: &mut IO,
    delay: &mut D,
) -> Result<ProbeResult, Error<IO::Error>>
where
    IO: ProbeIo,
    D: DelayNs,
{
    let first = run_probe_pass(io, delay, SHORT_RESET_MS).await?;
    delay.delay_ms(BETWEEN_PASSES_MS).await;

    let second_reset_ms = if first.matched {
        CONFIRM_RESET_MS
    } else {
        SHORT_RESET_MS
    };

    let second = run_probe_pass(io, delay, second_reset_ms).await?;
    let versions_agree = first.version == second.version;
    let mut confirmed = first.matched && second.matched && versions_agree;
    let flags_driven = first.flags.is_driven_idle();
    let mut mtp = None;
    if confirmed || flags_driven {
        let value = read_mtp(io)?;
        mtp = Some(value);
    }

    // FreeInk has also observed real UC81xx parts whose VER read is all 0xff.
    // a floating pulled-up bus also produces 0xff, so only accept that shape
    // when RMTP provides the real 0xa5 MTP key.
    if !confirmed
        && flags_driven
        && versions_agree
        && first.version.is_uniform()
        && first.version.is_all_ff()
        && mtp.as_ref().is_some_and(|mtp| mtp[0] == MTP_KEY)
    {
        confirmed = true;
    }

    let verdict = if confirmed {
        Verdict::Confirmed
    } else if !first.matched && !second.matched {
        Verdict::PrimaryAssumed
    } else {
        Verdict::Inconclusive
    };

    let controller = match verdict {
        Verdict::Confirmed => classify_ultrachip(first.version),
        Verdict::PrimaryAssumed | Verdict::Inconclusive => Controller::Ssd1677,
    };

    Ok(ProbeResult {
        controller,
        verdict,
        diagnostics: ProbeDiagnostics {
            version: first.version,
            flags: first.flags,
            mtp,
        },
    })
}

async fn run_probe_pass<IO, D>(
    io: &mut IO,
    delay: &mut D,
    reset_low_ms: u32,
) -> Result<ProbePass, Error<IO::Error>>
where
    IO: ProbeIo,
    D: DelayNs,
{
    prepare_probe_pins(io)?;

    io.reset_high().map_err(Error::Io)?;
    delay.delay_ms(2).await;

    io.reset_low().map_err(Error::Io)?;
    delay.delay_ms(reset_low_ms).await;

    io.reset_high().map_err(Error::Io)?;

    // we deliberately don't use BUSY here: knowing the controller family, and
    // therefore the BUSY convention, is precisely what this probe is trying to
    // establish.
    delay.delay_ms(RESET_SETTLE_MS).await;

    let flags = Flags(read_one(io, COMMAND_FLAG)?);

    let version = Version {
        bytes: read_five(io, COMMAND_VERSION)?,
    };

    let matched = matches_uc81xx(version, flags);

    Ok(ProbePass {
        version,
        flags,
        matched,
    })
}

fn prepare_probe_pins<IO>(io: &mut IO) -> Result<(), Error<IO::Error>>
where
    IO: ProbeIo,
{
    io.cs_high().map_err(Error::Io)?;
    io.clock_low().map_err(Error::Io)?;
    io.dc_low().map_err(Error::Io)?;
    io.data_output().map_err(Error::Io)?;

    Ok(())
}

fn matches_uc81xx(version: Version, flags: Flags) -> bool {
    flags.is_driven_idle() && !version.is_uniform()
}

fn classify_ultrachip(version: Version) -> Controller {
    match version.lut_version() {
        0x02 | 0x68 | 0x69 => Controller::Uc8279,
        // 0x01 is the known UC8179 value.
        //
        // FreeInk also defaults unrecognized but otherwise-valid UC81xx X4
        // signatures to UC8179, because every such X4 Pro unit observed so far
        // has used that variant.
        _ => Controller::Uc8179,
    }
}

fn read_one<IO>(io: &mut IO, command: u8) -> Result<u8, Error<IO::Error>>
where
    IO: ProbeIo,
{
    let mut bytes = [0; 1];
    command_read(io, command, &mut bytes)?;

    Ok(bytes[0])
}

fn read_five<IO>(io: &mut IO, command: u8) -> Result<[u8; 5], Error<IO::Error>>
where
    IO: ProbeIo,
{
    let mut bytes = [0; 5];
    command_read(io, command, &mut bytes)?;

    Ok(bytes)
}

fn read_mtp<IO>(io: &mut IO) -> Result<[u8; MTP_LEN], Error<IO::Error>>
where
    IO: ProbeIo,
{
    // RMTP returns one dummy byte followed by the MTP contents.
    let mut raw = [0; MTP_LEN + 1];
    command_read(io, COMMAND_READ_MTP, &mut raw)?;

    let mut mtp = [0; MTP_LEN];
    mtp.copy_from_slice(&raw[1..]);

    Ok(mtp)
}

fn command_read<IO>(io: &mut IO, command: u8, output: &mut [u8]) -> Result<(), Error<IO::Error>>
where
    IO: ProbeIo,
{
    io.data_output().map_err(Error::Io)?;
    io.dc_low().map_err(Error::Io)?;
    io.cs_low().map_err(Error::Io)?;

    write_byte(io, command)?;

    io.dc_high().map_err(Error::Io)?;
    io.data_input_pullup().map_err(Error::Io)?;
    io.clock_delay();

    for byte in output {
        *byte = read_byte(io)?;
    }

    io.cs_high().map_err(Error::Io)?;
    io.data_output().map_err(Error::Io)?;

    Ok(())
}

fn write_byte<IO>(io: &mut IO, mut value: u8) -> Result<(), Error<IO::Error>>
where
    IO: ProbeIo,
{
    for _ in 0..8 {
        if value & 0x80 != 0 {
            io.data_high().map_err(Error::Io)?;
        } else {
            io.data_low().map_err(Error::Io)?;
        }

        io.clock_delay();
        io.clock_high().map_err(Error::Io)?;
        io.clock_delay();
        io.clock_low().map_err(Error::Io)?;

        value <<= 1;
    }

    Ok(())
}

fn read_byte<IO>(io: &mut IO) -> Result<u8, Error<IO::Error>>
where
    IO: ProbeIo,
{
    let mut value = 0;

    for _ in 0..8 {
        // UC81xx shifts the next bit on the falling clock edge. FreeInk samples
        // while clock is low, then pulses it high.
        io.clock_delay();

        let bit = u8::from(io.data_is_high().map_err(Error::Io)?);
        value = (value << 1) | bit;

        io.clock_high().map_err(Error::Io)?;
        io.clock_delay();
        io.clock_low().map_err(Error::Io)?;
    }

    Ok(value)
}
