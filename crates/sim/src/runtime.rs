use inkpaper_ui::{Runtime, RuntimeResources};

use crate::READER_IMAGE_CAPACITY;

// allocated storage starts small and grows.
#[cfg(feature = "alloc")]
const FRAME_NODES: usize = 64;
// fixed storage keeps its hard limits
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
    RuntimeResources<'resources, 2, 128, { 16 * 1024 }, READER_IMAGE_CAPACITY>,
>;

pub(crate) fn new_runtime<'resources>() -> Box<SimulatorRuntime<'resources>> {
    Box::new(SimulatorRuntime::default())
}
