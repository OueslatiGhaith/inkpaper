//! Minimal SNTPv4 client packets (RFC 4330): build a request, then validate the
//! server's reply and read its transmit time.
#![no_std]

pub const PORT: u16 = 123;
pub const PACKET_LEN: usize = 48;

const VERSION: u8 = 4;
const MODE_CLIENT: u8 = 3;
const MODE_SERVER: u8 = 4;
const LEAP_UNSYNCHRONIZED: u8 = 3;

const ORIGIN_TIMESTAMP: usize = 24;
const TRANSMIT_TIMESTAMP: usize = 40;

/// seconds between the NTP epoch (1900) and the Unix epoch (1970)
const UNIX_OFFSET: u64 = 2_208_988_800;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// the packet is shorter than an SNTP header
    Length,
    /// the packet is not a server reply
    Mode,
    /// the server says its own clock is not synchronized
    Unsynchronized,
    /// stratum 0: a kiss-o'-death telling the client to back off
    KissOfDeath,
    /// the reply doesn't answer our request
    OriginMismatch,
    /// the server sent no transmit time
    MissingTime,
}

/// A client request. `nonce` is sent as the transmit timestamp; the server echoes
/// it back as the origin timestamp, which ties the reply to this request.
pub fn request(nonce: u64) -> [u8; PACKET_LEN] {
    let mut packet = [0; PACKET_LEN];
    packet[0] = VERSION << 3 | MODE_CLIENT;
    packet[TRANSMIT_TIMESTAMP..].copy_from_slice(&nonce.to_be_bytes());
    packet
}

/// Validates a server reply to [`request`]`(nonce)` and returns its transmit time
/// in whole seconds since the Unix epoch, rounded to the nearest second.
pub fn parse_response(packet: &[u8], nonce: u64) -> Result<u64, Error> {
    if packet.len() < PACKET_LEN {
        return Err(Error::Length);
    }

    if packet[0] & 0x07 != MODE_SERVER {
        return Err(Error::Mode);
    }

    if packet[0] >> 6 == LEAP_UNSYNCHRONIZED {
        return Err(Error::Unsynchronized);
    }

    if packet[1] == 0 {
        return Err(Error::KissOfDeath);
    }

    if timestamp(packet, ORIGIN_TIMESTAMP) != nonce {
        return Err(Error::OriginMismatch);
    }

    let transmit = timestamp(packet, TRANSMIT_TIMESTAMP);
    if transmit == 0 {
        return Err(Error::MissingTime);
    }

    let seconds = transmit >> 32;
    let rounding = (transmit >> 31) & 1;

    // era 0 ends in February 2036; like RFC 4330 §3, a set top bit means era 0
    // (1968–2036) and a clear one means era 1 (2036–2104)
    let era = if seconds & 0x8000_0000 == 0 { 1u64 << 32 } else { 0 };

    Ok(seconds + era + rounding - UNIX_OFFSET)
}

fn timestamp(packet: &[u8], offset: usize) -> u64 {
    let mut bytes = [0; 8];
    bytes.copy_from_slice(&packet[offset..offset + 8]);
    u64::from_be_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONCE: u64 = 0x0123_4567_89ab_cdef;

    fn reply(transmit: u64) -> [u8; PACKET_LEN] {
        let mut packet = [0; PACKET_LEN];
        packet[0] = VERSION << 3 | MODE_SERVER;
        packet[1] = 2;
        packet[ORIGIN_TIMESTAMP..ORIGIN_TIMESTAMP + 8].copy_from_slice(&NONCE.to_be_bytes());
        packet[TRANSMIT_TIMESTAMP..].copy_from_slice(&transmit.to_be_bytes());
        packet
    }

    fn ntp(seconds: u64, fraction: u32) -> u64 {
        (seconds << 32) | u64::from(fraction)
    }

    #[test]
    fn reads_transmit_time_as_unix_seconds() {
        // 2026-09-27 00:00:00 UTC
        let unix = 1_790_467_200;
        let packet = reply(ntp(unix + UNIX_OFFSET, 0));

        assert_eq!(parse_response(&packet, NONCE), Ok(unix));
    }

    #[test]
    fn rounds_to_nearest_second() {
        let unix = 1_790_467_200;

        let below = reply(ntp(unix + UNIX_OFFSET, 0x7fff_ffff));
        let above = reply(ntp(unix + UNIX_OFFSET, 0x8000_0000));

        assert_eq!(parse_response(&below, NONCE), Ok(unix));
        assert_eq!(parse_response(&above, NONCE), Ok(unix + 1));
    }

    #[test]
    fn crosses_the_2036_era_rollover() {
        // era 1 second 10 is 2036-02-07 06:28:26 UTC
        let packet = reply(ntp(10, 0));

        assert_eq!(parse_response(&packet, NONCE), Ok((1 << 32) + 10 - UNIX_OFFSET));
    }

    #[test]
    fn rejects_replies_that_cannot_be_trusted() {
        let valid = reply(ntp(1_790_467_200 + UNIX_OFFSET, 0));

        let mut client = valid;
        client[0] = VERSION << 3 | MODE_CLIENT;
        assert_eq!(parse_response(&client, NONCE), Err(Error::Mode));

        let mut unsynchronized = valid;
        unsynchronized[0] |= LEAP_UNSYNCHRONIZED << 6;
        assert_eq!(
            parse_response(&unsynchronized, NONCE),
            Err(Error::Unsynchronized)
        );

        let mut kiss = valid;
        kiss[1] = 0;
        assert_eq!(parse_response(&kiss, NONCE), Err(Error::KissOfDeath));

        assert_eq!(
            parse_response(&valid, NONCE + 1),
            Err(Error::OriginMismatch)
        );

        assert_eq!(
            parse_response(&valid[..PACKET_LEN - 1], NONCE),
            Err(Error::Length)
        );

        assert_eq!(parse_response(&reply(0), NONCE), Err(Error::MissingTime));
    }

    #[test]
    fn request_carries_version_mode_and_nonce() {
        let packet = request(NONCE);

        assert_eq!(packet[0], 0x23);
        assert_eq!(timestamp(&packet, TRANSMIT_TIMESTAMP), NONCE);
    }
}
