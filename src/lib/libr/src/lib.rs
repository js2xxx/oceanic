#![no_std]
#![feature(alloc_error_handler)]
#![feature(alloc_layout_extra)]
#![feature(allocator_api)]
#![feature(int_roundings)]
#![feature(lang_items)]
#![feature(never_type)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(thread_local)]

extern crate alloc;

mod alloc2;
pub mod env;
pub mod rt;
