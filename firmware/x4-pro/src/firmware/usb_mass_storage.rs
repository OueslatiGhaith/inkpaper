use core::{
    cmp::min,
    future::Future,
    sync::atomic::{AtomicBool, Ordering},
};

use aligned::Aligned;
use block_device_driver::BlockDevice as RawBlockDevice;
use defmt::{Debug2Format, debug, info, warn};
use embassy_futures::select::{Either3, select3};
use embassy_usb::{
    Builder, Handler, UsbVersion,
    control::{InResponse, OutResponse, Recipient, Request, RequestType},
    driver::{Endpoint, EndpointError, EndpointIn, EndpointOut},
    types::InterfaceNumber,
};
use esp_hal::{
    peripherals::{GPIO19, GPIO20, USB_FS},
    usb::otg::{
        Usb,
        embassy_usb_device::{Config as DriverConfig, Driver as UsbDriver},
    },
};

const SECTOR_SIZE: usize = 512;

const BULK_PACKET_SIZE: usize = 64;
const CONTROL_PACKET_SIZE: usize = 64;
const ENDPOINT_OUT_BUFFER_SIZE: usize = CONTROL_PACKET_SIZE + BULK_PACKET_SIZE;

const USB_CLASS_MASS_STORAGE: u8 = 0x08;
const USB_SUBCLASS_SCSI_TRANSPARENT: u8 = 0x06;
const USB_PROTOCOL_BULK_ONLY: u8 = 0x50;

// Development identifiers.
//
// Espressif documents 0x303a as its development VID, and 0x4002 follows
// TinyUSB's MSC-only development PID convention. These must be replaced by
// an appropriately assigned product identity before distributing hardware
// as a USB product.
const USB_VENDOR_ID: u16 = 0x303a;
const USB_PRODUCT_ID: u16 = 0x4002;

const MSC_REQUEST_RESET: u8 = 0xff;
const MSC_REQUEST_GET_MAX_LUN: u8 = 0xfe;

const CBW_LENGTH: usize = 31;
const CBW_SIGNATURE: u32 = 0x4342_5355;

const CSW_LENGTH: usize = 13;
const CSW_SIGNATURE: u32 = 0x5342_5355;

const SCSI_TEST_UNIT_READY: u8 = 0x00;
const SCSI_REQUEST_SENSE: u8 = 0x03;
const SCSI_READ_6: u8 = 0x08;
const SCSI_WRITE_6: u8 = 0x0a;
const SCSI_INQUIRY: u8 = 0x12;
const SCSI_MODE_SENSE_6: u8 = 0x1a;
const SCSI_START_STOP_UNIT: u8 = 0x1b;
const SCSI_PREVENT_ALLOW_MEDIUM_REMOVAL: u8 = 0x1e;
const SCSI_READ_FORMAT_CAPACITIES: u8 = 0x23;
const SCSI_READ_CAPACITY_10: u8 = 0x25;
const SCSI_READ_10: u8 = 0x28;
const SCSI_WRITE_10: u8 = 0x2a;
const SCSI_VERIFY_10: u8 = 0x2f;
const SCSI_SYNCHRONIZE_CACHE_10: u8 = 0x35;
const SCSI_MODE_SENSE_10: u8 = 0x5a;
const SCSI_READ_16: u8 = 0x88;
const SCSI_WRITE_16: u8 = 0x8a;
const SCSI_SERVICE_ACTION_IN_16: u8 = 0x9e;
const SCSI_READ_12: u8 = 0xa8;
const SCSI_WRITE_12: u8 = 0xaa;
const SCSI_REPORT_LUNS: u8 = 0xa0;

const SCSI_SERVICE_ACTION_READ_CAPACITY_16: u8 = 0x10;

const SENSE_KEY_NO_SENSE: u8 = 0x00;
const SENSE_KEY_MEDIUM_ERROR: u8 = 0x03;
const SENSE_KEY_ILLEGAL_REQUEST: u8 = 0x05;

