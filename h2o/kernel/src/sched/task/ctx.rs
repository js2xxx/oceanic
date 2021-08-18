cfg_if::cfg_if! {
      if #[cfg(target_arch = "x86_64")] {
            pub mod x86_64;
            pub use x86_64 as arch;
      }
}

use paging::LAddr;

use alloc::boxed::Box;
use core::fmt::Debug;

pub const KSTACK_SIZE: usize = paging::PAGE_SIZE * 6;

#[derive(Debug)]
pub struct Entry {
      pub entry: LAddr,
      pub stack: LAddr,
      pub args: [u64; 2],
}

#[repr(align(4096))]
pub struct Kstack([u8; KSTACK_SIZE]);

impl Kstack {
      pub fn new(entry: Entry, ty: super::Type) -> Box<Self> {
            let mut kstack = box unsafe { core::mem::zeroed::<Kstack>() };
            unsafe {
                  let frame = kstack.as_frame_mut();
                  frame.set_entry(entry, ty);
            }
            kstack
      }

      pub fn top(&self) -> LAddr {
            LAddr::new(self.0.as_ptr_range().end as *mut _)
      }

      #[cfg(target_arch = "x86_64")]
      pub unsafe fn as_frame(&self) -> &arch::Frame {
            let ptr = self.top().cast::<arch::Frame>();

            &*ptr.sub(1)
      }

      #[cfg(target_arch = "x86_64")]
      pub unsafe fn as_frame_mut(&mut self) -> &mut arch::Frame {
            let ptr = self.top().cast::<arch::Frame>();

            &mut *ptr.sub(1)
      }
}

impl Debug for Kstack {
      fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "Kstack {{ {:?} }} ", *unsafe { self.as_frame() })
      }
}
