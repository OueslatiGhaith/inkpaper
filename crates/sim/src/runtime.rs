use inkpaper_app::AppResources;
use inkpaper_ui::{Pool, Runtime, Table};

pub(crate) type SimulatorRuntime<'resources> =
    Runtime<AppResources<'resources, 128, { 16 * 1024 }>>;

/// the bytes the runtime's tables take from its pool, with room to spare
const POOL_BYTES: usize = 256 * 1024;

pub(crate) fn new_runtime<'resources>() -> SimulatorRuntime<'resources> {
    let pool = Pool::new(Box::leak(Box::<[u8]>::new_uninit_slice(POOL_BYTES)));
    let mut runtime = SimulatorRuntime::new(pool);
    // render only the entities that changed, like the device
    runtime.set_partial_rebuilds(true);

    runtime
}

/// prints how much of the pool the runtime used, to size the device's pool from
pub(crate) fn print_memory(runtime: &SimulatorRuntime<'_>) {
    let memory = runtime.memory();

    println!(
        "ui pool: peak {} of {} bytes, {} in use",
        memory.peak, memory.capacity, memory.used
    );
    for table in Table::ALL {
        println!("  {table:?}: {} bytes", memory.table(table));
    }
}
