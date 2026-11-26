use alloc::vec::Vec;

use defmt::debug;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex, signal::Signal,
};
use embassy_time::Delay;
use esp_hal::{Async, sdmmc::Slot};
use hadris_fat::r#async::FatVolume;
use sdio::{
    BlockDevice, BlockReadCommand, BlockWriteCommand, BusWidth, ByteReadCommand, ByteWriteCommand,
    ControlCommand, MmcBus, MmcError, TuningOp, sd::Card,
};

use super::{
    filesystem,
    partition::{SECTOR_SIZE, SdPartition},
    random_access::OpenRandomAccessFile,
    state,
    types::{MAX_STATE_BYTES, RandomAccessHandle, StorageEntry, StorageError},
};

/// The random-access files kept open together: a book, and a dictionary's
/// index, definitions and synonyms. Opening another closes the one read
/// least recently.
const MAX_OPEN_RANDOM_ACCESS: usize = 4;

/// Newtype around `esp_hal::sdmmc::Slot` whose fields are all `Send`; the only
/// `!Send` marker comes from `esp_hal::Async(PhantomData<*const ()>)`.
pub(super) struct SendSlot(Slot<'static, 1, Async>);

impl SendSlot {
    pub(super) fn new(slot: Slot<'static, 1, Async>) -> Self {
        Self(slot)
    }
}

// SAFETY: Every field of `Slot<'static, 1, Async>` is `Send` (plain config
// integers, `PeripheralGuard`, and `Option<OutputSignal>`), SDHOST async
// completion uses atomic/critical-section state, and all access is serialized
// under the async `STORAGE` mutex or exclusively owned by `storage_task`.
unsafe impl Send for SendSlot {}

impl MmcBus for SendSlot {
    async fn send_command<'a, C>(&mut self, cmd: C) -> Result<C::Resp<'a>, MmcError>
    where
        C: ControlCommand + 'a,
    {
        self.0.send_command(cmd).await
    }

    async fn read_blocks<'a, C>(
        &mut self,
        cmd: C,
        auto_stop: bool,
    ) -> Result<C::Resp<'a>, MmcError>
    where
        C: BlockReadCommand + 'a,
    {
        self.0.read_blocks(cmd, auto_stop).await
    }

    async fn write_blocks<'a, C>(
        &mut self,
        cmd: C,
        auto_stop: bool,
    ) -> Result<C::Resp<'a>, MmcError>
    where
        C: BlockWriteCommand + 'a,
    {
        self.0.write_blocks(cmd, auto_stop).await
    }

    async fn read_bytes<'a, C>(&mut self, cmd: C) -> Result<C::Resp<'a>, MmcError>
    where
        C: ByteReadCommand + 'a,
    {
        self.0.read_bytes(cmd).await
    }

    async fn write_bytes<'a, C>(&mut self, cmd: C) -> Result<C::Resp<'a>, MmcError>
    where
        C: ByteWriteCommand + 'a,
    {
        self.0.write_bytes(cmd).await
    }

    async fn init_idle(&mut self, hz: u32) -> Result<(), MmcError> {
        self.0.init_idle(hz).await
    }

    async fn tune_bus<O>(&mut self, width: BusWidth, hz: u32, op: O) -> Result<(), MmcError>
    where
        O: TuningOp,
    {
        self.0.tune_bus(width, hz, op).await
    }

    async fn wait_for_event(&mut self) -> Result<(), MmcError> {
        self.0.wait_for_event().await
    }

    fn set_bus(&mut self, width: BusWidth, hz: u32) -> Result<(), MmcError> {
        self.0.set_bus(width, hz)
    }

    fn supports_mmc(&self) -> bool {
        self.0.supports_mmc()
    }

    fn supports_auto_stop(&self) -> bool {
        self.0.supports_auto_stop()
    }

    fn supports_bus_width(&self) -> BusWidth {
        self.0.supports_bus_width()
    }

    fn supports_1v8(&self) -> bool {
        self.0.supports_1v8()
    }

    fn supports_frequency(&self) -> u32 {
        self.0.supports_frequency()
    }
}

pub(super) type SdCard = BlockDevice<Card, SendSlot, Delay, SECTOR_SIZE>;
pub(super) type MountedVolume = FatVolume<SdPartition<SdCard>>;

pub(super) struct MountedStorage {
    filesystem: MountedVolume,
    // Least recently read first.
    open_random_access: Vec<OpenRandomAccessFile>,
    last_handle: u32,
}

impl MountedStorage {
    pub(super) fn new(filesystem: MountedVolume) -> Self {
        Self {
            filesystem,
            open_random_access: Vec::new(),
            last_handle: 0,
        }
    }

    pub(super) fn into_card(self) -> SdCard {
        drop(self.open_random_access);
        self.filesystem.into_inner().into_inner()
    }
}