const ASC_INVALID_COMMAND: u8 = 0x20;
const ASC_LBA_OUT_OF_RANGE: u8 = 0x21;
const ASC_INVALID_FIELD: u8 = 0x24;
const ASC_UNRECOVERED_READ_ERROR: u8 = 0x11;
const ASC_WRITE_ERROR: u8 = 0x0c;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbMassStorageExit {
    Ejected,
    Shutdown,
}

pub async fn run<D, S>(
    card: &mut D,
    sector_count: u32,
    usb_fs: USB_FS<'static>,
    usb_dp: GPIO20<'static>,
    usb_dm: GPIO19<'static>,
    shutdown: S,
) -> UsbMassStorageExit
where
    D: RawBlockDevice<SECTOR_SIZE>,
    S: Future<Output = ()>,
{
    let reset_requested = AtomicBool::new(false);

    let usb = Usb::new_fs(usb_fs, usb_dp, usb_dm);

    // The Synopsys driver needs receive storage for every OUT endpoint.
    //
    // EP0 control OUT: 64 bytes
    // MSC bulk OUT:    64 bytes
    let mut endpoint_out_buffer = [0u8; ENDPOINT_OUT_BUFFER_SIZE];

    let driver = UsbDriver::new(usb, &mut endpoint_out_buffer, DriverConfig::default());

    let mut config = embassy_usb::Config::new(USB_VENDOR_ID, USB_PRODUCT_ID);

    config.max_packet_size_0 = CONTROL_PACKET_SIZE as u8;
    config.bcd_usb = UsbVersion::Two;
    config.device_class = 0;
    config.device_sub_class = 0;
    config.device_protocol = 0;
    config.device_release = 0x0100;
    config.manufacturer = Some("InkPaper");
    config.product = Some("InkPaper X4 Pro");
    config.serial_number = None;
    config.composite_with_iads = false;
    config.self_powered = true;
    config.max_power = 0;

    let mut config_descriptor = [0u8; 64];
    let mut bos_descriptor = [0u8; 32];
    let mut msos_descriptor = [];
    let mut control_buffer = [0u8; 64];

    let mut control = MassStorageControl {
        interface_number: None,
        reset_requested: &reset_requested,
    };

    let mut builder = Builder::new(
        driver,
        config,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut msos_descriptor,
        &mut control_buffer,
    );

    let (endpoint_out, endpoint_in, interface_number) = {
        let mut function = builder.function(
            USB_CLASS_MASS_STORAGE,
            USB_SUBCLASS_SCSI_TRANSPARENT,
            USB_PROTOCOL_BULK_ONLY,
        );

        let mut interface = function.interface();
        let interface_number = interface.interface_number();

        let mut alternate = interface.alt_setting(
            USB_CLASS_MASS_STORAGE,
            USB_SUBCLASS_SCSI_TRANSPARENT,
            USB_PROTOCOL_BULK_ONLY,
            None,
        );

        let endpoint_out = alternate.endpoint_bulk_out(None, BULK_PACKET_SIZE as u16);

        let endpoint_in = alternate.endpoint_bulk_in(None, BULK_PACKET_SIZE as u16);

        (endpoint_out, endpoint_in, interface_number)
    };

    control.interface_number = Some(interface_number);
    builder.handler(&mut control);

    let mut usb_device = builder.build();

    let transport = BotTransport::new(endpoint_out, endpoint_in, &reset_requested);

    info!(
        "USB MSC starting sectors={} bytes={}",
        sector_count,
        u64::from(sector_count) * SECTOR_SIZE as u64,
    );

    // Keep the USB device runner, BOT/SCSI transport, and storage shutdown
    // service alive together.
    //
    // If shutdown wins, this function still gets a chance to disable the USB
    // peripheral before returning to storage/mount.rs.
    let exit = match select3(
        usb_device.run(),
        transport.run(card, sector_count),
        shutdown,
    )
    .await
    {
        Either3::First(never) => match never {},

        Either3::Second(exit) => exit,

        Either3::Third(()) => {
            info!("USB MSC shutdown requested");
            UsbMassStorageExit::Shutdown
        }
    };

    usb_device.disable().await;

    info!("USB MSC stopped");

    exit
}

