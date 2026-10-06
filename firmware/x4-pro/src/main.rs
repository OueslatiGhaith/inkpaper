#![cfg_attr(target_arch = "xtensa", no_std)]
#![cfg_attr(target_arch = "xtensa", no_main)]
// explicit drops mark ownership boundaries (the SD card before power-off, the framebuffer
// borrow) and end trace span guards, which only implement Drop with `trace`
#![allow(clippy::drop_non_drop)]

#[cfg(target_arch = "xtensa")]
extern crate alloc;

#[cfg(target_arch = "xtensa")]
mod firmware;

#[cfg(not(target_arch = "xtensa"))]
fn main() {}
