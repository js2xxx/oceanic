cfg_if::cfg_if! {
    if #[cfg(target_arch = "x86_64")] {
        pub mod x86_64;
        pub use self::x86_64 as arch;
    }
}

use alloc::boxed::Box;
use core::{
    alloc::Layout,
    fmt::Debug,
    num::NonZeroU64,
    ops::{Deref, DerefMut},
    ptr::{self, NonNull},
};

use paging::{LAddr, PAGE_SIZE};

use crate::{
    cpu::arch::seg::ndt::INTR_CODE,
    mem::space::{self, Flags},
};

pub const KSTACK_SIZE: usize = paging::PAGE_SIZE * 18;

#[derive(Debug)]
pub struct Entry {
    pub entry: LAddr,
    pub stack: LAddr,
    pub args: [u64; 2],
}

#[repr(align(4096))]
pub struct KstackData([u8; KSTACK_SIZE]);

impl KstackData {
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
}

pub struct Kstack {
    ptr: NonNull<KstackData>,
    kframe_ptr: *mut u8,
    pf_resume: Option<NonZeroU64>,
}

unsafe impl Send for Kstack {}

impl Kstack {
    pub fn new(entry: Option<Entry>, ty: super::Type) -> Self {
        let ptr = space::allocate(
            Layout::new::<KstackData>().size(),
            Flags::READABLE | Flags::WRITABLE,
            false,
        )
        .expect("Failed to allocate kernel stack");
        unsafe {
            let pad = NonNull::slice_from_raw_parts(ptr.as_non_null_ptr(), PAGE_SIZE);
            space::reprotect_unchecked(pad, Flags::READABLE).expect("Failed to set padding");
        }

        let mut kstack = ptr.cast::<KstackData>();
        let kframe_ptr = unsafe {
            let this = kstack.as_mut();
            let frame = this.task_frame_mut();
            match entry {
                Some(entry) => frame.init_entry(&entry, ty),
                None => frame.init_zeroed(ty),
            }
            let kframe = (frame as *mut arch::Frame).cast::<arch::Kframe>().sub(1);
            kframe.write(arch::Kframe::new(
                (frame as *mut arch::Frame).cast(),
                INTR_CODE.into_val() as u64,
            ));
            kframe.cast()
        };
        Kstack {
            ptr: kstack,
            kframe_ptr,
            pf_resume: None,
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    pub fn kframe_ptr(&self) -> *mut u8 {
        self.kframe_ptr
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    pub fn kframe_ptr_mut(&mut self) -> *mut *mut u8 {
        &mut self.kframe_ptr
    }

    #[inline]
    pub fn pf_resume_mut(&mut self) -> *mut Option<NonZeroU64> {
        &mut self.pf_resume
    }

    #[cfg(target_arch = "x86_64")]
    pub unsafe fn pf_resume(
        &mut self,
        cur_frame: &mut arch::Frame,
        errc: u64,
        addr: u64,
    ) -> sv_call::Result {
        match self.pf_resume.take() {
            None => Err(sv_call::Error::ENOENT),
            Some(ret) => {
                cur_frame.set_pf_resume(ret.into(), errc, addr);
                Ok(())
            }
        }
    }
}

impl Deref for Kstack {
    type Target = KstackData;

    fn deref(&self) -> &Self::Target {
        unsafe { self.ptr.as_ref() }
    }
}

impl Drop for Kstack {
    #[inline]
    fn drop(&mut self) {
        let _ = unsafe { space::unmap(self.ptr.cast()) };
    }
}

impl DerefMut for Kstack {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.ptr.as_mut() }
    }
}

impl Debug for Kstack {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Kstack {{ {:?} }} ", *self.task_frame())
    }
}

#[derive(Debug)]
#[repr(align(16))]
struct ExtFrameData([u8; arch::EXTENDED_FRAME_SIZE]);

#[derive(Debug)]
pub struct ExtFrame(Box<ExtFrameData>);

impl ExtFrame {
    pub fn zeroed() -> Self {
        ExtFrame(box ExtFrameData([0; arch::EXTENDED_FRAME_SIZE]))
    }

    pub unsafe fn save(&mut self) {
        let ptr = (self.0).0.as_mut_ptr();
        archop::fpu::save(ptr);
    }

    pub unsafe fn load(&self) {
        let ptr = (self.0).0.as_ptr();
        archop::fpu::load(ptr);
    }
}

impl Deref for ExtFrame {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        &(self.0).0
    }
}

impl DerefMut for ExtFrame {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut (self.0).0
    }
}

pub unsafe fn switch_ctx(old: Option<*mut *mut u8>, new: *mut u8) {
    arch::switch_kframe(old.unwrap_or(ptr::null_mut()), new);
    arch::switch_finishing();
}
