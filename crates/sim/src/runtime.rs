use inkpaper_app::AppResources;
#[cfg(not(feature = "alloc"))]
use inkpaper_ui::FixedStorage;
#[cfg(feature = "alloc")]
use inkpaper_ui::{
    HeapCallbackArena, HeapElementStates, HeapEntityArena, HeapEvents, HeapGlobalArena,
    RuntimeStorage,
};
use inkpaper_ui::{Pool, Runtime};

/// heap tables, with these initial reservations
#[cfg(feature = "alloc")]
pub(crate) struct SimulatorStorage;

#[cfg(feature = "alloc")]
impl RuntimeStorage for SimulatorStorage {
    type Entities = HeapEntityArena<32>;
    type Callbacks = HeapCallbackArena<64>;
    type Globals = HeapGlobalArena<8>;
    type ElementStates = HeapElementStates<256>;
    type Events = HeapEvents;
}

#[cfg(not(feature = "alloc"))]
pub(crate) type SimulatorStorage = FixedStorage<16_384, 32, 8_192, 64, 256, 2_048, 8, 8, 8>;

pub(crate) type SimulatorRuntime<'resources> =
    Runtime<SimulatorStorage, AppResources<'resources, 128, { 16 * 1024 }>>;

/// the bytes the runtime's frame takes from its pool, with room to spare
const POOL_BYTES: usize = 256 * 1024;

pub(crate) fn new_runtime<'resources>() -> SimulatorRuntime<'resources> {
    let pool = Pool::new(Box::leak(Box::<[u8]>::new_uninit_slice(POOL_BYTES)));
    let mut runtime = SimulatorRuntime::new(pool);
    // render only the entities that changed, like the device
    runtime.set_partial_rebuilds(true);

    runtime
}