struct MassStorageControl<'a> {
    interface_number: Option<InterfaceNumber>,
    reset_requested: &'a AtomicBool,
}

impl MassStorageControl<'_> {
    fn matches(&self, request: Request) -> bool {
        let Some(interface_number) = self.interface_number else {
            return false;
        };

        request.request_type == RequestType::Class
            && request.recipient == Recipient::Interface
            && request.index == u16::from(u8::from(interface_number))
    }
}

impl Handler for MassStorageControl<'_> {
    fn reset(&mut self) {
        self.reset_requested.store(true, Ordering::Release);
    }

    fn configured(&mut self, configured: bool) {
        if !configured {
            self.reset_requested.store(true, Ordering::Release);
        }
    }

    fn control_out(&mut self, request: Request, data: &[u8]) -> Option<OutResponse> {
        if !self.matches(request) {
            return None;
        }

        if request.request == MSC_REQUEST_RESET
            && request.value == 0
            && request.length == 0
            && data.is_empty()
        {
            debug!("USB MSC bulk-only reset");

            self.reset_requested.store(true, Ordering::Release);

            return Some(OutResponse::Accepted);
        }

        Some(OutResponse::Rejected)
    }

    fn control_in<'a>(
        &'a mut self,
        request: Request,
        buffer: &'a mut [u8],
    ) -> Option<InResponse<'a>> {
        if !self.matches(request) {
            return None;
        }

        if request.request == MSC_REQUEST_GET_MAX_LUN
            && request.value == 0
            && request.length == 1
            && !buffer.is_empty()
        {
            // InkPaper exposes exactly one logical unit: LUN 0.
            buffer[0] = 0;

            return Some(InResponse::Accepted(&buffer[..1]));
        }

        Some(InResponse::Rejected)
    }
}

struct BotTransport<'a, O, I> {
    endpoint_out: O,
    endpoint_in: I,
    reset_requested: &'a AtomicBool,
    sense: Sense,
}

