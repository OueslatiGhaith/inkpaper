#![cfg_attr(target_arch = "xtensa", no_std)]
#![cfg_attr(target_arch = "xtensa", no_main)]

#[cfg(target_arch = "xtensa")]
mod firmware;

#[cfg(not(target_arch = "xtensa"))]
fn main() {}
