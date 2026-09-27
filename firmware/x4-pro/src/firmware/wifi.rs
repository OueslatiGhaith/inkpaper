use alloc::vec::Vec;

use bm8563::DateTime;
use defmt::{info, warn};
use embassy_futures::select::{Either, select};
use embassy_net::{
    Config as NetConfig, IpAddress, Stack, StackResources,
    dns::DnsQueryType,
    udp::{PacketMetadata, UdpSocket},
};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal,
};
use embassy_time::{Duration, with_timeout};
use esp_hal::{peripherals::WIFI, rng::Rng};
use esp_radio::wifi::{
    AuthenticationMethod, AuthenticationMethodConfig, Config as WifiConfig, ControllerConfig,
    Interface, WifiController, scan::ScanConfig, sta::StationConfig,
};
use inkpaper_app::{ClockSyncFailure, WifiCredentials, WifiNetwork, WifiScanError};

use crate::firmware::rtc::{self, RtcSyncResult};

const NTP_SERVER: &str = "pool.ntp.org";
const NTP_ATTEMPTS: usize = 3;

const MAX_SCAN_RESULTS: usize = 24;

const SCAN_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const DHCP_TIMEOUT: Duration = Duration::from_secs(15);
const DNS_TIMEOUT: Duration = Duration::from_secs(10);
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// seconds between the Unix epoch and 2000-01-01, the RTC's epoch
const UNIX_TO_2000: u64 = 946_684_800;

// DHCP, DNS and the NTP socket
const SOCKETS: usize = 3;

enum WifiRequest {
    Scan,
    SyncClock(WifiCredentials),
}

pub enum WifiEvent {
    Scanned(Result<Vec<WifiNetwork>, WifiScanError>),
    ClockSynced(Result<(), ClockSyncFailure>),
}

// the app runs one request at a time, so a newer one never overwrites a
// waiting one
static REQUESTS: Signal<CriticalSectionRawMutex, WifiRequest> = Signal::new();
pub static WIFI_EVENTS: Channel<CriticalSectionRawMutex, WifiEvent, 2> = Channel::new();

/// Asks the WiFi task for a scan; the networks arrive on [`WIFI_EVENTS`].
pub fn request_scan() {
    REQUESTS.signal(WifiRequest::Scan);
}

/// Asks the WiFi task to set the clock over `network`; the outcome arrives on
/// [`WIFI_EVENTS`].
pub fn request_clock_sync(network: WifiCredentials) {
    REQUESTS.signal(WifiRequest::SyncClock(network));
}

