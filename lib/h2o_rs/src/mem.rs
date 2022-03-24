mod phys;
mod space;

use core::alloc::Layout;

pub use sv_call::mem::Flags;

pub use self::{phys::Phys, space::Space};

cfg_if::cfg_if! { if #[cfg(target_arch = "x86_64")] {

pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;
pub const PAGE_MASK: usize = PAGE_SIZE - 1;

}}
// SAFETY: Both the size and the alignment are 2^n-bounded.
pub const PAGE_LAYOUT: Layout = unsafe { Layout::from_size_align_unchecked(PAGE_SIZE, PAGE_SIZE) };