impl<'a, O, I> BotTransport<'a, O, I>
where
    O: EndpointOut,
    I: EndpointIn,
{
    fn new(endpoint_out: O, endpoint_in: I, reset_requested: &'a AtomicBool) -> Self {
        Self {
            endpoint_out,
            endpoint_in,
            reset_requested,
            sense: Sense::NONE,
        }
    }

    async fn run<D>(mut self, card: &mut D, sector_count: u32) -> UsbMassStorageExit
    where
        D: RawBlockDevice<SECTOR_SIZE>,
    {
        loop {
            self.endpoint_out.wait_enabled().await;
            self.endpoint_in.wait_enabled().await;

            match self.process_next(card, sector_count).await {
                Ok(true) => {
                    info!("USB MSC host ejected media");

                    return UsbMassStorageExit::Ejected;
                }

                Ok(false) => {}

                Err(EndpointError::Disabled) => {
                    debug!("USB MSC endpoints disabled");
                }

                Err(EndpointError::BufferOverflow) => {
                    warn!("USB MSC endpoint buffer overflow");
                }
            }
        }
    }

    async fn process_next<D>(
        &mut self,
        card: &mut D,
        sector_count: u32,
    ) -> Result<bool, EndpointError>
    where
        D: RawBlockDevice<SECTOR_SIZE>,
    {
        if self.reset_requested.swap(false, Ordering::AcqRel) {
            self.sense = Sense::NONE;
        }

        let mut bytes = [0u8; CBW_LENGTH];

        let received = self.endpoint_out.read_transfer(&mut bytes).await?;

        if received != CBW_LENGTH {
            warn!("USB MSC invalid CBW length={}", received);

            return Ok(false);
        }

        let Some(cbw) = CommandBlockWrapper::parse(&bytes) else {
            warn!("USB MSC invalid CBW");

            return Ok(false);
        };

        let execution = self.execute(card, sector_count, &cbw).await?;

        // Failed or unsupported OUT commands still need their data stage
        // consumed before the next CBW can be read.
        if !cbw.direction_in && cbw.transfer_length > execution.out_consumed {
            self.discard_out(cbw.transfer_length - execution.out_consumed)
                .await?;
        }

        // For a short IN data stage whose payload ended exactly on a packet
        // boundary, terminate the data stage explicitly before the CSW.
        if cbw.direction_in
            && cbw.transfer_length > execution.transferred
            && execution.transferred % BULK_PACKET_SIZE as u32 == 0
        {
            self.endpoint_in.write(&[]).await?;
        }

        let residue = cbw.transfer_length.saturating_sub(execution.transferred);

        let csw = CommandStatusWrapper::new(cbw.tag, residue, execution.status).encode();

        self.endpoint_in.write_transfer(&csw, false).await?;

        Ok(execution.ejected)
    }

    async fn execute<D>(
        &mut self,
        card: &mut D,
        sector_count: u32,
        cbw: &CommandBlockWrapper,
    ) -> Result<Execution, EndpointError>
    where
        D: RawBlockDevice<SECTOR_SIZE>,
    {
        let cdb = cbw.command();

        match cdb[0] {
            SCSI_READ_6 | SCSI_WRITE_6 | SCSI_READ_10 | SCSI_WRITE_10 | SCSI_READ_12
            | SCSI_WRITE_12 | SCSI_READ_16 | SCSI_WRITE_16 => {
                let Some((lba, blocks, write)) = parse_read_write(cdb) else {
                    return Ok(self.invalid_field());
                };

                self.execute_read_write(card, sector_count, cbw, lba, blocks, write)
                    .await
            }

            SCSI_TEST_UNIT_READY => {
                if cdb.len() < 6 {
                    return Ok(self.invalid_field());
                }

                Ok(self.no_data(cbw))
            }

            SCSI_REQUEST_SENSE => self.request_sense(cbw, cdb).await,

            SCSI_INQUIRY => self.inquiry(cbw, cdb).await,

            SCSI_MODE_SENSE_6 => self.mode_sense_6(cbw, cdb).await,

            SCSI_START_STOP_UNIT => self.start_stop_unit(cbw, cdb),

            SCSI_PREVENT_ALLOW_MEDIUM_REMOVAL => {
                if cdb.len() < 6 {
                    return Ok(self.invalid_field());
                }

                Ok(self.no_data(cbw))
            }

            SCSI_READ_FORMAT_CAPACITIES => {
                self.read_format_capacities(cbw, cdb, sector_count).await
            }

            SCSI_READ_CAPACITY_10 => self.read_capacity_10(cbw, cdb, sector_count).await,

            SCSI_VERIFY_10 => {
                if cdb.len() < 10 {
                    return Ok(self.invalid_field());
                }

                // BYTCHK=0 means verify without transferring comparison data.
                if cdb[1] & 0x02 != 0 {
                    return Ok(self.invalid_field());
                }

                Ok(self.no_data(cbw))
            }

            SCSI_SYNCHRONIZE_CACHE_10 => {
                if cdb.len() < 10 {
                    return Ok(self.invalid_field());
                }

                // sdio::BlockDevice writes complete before the async write
                // operation returns, so there is no additional cache to flush.
                Ok(self.no_data(cbw))
            }

            SCSI_MODE_SENSE_10 => self.mode_sense_10(cbw, cdb).await,

            SCSI_SERVICE_ACTION_IN_16 => self.service_action_in_16(cbw, cdb, sector_count).await,

            SCSI_REPORT_LUNS => self.report_luns(cbw, cdb).await,

            opcode => {
                warn!("USB MSC unsupported SCSI opcode={:#04x}", opcode);

                Ok(self.invalid_command())
            }
        }
    }

    async fn inquiry(
        &mut self,
        cbw: &CommandBlockWrapper,
        cdb: &[u8],
    ) -> Result<Execution, EndpointError> {
        if cdb.len() < 6 {
            return Ok(self.invalid_field());
        }

        let evpd = cdb[1] & 0x01 != 0;
        let command_support_data = cdb[1] & 0x02 != 0;
        let page = cdb[2];
        let allocation = usize::from(cdb[4]);

        if command_support_data {
            return Ok(self.invalid_field());
        }

        if evpd {
            // Supported VPD pages. For MVP there is intentionally no fake
            // serial-number or device-identification page.
            if page != 0x00 {
                return Ok(self.invalid_field());
            }

            let response = [
                0x00, // direct-access block device
                0x00, // supported VPD pages
                0x00, 0x01, // one supported page follows
                0x00, // page 0x00
            ];

            let length = min(response.len(), allocation);

            return self.send_in_response(cbw, &response[..length]).await;
        }

        if page != 0 {
            return Ok(self.invalid_field());
        }

        let mut response = [0u8; 36];

        response[0] = 0x00;
        response[1] = 0x80; // removable medium
        response[2] = 0x06;
        response[3] = 0x02;
        response[4] = 31;

        response[8..16].copy_from_slice(b"INKPAPER");
        response[16..32].copy_from_slice(b"X4 PRO SD       ");
        response[32..36].copy_from_slice(b"1.0 ");

        let length = min(response.len(), allocation);

        self.send_in_response(cbw, &response[..length]).await
    }

    async fn request_sense(
        &mut self,
        cbw: &CommandBlockWrapper,
        cdb: &[u8],
    ) -> Result<Execution, EndpointError> {
        if cdb.len() < 6 || cdb[1] & 0x01 != 0 {
            return Ok(self.invalid_field());
        }

        let allocation = usize::from(cdb[4]);
        let sense = self.sense;

        let mut response = [0u8; 18];

        response[0] = 0x70;
        response[2] = sense.key;
        response[7] = 10;
        response[12] = sense.asc;
        response[13] = sense.ascq;

        let length = min(response.len(), allocation);

        let execution = self.send_in_response(cbw, &response[..length]).await?;

        if execution.status == CswStatus::Passed {
            self.sense = Sense::NONE;
        }

        Ok(execution)
    }

    async fn mode_sense_6(
        &mut self,
        cbw: &CommandBlockWrapper,
        cdb: &[u8],
    ) -> Result<Execution, EndpointError> {
        if cdb.len() < 6 {
            return Ok(self.invalid_field());
        }

        let allocation = usize::from(cdb[4]);

        // Four-byte mode parameter header, no block descriptors and no
        // optional mode pages. Write-protect is clear because USB Drive is
        // writable.
        let response = [3u8, 0, 0, 0];

        let length = min(response.len(), allocation);

        self.send_in_response(cbw, &response[..length]).await
    }

    async fn mode_sense_10(
        &mut self,
        cbw: &CommandBlockWrapper,
        cdb: &[u8],
    ) -> Result<Execution, EndpointError> {
        if cdb.len() < 10 {
            return Ok(self.invalid_field());
        }

        let allocation = usize::from(be_u16(&cdb[7..9]));

        let response = [
            0x00, 0x06, // mode data length
            0x00, // medium type
            0x00, // device-specific parameter
            0x00, 0x00, 0x00, 0x00, // block descriptor length
        ];

        let length = min(response.len(), allocation);

        self.send_in_response(cbw, &response[..length]).await
    }

    async fn read_capacity_10(
        &mut self,
        cbw: &CommandBlockWrapper,
        cdb: &[u8],
        sector_count: u32,
    ) -> Result<Execution, EndpointError> {
        if cdb.len() < 10 || sector_count == 0 {
            return Ok(self.invalid_field());
        }

        let mut response = [0u8; 8];

        response[..4].copy_from_slice(&(sector_count - 1).to_be_bytes());
        response[4..8].copy_from_slice(&(SECTOR_SIZE as u32).to_be_bytes());

        self.send_in_response(cbw, &response).await
    }

    async fn read_format_capacities(
        &mut self,
        cbw: &CommandBlockWrapper,
        cdb: &[u8],
        sector_count: u32,
    ) -> Result<Execution, EndpointError> {
        if cdb.len() < 10 || sector_count == 0 {
            return Ok(self.invalid_field());
        }

        let allocation = usize::from(be_u16(&cdb[7..9]));

        let mut response = [0u8; 12];

        response[3] = 8;
        response[4..8].copy_from_slice(&sector_count.to_be_bytes());

        // Descriptor code 0x02 means formatted media.
        response[8] = 0x02;

        let block_size = (SECTOR_SIZE as u32).to_be_bytes();

        response[9..12].copy_from_slice(&block_size[1..4]);

        let length = min(response.len(), allocation);

        self.send_in_response(cbw, &response[..length]).await
    }

    async fn service_action_in_16(
        &mut self,
        cbw: &CommandBlockWrapper,
        cdb: &[u8],
        sector_count: u32,
    ) -> Result<Execution, EndpointError> {
        if cdb.len() < 16
            || cdb[1] & 0x1f != SCSI_SERVICE_ACTION_READ_CAPACITY_16
            || sector_count == 0
        {
            return Ok(self.invalid_field());
        }

        let allocation = usize::try_from(be_u32(&cdb[10..14])).unwrap_or(usize::MAX);

        let mut response = [0u8; 32];

        response[..8].copy_from_slice(&(u64::from(sector_count) - 1).to_be_bytes());

        response[8..12].copy_from_slice(&(SECTOR_SIZE as u32).to_be_bytes());

        let length = min(response.len(), allocation);

        self.send_in_response(cbw, &response[..length]).await
    }

    async fn report_luns(
        &mut self,
        cbw: &CommandBlockWrapper,
        cdb: &[u8],
    ) -> Result<Execution, EndpointError> {
        if cdb.len() < 12 {
            return Ok(self.invalid_field());
        }

        let allocation = usize::try_from(be_u32(&cdb[6..10])).unwrap_or(usize::MAX);

        let mut response = [0u8; 16];

        // Eight bytes of LUN list data follow the header.
        response[..4].copy_from_slice(&8u32.to_be_bytes());

        // response[8..16] is already zero, representing LUN 0.

        let length = min(response.len(), allocation);

        self.send_in_response(cbw, &response[..length]).await
    }

    fn start_stop_unit(
        &mut self,
        cbw: &CommandBlockWrapper,
        cdb: &[u8],
    ) -> Result<Execution, EndpointError> {
        if cdb.len() < 6 {
            return Ok(self.invalid_field());
        }

        let mut execution = self.no_data(cbw);

        if execution.status != CswStatus::Passed {
            return Ok(execution);
        }

        let load_eject = cdb[4] & 0x02 != 0;
        let start = cdb[4] & 0x01 != 0;

        if load_eject && !start {
            execution.ejected = true;
        }

        Ok(execution)
    }

    async fn execute_read_write<D>(
        &mut self,
        card: &mut D,
        sector_count: u32,
        cbw: &CommandBlockWrapper,
        lba: u64,
        blocks: u32,
        write: bool,
    ) -> Result<Execution, EndpointError>
    where
        D: RawBlockDevice<SECTOR_SIZE>,
    {
        let Some(expected_bytes) = u64::from(blocks).checked_mul(SECTOR_SIZE as u64) else {
            return Ok(self.phase_error());
        };

        let Ok(expected_bytes) = u32::try_from(expected_bytes) else {
            return Ok(self.phase_error());
        };

        if cbw.transfer_length != expected_bytes {
            return Ok(self.phase_error());
        }

        if expected_bytes != 0 && cbw.direction_in != !write {
            return Ok(self.phase_error());
        }

        if blocks == 0 {
            return Ok(Execution::passed(0));
        }

        let Some(end) = lba.checked_add(u64::from(blocks)) else {
            return Ok(self.lba_out_of_range());
        };

        if end > u64::from(sector_count) {
            return Ok(self.lba_out_of_range());
        }

        let mut sector =
            Aligned::<<D as RawBlockDevice<SECTOR_SIZE>>::Align, _>([0u8; SECTOR_SIZE]);

        let mut execution = Execution::passed(0);

        for block in 0..blocks {
            let address =
                u32::try_from(lba + u64::from(block)).expect("validated USB MSC LBA must fit u32");

            if write {
                let received = self.endpoint_out.read_transfer(&mut sector[..]).await?;

                execution.out_consumed = execution.out_consumed.saturating_add(received as u32);

                if received != SECTOR_SIZE {
                    warn!(
                        "USB MSC short WRITE data lba={} bytes={}",
                        address, received,
                    );

                    // A short OUT packet terminates the host data stage, so
                    // there is nothing further to drain even though the CSW
                    // reports the remaining residue.
                    execution.out_consumed = cbw.transfer_length;
                    execution.status = CswStatus::PhaseError;
                    self.sense = Sense::new(SENSE_KEY_ILLEGAL_REQUEST, ASC_INVALID_FIELD, 0);

                    break;
                }

                if let Err(error) = card.write(address, core::slice::from_ref(&sector)).await {
                    warn!(
                        "USB MSC SD write failed lba={} error={}",
                        address,
                        Debug2Format(&error),
                    );

                    execution.status = CswStatus::Failed;
                    self.sense = Sense::new(SENSE_KEY_MEDIUM_ERROR, ASC_WRITE_ERROR, 0);

                    break;
                }

                execution.transferred = execution.transferred.saturating_add(SECTOR_SIZE as u32);
            } else {
                if let Err(error) = card.read(address, core::slice::from_mut(&mut sector)).await {
                    warn!(
                        "USB MSC SD read failed lba={} error={}",
                        address,
                        Debug2Format(&error),
                    );

                    execution.status = CswStatus::Failed;
                    self.sense = Sense::new(SENSE_KEY_MEDIUM_ERROR, ASC_UNRECOVERED_READ_ERROR, 0);

                    break;
                }

                self.endpoint_in.write_transfer(&sector[..], false).await?;

                execution.transferred = execution.transferred.saturating_add(SECTOR_SIZE as u32);
            }
        }

        Ok(execution)
    }

    async fn send_in_response(
        &mut self,
        cbw: &CommandBlockWrapper,
        data: &[u8],
    ) -> Result<Execution, EndpointError> {
        if cbw.transfer_length != 0 && !cbw.direction_in {
            return Ok(self.phase_error());
        }

        let length = min(data.len(), cbw.transfer_length as usize);

        if length != 0 {
            self.endpoint_in
                .write_transfer(&data[..length], false)
                .await?;
        }

        Ok(Execution::passed(length as u32))
    }

    async fn discard_out(&mut self, mut remaining: u32) -> Result<(), EndpointError> {
        let mut buffer = [0u8; BULK_PACKET_SIZE];

        while remaining != 0 {
            let wanted = min(remaining as usize, buffer.len());

            let received = self.endpoint_out.read(&mut buffer[..wanted]).await?;

            if received == 0 {
                break;
            }

            remaining = remaining.saturating_sub(received as u32);

            if received < wanted {
                break;
            }
        }

        Ok(())
    }

    fn no_data(&mut self, cbw: &CommandBlockWrapper) -> Execution {
        if cbw.transfer_length == 0 {
            Execution::passed(0)
        } else {
            self.phase_error()
        }
    }

    fn invalid_command(&mut self) -> Execution {
        self.sense = Sense::new(SENSE_KEY_ILLEGAL_REQUEST, ASC_INVALID_COMMAND, 0);

        Execution::failed()
    }

    fn invalid_field(&mut self) -> Execution {
        self.sense = Sense::new(SENSE_KEY_ILLEGAL_REQUEST, ASC_INVALID_FIELD, 0);

        Execution::failed()
    }

    fn lba_out_of_range(&mut self) -> Execution {
        self.sense = Sense::new(SENSE_KEY_ILLEGAL_REQUEST, ASC_LBA_OUT_OF_RANGE, 0);

        Execution::failed()
    }

    fn phase_error(&mut self) -> Execution {
        self.sense = Sense::new(SENSE_KEY_ILLEGAL_REQUEST, ASC_INVALID_FIELD, 0);

        Execution::phase_error()
    }
}

