#![no_std]
#![feature(allocator_api)]
#![feature(allow_internal_unstable)]
#![feature(build_hasher_simple_hash_one)]
#![feature(coerce_unsized)]
#![feature(const_mut_refs)]
#![feature(const_trait_impl)]
#![feature(dispatch_from_dyn)]
#![feature(dropck_eyepatch)]
#![feature(error_in_core)]
#![feature(extend_one)]
#![feature(hashmap_internals)]
#![feature(layout_for_ptr)]
#![feature(never_type)]
#![feature(pointer_byte_offsets)]
#![feature(receiver_trait)]
#![feature(result_option_inspect)]
#![feature(slice_concat_trait)]
#![feature(slice_ptr_get)]
#![feature(str_internals)]
#![feature(unsize)]
#![feature(utf8_chunks)]

extern crate alloc;

pub mod ffi;
pub mod hash;
pub mod io;
pub mod path;
pub mod sync;
pub mod thread;
