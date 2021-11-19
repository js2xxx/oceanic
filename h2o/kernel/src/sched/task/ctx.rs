cfg_if::cfg_if! {
    if #[cfg(target_arch = "x86_64")] {
        pub mod x86_64;
        pub use x86_64 as arch;
    }
}

use alloc::boxed::Box;
use core::{fmt::Debug, ptr};

use paging::LAddr;

pub const KSTACK_SIZE: usize = paging::PAGE_SIZE * 16;

#[derive(Debug)]
pub struct Entry {
    pub entry: LAddr,
    pub stack: LAddr,
    pub tls: Option<LAddr>,
    pub args: [u64; 2],
}

#[repr(align(4096))]
pub struct Kstack([u8; KSTACK_SIZE], *mut u8);

unsafe impl Send for Kstack {}

impl Kstack {
    pub fn new(entry: Entry, ty: super::Type) -> Box<Self> {
        let mut kstack = box core::mem::MaybeUninit::<Self>::uninit();
        unsafe {
            let this = kstack.assume_init_mut();
            let frame = this.task_frame_mut();
            frame.set_entry(entry, ty);
            let kframe = (frame as *mut arch::Frame).cast::<arch::Kframe>().sub(1);
            kframe.write(arch::Kframe::new((frame as *mut arch::Frame).cast()));
            this.1 = kframe.cast();
        }
        unsafe { kstack.assume_init() }
    }

    pub fn top(&self) -> LAddr {
        LAddr::new(self.0.as_ptr_range().end as *mut u8)
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

    #[cfg(target_arch = "x86_64")]
    pub fn kframe_ptr(&self) -> *mut u8 {
        self.1
    }

    #[cfg(target_arch = "x86_64")]
    pub fn kframe_ptr_mut(&mut self) -> *mut *mut u8 {
        &mut self.1
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
    pub fn zeroed() -> Box<Self> {
        box ExtendedFrame([0; arch::EXTENDED_FRAME_SIZE])
    }

    pub unsafe fn save(&mut self) {
        let ptr = self.0.as_mut_ptr();
        archop::fpu::save(ptr);
    }

    pub unsafe fn load(&self) {
        let ptr = self.0.as_ptr();
        archop::fpu::load(ptr);
    }
}

pub unsafe fn switch_ctx(old: Option<*mut *mut u8>, new: *mut u8) {
    let _lock = archop::IntrState::lock();
    arch::switch_kframe(old.unwrap_or(ptr::null_mut()), new);
    arch::switch_finishing();
}
