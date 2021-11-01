cfg_if::cfg_if! {
      if #[cfg(target_arch = "x86_64")] {
            pub mod x86_64;
            pub use x86_64 as arch;
      }
}

use alloc::boxed::Box;
use core::fmt::Debug;

use paging::LAddr;

pub const KSTACK_SIZE: usize = paging::PAGE_SIZE * 16;

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
            let frame = kstack.assume_init_mut().task_frame_mut();
            let rem = frame.set_entry(entry, ty);
            rem
        };
        (unsafe { Box::from_raw(Box::into_raw(kstack).cast()) }, rem)
    }

    pub fn new_syscall() -> Box<Self> {
        unsafe { Box::from_raw(Box::into_raw(box core::mem::MaybeUninit::<Self>::uninit()).cast()) }
    }

    #[cfg(target_arch = "x86_64")]
    pub fn task_frame(&self) -> &arch::Frame {
        let ptr = self.0.as_ptr_range().end.cast::<arch::Frame>();

        unsafe { &*ptr.sub(1) }
    }

    #[cfg(target_arch = "x86_64")]
    pub fn task_frame_mut(&mut self) -> &mut arch::Frame {
        let ptr = self.0.as_mut_ptr_range().end.cast::<arch::Frame>();

        unsafe { &mut *ptr.sub(1) }
    }
}

impl Debug for Kstack {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Kstack {{ {:?} }} ", *self.task_frame())
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
