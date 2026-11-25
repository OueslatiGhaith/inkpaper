use inkpaper_app::AppResources;
use inkpaper_ui::{Pool, Runtime, Table};

pub(crate) type SimulatorRuntime<'resources> =
    Runtime<AppResources<'resources, 128, { 16 * 1024 }>>;

pub(crate) fn new_runtime<'resources>() -> SimulatorRuntime<'resources> {
    let mut runtime = SimulatorRuntime::new(Pool::global());
    // render only the entities that changed, like the device
    runtime.set_partial_rebuilds(true);

    runtime
}

/// prints how much memory the runtime's tables use
pub(crate) fn print_memory(runtime: &SimulatorRuntime<'_>) {
    let memory = runtime.memory();

    match memory.capacity {
        Some(capacity) => println!(
            "ui pool: peak {} of {} bytes, {} in use",
            memory.peak, capacity, memory.used
        ),
        None => println!(
            "ui pool: peak {} bytes, {} in use",
            memory.peak, memory.used
        ),
    }
    for table in Table::ALL {
        println!("  {table:?}: {} bytes", memory.table(table));
    }
}