/// The network from `INKPAPER_WIFI_SSID` and `INKPAPER_WIFI_PASS` at build time.
/// A development stand-in until passwords can be typed.
pub fn build_credentials() -> Option<WifiCredentials> {
    WifiCredentials::new(
        option_env!("INKPAPER_WIFI_SSID")?,
        option_env!("INKPAPER_WIFI_PASS").unwrap_or(""),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
enum SyncFailure {
    Credentials,
    Radio,
    Connect,
    Dhcp,
    Dns,
    Ntp,
    Time,
    Rtc,
}

impl From<SyncFailure> for ClockSyncFailure {
    fn from(failure: SyncFailure) -> Self {
        match failure {
            SyncFailure::Credentials | SyncFailure::Connect => Self::Join,
            SyncFailure::Radio => Self::Radio,
            SyncFailure::Dhcp | SyncFailure::Dns => Self::NoInternet,
            SyncFailure::Ntp => Self::TimeServer,
            SyncFailure::Time | SyncFailure::Rtc => Self::ClockWrite,
        }
    }
}

/// Owns the radio. Each request starts WiFi, does its work, and shuts the
/// radio down again.
#[embassy_executor::task]
pub async fn wifi_task(mut wifi: WIFI<'static>) {
    loop {
        let event = match REQUESTS.wait().await {
            WifiRequest::Scan => WifiEvent::Scanned(scan(wifi.reborrow()).await),
            WifiRequest::SyncClock(network) => {
                WifiEvent::ClockSynced(sync_clock(wifi.reborrow(), &network).await)
            }
        };

        WIFI_EVENTS.send(event).await;
    }
}

async fn scan(wifi: WIFI<'_>) -> Result<Vec<WifiNetwork>, WifiScanError> {
    let mut controller =
        WifiController::new(wifi, ControllerConfig::default()).map_err(|error| {
            warn!("WiFi init failed: {:?}", error);
            WifiScanError
        })?;

    let config = ScanConfig::default().with_max(MAX_SCAN_RESULTS);
    let found = match with_timeout(SCAN_TIMEOUT, controller.scan_async(&config)).await {
        Ok(Ok(found)) => found,
        Ok(Err(error)) => {
            warn!("WiFi scan failed: {:?}", error);
            return Err(WifiScanError);
        }
        Err(_) => {
            warn!("WiFi scan timed out");
            return Err(WifiScanError);
        }
    };

    info!("WiFi scan found {} access points", found.len());

    Ok(found
        .iter()
        .map(|access_point| {
            let secured = !matches!(
                access_point.auth_method,
                None | Some(AuthenticationMethod::None)
            );

            WifiNetwork::new(
                access_point.ssid.as_str(),
                access_point.signal_strength,
                secured,
            )
        })
        .collect())
}

async fn sync_clock(wifi: WIFI<'_>, network: &WifiCredentials) -> Result<(), ClockSyncFailure> {
    info!("clock sync: joining {}", network.ssid());

    match sync(wifi, network).await {
        Ok(datetime) => {
            info!(
                "clock sync: RTC set to {:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
                datetime.year(),
                datetime.month(),
                datetime.day(),
                datetime.hour(),
                datetime.minute(),
                datetime.second(),
            );

            Ok(())
        }
        Err(failure) => {
            warn!("clock sync failed at {}", failure);

            Err(failure.into())
        }
    }
}

async fn sync(wifi: WIFI<'_>, network: &WifiCredentials) -> Result<DateTime, SyncFailure> {
    let ssid = network
        .ssid()
        .try_into()
        .map_err(|_| SyncFailure::Credentials)?;
    let authentication = if network.password().is_empty() {
        AuthenticationMethodConfig::Open
    } else {
        AuthenticationMethodConfig::Wpa2Personal(
            network
                .password()
                .try_into()
                .map_err(|_| SyncFailure::Credentials)?,
        )
    };
    let station = StationConfig::default()
        .with_ssid(ssid)
        .with_authentication(authentication);

    let mut controller = WifiController::new(
        wifi,
        ControllerConfig::default().with_initial_config(WifiConfig::Station(station)),
    )
    .map_err(|error| {
        warn!("WiFi init failed: {:?}", error);
        SyncFailure::Radio
    })?;

    match with_timeout(CONNECT_TIMEOUT, controller.connect_async()).await {
        Ok(Ok(_)) => info!("clock sync: WiFi connected"),
        Ok(Err(error)) => {
            warn!("WiFi connect failed: {:?}", error);
            return Err(SyncFailure::Connect);
        }
        Err(_) => {
            warn!("WiFi connect timed out");
            return Err(SyncFailure::Connect);
        }
    }

    let rng = Rng::new();
    let seed = u64::from(rng.random()) << 32 | u64::from(rng.random());
    let mut resources = StackResources::<SOCKETS>::new();
    let (stack, mut runner) = embassy_net::new(
        Interface::station(),
        NetConfig::dhcpv4(Default::default()),
        &mut resources,
        seed,
    );

    // the stack only makes progress while its runner is polled
    let result = match select(runner.run(), fetch_and_apply(stack, rng)).await {
        Either::First(never) => match never {},
        Either::Second(result) => result,
    };

    if let Err(error) = controller.disconnect_async().await {
        warn!("WiFi disconnect failed: {:?}", error);
    }

    // dropping the controller deinitializes WiFi and powers the radio down
    drop(controller);

    result
}

async fn fetch_and_apply(stack: Stack<'_>, rng: Rng) -> Result<DateTime, SyncFailure> {
    if with_timeout(DHCP_TIMEOUT, stack.wait_config_up())
        .await
        .is_err()
    {
        warn!("DHCP timed out");
        return Err(SyncFailure::Dhcp);
    }

    let servers =
        match with_timeout(DNS_TIMEOUT, stack.dns_query(NTP_SERVER, DnsQueryType::A)).await {
            Ok(Ok(servers)) if !servers.is_empty() => servers,
            Ok(Ok(_)) => {
                warn!("DNS returned no address for {}", NTP_SERVER);
                return Err(SyncFailure::Dns);
            }
            Ok(Err(error)) => {
                warn!("DNS query failed: {:?}", error);
                return Err(SyncFailure::Dns);
            }
            Err(_) => {
                warn!("DNS query timed out");
                return Err(SyncFailure::Dns);
            }
        };

    let unix = query_ntp(stack, &servers, rng).await?;

    // write the RTC while the reply is fresh, before tearing WiFi down
    let seconds = unix.checked_sub(UNIX_TO_2000).ok_or(SyncFailure::Time)?;
    let datetime = DateTime::from_seconds_since_2000(seconds).map_err(|_| SyncFailure::Time)?;

    match rtc::sync_utc(datetime).await {
        RtcSyncResult::Applied(actual) => Ok(actual),
        RtcSyncResult::Failed(_) => Err(SyncFailure::Rtc),
    }
}

/// Unix seconds from the first valid NTP reply, cycling through the resolved
/// servers.
async fn query_ntp(stack: Stack<'_>, servers: &[IpAddress], rng: Rng) -> Result<u64, SyncFailure> {
    let mut rx_meta = [PacketMetadata::EMPTY; 1];
    let mut tx_meta = [PacketMetadata::EMPTY; 1];
    let mut rx_buffer = [0; sntp::PACKET_LEN * 2];
    let mut tx_buffer = [0; sntp::PACKET_LEN * 2];
    let mut socket = UdpSocket::new(
        stack,
        &mut rx_meta,
        &mut rx_buffer,
        &mut tx_meta,
        &mut tx_buffer,
    );

    // port 0 picks an ephemeral local port
    socket.bind(0).map_err(|error| {
        warn!("NTP socket bind failed: {:?}", error);
        SyncFailure::Ntp
    })?;

    let mut reply = [0; sntp::PACKET_LEN * 2];

    for attempt in 0..NTP_ATTEMPTS {
        let server = servers[attempt % servers.len()];
        let nonce = u64::from(rng.random()) << 32 | u64::from(rng.random());

        if let Err(error) = socket
            .send_to(&sntp::request(nonce), (server, sntp::PORT))
            .await
        {
            warn!("NTP send to {} failed: {:?}", server, error);
            continue;
        }

        match with_timeout(REPLY_TIMEOUT, socket.recv_from(&mut reply)).await {
            Ok(Ok((len, _))) => match sntp::parse_response(&reply[..len], nonce) {
                Ok(unix) => return Ok(unix),
                Err(error) => warn!("NTP reply from {} rejected: {}", server, error),
            },
            Ok(Err(error)) => warn!("NTP receive failed: {:?}", error),
            Err(_) => warn!("NTP reply from {} timed out", server),
        }
    }

    Err(SyncFailure::Ntp)
}
