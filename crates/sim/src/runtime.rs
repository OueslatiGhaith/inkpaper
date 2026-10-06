use inkpaper_app::AppResources;
#[cfg(not(feature = "alloc"))]
use inkpaper_ui::FixedStorage;
use inkpaper_ui::Runtime;
#[cfg(feature = "alloc")]
use inkpaper_ui::{
    HeapCallbackArena, HeapElementStates, HeapEntityArena, HeapFrame, HeapGlobalArena,
    RuntimeStorage,
};

/// heap tables, with these initial reservations
#[cfg(feature = "alloc")]
pub(crate) struct SimulatorStorage;

#[cfg(feature = "alloc")]
impl RuntimeStorage for SimulatorStorage {
    type Entities = HeapEntityArena<32>;
    type Callbacks = HeapCallbackArena<64>;
    type Globals = HeapGlobalArena<8>;
    type Frame = HeapFrame<64, 2_048>;
    type ElementStates = HeapElementStates<256>;
}

#[cfg(not(feature = "alloc"))]
pub(crate) type SimulatorStorage =
    FixedStorage<16_384, 32, 8_192, 64, 2_048, 32_768, 256, 2_048, 8>;

pub(crate) type SimulatorRuntime<'resources> =
    Runtime<SimulatorStorage, AppResources<'resources, 128, { 16 * 1024 }>>;

pub(crate) fn new_runtime<'resources>() -> SimulatorRuntime<'resources> {
    let mut runtime = SimulatorRuntime::default();
    // render only the entities that changed, like the device
    runtime.set_partial_rebuilds(true);

    runtime
}