#[derive(Debug, Clone, Copy)]
struct CommandBlockWrapper {
    tag: u32,
    transfer_length: u32,
    direction_in: bool,
    command_length: u8,
    command: [u8; 16],
}

impl CommandBlockWrapper {
    fn parse(bytes: &[u8; CBW_LENGTH]) -> Option<Self> {
        if le_u32(&bytes[0..4]) != CBW_SIGNATURE {
            return None;
        }

        let flags = bytes[12];
        let lun = bytes[13];
        let command_length = bytes[14];

        if flags & 0x7f != 0 || lun != 0 || command_length == 0 || command_length > 16 {
            return None;
        }

        let mut command = [0u8; 16];

        command.copy_from_slice(&bytes[15..31]);

        Some(Self {
            tag: le_u32(&bytes[4..8]),
            transfer_length: le_u32(&bytes[8..12]),
            direction_in: flags & 0x80 != 0,
            command_length,
            command,
        })
    }

    fn command(&self) -> &[u8] {
        &self.command[..usize::from(self.command_length)]
    }
}

#[derive(Debug, Clone, Copy)]
struct CommandStatusWrapper {
    tag: u32,
    residue: u32,
    status: CswStatus,
}

impl CommandStatusWrapper {
    const fn new(tag: u32, residue: u32, status: CswStatus) -> Self {
        Self {
            tag,
            residue,
            status,
        }
    }

