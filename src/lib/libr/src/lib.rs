#![no_std]
#![feature(alloc_error_handler)]
#![feature(alloc_layout_extra)]
#![feature(lang_items)]
#![feature(never_type)]
#![feature(nonnull_slice_from_raw_parts)]

extern crate alloc;

mod alloc2;
pub mod env;
pub mod rt;
