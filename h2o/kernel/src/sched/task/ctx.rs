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
pub struct Entry<'a> {
      pub entry: LAddr,
      pub stack: LAddr,
      pub tls: Option<LAddr>,
      pub args: &'a [u64],
}

#[repr(align(4096))]
pub struct Kstack([u8; KSTACK_SIZE]);

impl Kstack {
      pub fn new<'a>(entry: Entry<'a>, ty: super::Type) -> (Box<Self>, Option<&'a [u64]>) {
            let mut kstack = box core::mem::MaybeUninit::<Self>::uninit();
            let rem = unsafe {
                  let frame = kstack.assume_init_mut().as_frame_mut();
                  frame.set_entry(entry, ty)
            };
            (unsafe { Box::from_raw(Box::into_raw(kstack).cast()) }, rem)
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

#[derive(Debug)]
#[repr(align(16))]
pub struct ExtendedFrame([u8; arch::EXTENDED_FRAME_SIZE]);

impl ExtendedFrame {
      pub unsafe fn save(&mut self) {
            let ptr = self.0.as_mut_ptr();
            archop::fpu::save(ptr);
      }

      pub unsafe fn load(&self) {
            let ptr = self.0.as_ptr();
            archop::fpu::load(ptr);
      }
}
