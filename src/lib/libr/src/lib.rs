#![no_std]
#![feature(alloc_error_handler)]
#![feature(alloc_layout_extra)]
#![feature(allocator_api)]
#![feature(allow_internal_unstable)]
#![feature(coerce_unsized)]
#![feature(const_trait_impl)]
#![feature(dropck_eyepatch)]
#![feature(int_roundings)]
#![feature(lang_items)]
#![feature(layout_for_ptr)]
#![feature(once_cell)]
#![feature(never_type)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(receiver_trait)]
#![feature(result_option_inspect)]
#![feature(slice_ptr_get)]
#![feature(thread_local)]
#![feature(unsize)]

extern crate alloc;

mod alloc2;
pub mod env;
pub mod rt;
pub mod sync;
pub mod thread;