/// Inline in static internal DRAM so `SdPartition`'s sector buffer stays
/// reachable by ESP32-S3 SDHOST IDMAC (which cannot DMA into PSRAM).
#[allow(clippy::large_enum_variant)]
pub(super) enum StorageState {
    Unavailable,
    Mounted(MountedStorage),
    UsbDrive,
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LifecycleCommand {
    EnterUsbDrive,
    Shutdown,
}

static STORAGE: Mutex<CriticalSectionRawMutex, StorageState> =
    Mutex::new(StorageState::Unavailable);
static READY: Signal<CriticalSectionRawMutex, bool> = Signal::new();

static LIFECYCLE_CMD: Signal<CriticalSectionRawMutex, LifecycleCommand> = Signal::new();
static USB_DRIVE_ACK: Signal<CriticalSectionRawMutex, bool> = Signal::new();
static SHUTDOWN_ACK: Signal<CriticalSectionRawMutex, ()> = Signal::new();

pub async fn wait_ready() -> bool {
    READY.wait().await
}

pub async fn list_directory(path: &str) -> Result<Vec<StorageEntry>, StorageError> {
    let guard = STORAGE.lock().await;
    let StorageState::Mounted(mounted) = &*guard else {
        return Err(StorageError::Unavailable);
    };

    filesystem::list_directory(&mounted.filesystem, path).await
}

pub async fn open_random_access(path: &str) -> Result<RandomAccessHandle, StorageError> {
    let mut guard = STORAGE.lock().await;
    let StorageState::Mounted(mounted) = &mut *guard else {
        return Err(StorageError::Unavailable);
    };

    let id = next_handle(&mut mounted.last_handle);
    let file = OpenRandomAccessFile::open(&mounted.filesystem, path, id).await?;
    let handle = file.handle();

    if mounted.open_random_access.len() == MAX_OPEN_RANDOM_ACCESS {
        mounted.open_random_access.remove(0);
    }

    mounted.open_random_access.push(file);

    Ok(handle)
}

pub async fn read_random_access_into(
    handle: u32,
    offset: u64,
    buffer: &mut [u8],
) -> Result<(), StorageError> {
    let mut guard = STORAGE.lock().await;
    let StorageState::Mounted(mounted) = &mut *guard else {
        return Err(StorageError::Unavailable);
    };

    let Some(index) = mounted
        .open_random_access
        .iter()
        .position(|file| file.handle().id() == handle)
    else {
        return Err(StorageError::StaleHandle);
    };

    let result = mounted.open_random_access[index]
        .read_exact_at(&mounted.filesystem, offset, buffer)
        .await;

    // Most recently read last.
    mounted.open_random_access[index..].rotate_left(1);

    result
}

/// Reads a whole file, apart from the open random-access handles.
pub async fn read_file(path: &str, max_bytes: usize) -> Result<Vec<u8>, StorageError> {
    let guard = STORAGE.lock().await;
    let StorageState::Mounted(mounted) = &*guard else {
        return Err(StorageError::Unavailable);
    };

    filesystem::read_file(&mounted.filesystem, path, max_bytes).await
}

pub async fn load_state(name: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, StorageError> {
    if max_bytes > MAX_STATE_BYTES {
        return Err(StorageError::StateTooLarge);
    }

    let guard = STORAGE.lock().await;
    let StorageState::Mounted(mounted) = &*guard else {
        return Err(StorageError::Unavailable);
    };

    state::load_state(&mounted.filesystem, name, max_bytes).await
}

pub async fn save_state(name: &str, bytes: &[u8]) -> Result<(), StorageError> {
    if bytes.len() > MAX_STATE_BYTES {
        return Err(StorageError::StateTooLarge);
    }

    let guard = STORAGE.lock().await;
    let StorageState::Mounted(mounted) = &*guard else {
        return Err(StorageError::Unavailable);
    };

    state::save_state(&mounted.filesystem, name, bytes).await
}

pub async fn enter_usb_drive_and_wait() -> bool {
    USB_DRIVE_ACK.reset();
    LIFECYCLE_CMD.signal(LifecycleCommand::EnterUsbDrive);
    USB_DRIVE_ACK.wait().await
}

pub async fn shutdown_and_wait() {
    SHUTDOWN_ACK.reset();
    LIFECYCLE_CMD.signal(LifecycleCommand::Shutdown);
    SHUTDOWN_ACK.wait().await;
}

pub(super) fn signal_ready(ready: bool) {
    READY.signal(ready);
}

pub(super) async fn publish_mounted(filesystem: MountedVolume) {
    let mut guard = STORAGE.lock().await;
    *guard = StorageState::Mounted(MountedStorage::new(filesystem));
    drop(guard);

    signal_ready(true);
}

pub(super) async fn take_mounted_for_usb_drive() -> Option<SdCard> {
    let mut guard = STORAGE.lock().await;
    let previous = core::mem::replace(&mut *guard, StorageState::UsbDrive);
    drop(guard);

    match previous {
        StorageState::Mounted(mounted) => Some(mounted.into_card()),
        _ => None,
    }
}

pub(super) async fn mark_unavailable() {
    let mut guard = STORAGE.lock().await;
    *guard = StorageState::Unavailable;
}

pub(super) async fn teardown_for_shutdown() {
    let mut guard = STORAGE.lock().await;
    *guard = StorageState::Shutdown;
}

pub(super) async fn wait_lifecycle_command() -> LifecycleCommand {
    let command = LIFECYCLE_CMD.wait().await;
    match command {
        LifecycleCommand::EnterUsbDrive => debug!("USB Drive storage handoff requested"),
        LifecycleCommand::Shutdown => debug!("storage shutdown requested"),
    }
    command
}

pub(super) fn ack_usb_drive(ready: bool) {
    USB_DRIVE_ACK.signal(ready);
}

pub(super) fn ack_shutdown() {
    SHUTDOWN_ACK.signal(());
}

pub(super) async fn wait_unavailable_shutdown() {
    signal_ready(false);
    loop {
        match LIFECYCLE_CMD.wait().await {
            LifecycleCommand::EnterUsbDrive => ack_usb_drive(false),
            LifecycleCommand::Shutdown => return,
        }
    }
}

/// Refuses extra USB drive requests while the card is a USB drive, returning
/// once shutdown is requested.
pub(super) async fn wait_usb_drive_shutdown() {
    loop {
        match LIFECYCLE_CMD.wait().await {
            LifecycleCommand::EnterUsbDrive => ack_usb_drive(true),
            LifecycleCommand::Shutdown => return,
        }
    }
}

fn next_handle(last: &mut u32) -> u32 {
    *last = last.wrapping_add(1);

    if *last == 0 {
        *last = 1;
    }

    *last
}