    fn encode(self) -> [u8; CSW_LENGTH] {
        let mut bytes = [0u8; CSW_LENGTH];

        bytes[0..4].copy_from_slice(&CSW_SIGNATURE.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.tag.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.residue.to_le_bytes());
        bytes[12] = self.status as u8;

        bytes
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CswStatus {
    Passed = 0,
    Failed = 1,
    PhaseError = 2,
}

#[derive(Debug, Clone, Copy)]
struct Execution {
    transferred: u32,
    out_consumed: u32,
    status: CswStatus,
    ejected: bool,
}

impl Execution {
    const fn passed(transferred: u32) -> Self {
        Self {
            transferred,
            out_consumed: 0,
            status: CswStatus::Passed,
            ejected: false,
        }
    }

    const fn failed() -> Self {
        Self {
            transferred: 0,
            out_consumed: 0,
            status: CswStatus::Failed,
            ejected: false,
        }
    }

    const fn phase_error() -> Self {
        Self {
            transferred: 0,
            out_consumed: 0,
            status: CswStatus::PhaseError,
            ejected: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Sense {
    key: u8,
    asc: u8,
    ascq: u8,
}

impl Sense {
    const NONE: Self = Self::new(SENSE_KEY_NO_SENSE, 0, 0);

    const fn new(key: u8, asc: u8, ascq: u8) -> Self {
        Self { key, asc, ascq }
    }
}

fn parse_read_write(cdb: &[u8]) -> Option<(u64, u32, bool)> {
    match cdb.first().copied()? {
        SCSI_READ_6 | SCSI_WRITE_6 if cdb.len() >= 6 => {
            let lba =
                (u64::from(cdb[1] & 0x1f) << 16) | (u64::from(cdb[2]) << 8) | u64::from(cdb[3]);

            let blocks = if cdb[4] == 0 { 256 } else { u32::from(cdb[4]) };

            Some((lba, blocks, cdb[0] == SCSI_WRITE_6))
        }

        SCSI_READ_10 | SCSI_WRITE_10 if cdb.len() >= 10 => Some((
            u64::from(be_u32(&cdb[2..6])),
            u32::from(be_u16(&cdb[7..9])),
            cdb[0] == SCSI_WRITE_10,
        )),

        SCSI_READ_12 | SCSI_WRITE_12 if cdb.len() >= 12 => Some((
            u64::from(be_u32(&cdb[2..6])),
            be_u32(&cdb[6..10]),
            cdb[0] == SCSI_WRITE_12,
        )),

        SCSI_READ_16 | SCSI_WRITE_16 if cdb.len() >= 16 => Some((
            be_u64(&cdb[2..10]),
            be_u32(&cdb[10..14]),
            cdb[0] == SCSI_WRITE_16,
        )),

        _ => None,
    }
}

fn le_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn be_u16(bytes: &[u8]) -> u16 {
    u16::from_be_bytes([bytes[0], bytes[1]])
}

fn be_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn be_u64(bytes: &[u8]) -> u64 {
    u64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ])
}
