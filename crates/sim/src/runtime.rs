use inkpaper_ui::{Runtime, RuntimeResources};

#[cfg(feature = "alloc")]
const FRAME_NODES: usize = 64;
#[cfg(not(feature = "alloc"))]
const FRAME_NODES: usize = 2_048;

#[cfg(feature = "alloc")]
const FRAME_TEXT_BYTES: usize = 2_048;
#[cfg(not(feature = "alloc"))]
const FRAME_TEXT_BYTES: usize = 32_768;

pub(crate) type SimulatorRuntime<'resources> = Runtime<
    16_384,
    32,
    8_192,
    64,
    FRAME_NODES,
    FRAME_TEXT_BYTES,
    256,
    2_048,
    8,
    RuntimeResources<'resources, 2, 128, { 16 * 1024 }, 32>,
>;

pub(crate) fn new_runtime<'resources>() -> SimulatorRuntime<'resources> {
    SimulatorRuntime::default()
}
